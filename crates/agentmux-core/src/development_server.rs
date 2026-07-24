//! ANSI/OSC-safe development server URL discovery from a raw PTY byte stream.

use std::collections::HashMap;
use std::net::IpAddr;

const DEFAULT_MAX_BUFFER_BYTES: usize = 16 * 1024;
const DEFAULT_MAX_CANDIDATE_BYTES: usize = 2048;
const DEFAULT_DEDUPE_TTL_MS: u64 = 5 * 60 * 1000;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PtyStreamMetadata {
    pub session_id: String,
    pub process_id: Option<u32>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DevelopmentServerUrlCandidate {
    pub origin: String,
    pub session_id: String,
    pub process_id: Option<u32>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PtyUrlDetectorConfig {
    pub max_buffer_bytes: usize,
    pub max_candidate_bytes: usize,
    pub dedupe_ttl_ms: u64,
}

impl Default for PtyUrlDetectorConfig {
    fn default() -> Self {
        Self {
            max_buffer_bytes: DEFAULT_MAX_BUFFER_BYTES,
            max_candidate_bytes: DEFAULT_MAX_CANDIDATE_BYTES,
            dedupe_ttl_ms: DEFAULT_DEDUPE_TTL_MS,
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum EscapeState {
    Text,
    Escape,
    Csi,
    Osc,
    OscEscape,
}

/// Maintains enough rendered-text history to recognize origins split across
/// arbitrary PTY reads. OSC contents are never inspected because they are
/// terminal metadata rather than visible output.
#[derive(Debug)]
pub struct PtyUrlDetector {
    config: PtyUrlDetectorConfig,
    state: EscapeState,
    text: Vec<u8>,
    recent: HashMap<String, u64>,
}

impl PtyUrlDetector {
    pub fn new(config: PtyUrlDetectorConfig) -> Self {
        Self {
            config,
            state: EscapeState::Text,
            text: Vec::new(),
            recent: HashMap::new(),
        }
    }

    pub fn push(
        &mut self,
        bytes: &[u8],
        metadata: &PtyStreamMetadata,
        now_ms: u64,
    ) -> Vec<DevelopmentServerUrlCandidate> {
        for &byte in bytes {
            self.consume(byte);
        }
        self.collect(metadata, now_ms, false)
    }

    /// Flush an unterminated final line, for example when a process exits
    /// immediately after logging its development-server URL.
    pub fn finish(
        &mut self,
        metadata: &PtyStreamMetadata,
        now_ms: u64,
    ) -> Vec<DevelopmentServerUrlCandidate> {
        self.collect(metadata, now_ms, true)
    }

    fn collect(
        &mut self,
        metadata: &PtyStreamMetadata,
        now_ms: u64,
        include_unterminated: bool,
    ) -> Vec<DevelopmentServerUrlCandidate> {
        self.prune_expired(now_ms);
        let mut candidates = Vec::new();
        for origin in extract_origins(
            &self.text,
            self.config.max_candidate_bytes,
            include_unterminated,
        ) {
            let key = format!("{}|{}", metadata.session_id, origin);
            if self.recent.contains_key(&key) {
                continue;
            }
            self.recent
                .insert(key, now_ms.saturating_add(self.config.dedupe_ttl_ms));
            candidates.push(DevelopmentServerUrlCandidate {
                origin,
                session_id: metadata.session_id.clone(),
                process_id: metadata.process_id,
            });
        }
        candidates
    }

    fn consume(&mut self, byte: u8) {
        self.state = match self.state {
            EscapeState::Text if byte == 0x1b => {
                // A control sequence terminates visible text, while its
                // control payload stays invisible to the URL scanner.
                self.push_text(b' ');
                EscapeState::Escape
            }
            EscapeState::Text => {
                self.push_text(byte);
                EscapeState::Text
            }
            EscapeState::Escape if byte == b'[' => EscapeState::Csi,
            EscapeState::Escape if byte == b']' => EscapeState::Osc,
            EscapeState::Escape => EscapeState::Text,
            EscapeState::Csi if (0x40..=0x7e).contains(&byte) => EscapeState::Text,
            EscapeState::Csi => EscapeState::Csi,
            EscapeState::Osc if byte == 0x07 => EscapeState::Text,
            EscapeState::Osc if byte == 0x1b => EscapeState::OscEscape,
            EscapeState::Osc => EscapeState::Osc,
            EscapeState::OscEscape if byte == b'\\' || byte == 0x07 => EscapeState::Text,
            EscapeState::OscEscape if byte == 0x1b => EscapeState::OscEscape,
            EscapeState::OscEscape => EscapeState::Osc,
        };
    }

    fn push_text(&mut self, byte: u8) {
        self.text.push(byte);
        if self.text.len() > self.config.max_buffer_bytes {
            let overflow = self.text.len() - self.config.max_buffer_bytes;
            self.text.drain(..overflow);
        }
    }

    fn prune_expired(&mut self, now_ms: u64) {
        self.recent.retain(|_, expiry| *expiry > now_ms);
    }
}

fn extract_origins(
    bytes: &[u8],
    max_candidate_bytes: usize,
    include_unterminated: bool,
) -> Vec<String> {
    let mut origins = Vec::new();
    let mut index = 0;
    while index < bytes.len() {
        let scheme_len = if bytes[index..].starts_with(b"http://") {
            7
        } else if bytes[index..].starts_with(b"https://") {
            8
        } else {
            index += 1;
            continue;
        };
        let delimiter = bytes[index..]
            .iter()
            .take(max_candidate_bytes)
            .position(|byte| is_url_delimiter(*byte));
        let end = delimiter
            .map(|offset| index + offset)
            .unwrap_or_else(|| (index + max_candidate_bytes).min(bytes.len()));
        if (delimiter.is_some() || include_unterminated) && end > index + scheme_len {
            if let Some(origin) = parse_origin(&bytes[index..end]) {
                if !origins.contains(&origin) {
                    origins.push(origin);
                }
            }
        }
        index = end.max(index + scheme_len);
    }
    origins
}

fn is_url_delimiter(byte: u8) -> bool {
    byte.is_ascii_whitespace()
        || byte.is_ascii_control()
        || matches!(byte, b'\'' | b'"' | b'<' | b'>' | b'`')
}

fn parse_origin(raw: &[u8]) -> Option<String> {
    let text = std::str::from_utf8(raw).ok()?;
    let (scheme, rest) = text
        .strip_prefix("https://")
        .map(|rest| ("https", rest))
        .or_else(|| text.strip_prefix("http://").map(|rest| ("http", rest)))?;
    let authority_end = rest.find(['/', '?', '#']).unwrap_or(rest.len());
    let authority = &rest[..authority_end];
    if authority.is_empty()
        || authority.contains('@')
        || authority.len() > DEFAULT_MAX_CANDIDATE_BYTES
    {
        return None;
    }
    let (host, port) = split_host_port(authority)?;
    let host = normalize_host(host)?;
    let authority = match port {
        Some(port) => format!("{host}:{port}"),
        None => host,
    };
    Some(format!("{scheme}://{authority}"))
}

fn split_host_port(authority: &str) -> Option<(&str, Option<u16>)> {
    if authority.starts_with('[') {
        let close = authority.find(']')?;
        let host = &authority[..=close];
        let rest = &authority[close + 1..];
        return match rest {
            "" => Some((host, None)),
            _ if rest.starts_with(':') => rest[1..].parse().ok().map(|port| (host, Some(port))),
            _ => None,
        };
    }
    let mut parts = authority.rsplitn(2, ':');
    let last = parts.next()?;
    let before = parts.next();
    match before {
        Some(host) if !host.is_empty() && last.bytes().all(|byte| byte.is_ascii_digit()) => {
            last.parse().ok().map(|port| (host, Some(port)))
        }
        Some(_) => None,
        None => Some((last, None)),
    }
}

fn normalize_host(host: &str) -> Option<String> {
    let lower = host.to_ascii_lowercase();
    let normalized = match lower.as_str() {
        "0.0.0.0" => "127.0.0.1".to_string(),
        "::" | "[::]" => "[::1]".to_string(),
        _ => lower,
    };
    let inner = normalized.trim_matches(&['[', ']'][..]);
    if inner.is_empty()
        || inner.len() > 253
        || inner.ends_with('.')
        || !inner
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'-' | b':'))
    {
        return None;
    }
    if !is_local_development_host(inner) {
        return None;
    }
    Some(normalized)
}

fn is_local_development_host(host: &str) -> bool {
    host.eq_ignore_ascii_case("localhost")
        || host
            .parse::<IpAddr>()
            .is_ok_and(|address| address.is_loopback())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn metadata() -> PtyStreamMetadata {
        PtyStreamMetadata {
            session_id: "ses_1".to_string(),
            process_id: Some(42),
        }
    }

    #[test]
    fn detects_split_url_with_split_utf8_prefix_and_normalizes_wildcard() {
        let mut detector = PtyUrlDetector::new(PtyUrlDetectorConfig::default());
        assert!(detector.push(b"prefix \xea", &metadata(), 1).is_empty());
        assert!(detector
            .push(b"\xb8\x80 http://0.0.0.", &metadata(), 2)
            .is_empty());
        let found = detector.push(b"0:5173/path?q=1#section\n", &metadata(), 3);
        assert_eq!(
            found,
            vec![DevelopmentServerUrlCandidate {
                origin: "http://127.0.0.1:5173".to_string(),
                session_id: "ses_1".to_string(),
                process_id: Some(42),
            }]
        );
    }

    #[test]
    fn ignores_urls_inside_split_osc_and_detects_colored_visible_url() {
        let mut detector = PtyUrlDetector::new(PtyUrlDetectorConfig::default());
        assert!(detector
            .push(b"\x1b]8;;https://evil.", &metadata(), 1)
            .is_empty());
        assert!(detector.push(b"example\x1b\\", &metadata(), 2).is_empty());
        assert_eq!(
            detector
                .push(b"\x1b[31mhttps://localhost:3000\x1b[0m", &metadata(), 3)
                .len(),
            1
        );
    }

    #[test]
    fn ignores_ordinary_external_links_in_terminal_output() {
        let mut detector = PtyUrlDetector::new(PtyUrlDetectorConfig::default());
        let found = detector.push(
            b"documentation: https://www.kopus.org and https://example.com/guide\n",
            &metadata(),
            1,
        );
        assert!(found.is_empty());
    }

    #[test]
    fn rejects_malicious_or_unsupported_urls() {
        let mut detector = PtyUrlDetector::new(PtyUrlDetectorConfig::default());
        let found = detector.push(
            b"ftp://localhost:21 https://user@evil.test http://localhost:99999 http://bad:abc ",
            &metadata(),
            1,
        );
        assert!(found.is_empty());
    }

    #[test]
    fn dedupes_candidates_then_allows_ttl_expiry() {
        let mut detector = PtyUrlDetector::new(PtyUrlDetectorConfig {
            max_buffer_bytes: 4096,
            max_candidate_bytes: 256,
            dedupe_ttl_ms: 10,
        });
        assert_eq!(
            detector
                .push(b"ready http://localhost:5173\n", &metadata(), 100)
                .len(),
            1
        );
        assert!(detector
            .push(b"again http://localhost:5173\n", &metadata(), 105)
            .is_empty());
        assert_eq!(
            detector
                .push(b"again http://localhost:5173\n", &metadata(), 111)
                .len(),
            1
        );
    }

    #[test]
    fn strips_paths_queries_fragments_and_normalizes_ipv6_wildcard() {
        let mut detector = PtyUrlDetector::new(PtyUrlDetectorConfig::default());
        let found = detector.push(b"http://[::]:8080/api?token=no#x\n", &metadata(), 1);
        assert_eq!(found[0].origin, "http://[::1]:8080");
    }

    #[test]
    fn flushes_an_unterminated_url_when_a_session_ends() {
        let mut detector = PtyUrlDetector::new(PtyUrlDetectorConfig::default());
        assert!(detector
            .push(b"ready at https://localhost:4173", &metadata(), 1)
            .is_empty());
        assert_eq!(
            detector.finish(&metadata(), 2)[0].origin,
            "https://localhost:4173"
        );
    }
}

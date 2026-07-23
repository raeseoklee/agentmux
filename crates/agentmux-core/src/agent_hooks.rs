//! Additive normalization primitives for trusted agent hook payloads.
//!
//! These types deliberately do not open a listener or mutate runtime state. A
//! host validates the caller/session token, then passes a single JSON payload
//! here before applying the resulting event to its control plane.

use std::collections::{HashMap, HashSet};
use std::fmt;

use serde_json::Value;

const DEFAULT_MAX_PAYLOAD_BYTES: usize = 16 * 1024;
const DEFAULT_DEDUPE_TTL_MS: u64 = 30_000;
const MAX_TEXT_LEN: usize = 1024;

#[derive(Clone, Copy, Debug, Eq, PartialEq, Hash)]
pub enum AgentHookProvider {
    Claude,
    Codex,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Ord, PartialOrd)]
pub enum AgentHookSource {
    HeuristicOutput,
    ExplicitTerminalMarker,
    VerifiedHook,
}

impl AgentHookSource {
    pub const fn precedence(self) -> u8 {
        match self {
            Self::HeuristicOutput => 1,
            Self::ExplicitTerminalMarker => 2,
            Self::VerifiedHook => 3,
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum AgentHookState {
    Started,
    Running,
    WaitingForInput,
    Completed,
    Failed,
    Exited,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct NormalizedAgentHookEvent {
    pub provider: AgentHookProvider,
    pub source: AgentHookSource,
    pub state: AgentHookState,
    pub event_name: String,
    pub session_id: String,
    pub process_id: Option<u32>,
    pub sequence: Option<u64>,
    pub occurred_at_ms: Option<u64>,
    pub reason: Option<String>,
    pub conversation_id: Option<String>,
    pub working_directory: Option<String>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AgentHookNormalizerConfig {
    pub max_payload_bytes: usize,
    pub dedupe_ttl_ms: u64,
}

impl Default for AgentHookNormalizerConfig {
    fn default() -> Self {
        Self {
            max_payload_bytes: DEFAULT_MAX_PAYLOAD_BYTES,
            dedupe_ttl_ms: DEFAULT_DEDUPE_TTL_MS,
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum AgentHookValidationError {
    PayloadTooLarge,
    InvalidJson,
    PayloadMustBeObject,
    MissingField(&'static str),
    InvalidField(&'static str),
    SessionMismatch,
    ProcessMismatch,
    StaleSequence,
}

impl fmt::Display for AgentHookValidationError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::PayloadTooLarge => f.write_str("agent hook payload is too large"),
            Self::InvalidJson => f.write_str("agent hook payload is not valid JSON"),
            Self::PayloadMustBeObject => f.write_str("agent hook payload must be a JSON object"),
            Self::MissingField(field) => write!(f, "agent hook payload is missing '{field}'"),
            Self::InvalidField(field) => write!(f, "agent hook payload has an invalid '{field}'"),
            Self::SessionMismatch => {
                f.write_str("agent hook session does not match the target session")
            }
            Self::ProcessMismatch => {
                f.write_str("agent hook process does not match the target process")
            }
            Self::StaleSequence => f.write_str("agent hook sequence is stale"),
        }
    }
}

impl std::error::Error for AgentHookValidationError {}

/// Normalizes provider payloads and suppresses duplicate/stale events.
///
/// `normalize` returns `Ok(None)` for an idempotent duplicate. Hosts should
/// call it only after authenticating the hook's per-session token.
#[derive(Debug)]
pub struct AgentHookNormalizer {
    config: AgentHookNormalizerConfig,
    latest_sequences: HashMap<(AgentHookProvider, String), u64>,
    recent_events: HashMap<(AgentHookProvider, String, String), u64>,
}

impl AgentHookNormalizer {
    pub fn new(config: AgentHookNormalizerConfig) -> Self {
        Self {
            config,
            latest_sequences: HashMap::new(),
            recent_events: HashMap::new(),
        }
    }

    pub fn normalize(
        &mut self,
        provider: AgentHookProvider,
        payload: &[u8],
        expected_session_id: Option<&str>,
        expected_process_id: Option<u32>,
        now_ms: u64,
    ) -> Result<Option<NormalizedAgentHookEvent>, AgentHookValidationError> {
        self.prune_expired(now_ms);
        if payload.len() > self.config.max_payload_bytes {
            return Err(AgentHookValidationError::PayloadTooLarge);
        }
        let value: Value =
            serde_json::from_slice(payload).map_err(|_| AgentHookValidationError::InvalidJson)?;
        let object = value
            .as_object()
            .ok_or(AgentHookValidationError::PayloadMustBeObject)?;

        let session_id = string_field(object, &["session_id", "sessionId"])?;
        if let Some(expected) = expected_session_id {
            if expected != session_id {
                return Err(AgentHookValidationError::SessionMismatch);
            }
        }
        let process_id = optional_u32(object, &["pid", "process_id", "processId"])?;
        if let Some(expected) = expected_process_id {
            if process_id != Some(expected) {
                return Err(AgentHookValidationError::ProcessMismatch);
            }
        }
        let event_name = string_field(object, event_name_keys(provider))?;
        let state = state_from_payload(provider, event_name, object)?;
        let sequence = optional_u64(object, &["sequence", "seq"])?;
        let sequence_key = (provider, session_id.to_string());
        if let Some(sequence) = sequence {
            if self
                .latest_sequences
                .get(&sequence_key)
                .is_some_and(|latest| sequence < *latest)
            {
                return Err(AgentHookValidationError::StaleSequence);
            }
        }
        let event = NormalizedAgentHookEvent {
            provider,
            source: AgentHookSource::VerifiedHook,
            state,
            event_name: event_name.to_string(),
            session_id: session_id.to_string(),
            process_id,
            sequence,
            occurred_at_ms: optional_u64(object, &["occurred_at_ms", "timestamp_ms", "timestamp"])?,
            reason: optional_string(object, &["reason", "message", "summary"])?,
            conversation_id: optional_string(
                object,
                &["conversation_id", "conversationId", "thread_id", "threadId"],
            )?,
            working_directory: optional_string(
                object,
                &["cwd", "working_directory", "workingDirectory"],
            )?,
        };
        let fingerprint = fingerprint(&event);
        let recent_event_key = (provider, event.session_id.clone(), fingerprint);
        if self.recent_events.contains_key(&recent_event_key) {
            return Ok(None);
        }
        if let Some(sequence) = sequence {
            self.latest_sequences.insert(sequence_key, sequence);
        }
        self.recent_events.insert(
            recent_event_key,
            now_ms.saturating_add(self.config.dedupe_ttl_ms),
        );
        Ok(Some(event))
    }

    fn prune_expired(&mut self, now_ms: u64) {
        self.recent_events.retain(|_, expiry| *expiry > now_ms);
        let retained_sessions: HashSet<_> = self
            .recent_events
            .keys()
            .map(|(provider, session_id, _)| (*provider, session_id.clone()))
            .collect();
        self.latest_sequences
            .retain(|session, _| retained_sessions.contains(session));
    }
}

fn event_name_keys(provider: AgentHookProvider) -> &'static [&'static str] {
    match provider {
        AgentHookProvider::Claude => &["hook_event_name", "event_name", "event", "type"],
        AgentHookProvider::Codex => &["event", "event_name", "type", "hook_event_name"],
    }
}

fn state_from_payload(
    provider: AgentHookProvider,
    event_name: &str,
    object: &serde_json::Map<String, Value>,
) -> Result<AgentHookState, AgentHookValidationError> {
    if let Some(state) = optional_string(object, &["state", "status"])? {
        return parse_state(&state).ok_or(AgentHookValidationError::InvalidField("state"));
    }
    let normalized = event_name.to_ascii_lowercase();
    let state = match provider {
        AgentHookProvider::Claude
            if matches!(normalized.as_str(), "sessionstart" | "userpromptsubmit") =>
        {
            AgentHookState::Started
        }
        AgentHookProvider::Claude
            if matches!(normalized.as_str(), "pretooluse" | "posttooluse") =>
        {
            AgentHookState::Running
        }
        AgentHookProvider::Claude
            if matches!(normalized.as_str(), "notification" | "permissionrequest") =>
        {
            AgentHookState::WaitingForInput
        }
        AgentHookProvider::Claude if normalized == "stop" => AgentHookState::Completed,
        AgentHookProvider::Claude if normalized == "sessionend" => AgentHookState::Exited,
        AgentHookProvider::Codex
            if normalized.contains("waiting")
                || normalized.contains("input")
                || normalized.contains("approval") =>
        {
            AgentHookState::WaitingForInput
        }
        AgentHookProvider::Codex
            if normalized.contains("complete") || normalized.contains("finish") =>
        {
            AgentHookState::Completed
        }
        AgentHookProvider::Codex if normalized.contains("fail") || normalized.contains("error") => {
            AgentHookState::Failed
        }
        AgentHookProvider::Codex if normalized.contains("exit") || normalized.contains("end") => {
            AgentHookState::Exited
        }
        AgentHookProvider::Codex if normalized.contains("start") || normalized.contains("run") => {
            AgentHookState::Running
        }
        _ => return Err(AgentHookValidationError::InvalidField("event")),
    };
    Ok(state)
}

fn parse_state(value: &str) -> Option<AgentHookState> {
    match value.trim().to_ascii_lowercase().as_str() {
        "started" | "start" => Some(AgentHookState::Started),
        "running" | "active" => Some(AgentHookState::Running),
        "waiting_for_input" | "waiting" | "needs_input" | "approval_required" => {
            Some(AgentHookState::WaitingForInput)
        }
        "completed" | "complete" | "finished" | "stop" => Some(AgentHookState::Completed),
        "failed" | "error" => Some(AgentHookState::Failed),
        "exited" | "exit" | "ended" => Some(AgentHookState::Exited),
        _ => None,
    }
}

fn string_field<'a>(
    object: &'a serde_json::Map<String, Value>,
    keys: &[&'static str],
) -> Result<&'a str, AgentHookValidationError> {
    let value = keys.iter().find_map(|key| object.get(*key));
    let Some(value) = value else {
        return Err(AgentHookValidationError::MissingField(keys[0]));
    };
    let Some(value) = value.as_str() else {
        return Err(AgentHookValidationError::InvalidField(keys[0]));
    };
    if value.trim().is_empty() || value.len() > MAX_TEXT_LEN || value.contains('\0') {
        return Err(AgentHookValidationError::InvalidField(keys[0]));
    }
    Ok(value)
}

fn optional_string(
    object: &serde_json::Map<String, Value>,
    keys: &[&'static str],
) -> Result<Option<String>, AgentHookValidationError> {
    let Some(value) = keys.iter().find_map(|key| object.get(*key)) else {
        return Ok(None);
    };
    let Some(value) = value.as_str() else {
        return Err(AgentHookValidationError::InvalidField(keys[0]));
    };
    if value.len() > MAX_TEXT_LEN || value.contains('\0') {
        return Err(AgentHookValidationError::InvalidField(keys[0]));
    }
    Ok((!value.trim().is_empty()).then(|| value.to_string()))
}

fn optional_u64(
    object: &serde_json::Map<String, Value>,
    keys: &[&'static str],
) -> Result<Option<u64>, AgentHookValidationError> {
    let Some(value) = keys.iter().find_map(|key| object.get(*key)) else {
        return Ok(None);
    };
    value
        .as_u64()
        .map(Some)
        .ok_or(AgentHookValidationError::InvalidField(keys[0]))
}

fn optional_u32(
    object: &serde_json::Map<String, Value>,
    keys: &[&'static str],
) -> Result<Option<u32>, AgentHookValidationError> {
    let Some(value) = optional_u64(object, keys)? else {
        return Ok(None);
    };
    u32::try_from(value)
        .map(Some)
        .map_err(|_| AgentHookValidationError::InvalidField(keys[0]))
}

fn fingerprint(event: &NormalizedAgentHookEvent) -> String {
    format!(
        "{:?}|{}|{:?}|{}|{:?}|{:?}",
        event.provider,
        event.session_id,
        event.state,
        event.event_name,
        event.sequence,
        event.reason
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalizes_representative_claude_payload() {
        let mut normalizer = AgentHookNormalizer::new(AgentHookNormalizerConfig::default());
        let event = normalizer
            .normalize(
                AgentHookProvider::Claude,
                br#"{"hook_event_name":"Notification","session_id":"claude-1","pid":41,"sequence":7,"message":"permission needed","cwd":"D:/repo"}"#,
                Some("claude-1"),
                Some(41),
                10,
            )
            .unwrap()
            .unwrap();
        assert_eq!(event.state, AgentHookState::WaitingForInput);
        assert_eq!(
            event.source.precedence(),
            AgentHookSource::VerifiedHook.precedence()
        );
        assert_eq!(event.reason.as_deref(), Some("permission needed"));
    }

    #[test]
    fn normalizes_representative_codex_payload_and_rejects_stale_sequence() {
        let mut normalizer = AgentHookNormalizer::new(AgentHookNormalizerConfig::default());
        let payload = br#"{"event":"agent.completed","session_id":"codex-1","process_id":99,"seq":2,"thread_id":"thread-7"}"#;
        let event = normalizer
            .normalize(
                AgentHookProvider::Codex,
                payload,
                Some("codex-1"),
                Some(99),
                10,
            )
            .unwrap()
            .unwrap();
        assert_eq!(event.state, AgentHookState::Completed);
        assert_eq!(event.conversation_id.as_deref(), Some("thread-7"));
        let stale = normalizer.normalize(
            AgentHookProvider::Codex,
            br#"{"event":"agent.running","session_id":"codex-1","process_id":99,"seq":1}"#,
            Some("codex-1"),
            Some(99),
            11,
        );
        assert_eq!(stale, Err(AgentHookValidationError::StaleSequence));
    }

    #[test]
    fn dedupes_then_allows_expired_events() {
        let mut normalizer = AgentHookNormalizer::new(AgentHookNormalizerConfig {
            max_payload_bytes: 1024,
            dedupe_ttl_ms: 10,
        });
        let payload = br#"{"event":"agent.running","session_id":"codex-1"}"#;
        assert!(normalizer
            .normalize(AgentHookProvider::Codex, payload, None, None, 100)
            .unwrap()
            .is_some());
        assert!(normalizer
            .normalize(AgentHookProvider::Codex, payload, None, None, 105)
            .unwrap()
            .is_none());
        assert!(normalizer
            .normalize(AgentHookProvider::Codex, payload, None, None, 111)
            .unwrap()
            .is_some());
    }

    #[test]
    fn prunes_expired_sequence_tracking_for_claude_and_codex_sessions() {
        let mut normalizer = AgentHookNormalizer::new(AgentHookNormalizerConfig {
            max_payload_bytes: 1024,
            dedupe_ttl_ms: 10,
        });
        for (provider, session_id, payload) in [
            (
                AgentHookProvider::Claude,
                "claude-sequence",
                br#"{"hook_event_name":"PreToolUse","session_id":"claude-sequence","sequence":7}"#
                    as &[u8],
            ),
            (
                AgentHookProvider::Codex,
                "codex-sequence",
                br#"{"event":"agent.running","session_id":"codex-sequence","seq":7}"#,
            ),
        ] {
            assert!(normalizer
                .normalize(provider, payload, Some(session_id), None, 100)
                .unwrap()
                .is_some());
        }
        assert_eq!(normalizer.latest_sequences.len(), 2);

        assert!(normalizer
            .normalize(
                AgentHookProvider::Codex,
                br#"{"event":"agent.running","session_id":"cleanup"}"#,
                None,
                None,
                111,
            )
            .unwrap()
            .is_some());
        assert!(normalizer.latest_sequences.is_empty());
    }

    #[test]
    fn retained_unsequenced_event_preserves_sequence_ordering() {
        let mut normalizer = AgentHookNormalizer::new(AgentHookNormalizerConfig {
            max_payload_bytes: 1024,
            dedupe_ttl_ms: 10,
        });
        assert!(normalizer
            .normalize(
                AgentHookProvider::Claude,
                br#"{"hook_event_name":"PreToolUse","session_id":"claude-1","sequence":5}"#,
                None,
                None,
                100,
            )
            .unwrap()
            .is_some());
        assert!(normalizer
            .normalize(
                AgentHookProvider::Claude,
                br#"{"hook_event_name":"PostToolUse","session_id":"claude-1"}"#,
                None,
                None,
                105,
            )
            .unwrap()
            .is_some());

        assert_eq!(
            normalizer.normalize(
                AgentHookProvider::Claude,
                br#"{"hook_event_name":"Stop","session_id":"claude-1","sequence":4}"#,
                None,
                None,
                111,
            ),
            Err(AgentHookValidationError::StaleSequence)
        );
        assert!(normalizer
            .normalize(
                AgentHookProvider::Claude,
                br#"{"hook_event_name":"Stop","session_id":"claude-1","sequence":4}"#,
                None,
                None,
                116,
            )
            .unwrap()
            .is_some());
    }

    #[test]
    fn validates_session_process_and_payload_shape() {
        let mut normalizer = AgentHookNormalizer::new(AgentHookNormalizerConfig::default());
        assert_eq!(
            normalizer.normalize(AgentHookProvider::Claude, b"[]", None, None, 0),
            Err(AgentHookValidationError::PayloadMustBeObject)
        );
        assert_eq!(
            normalizer.normalize(
                AgentHookProvider::Claude,
                br#"{"hook_event_name":"Stop","session_id":"other"}"#,
                Some("wanted"),
                None,
                0
            ),
            Err(AgentHookValidationError::SessionMismatch)
        );
        assert_eq!(
            normalizer.normalize(
                AgentHookProvider::Claude,
                br#"{"hook_event_name":"Stop","session_id":"wanted","pid":2}"#,
                Some("wanted"),
                Some(1),
                0
            ),
            Err(AgentHookValidationError::ProcessMismatch)
        );
    }
}

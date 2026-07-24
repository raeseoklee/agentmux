import {
  type CSSProperties,
  type MouseEvent,
  useCallback,
  useEffect,
  useMemo,
  useRef,
  useState,
} from "react";
import ReactMarkdown from "react-markdown";
import rehypeSanitize from "rehype-sanitize";
import remarkBreaks from "remark-breaks";
import remarkGfm from "remark-gfm";
import type { ControlClient, MarkdownDocument, SurfaceSummary } from "../control/ControlClient";
import { IconArrowLeft, IconArrowRight, IconFileText, IconReload, IconSearch } from "./icons";
import type { Translator } from "./i18n";

const FONT_SANS =
  "'Pretendard Variable',Pretendard,-apple-system,'Segoe UI','Malgun Gothic',system-ui,sans-serif";

function stripResourceSuffix(value: string): string {
  return value.split(/[?#]/, 1)[0] ?? value;
}

export function isMarkdownDocumentLink(value: string): boolean {
  return /\.(?:md|markdown|mdown|mkd)(?:[?#].*)?$/i.test(value.trim());
}

export function resolveRelativeDocumentPath(documentPath: string, href: string): string {
  const raw = stripResourceSuffix(href.trim());
  if (!raw) return documentPath;
  let decoded = raw;
  try {
    decoded = decodeURIComponent(raw);
  } catch {
    decoded = raw;
  }
  if (/^[A-Za-z]:[\\/]/.test(decoded) || decoded.startsWith("\\\\") || decoded.startsWith("/")) {
    return decoded;
  }
  const separator = documentPath.includes("\\") ? "\\" : "/";
  const base = documentPath.replace(/[\\/][^\\/]*$/, "");
  const drive = /^[A-Za-z]:/.exec(base)?.[0] ?? "";
  const unc = !drive && base.startsWith("\\\\");
  const posix = !drive && !unc && base.startsWith("/");
  const baseWithoutRoot = drive
    ? base.slice(drive.length)
    : unc
      ? base.slice(2)
      : posix
        ? base.slice(1)
        : base;
  const pieces = `${baseWithoutRoot}${separator}${decoded}`.split(/[\\/]+/);
  const normalized: string[] = [];
  for (const piece of pieces) {
    if (!piece || piece === ".") continue;
    if (piece === "..") normalized.pop();
    else normalized.push(piece);
  }
  if (drive) return `${drive}${separator}${normalized.join(separator)}`;
  if (unc) return `\\\\${normalized.join(separator)}`;
  if (posix) return `/${normalized.join("/")}`;
  return normalized.join(separator);
}

function countMatches(content: string, query: string): number {
  const needle = query.trim().toLocaleLowerCase();
  if (!needle) return 0;
  let count = 0;
  let offset = 0;
  const haystack = content.toLocaleLowerCase();
  while ((offset = haystack.indexOf(needle, offset)) >= 0) {
    count += 1;
    offset += Math.max(needle.length, 1);
  }
  return count;
}

function selectRenderedMatch(root: HTMLElement, query: string, index: number): boolean {
  const needle = query.trim().toLocaleLowerCase();
  if (!needle) return false;
  const walker = document.createTreeWalker(root, NodeFilter.SHOW_TEXT);
  let seen = 0;
  let node = walker.nextNode();
  while (node) {
    const text = node.textContent ?? "";
    let offset = 0;
    const lower = text.toLocaleLowerCase();
    while ((offset = lower.indexOf(needle, offset)) >= 0) {
      if (seen === index) {
        const range = document.createRange();
        range.setStart(node, offset);
        range.setEnd(node, offset + needle.length);
        const selection = window.getSelection();
        selection?.removeAllRanges();
        selection?.addRange(range);
        (node.parentElement ?? root).scrollIntoView({ block: "center" });
        return true;
      }
      seen += 1;
      offset += Math.max(needle.length, 1);
    }
    node = walker.nextNode();
  }
  return false;
}

function MarkdownImage({
  client,
  workspaceId,
  surfaceId,
  src,
  alt,
}: {
  client: ControlClient;
  workspaceId: string;
  surfaceId: string;
  src?: string;
  alt?: string;
}) {
  const [resolved, setResolved] = useState<string | null>(null);
  const remote = Boolean(src && /^(?:https?:|data:)/i.test(src));
  useEffect(() => {
    let cancelled = false;
    if (!src) {
      setResolved(null);
      return;
    }
    if (remote) {
      // Do not let a local artifact silently beacon to remote image hosts or
      // embed arbitrary data URLs. Workspace assets must pass through the
      // host-side path and MIME validation below.
      setResolved(null);
      return;
    }
    if (!client.readMarkdownAsset) {
      setResolved(null);
      return;
    }
    void client
      .readMarkdownAsset(workspaceId, surfaceId, src)
      .then((value) => {
        if (!cancelled) setResolved(value);
      })
      .catch(() => {
        if (!cancelled) setResolved(null);
      });
    return () => {
      cancelled = true;
    };
  }, [client, remote, src, surfaceId, workspaceId]);

  return resolved ? <img src={resolved} alt={alt ?? ""} loading="lazy" /> : <span>{alt ?? src}</span>;
}

export function MarkdownSurfacePanel({
  client,
  surface,
  visible,
  t,
  onOpenDocument,
  onOpenExternal,
}: {
  client: ControlClient;
  surface: SurfaceSummary;
  visible: boolean;
  t: Translator;
  onOpenDocument: (path: string) => void;
  onOpenExternal: (url: string) => void;
}) {
  const [documentState, setDocumentState] = useState<MarkdownDocument | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [loading, setLoading] = useState(true);
  const [query, setQuery] = useState("");
  const [matchIndex, setMatchIndex] = useState(0);
  const articleRef = useRef<HTMLElement | null>(null);
  const requestRef = useRef(0);
  const scrollRef = useRef<HTMLDivElement | null>(null);

  const load = useCallback(async (quiet = false) => {
    if (!client.readMarkdown) {
      setError(t("markdown.unavailable"));
      setLoading(false);
      return;
    }
    const request = ++requestRef.current;
    if (!quiet) setLoading(true);
    try {
      const next = await client.readMarkdown(surface.workspaceId, surface.surfaceId);
      if (request !== requestRef.current) return;
      setDocumentState((current) =>
        current?.modifiedAtMs === next.modifiedAtMs && current.content === next.content ? current : next,
      );
      setError(null);
    } catch (cause) {
      if (request !== requestRef.current) return;
      setError(cause instanceof Error ? cause.message : t("markdown.loadFailed"));
    } finally {
      if (request === requestRef.current) setLoading(false);
    }
  }, [client, surface.surfaceId, surface.workspaceId, t]);

  useEffect(() => {
    void load();
  }, [load]);

  useEffect(() => {
    if (!visible) return;
    const timer = window.setInterval(() => void load(true), 2000);
    return () => window.clearInterval(timer);
  }, [load, visible]);

  useEffect(() => {
    const viewport = scrollRef.current;
    const key = `agentmux.markdown.scroll.${surface.surfaceId}`;
    if (!viewport) return;
    const stored = Number.parseFloat(sessionStorage.getItem(key) ?? "0");
    if (Number.isFinite(stored)) viewport.scrollTop = stored;
    const save = () => sessionStorage.setItem(key, String(viewport.scrollTop));
    viewport.addEventListener("scroll", save, { passive: true });
    return () => viewport.removeEventListener("scroll", save);
  }, [surface.surfaceId]);

  const matchCount = useMemo(
    () => countMatches(documentState?.content ?? "", query),
    [documentState?.content, query],
  );
  useEffect(() => {
    setMatchIndex(0);
  }, [query]);

  const moveMatch = (direction: number) => {
    if (!matchCount || !articleRef.current) return;
    const next = (matchIndex + direction + matchCount) % matchCount;
    setMatchIndex(next);
    selectRenderedMatch(articleRef.current, query, next);
  };

  const handleLink = (event: MouseEvent<HTMLAnchorElement>, href?: string) => {
    if (!href) return;
    if (href.startsWith("#")) return;
    event.preventDefault();
    if (/^https?:\/\//i.test(href)) {
      onOpenExternal(href);
      return;
    }
    if (isMarkdownDocumentLink(href) && documentState) {
      onOpenDocument(resolveRelativeDocumentPath(documentState.path, href));
    }
  };

  const iconButton: CSSProperties = {
    width: 28,
    height: 28,
    border: 0,
    borderRadius: 6,
    background: "transparent",
    color: "var(--fg3)",
    display: "inline-flex",
    alignItems: "center",
    justifyContent: "center",
    cursor: "pointer",
  };

  return (
    <div className="agentmux-markdown-surface" data-agentmux-markdown-surface={surface.surfaceId}>
      <div className="agentmux-markdown-toolbar">
        <IconFileText size={14} />
        <div className="agentmux-markdown-location" title={documentState?.path ?? surface.resourceUri ?? ""}>
          <strong>{documentState?.title ?? surface.title}</strong>
          <span>{documentState?.path ?? surface.resourceUri}</span>
        </div>
        <label className="agentmux-markdown-search">
          <IconSearch size={13} />
          <input
            value={query}
            onChange={(event) => setQuery(event.currentTarget.value)}
            placeholder={t("markdown.search")}
            aria-label={t("markdown.search")}
          />
          <span>{query ? `${matchCount ? matchIndex + 1 : 0}/${matchCount}` : ""}</span>
        </label>
        <button type="button" style={iconButton} title={t("markdown.previousMatch")} onClick={() => moveMatch(-1)}>
          <IconArrowLeft size={13} />
        </button>
        <button type="button" style={iconButton} title={t("markdown.nextMatch")} onClick={() => moveMatch(1)}>
          <IconArrowRight size={13} />
        </button>
        <button type="button" style={iconButton} title={t("markdown.reload")} onClick={() => void load()}>
          <IconReload size={13} />
        </button>
      </div>
      <div ref={scrollRef} className="agentmux-markdown-scroll">
        {loading && !documentState ? <div className="agentmux-markdown-state">{t("markdown.loading")}</div> : null}
        {error ? <div className="agentmux-markdown-error" role="alert">{error}</div> : null}
        {documentState ? (
          <article ref={articleRef} className="agentmux-markdown-article">
            <ReactMarkdown
              remarkPlugins={[remarkGfm, remarkBreaks]}
              rehypePlugins={[rehypeSanitize]}
              components={{
                a: ({ href, children, ...props }) => (
                  <a {...props} href={href} onClick={(event) => handleLink(event, href)}>{children}</a>
                ),
                img: ({ src, alt }) => (
                  <MarkdownImage
                    client={client}
                    workspaceId={surface.workspaceId}
                    surfaceId={surface.surfaceId}
                    src={src}
                    alt={alt}
                  />
                ),
              }}
            >
              {documentState.content}
            </ReactMarkdown>
          </article>
        ) : null}
      </div>
    </div>
  );
}

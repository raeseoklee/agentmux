import {
  type CSSProperties,
  useCallback,
  useEffect,
  useMemo,
  useRef,
  useState,
} from "react";
import type { ControlClient } from "../control/ControlClient";
import { IconArrowLeft, IconArrowRight, IconGlobe, IconReload } from "./icons";
import type { Translator } from "./i18n";

const FONT_SANS =
  "'Pretendard Variable',Pretendard,-apple-system,'Segoe UI','Malgun Gothic',system-ui,sans-serif";
const FONT_MONO =
  "'Cascadia Mono','JetBrains Mono','D2Coding','Consolas',monospace";
const MAX_PREVIEW_CHARS = 12_000;

const buttonStyle: CSSProperties = {
  alignItems: "center",
  background: "var(--s2)",
  border: "1px solid var(--border)",
  borderRadius: 6,
  color: "var(--fg1)",
  cursor: "pointer",
  display: "inline-flex",
  fontFamily: FONT_SANS,
  fontSize: 12,
  justifyContent: "center",
  minHeight: 30,
  padding: "5px 9px",
};

const inputStyle: CSSProperties = {
  background: "var(--canvas)",
  border: "1px solid var(--border)",
  borderRadius: 6,
  color: "var(--fg1)",
  fontFamily: FONT_MONO,
  fontSize: 12,
  minHeight: 30,
  outline: "none",
  padding: "5px 9px",
};

type BrowserLoadState = "idle" | "loading" | "ready" | "error";

type BrowserPageFrame = {
  byteCount: number;
  dataUrl: string;
};

export type BrowserPagePreview = {
  title: string | null;
  text: string;
  truncated: boolean;
};

function decodeHtmlEntities(value: string): string {
  const named: Record<string, string> = {
    amp: "&",
    apos: "'",
    gt: ">",
    lt: "<",
    nbsp: " ",
    quot: '"',
  };
  return value.replace(/&(#x[0-9a-f]+|#\d+|[a-z]+);/gi, (entity, token: string) => {
    const isValidCodePoint = (codePoint: number) =>
      Number.isFinite(codePoint) && codePoint >= 0 && codePoint <= 0x10ffff;
    const normalized = token.toLowerCase();
    if (normalized.startsWith("#x")) {
      const codePoint = Number.parseInt(normalized.slice(2), 16);
      return isValidCodePoint(codePoint) ? String.fromCodePoint(codePoint) : entity;
    }
    if (normalized.startsWith("#")) {
      const codePoint = Number.parseInt(normalized.slice(1), 10);
      return isValidCodePoint(codePoint) ? String.fromCodePoint(codePoint) : entity;
    }
    return named[normalized] ?? entity;
  });
}

function htmlToText(html: string): string {
  const documentBody = html.match(/<body\b[^>]*>([\s\S]*?)<\/body\s*>/i)?.[1] ?? html;
  return decodeHtmlEntities(
    documentBody
      .replace(/<!--[^]*?-->/g, "")
      .replace(/<(script|style|noscript|template)\b[^>]*>[\s\S]*?<\/\1\s*>/gi, "")
      .replace(/<\/?(article|aside|blockquote|br|caption|div|dt|dd|figcaption|figure|h[1-6]|header|footer|li|main|nav|p|pre|section|table|tr)\b[^>]*>/gi, "\n")
      .replace(/<[^>]+>/g, " ")
      .replace(/[ \t]+\n/g, "\n")
      .replace(/\n[ \t]+/g, "\n")
      .replace(/[ \t]{2,}/g, " ")
      .replace(/\n{2,}/g, "\n")
      .trim(),
  );
}

export function summarizeBrowserDom(html: string): BrowserPagePreview {
  const titleMatch = html.match(/<title\b[^>]*>([\s\S]*?)<\/title\s*>/i);
  const title = titleMatch ? htmlToText(titleMatch[1]) || null : null;
  const fullText = htmlToText(html);
  return {
    title,
    text: fullText.slice(0, MAX_PREVIEW_CHARS),
    truncated: fullText.length > MAX_PREVIEW_CHARS,
  };
}

export function normalizeBrowserNavigationUrl(value: string): string | null {
  const trimmed = value.trim();
  if (!trimmed) {
    return null;
  }
  if (/^(?:localhost|\d{1,3}(?:\.\d{1,3}){3})(?::\d+)?(?:[/?#]|$)/i.test(trimmed)) {
    return `http://${trimmed}`;
  }
  if (/^[a-z][a-z\d+.-]*:/i.test(trimmed)) {
    return trimmed;
  }
  return `https://${trimmed}`;
}

function isBlankUrl(value: string | null): boolean {
  return !value || value === "about:blank";
}

export function BrowserSurfacePanel({
  client,
  surfaceId,
  t,
}: {
  client: ControlClient;
  surfaceId: string;
  t: Translator;
}) {
  const [address, setAddress] = useState("");
  const [currentUrl, setCurrentUrl] = useState<string | null>(null);
  const [loadState, setLoadState] = useState<BrowserLoadState>("idle");
  const [preview, setPreview] = useState<BrowserPagePreview | null>(null);
  const [pageFrame, setPageFrame] = useState<BrowserPageFrame | null>(null);
  const [previewError, setPreviewError] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);
  const frameElementRef = useRef<HTMLDivElement | null>(null);
  const frameImageRef = useRef<HTMLImageElement | null>(null);
  const refreshTimerRef = useRef<number | null>(null);

  const loadPageFrame = useCallback(
    async (url: string, failOnError = false) => {
      if (isBlankUrl(url)) {
        setPreview(null);
        setPageFrame(null);
        setPreviewError(null);
        return;
      }
      try {
        const [snapshot, screenshot] = await Promise.all([
          client.browserDomSnapshot(surfaceId),
          client.browserScreenshot(surfaceId, "png"),
        ]);
        setPreview(summarizeBrowserDom(snapshot.html));
        setPageFrame({
          byteCount: screenshot.byteCount,
          dataUrl: `data:image/png;base64,${screenshot.dataBase64}`,
        });
        setPreviewError(null);
      } catch (cause) {
        const message =
          cause instanceof Error ? cause.message : t("browser.surface.renderFailed");
        setPreview(null);
        setPageFrame(null);
        setPreviewError(message);
        if (failOnError) {
          throw cause instanceof Error ? cause : new Error(message);
        }
      }
    },
    [client, surfaceId, t],
  );

  const applyNavigationResult = useCallback(
    async (url: string) => {
      setCurrentUrl(url);
      setAddress(url);
      await loadPageFrame(url, true);
    },
    [loadPageFrame],
  );

  const scheduleFrameRefresh = useCallback(() => {
    if (refreshTimerRef.current !== null) {
      window.clearTimeout(refreshTimerRef.current);
    }
    refreshTimerRef.current = window.setTimeout(() => {
      refreshTimerRef.current = null;
      if (currentUrl) {
        void loadPageFrame(currentUrl);
      }
    }, 140);
  }, [currentUrl, loadPageFrame]);

  const refreshCurrentUrl = useCallback(async () => {
    setLoadState("loading");
    try {
      const result = await client.browserCurrentUrl(surfaceId);
      await applyNavigationResult(result.url);
      setError(null);
      setLoadState("ready");
    } catch (cause) {
      setError(cause instanceof Error ? cause.message : t("browser.surface.connectionError"));
      setLoadState("error");
    }
  }, [applyNavigationResult, client, surfaceId, t]);

  useEffect(() => {
    void refreshCurrentUrl();
  }, [refreshCurrentUrl]);

  useEffect(
    () => () => {
      if (refreshTimerRef.current !== null) {
        window.clearTimeout(refreshTimerRef.current);
      }
    },
    [],
  );

  const navigate = useCallback(async () => {
    const url = normalizeBrowserNavigationUrl(address);
    if (!url) {
      setError(t("browser.surface.enterAddress"));
      setLoadState("error");
      return;
    }
    setLoadState("loading");
    setError(null);
    try {
      const result = await client.browserNavigate(surfaceId, url);
      await applyNavigationResult(result.url);
      setLoadState("ready");
    } catch (cause) {
      setError(cause instanceof Error ? cause.message : t("browser.surface.navigationFailed"));
      setLoadState("error");
    }
  }, [address, applyNavigationResult, client, surfaceId, t]);

  const runNavigationAction = useCallback(
    async (action: "back" | "forward" | "reload") => {
      setLoadState("loading");
      setError(null);
      try {
        const result = await (
          action === "back"
            ? client.browserBack(surfaceId)
            : action === "forward"
              ? client.browserForward(surfaceId)
              : client.browserReload(surfaceId)
        );
        await applyNavigationResult(result.url);
        setLoadState("ready");
      } catch (cause) {
        setError(cause instanceof Error ? cause.message : t("browser.surface.connectionError"));
        setLoadState("error");
      }
    },
    [applyNavigationResult, client, surfaceId, t],
  );

  const statusLabel = useMemo(() => {
    if (loadState === "loading") {
      return t("browser.surface.loading");
    }
    if (loadState === "error") {
      return t("browser.surface.connectionError");
    }
    if (isBlankUrl(currentUrl)) {
      return t("browser.surface.newTab");
    }
    return t("browser.surface.pageReady");
  }, [currentUrl, loadState, t]);

  const controlsDisabled = loadState === "loading";

  const handleFrameClick = useCallback(
    async (event: React.MouseEvent<HTMLImageElement>) => {
      const image = frameImageRef.current;
      if (!image || image.naturalWidth <= 0 || image.naturalHeight <= 0) {
        return;
      }
      const bounds = image.getBoundingClientRect();
      const x = ((event.clientX - bounds.left) / bounds.width) * image.naturalWidth;
      const y = ((event.clientY - bounds.top) / bounds.height) * image.naturalHeight;
      frameElementRef.current?.focus();
      try {
        await client.browserClick(surfaceId, { x, y });
        setError(null);
        scheduleFrameRefresh();
      } catch (cause) {
        setError(cause instanceof Error ? cause.message : t("browser.surface.clickFailed"));
      }
    },
    [client, scheduleFrameRefresh, surfaceId, t],
  );

  const handleFrameWheel = useCallback(
    (event: React.WheelEvent<HTMLDivElement>) => {
      event.preventDefault();
      void client
        .browserScroll(surfaceId, { x: event.deltaX, y: event.deltaY })
        .then(() => {
          setError(null);
          scheduleFrameRefresh();
        })
        .catch((cause: unknown) => {
          setError(cause instanceof Error ? cause.message : t("browser.surface.scrollFailed"));
        });
    },
    [client, scheduleFrameRefresh, surfaceId, t],
  );

  const handleFrameKeyDown = useCallback(
    (event: React.KeyboardEvent<HTMLDivElement>) => {
      if (event.ctrlKey || event.metaKey || event.altKey) {
        return;
      }
      const supportedKey = event.key.length === 1 || [
        "Backspace",
        "Delete",
        "Enter",
        "Escape",
        "Tab",
        "ArrowDown",
        "ArrowLeft",
        "ArrowRight",
        "ArrowUp",
        "Home",
        "End",
        "PageDown",
        "PageUp",
        "Space",
      ].includes(event.key);
      if (!supportedKey) {
        return;
      }
      event.preventDefault();
      void client
        .browserPress(surfaceId, ":focus", event.key)
        .then(() => {
          setError(null);
          scheduleFrameRefresh();
        })
        .catch((cause: unknown) => {
          setError(cause instanceof Error ? cause.message : t("browser.surface.inputFailed"));
        });
    },
    [client, scheduleFrameRefresh, surfaceId, t],
  );

  return (
    <section
      aria-label="Browser surface"
      style={{
        background: "var(--term)",
        color: "var(--fg2)",
        display: "flex",
        flexDirection: "column",
        fontFamily: FONT_SANS,
        fontSize: 12,
        height: "100%",
        minHeight: 0,
      }}
    >
      <header
        style={{
          alignItems: "center",
          borderBottom: "1px solid var(--border)",
          display: "flex",
          flexWrap: "wrap",
          gap: 6,
          padding: "8px 10px",
        }}
      >
        <span
          aria-hidden="true"
          className="agentmux-browser-surface-icon"
          style={{ color: "var(--fg3)", display: "inline-flex", flex: "none" }}
        >
          <IconGlobe size={16} />
        </span>
        <button
          aria-label={t("browser.surface.back")}
          disabled={controlsDisabled}
          onClick={() => void runNavigationAction("back")}
          style={{ ...buttonStyle, padding: 0, width: 30 }}
          title={t("browser.surface.back")}
          type="button"
        >
          <IconArrowLeft size={15} />
        </button>
        <button
          aria-label={t("browser.surface.forward")}
          disabled={controlsDisabled}
          onClick={() => void runNavigationAction("forward")}
          style={{ ...buttonStyle, padding: 0, width: 30 }}
          title={t("browser.surface.forward")}
          type="button"
        >
          <IconArrowRight size={15} />
        </button>
        <button
          aria-label={t("browser.surface.reload")}
          disabled={controlsDisabled}
          onClick={() => void runNavigationAction("reload")}
          style={{ ...buttonStyle, padding: 0, width: 30 }}
          title={t("browser.surface.reload")}
          type="button"
        >
          <IconReload size={14} />
        </button>
        <form
          onSubmit={(event) => {
            event.preventDefault();
            void navigate();
          }}
          style={{ display: "flex", flex: "1 1 260px", gap: 6, minWidth: 180 }}
        >
          <label htmlFor={`browser-address-${surfaceId}`} style={{ display: "contents" }}>
            <span style={{ position: "absolute", width: 1, height: 1, overflow: "hidden", clipPath: "inset(50%)" }}>
              {t("browser.surface.address")}
            </span>
            <input
              aria-label={t("browser.surface.address")}
              autoCapitalize="none"
              autoCorrect="off"
              disabled={controlsDisabled}
              id={`browser-address-${surfaceId}`}
              onChange={(event) => setAddress(event.target.value)}
              placeholder="https://example.com"
              spellCheck={false}
              style={{ ...inputStyle, flex: 1, minWidth: 0 }}
              value={address}
            />
          </label>
          <button disabled={controlsDisabled} style={buttonStyle} type="submit">
            {t("browser.surface.go")}
          </button>
        </form>
      </header>

      <div
        aria-live="polite"
        style={{
          borderBottom: "1px solid var(--border)",
          color: loadState === "error" ? "var(--red, #F87171)" : "var(--fg3)",
          display: "flex",
          gap: 8,
          minHeight: 30,
          padding: "7px 12px",
        }}
      >
        <strong style={{ color: "var(--fg1)", fontWeight: 600 }}>{statusLabel}</strong>
        <span style={{ overflow: "hidden", textOverflow: "ellipsis", whiteSpace: "nowrap" }}>
          {currentUrl ?? t("browser.surface.connecting")}
        </span>
      </div>

      <main
        className="agentmux-scroll"
        style={{
          display: "flex",
          flex: 1,
          flexDirection: "column",
          gap: 12,
          minHeight: 0,
          overflow: "auto",
          padding: "16px 18px",
        }}
      >
        {error && (
          <div
            role="alert"
            style={{
              background: "color-mix(in srgb, var(--red, #F87171) 12%, var(--s1))",
              border: "1px solid color-mix(in srgb, var(--red, #F87171) 45%, var(--border))",
              borderRadius: 6,
              color: "var(--fg1)",
              lineHeight: 1.45,
              padding: "10px 12px",
              wordBreak: "break-word",
            }}
          >
            {error}
          </div>
        )}

        {pageFrame ? (
          <div
            aria-label="Interactive page preview"
            onKeyDown={handleFrameKeyDown}
            onWheel={handleFrameWheel}
            ref={frameElementRef}
            style={{
              alignItems: "flex-start",
              background: "#fff",
              border: "1px solid var(--border)",
              borderRadius: 4,
              display: "flex",
              flex: "1 1 auto",
              justifyContent: "center",
              minHeight: 180,
              minWidth: 0,
              overflow: "auto",
              outline: "none",
            }}
            tabIndex={0}
            title={`${pageFrame.byteCount.toLocaleString()} byte page frame`}
          >
            <img
              alt={preview?.title ?? currentUrl ?? "Browser page"}
              draggable={false}
              onClick={(event) => void handleFrameClick(event)}
              ref={frameImageRef}
              src={pageFrame.dataUrl}
              style={{
                cursor: "default",
                display: "block",
                height: "auto",
                maxWidth: "100%",
                userSelect: "none",
                width: "100%",
              }}
            />
          </div>
        ) : preview ? (
          <article aria-label="Read-only page preview" style={{ minWidth: 0 }}>
            {preview.title && (
              <h2 style={{ color: "var(--fg1)", fontSize: 15, fontWeight: 650, margin: "0 0 10px" }}>
                {preview.title}
              </h2>
            )}
            {preview.text ? (
              <pre
                style={{
                  background: "var(--canvas)",
                  border: "1px solid var(--border)",
                  borderRadius: 6,
                  color: "var(--fg1)",
                  fontFamily: FONT_MONO,
                  fontSize: 12,
                  lineHeight: 1.5,
                  margin: 0,
                  overflow: "auto",
                  padding: 12,
                  whiteSpace: "pre-wrap",
                  wordBreak: "break-word",
                }}
              >
                {preview.text}
                {preview.truncated ? `\n\n${t("browser.surface.previewTruncated")}` : ""}
              </pre>
            ) : (
              <p style={{ color: "var(--fg3)", margin: 0 }}>
                {t("browser.surface.noReadableText")}
              </p>
            )}
          </article>
        ) : (
          <div style={{ color: "var(--fg3)", lineHeight: 1.5, maxWidth: 620 }}>
            <strong style={{ color: "var(--fg1)" }}>{t("browser.surface.newTab")}</strong>
            {previewError && <p style={{ color: "var(--red, #F87171)", margin: "8px 0 0" }}>{previewError}</p>}
          </div>
        )}
      </main>
    </section>
  );
}

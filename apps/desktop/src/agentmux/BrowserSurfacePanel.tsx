import {
  useCallback,
  useEffect,
  useRef,
  useState,
} from "react";
import type { ControlClient } from "../control/ControlClient";
import {
  IconArrowLeft,
  IconArrowRight,
  IconCamera,
  IconClose,
  IconCopy,
  IconExternalLink,
  IconMinus,
  IconMoreVertical,
  IconPlus,
  IconReload,
  IconSearch,
} from "./icons";
import type { Translator } from "./i18n";
import {
  NATIVE_BROWSER_LAYOUT_EVENT,
  findInNativeBrowser,
  hideNativeBrowser,
  listenToNativeBrowser,
  measureNativeBrowserBounds,
  mountNativeBrowser,
  navigateNativeBrowser,
  runNativeBrowserNavigation,
  setNativeBrowserZoom,
  supportsNativeBrowser,
  updateNativeBrowserBounds,
} from "./nativeBrowser";

const FONT_SANS =
  "'Pretendard Variable',Pretendard,-apple-system,'Segoe UI','Malgun Gothic',system-ui,sans-serif";
const FONT_MONO =
  "'Cascadia Mono','JetBrains Mono','D2Coding','Consolas',monospace";
const MAX_PREVIEW_CHARS = 12_000;

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

function fallbackPageTitle(url: string | null): string | null {
  if (isBlankUrl(url)) return null;
  try {
    const parsed = new URL(url as string);
    return parsed.hostname || parsed.href;
  } catch {
    return url?.trim() || null;
  }
}

export function BrowserSurfacePanel({
  client,
  surfaceId,
  visible = true,
  t,
  onTitleChange,
  onOpenExternal,
}: {
  client: ControlClient;
  surfaceId: string;
  visible?: boolean;
  t: Translator;
  onTitleChange?: (surfaceId: string, title: string | null) => void;
  onOpenExternal: (url: string) => Promise<boolean>;
}) {
  const [address, setAddress] = useState("");
  const [currentUrl, setCurrentUrl] = useState<string | null>(null);
  const [loadState, setLoadState] = useState<BrowserLoadState>("idle");
  const [preview, setPreview] = useState<BrowserPagePreview | null>(null);
  const [pageFrame, setPageFrame] = useState<BrowserPageFrame | null>(null);
  const [previewError, setPreviewError] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [menuOpen, setMenuOpen] = useState(false);
  const [findOpen, setFindOpen] = useState(false);
  const [findQuery, setFindQuery] = useState("");
  const [findCount, setFindCount] = useState<number | null>(null);
  const [findBusy, setFindBusy] = useState(false);
  const [zoomPercent, setZoomPercent] = useState(100);
  const [copiedLink, setCopiedLink] = useState(false);
  const [copiedScreenshot, setCopiedScreenshot] = useState(false);
  const [nativeBrowserReady, setNativeBrowserReady] = useState(false);
  const [nativeTitle, setNativeTitle] = useState<string | null>(null);
  const nativeBrowser = supportsNativeBrowser();
  const addressInputRef = useRef<HTMLInputElement | null>(null);
  const findInputRef = useRef<HTMLInputElement | null>(null);
  const menuRootRef = useRef<HTMLDivElement | null>(null);
  const frameElementRef = useRef<HTMLDivElement | null>(null);
  const frameImageRef = useRef<HTMLImageElement | null>(null);
  const refreshTimerRef = useRef<number | null>(null);
  const viewportTimerRef = useRef<number | null>(null);
  const copyTimerRef = useRef<number | null>(null);
  const lastViewportRef = useRef("");
  const viewportQueueRef = useRef<Promise<void>>(Promise.resolve());
  const frameRequestRef = useRef(0);
  const wheelDeltaRef = useRef({ x: 0, y: 0 });
  const wheelAnimationFrameRef = useRef<number | null>(null);
  const wheelQueueRef = useRef<Promise<void>>(Promise.resolve());
  const nativeBrowserMountedRef = useRef(false);
  const nativeBrowserMountRef = useRef<ReturnType<typeof mountNativeBrowser> | null>(null);
  const nativeBrowserGenerationRef = useRef(0);

  const syncNativeBrowser = useCallback(
    async (url: string, shouldShow = visible) => {
      const element = frameElementRef.current;
      if (!nativeBrowser || !element) return false;
      const bounds = measureNativeBrowserBounds(element);
      if (!bounds) return false;
      const unobstructed =
        shouldShow &&
        !menuOpen &&
        !Boolean(document.querySelector('[role="dialog"]'));
      if (nativeBrowserMountedRef.current) {
        await updateNativeBrowserBounds(surfaceId, bounds, unobstructed);
        return true;
      }
      const generation = nativeBrowserGenerationRef.current;
      const mount =
        nativeBrowserMountRef.current ??
        mountNativeBrowser(surfaceId, url, bounds, unobstructed);
      nativeBrowserMountRef.current = mount;
      const result = await mount.finally(() => {
        if (nativeBrowserMountRef.current === mount) {
          nativeBrowserMountRef.current = null;
        }
      });
      if (generation !== nativeBrowserGenerationRef.current) {
        await hideNativeBrowser(surfaceId).catch(() => undefined);
        return false;
      }
      nativeBrowserMountedRef.current = true;
      await updateNativeBrowserBounds(surfaceId, bounds, unobstructed);
      setNativeBrowserReady(true);
      if (result.url && result.url !== "about:blank") {
        setCurrentUrl(result.url);
        setAddress(result.url);
      }
      return true;
    },
    [menuOpen, nativeBrowser, surfaceId, visible],
  );

  const syncViewport = useCallback(async () => {
    const element = frameElementRef.current;
    if (!element) return false;
    const width = Math.max(1, Math.min(8192, Math.round(element.clientWidth)));
    const height = Math.max(1, Math.min(8192, Math.round(element.clientHeight)));
    const key = `${width}x${height}`;
    if (key === lastViewportRef.current) return false;
    let changed = false;
    const queued = viewportQueueRef.current
      .catch(() => undefined)
      .then(async () => {
        if (key === lastViewportRef.current) return;
        await client.browserSetViewport(surfaceId, width, height);
        lastViewportRef.current = key;
        changed = true;
      });
    viewportQueueRef.current = queued.then(
      () => undefined,
      () => undefined,
    );
    await queued;
    return changed;
  }, [client, surfaceId]);

  const loadPageFrame = useCallback(
    async (url: string, failOnError = false) => {
      const request = ++frameRequestRef.current;
      if (nativeBrowser) {
        try {
          await syncNativeBrowser(url || "about:blank");
          setPreview(null);
          setPageFrame(null);
          setPreviewError(null);
        } catch (cause) {
          const message =
            cause instanceof Error ? cause.message : t("browser.surface.renderFailed");
          setPreviewError(message);
          if (failOnError) throw cause instanceof Error ? cause : new Error(message);
        }
        return;
      }
      if (isBlankUrl(url)) {
        setPreview(null);
        setPageFrame(null);
        setPreviewError(null);
        return;
      }
      try {
        await syncViewport();
        const [snapshot, screenshot] = await Promise.all([
          client.browserDomSnapshot(surfaceId),
          client.browserScreenshot(surfaceId, "png"),
        ]);
        if (request !== frameRequestRef.current) return;
        setPreview(summarizeBrowserDom(snapshot.html));
        setPageFrame({
          byteCount: screenshot.byteCount,
          dataUrl: `data:image/png;base64,${screenshot.dataBase64}`,
        });
        setPreviewError(null);
      } catch (cause) {
        if (request !== frameRequestRef.current) return;
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
    [client, nativeBrowser, surfaceId, syncNativeBrowser, syncViewport, t],
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

  useEffect(() => {
    if (!nativeBrowser) return;
    let disposed = false;
    let unlisten: (() => void) | undefined;
    void listenToNativeBrowser(surfaceId, {
      onPageLoad: (event) => {
        if (disposed) return;
        setCurrentUrl(event.url);
        setAddress(event.url);
        setLoadState(event.state === "started" ? "loading" : "ready");
        setError(null);
      },
      onTitle: (event) => {
        if (!disposed) setNativeTitle(event.title.trim() || null);
      },
    })
      .then((cleanup) => {
        if (disposed) cleanup();
        else unlisten = cleanup;
      })
      .catch((cause: unknown) => {
        if (!disposed) {
          setError(cause instanceof Error ? cause.message : t("browser.surface.connectionError"));
        }
      });
    return () => {
      disposed = true;
      unlisten?.();
    };
  }, [nativeBrowser, surfaceId, t]);

  useEffect(() => {
    if (!nativeBrowser || !currentUrl) return;
    void syncNativeBrowser(currentUrl).catch((cause: unknown) => {
      setError(cause instanceof Error ? cause.message : t("browser.surface.renderFailed"));
    });
  }, [currentUrl, nativeBrowser, syncNativeBrowser, t]);

  useEffect(() => {
    if (!nativeBrowser) return;
    let frame: number | null = null;
    const syncLayout = () => {
      if (frame !== null) return;
      frame = window.requestAnimationFrame(() => {
        frame = null;
        void syncNativeBrowser(currentUrl || "about:blank").catch((cause: unknown) => {
          setError(cause instanceof Error ? cause.message : t("browser.surface.renderFailed"));
        });
      });
    };
    window.addEventListener(NATIVE_BROWSER_LAYOUT_EVENT, syncLayout);
    return () => {
      window.removeEventListener(NATIVE_BROWSER_LAYOUT_EVENT, syncLayout);
      if (frame !== null) window.cancelAnimationFrame(frame);
    };
  }, [currentUrl, nativeBrowser, syncNativeBrowser, t]);

  useEffect(() => {
    if (!nativeBrowser) return;
    let disposed = false;
    let queued = false;
    const updateVisibility = () => {
      if (queued || disposed) return;
      queued = true;
      window.requestAnimationFrame(() => {
        queued = false;
        if (disposed || !nativeBrowserMountedRef.current) return;
        if (!visible) {
          void hideNativeBrowser(surfaceId).catch(() => undefined);
          return;
        }
        const element = frameElementRef.current;
        const bounds = element ? measureNativeBrowserBounds(element) : null;
        if (!bounds) return;
        const occluded = menuOpen || Boolean(document.querySelector('[role="dialog"]'));
        void updateNativeBrowserBounds(surfaceId, bounds, !occluded).catch(() => undefined);
      });
    };
    updateVisibility();
    const observer = new MutationObserver(updateVisibility);
    observer.observe(document.body, { childList: true, subtree: true });
    return () => {
      disposed = true;
      observer.disconnect();
    };
  }, [menuOpen, nativeBrowser, surfaceId, visible]);

  useEffect(
    () => () => {
      nativeBrowserGenerationRef.current += 1;
      if (nativeBrowserMountedRef.current) {
        void hideNativeBrowser(surfaceId).catch(() => undefined);
        nativeBrowserMountedRef.current = false;
      }
    },
    [surfaceId],
  );

  useEffect(
    () => () => {
      if (refreshTimerRef.current !== null) {
        window.clearTimeout(refreshTimerRef.current);
      }
      if (viewportTimerRef.current !== null) {
        window.clearTimeout(viewportTimerRef.current);
      }
      if (copyTimerRef.current !== null) {
        window.clearTimeout(copyTimerRef.current);
      }
      if (wheelAnimationFrameRef.current !== null) {
        window.cancelAnimationFrame(wheelAnimationFrameRef.current);
      }
    },
    [],
  );

  useEffect(() => {
    if (!menuOpen) return;
    const closeMenu = (event: PointerEvent) => {
      if (!menuRootRef.current?.contains(event.target as Node)) {
        setMenuOpen(false);
      }
    };
    document.addEventListener("pointerdown", closeMenu);
    return () => document.removeEventListener("pointerdown", closeMenu);
  }, [menuOpen]);

  useEffect(() => {
    if (findOpen) {
      findInputRef.current?.focus();
      findInputRef.current?.select();
    }
  }, [findOpen]);

  useEffect(() => {
    onTitleChange?.(
      surfaceId,
      nativeTitle || preview?.title?.trim() || fallbackPageTitle(currentUrl),
    );
  }, [currentUrl, nativeTitle, onTitleChange, preview?.title, surfaceId]);

  useEffect(() => {
    const element = frameElementRef.current;
    if (!element || typeof ResizeObserver === "undefined") return;
    const refreshForViewport = () => {
      if (viewportTimerRef.current !== null) {
        window.clearTimeout(viewportTimerRef.current);
      }
      viewportTimerRef.current = window.setTimeout(() => {
        viewportTimerRef.current = null;
        if (nativeBrowser) {
          void syncNativeBrowser(currentUrl || "about:blank").catch((cause: unknown) => {
            setError(cause instanceof Error ? cause.message : t("browser.surface.renderFailed"));
          });
          return;
        }
        void syncViewport()
          .then((changed) => {
            if (changed && currentUrl && !isBlankUrl(currentUrl)) {
              void loadPageFrame(currentUrl);
            }
          })
          .catch((cause: unknown) => {
            setError(cause instanceof Error ? cause.message : t("browser.surface.renderFailed"));
          });
      }, nativeBrowser ? 0 : 80);
    };
    const observer = new ResizeObserver(refreshForViewport);
    observer.observe(element);
    refreshForViewport();
    return () => {
      observer.disconnect();
      if (viewportTimerRef.current !== null) {
        window.clearTimeout(viewportTimerRef.current);
        viewportTimerRef.current = null;
      }
    };
  }, [currentUrl, loadPageFrame, nativeBrowser, syncNativeBrowser, syncViewport, t]);

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
      if (nativeBrowser) {
        await syncNativeBrowser(currentUrl || "about:blank");
        await navigateNativeBrowser(surfaceId, url);
        setCurrentUrl(url);
        setAddress(url);
        void client.browserNavigate(surfaceId, url).catch(() => undefined);
        return;
      }
      const result = await client.browserNavigate(surfaceId, url);
      await applyNavigationResult(result.url);
      setLoadState("ready");
    } catch (cause) {
      setError(cause instanceof Error ? cause.message : t("browser.surface.navigationFailed"));
      setLoadState("error");
    }
  }, [address, applyNavigationResult, client, currentUrl, nativeBrowser, surfaceId, syncNativeBrowser, t]);

  const runNavigationAction = useCallback(
    async (action: "back" | "forward" | "reload") => {
      setLoadState("loading");
      setError(null);
      try {
        if (nativeBrowser) {
          await runNativeBrowserNavigation(surfaceId, action);
          void (
            action === "back"
              ? client.browserBack(surfaceId)
              : action === "forward"
                ? client.browserForward(surfaceId)
                : client.browserReload(surfaceId)
          ).catch(() => undefined);
          return;
        }
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
    [applyNavigationResult, client, nativeBrowser, surfaceId, t],
  );

  const controlsDisabled = loadState === "loading";
  const addressUrl = normalizeBrowserNavigationUrl(address);
  const externalUrl = normalizeBrowserNavigationUrl(
    currentUrl && !isBlankUrl(currentUrl) ? currentUrl : address,
  );
  const showNavigateAction = Boolean(addressUrl && addressUrl !== currentUrl);

  const openExternal = useCallback(async () => {
    if (!externalUrl) return;
    setError(null);
    try {
      if (await onOpenExternal(externalUrl)) return;
      setError(t("browser.surface.externalOpenFailed"));
    } catch (cause) {
      setError(
        cause instanceof Error
          ? cause.message
          : t("browser.surface.externalOpenFailed"),
      );
    }
  }, [externalUrl, onOpenExternal, t]);

  const runFind = useCallback(async () => {
    const query = findQuery.trim();
    if (!query) {
      setFindCount(null);
      return;
    }
    setFindBusy(true);
    setError(null);
    try {
      if (nativeBrowser) {
        await findInNativeBrowser(surfaceId, query);
        void client.browserFind(surfaceId, query, { limit: 1 }).then((result) => {
          setFindCount(result.count);
        }).catch(() => undefined);
        return;
      }
      const result = await client.browserFind(surfaceId, query, { limit: 1 });
      await client.browserEvaluate(
        surfaceId,
        `window.find(${JSON.stringify(query)}, false, false, true, false, false, false)`,
      );
      setFindCount(result.count);
      if (currentUrl && !isBlankUrl(currentUrl)) {
        await loadPageFrame(currentUrl);
      }
    } catch (cause) {
      setError(cause instanceof Error ? cause.message : t("browser.surface.findFailed"));
    } finally {
      setFindBusy(false);
    }
  }, [client, currentUrl, findQuery, loadPageFrame, nativeBrowser, surfaceId, t]);

  const applyZoom = useCallback(
    async (percent: number) => {
      const next = Math.max(25, Math.min(500, percent));
      setLoadState("loading");
      setError(null);
      try {
        if (nativeBrowser) {
          await setNativeBrowserZoom(surfaceId, next);
          setZoomPercent(next);
          setLoadState("ready");
          void client.browserZoom(surfaceId, next).catch(() => undefined);
          return;
        }
        await client.browserZoom(surfaceId, next);
        setZoomPercent(next);
        if (currentUrl && !isBlankUrl(currentUrl)) {
          await loadPageFrame(currentUrl);
        }
        setLoadState("ready");
      } catch (cause) {
        setError(cause instanceof Error ? cause.message : t("browser.surface.zoomFailed"));
        setLoadState("error");
      }
    },
    [client, currentUrl, loadPageFrame, nativeBrowser, surfaceId, t],
  );

  const copyCurrentLink = useCallback(async () => {
    if (!externalUrl) return;
    setError(null);
    try {
      try {
        await navigator.clipboard.writeText(externalUrl);
      } catch {
        const clipboard = await import("@tauri-apps/plugin-clipboard-manager");
        await clipboard.writeText(externalUrl);
      }
      setCopiedLink(true);
      setCopiedScreenshot(false);
      if (copyTimerRef.current !== null) {
        window.clearTimeout(copyTimerRef.current);
      }
      copyTimerRef.current = window.setTimeout(() => {
        copyTimerRef.current = null;
        setCopiedLink(false);
      }, 1400);
    } catch (cause) {
      setError(
        cause instanceof Error
          ? cause.message
          : t("browser.surface.externalOpenFailed"),
      );
    }
  }, [externalUrl, t]);

  const copyPageScreenshot = useCallback(async () => {
    if (!pageFrame) return;
    setError(null);
    try {
      const response = await fetch(pageFrame.dataUrl);
      const blob = await response.blob();
      const bytes = new Uint8Array(await blob.arrayBuffer());
      try {
        const clipboard = await import("@tauri-apps/plugin-clipboard-manager");
        await clipboard.writeImage(bytes);
      } catch (tauriError) {
        if (!navigator.clipboard?.write || typeof ClipboardItem === "undefined") {
          throw tauriError;
        }
        await navigator.clipboard.write([
          new ClipboardItem({ "image/png": blob }),
        ]);
      }
      setCopiedLink(false);
      setCopiedScreenshot(true);
      if (copyTimerRef.current !== null) {
        window.clearTimeout(copyTimerRef.current);
      }
      copyTimerRef.current = window.setTimeout(() => {
        copyTimerRef.current = null;
        setCopiedScreenshot(false);
      }, 1400);
    } catch (cause) {
      setError(
        cause instanceof Error
          ? cause.message
          : t("browser.surface.screenshotFailed"),
      );
    }
  }, [pageFrame, t]);

  const handleBrowserShortcut = useCallback((event: React.KeyboardEvent<HTMLElement>) => {
    if (!(event.ctrlKey || event.metaKey) || event.altKey) return;
    const key = event.key.toLowerCase();
    if (key === "l") {
      event.preventDefault();
      addressInputRef.current?.focus();
      addressInputRef.current?.select();
    } else if (key === "f") {
      event.preventDefault();
      setMenuOpen(false);
      setFindOpen(true);
    }
  }, []);

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
      const pageSize = Math.max(1, frameElementRef.current?.clientHeight ?? 800);
      const scale = event.deltaMode === 1 ? 32 : event.deltaMode === 2 ? pageSize : 1;
      wheelDeltaRef.current.x += event.deltaX * scale;
      wheelDeltaRef.current.y += event.deltaY * scale;
      if (wheelAnimationFrameRef.current !== null) return;
      wheelAnimationFrameRef.current = window.requestAnimationFrame(() => {
        wheelAnimationFrameRef.current = null;
        const x = Math.trunc(wheelDeltaRef.current.x);
        const y = Math.trunc(wheelDeltaRef.current.y);
        wheelDeltaRef.current.x -= x;
        wheelDeltaRef.current.y -= y;
        if (x === 0 && y === 0) return;
        const queued = wheelQueueRef.current
          .catch(() => undefined)
          .then(() => client.browserScroll(surfaceId, { x, y }))
          .then(() => {
            setError(null);
            scheduleFrameRefresh();
          })
          .catch((cause: unknown) => {
            setError(
              cause instanceof Error
                ? cause.message
                : t("browser.surface.scrollFailed"),
            );
          });
        wheelQueueRef.current = queued.then(
          () => undefined,
          () => undefined,
        );
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
      onKeyDown={handleBrowserShortcut}
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
      <header className="agentmux-browser-toolbar">
        <button
          aria-label={t("browser.surface.back")}
          className="agentmux-browser-nav-button"
          disabled={controlsDisabled}
          onClick={() => void runNavigationAction("back")}
          title={t("browser.surface.back")}
          type="button"
        >
          <IconArrowLeft size={15} />
        </button>
        <button
          aria-label={t("browser.surface.forward")}
          className="agentmux-browser-nav-button"
          disabled={controlsDisabled}
          onClick={() => void runNavigationAction("forward")}
          title={t("browser.surface.forward")}
          type="button"
        >
          <IconArrowRight size={15} />
        </button>
        <button
          aria-label={t("browser.surface.reload")}
          className={`agentmux-browser-nav-button${controlsDisabled ? " is-loading" : ""}`}
          disabled={controlsDisabled}
          onClick={() => void runNavigationAction("reload")}
          title={t("browser.surface.reload")}
          type="button"
        >
          <IconReload size={14} />
        </button>
        <form
          className="agentmux-browser-address-form"
          onSubmit={(event) => {
            event.preventDefault();
            void navigate();
          }}
        >
          <div className="agentmux-browser-address-shell">
            <input
              aria-label={t("browser.surface.address")}
              autoCapitalize="none"
              autoCorrect="off"
              disabled={controlsDisabled}
              id={`browser-address-${surfaceId}`}
              onChange={(event) => setAddress(event.target.value)}
              placeholder="https://example.com"
              ref={addressInputRef}
              spellCheck={false}
              value={address}
            />
            <span className="agentmux-browser-address-actions">
              {showNavigateAction && (
                <button
                  aria-label={t("browser.surface.go")}
                  className="agentmux-browser-address-action agentmux-browser-address-go"
                  disabled={controlsDisabled}
                  title={t("browser.surface.go")}
                  type="submit"
                >
                  <IconArrowRight size={14} />
                </button>
              )}
              <button
                aria-label={t("browser.surface.openExternal")}
                className="agentmux-browser-address-action agentmux-browser-open-external"
                disabled={controlsDisabled || !externalUrl}
                onClick={() => void openExternal()}
                title={t("browser.surface.openExternal")}
                type="button"
              >
                <IconExternalLink size={14} />
              </button>
            </span>
          </div>
        </form>
        <div className="agentmux-browser-menu-root" ref={menuRootRef}>
          <button
            aria-expanded={menuOpen}
            aria-haspopup="menu"
            aria-label={t("browser.surface.more")}
            className="agentmux-browser-nav-button"
            onClick={() => setMenuOpen((open) => !open)}
            title={t("browser.surface.more")}
            type="button"
          >
            <IconMoreVertical size={14} />
          </button>
          {menuOpen && (
            <div className="agentmux-browser-menu" role="menu">
              <button
                className="agentmux-browser-menu-item"
                onClick={() => {
                  setMenuOpen(false);
                  setFindOpen(true);
                }}
                role="menuitem"
                type="button"
              >
                <IconSearch size={14} />
                <span>{t("browser.surface.find")}</span>
              </button>
              <div
                aria-label={t("browser.surface.zoom")}
                className="agentmux-browser-menu-zoom"
                role="group"
              >
                <span>{t("browser.surface.zoom")}</span>
                <div>
                  <button
                    aria-label={t("browser.surface.zoomOut")}
                    disabled={controlsDisabled || zoomPercent <= 25}
                    onClick={() => void applyZoom(zoomPercent - 25)}
                    title={t("browser.surface.zoomOut")}
                    type="button"
                  >
                    <IconMinus size={13} />
                  </button>
                  <button
                    aria-label={t("browser.surface.zoomReset")}
                    disabled={controlsDisabled || zoomPercent === 100}
                    onClick={() => void applyZoom(100)}
                    title={t("browser.surface.zoomReset")}
                    type="button"
                  >
                    {zoomPercent}%
                  </button>
                  <button
                    aria-label={t("browser.surface.zoomIn")}
                    disabled={controlsDisabled || zoomPercent >= 500}
                    onClick={() => void applyZoom(zoomPercent + 25)}
                    title={t("browser.surface.zoomIn")}
                    type="button"
                  >
                    <IconPlus size={13} />
                  </button>
                </div>
              </div>
              <div className="agentmux-browser-menu-separator" />
              <button
                className="agentmux-browser-menu-item"
                disabled={!externalUrl}
                onClick={() => void copyCurrentLink()}
                role="menuitem"
                type="button"
              >
                <IconCopy size={14} />
                <span>
                  {copiedLink
                    ? t("browser.surface.copied")
                    : t("browser.surface.copyLink")}
                </span>
              </button>
              <button
                className="agentmux-browser-menu-item"
                disabled={!pageFrame}
                onClick={() => void copyPageScreenshot()}
                role="menuitem"
                type="button"
              >
                <IconCamera size={14} />
                <span>
                  {copiedScreenshot
                    ? t("browser.surface.screenshotCopied")
                    : t("browser.surface.screenshotCopy")}
                </span>
              </button>
            </div>
          )}
        </div>
      </header>

      {controlsDisabled && (
        <div
          aria-hidden="true"
          className="agentmux-browser-loading-track"
        >
          <span />
        </div>
      )}

      {findOpen && (
        <form
          className="agentmux-browser-findbar"
          onSubmit={(event) => {
            event.preventDefault();
            void runFind();
          }}
        >
          <IconSearch size={13} />
          <input
            aria-label={t("browser.surface.find")}
            disabled={findBusy}
            onChange={(event) => {
              setFindQuery(event.target.value);
              setFindCount(null);
            }}
            ref={findInputRef}
            value={findQuery}
          />
          <span aria-live="polite">
            {findCount === null
              ? ""
              : t("browser.surface.findCount", { count: findCount })}
          </span>
          <button
            aria-label={t("browser.surface.find")}
            disabled={findBusy || !findQuery.trim()}
            title={t("browser.surface.find")}
            type="submit"
          >
            <IconArrowRight size={13} />
          </button>
          <button
            aria-label={t("common.dismiss")}
            onClick={() => setFindOpen(false)}
            title={t("common.dismiss")}
            type="button"
          >
            <IconClose size={11} />
          </button>
        </form>
      )}

      {error && (
        <div className="agentmux-browser-error-banner" role="alert">
          <span>{error}</span>
          <button
            aria-label={t("common.dismiss")}
            onClick={() => setError(null)}
            title={t("common.dismiss")}
            type="button"
          >
            <IconClose size={11} />
          </button>
        </div>
      )}

      <main
        style={{
          display: "flex",
          flex: 1,
          flexDirection: "column",
          minHeight: 0,
          minWidth: 0,
          overflow: "hidden",
          padding: 0,
          position: "relative",
        }}
      >
        <div
          aria-label={nativeBrowser ? "Native browser viewport" : "Interactive page preview"}
          onKeyDown={nativeBrowser ? undefined : handleFrameKeyDown}
          onWheel={nativeBrowser ? undefined : handleFrameWheel}
          ref={frameElementRef}
          style={{
            alignItems: "stretch",
            background: nativeBrowser || pageFrame ? "#fff" : "var(--term)",
            display: "flex",
            flex: "1 1 0",
            justifyContent: "stretch",
            minHeight: 0,
            minWidth: 0,
            overflow: "hidden",
            outline: "none",
          }}
          tabIndex={nativeBrowser ? -1 : 0}
        >
          {nativeBrowser ? (
            <div
              aria-hidden="true"
              className="agentmux-native-browser-placeholder"
              data-native-browser-ready={nativeBrowserReady ? "true" : "false"}
            />
          ) : pageFrame ? (
            <img
              alt={preview?.title ?? currentUrl ?? "Browser page"}
              draggable={false}
              onClick={(event) => void handleFrameClick(event)}
              ref={frameImageRef}
              src={pageFrame.dataUrl}
              style={{
                cursor: "default",
                display: "block",
                height: "100%",
                maxWidth: "none",
                objectFit: "fill",
                userSelect: "none",
                width: "100%",
              }}
            />
          ) : preview ? (
            <article
              aria-label="Read-only page preview"
              style={{ minWidth: 0, overflow: "auto", padding: 18 }}
            >
              {preview.title && (
                <h2
                  style={{
                    color: "var(--fg1)",
                    fontSize: 15,
                    fontWeight: 650,
                    margin: "0 0 10px",
                  }}
                >
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
                  {preview.truncated
                    ? `\n\n${t("browser.surface.previewTruncated")}`
                    : ""}
                </pre>
              ) : (
                <p style={{ color: "var(--fg3)", margin: 0 }}>
                  {t("browser.surface.noReadableText")}
                </p>
              )}
            </article>
          ) : (
            <div
              style={{
                alignSelf: "center",
                color: "var(--fg3)",
                lineHeight: 1.5,
                maxWidth: 620,
                padding: 18,
              }}
            >
              <strong style={{ color: "var(--fg1)" }}>
                {t("browser.surface.newTab")}
              </strong>
              {previewError && (
                <p
                  style={{
                    color: "var(--red, #F87171)",
                    margin: "8px 0 0",
                  }}
                >
                  {previewError}
                </p>
              )}
            </div>
          )}
        </div>
      </main>
    </section>
  );
}

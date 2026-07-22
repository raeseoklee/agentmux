interface ResolutionMediaQuery {
  addEventListener(type: "change", listener: () => void): void;
  removeEventListener(type: "change", listener: () => void): void;
}

export interface DevicePixelRatioSource {
  read(): number;
  match(query: string): ResolutionMediaQuery;
  subscribeResize?(listener: () => void): () => void;
}

function normalizeDevicePixelRatio(value: number): number {
  return Number.isFinite(value) && value > 0 ? value : 1;
}

/** Watches display-scale changes even when WebView2 does not emit a resize. */
export function observeDevicePixelRatio(
  onChange: (devicePixelRatio: number) => void,
  source: DevicePixelRatioSource = {
    read: () => window.devicePixelRatio,
    match: (query) => window.matchMedia(query),
    subscribeResize: (listener) => {
      window.addEventListener("resize", listener);
      return () => window.removeEventListener("resize", listener);
    },
  },
): () => void {
  let disposed = false;
  let current = normalizeDevicePixelRatio(source.read());
  let mediaQuery: ResolutionMediaQuery | null = null;
  let listener: (() => void) | null = null;

  const detach = () => {
    if (mediaQuery && listener) {
      mediaQuery.removeEventListener("change", listener);
    }
    mediaQuery = null;
    listener = null;
  };

  const inspect = () => {
    if (disposed) {
      return;
    }
    const next = normalizeDevicePixelRatio(source.read());
    if (Math.abs(next - current) <= 0.001) {
      return;
    }
    current = next;
    detach();
    onChange(next);
    arm();
  };

  const arm = () => {
    if (disposed) {
      return;
    }
    mediaQuery = source.match(`(resolution: ${current}dppx)`);
    listener = inspect;
    mediaQuery.addEventListener("change", listener);
  };

  arm();
  const unsubscribeResize = source.subscribeResize?.(inspect) ?? (() => {});

  return () => {
    disposed = true;
    unsubscribeResize();
    detach();
  };
}

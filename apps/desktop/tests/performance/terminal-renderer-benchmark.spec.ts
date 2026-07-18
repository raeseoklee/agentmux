import { expect, test } from "@playwright/test";

interface RendererMeasurement {
  mode: "dom" | "auto" | "webgl";
  available: boolean;
  bytes: number;
  durationMs: number | null;
  throughputMiBPerSecond: number | null;
  diagnostics: unknown;
}

test("records DOM and WebGL terminal write performance", async ({ page }, testInfo) => {
  await page.goto("/");
  const measurements = await page.evaluate(async () => {
    const { XtermTerminalRenderer } = await import(
      "/src/terminal/XtermTerminalRenderer.ts"
    );
    const host = document.createElement("div");
    host.dataset.agentmuxPerformanceFixture = "true";
    host.style.cssText =
      "position:fixed;inset:0;width:1280px;height:720px;background:#0d1117;z-index:99999";
    document.body.appendChild(host);

    const renderer = new XtermTerminalRenderer();
    renderer.mount(host, { columns: 160, rows: 45, bytes: new Uint8Array() });
    renderer.fit();

    const encoder = new TextEncoder();
    const line =
      "\u001b[38;5;45mAgentMux\u001b[0m | Powerline \ue0b0 \ue0b2 | 한글 | 日本語 | emoji ✓ | 0123456789abcdef\r\n";
    const fixture = encoder.encode(line.repeat(24_000));

    const writeFixture = async () => {
      renderer.reset();
      const startedAt = performance.now();
      for (let offset = 0; offset < fixture.length; offset += 64 * 1024) {
        const chunk = fixture.subarray(offset, Math.min(fixture.length, offset + 64 * 1024));
        await new Promise<void>((resolve) => renderer.write(chunk, resolve));
      }
      const durationMs = performance.now() - startedAt;
      return {
        bytes: fixture.length,
        durationMs,
        throughputMiBPerSecond:
          fixture.length / (1024 * 1024) / Math.max(durationMs / 1000, 0.001),
      };
    };

    renderer.disableWebgl();
    const dom = await writeFixture();

    const waitForWebglDecision = async () => {
      const deadline = performance.now() + 5_000;
      while (performance.now() < deadline) {
        const diagnostics = renderer.getWebglDiagnostics();
        if (diagnostics.state !== "loading" && diagnostics.state !== "idle") {
          return diagnostics;
        }
        await new Promise((resolve) => window.setTimeout(resolve, 25));
      }
      return renderer.getWebglDiagnostics();
    };

    renderer.resetWebglRecovery();
    renderer.enableWebgl("auto");
    const autoDiagnostics = await waitForWebglDecision();
    const autoAvailable = renderer.isWebglEnabled();
    const auto = autoAvailable ? await writeFixture() : null;

    renderer.disableWebgl();
    renderer.resetWebglRecovery();
    renderer.enableWebgl("on");
    await waitForWebglDecision();

    const webglAvailable = renderer.isWebglEnabled();
    const webgl = webglAvailable ? await writeFixture() : null;
    const diagnostics = renderer.getWebglDiagnostics();
    (
      window as Window & { __AGENTMUX_BENCHMARK_CLEANUP__?: () => void }
    ).__AGENTMUX_BENCHMARK_CLEANUP__ = () => {
      renderer.dispose();
      host.remove();
    };

    return [
      {
        mode: "dom" as const,
        available: true,
        ...dom,
        diagnostics: null,
      },
      {
        mode: "auto" as const,
        available: autoAvailable,
        bytes: fixture.length,
        durationMs: auto?.durationMs ?? null,
        throughputMiBPerSecond: auto?.throughputMiBPerSecond ?? null,
        diagnostics: autoDiagnostics,
      },
      {
        mode: "webgl" as const,
        available: webglAvailable,
        bytes: fixture.length,
        durationMs: webgl?.durationMs ?? null,
        throughputMiBPerSecond: webgl?.throughputMiBPerSecond ?? null,
        diagnostics,
      },
    ];
  });

  await testInfo.attach("terminal-renderer-metrics", {
    body: Buffer.from(JSON.stringify(measurements, null, 2)),
    contentType: "application/json",
  });
  await expect(
    page.locator('[data-agentmux-performance-fixture="true"] .xterm-screen'),
  ).toBeVisible();
  await testInfo.attach("terminal-renderer-fixture", {
    body: await page.screenshot(),
    contentType: "image/png",
  });
  await page.evaluate(() => {
    const target = window as Window & {
      __AGENTMUX_BENCHMARK_CLEANUP__?: () => void;
    };
    target.__AGENTMUX_BENCHMARK_CLEANUP__?.();
    delete target.__AGENTMUX_BENCHMARK_CLEANUP__;
  });

  const dom = measurements[0] as RendererMeasurement;
  const auto = measurements[1] as RendererMeasurement;
  const webgl = measurements[2] as RendererMeasurement;
  expect(dom.available).toBe(true);
  expect(dom.bytes).toBeGreaterThan(1_000_000);
  expect(dom.durationMs).toBeGreaterThan(0);
  expect(auto.diagnostics).toBeTruthy();
  if (auto.available) {
    expect(auto.durationMs).toBeGreaterThan(0);
  }
  if (webgl.available) {
    expect(webgl.durationMs).toBeGreaterThan(0);
    expect(webgl.bytes).toBe(dom.bytes);
  }
});

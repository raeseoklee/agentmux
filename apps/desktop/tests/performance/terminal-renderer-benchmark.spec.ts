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
  test.skip(
    testInfo.project.metadata.deviceScaleFactor === 1,
    "Performance throughput only needs one display-scale sample.",
  );
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

test("renders continuous box drawing glyphs at Windows display scales", async ({ page }, testInfo) => {
  await page.goto("/");
  const diagnostics = await page.evaluate(async () => {
    const { XtermTerminalRenderer } = await import(
      "/src/terminal/XtermTerminalRenderer.ts"
    );
    const host = document.createElement("div");
    host.dataset.agentmuxBoxDrawingFixture = "true";
    host.style.cssText = [
      "position:fixed",
      "left:0",
      "top:0",
      "width:900px",
      "height:420px",
      "padding:12px",
      "box-sizing:border-box",
      "background:#0e1116",
      "z-index:99999",
    ].join(";");
    document.body.appendChild(host);

    const renderer = new XtermTerminalRenderer();
    renderer.mount(
      host,
      { columns: 80, rows: 16, bytes: new Uint8Array() },
      { fontSize: 15.5, lineHeight: 1 },
    );
    renderer.fit();
    renderer.resetWebglRecovery();
    renderer.enableWebgl("on");

    const deadline = performance.now() + 5_000;
    while (performance.now() < deadline) {
      const state = renderer.getWebglDiagnostics();
      if (state.state !== "loading" && state.state !== "idle") {
        break;
      }
      await new Promise((resolve) => window.setTimeout(resolve, 25));
    }

    await new Promise((resolve) => window.setTimeout(resolve, 150));
    renderer.fit();
    const fixture = [
      "┌────────────────────┬──────────────────────────────────────┐",
      "│ document           │ checkout                             │",
      "├────────────────────┼──────────────────────────────────────┤",
      "│ nurumteul-sample   │ 2 -> 22 files                        │",
      "├────────────────────┼──────────────────────────────────────┤",
      "│ nurumteul-column   │ 2 -> 6 files                         │",
      "├────────────────────┼──────────────────────────────────────┤",
      "│ nurumteul-row1     │ 2 -> 9 files                         │",
      "├────────────────────┼──────────────────────────────────────┤",
      "│ nurumteul-row2     │ 2 -> 8 files                         │",
      "└────────────────────┴──────────────────────────────────────┘",
    ].join("\r\n");
    await new Promise<void>((resolve) =>
      renderer.write(new TextEncoder().encode(fixture), resolve),
    );

    (
      window as Window & { __AGENTMUX_BOX_DRAWING_CLEANUP__?: () => void }
    ).__AGENTMUX_BOX_DRAWING_CLEANUP__ = () => {
      renderer.dispose();
      host.remove();
    };
    return {
      ...renderer.getWebglDiagnostics(),
      dpr: window.devicePixelRatio,
      fontSize: 15.5,
    };
  });

  const expectedDpr = Number(testInfo.project.metadata.deviceScaleFactor);
  expect(diagnostics.dpr).toBe(expectedDpr);
  expect(diagnostics.state).toBe("enabled");
  const fixture = page.locator('[data-agentmux-box-drawing-fixture="true"]');
  await expect(fixture.locator(".xterm-screen canvas").first()).toBeVisible();
  const screenshot = await fixture.screenshot({
    path: testInfo.outputPath(`box-drawing-${expectedDpr * 100}-percent.png`),
  });
  const continuity = await page.evaluate(async (pngBase64) => {
    const binary = window.atob(pngBase64);
    const bytes = Uint8Array.from(binary, (char) => char.charCodeAt(0));
    const bitmap = await createImageBitmap(
      new Blob([bytes], { type: "image/png" }),
    );
    const canvas = document.createElement("canvas");
    canvas.width = bitmap.width;
    canvas.height = bitmap.height;
    const context = canvas.getContext("2d", { willReadFrequently: true });
    if (!context) {
      throw new Error("2D screenshot analysis context is unavailable");
    }
    context.drawImage(bitmap, 0, 0);
    bitmap.close();
    const data = context.getImageData(0, 0, canvas.width, canvas.height).data;
    const scanHeight = Math.floor(canvas.height * 0.3);
    const scanWidth = Math.floor(canvas.width * 0.85);
    const minimumRun = Math.floor(scanHeight * 0.5);
    const qualifyingColumns: number[] = [];

    for (let x = 0; x < scanWidth; x += 1) {
      let currentRun = 0;
      let longestRun = 0;
      for (let y = 0; y < scanHeight; y += 1) {
        const offset = (y * canvas.width + x) * 4;
        const bright = Math.max(data[offset], data[offset + 1], data[offset + 2]) > 80;
        if (bright) {
          currentRun += 1;
          longestRun = Math.max(longestRun, currentRun);
        } else {
          currentRun = 0;
        }
      }
      if (longestRun >= minimumRun) {
        qualifyingColumns.push(x);
      }
    }

    const groups: Array<{ start: number; end: number }> = [];
    for (const x of qualifyingColumns) {
      const last = groups.at(-1);
      if (last && x === last.end + 1) {
        last.end = x;
      } else {
        groups.push({ start: x, end: x });
      }
    }
    return { width: canvas.width, height: canvas.height, minimumRun, groups };
  }, screenshot.toString("base64"));
  expect(continuity.groups.length).toBeGreaterThanOrEqual(3);
  await testInfo.attach(`box-drawing-${expectedDpr * 100}-percent`, {
    body: screenshot,
    contentType: "image/png",
  });
  await page.evaluate(() => {
    const target = window as Window & {
      __AGENTMUX_BOX_DRAWING_CLEANUP__?: () => void;
    };
    target.__AGENTMUX_BOX_DRAWING_CLEANUP__?.();
    delete target.__AGENTMUX_BOX_DRAWING_CLEANUP__;
  });
});

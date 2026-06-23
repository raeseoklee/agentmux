import { type CSSProperties, useState } from "react";
import type { ControlClient } from "../control/ControlClient";

const FONT_SANS =
  "'Pretendard Variable',Pretendard,-apple-system,'Segoe UI','Malgun Gothic',system-ui,sans-serif";
const FONT_MONO = FONT_SANS;

const buttonStyle: CSSProperties = {
  background: "var(--s2)",
  border: "1px solid var(--border)",
  borderRadius: 7,
  padding: "6px 10px",
  color: "var(--fg1)",
  cursor: "pointer",
  fontFamily: FONT_SANS,
  fontSize: 12,
};

const inputStyle: CSSProperties = {
  background: "var(--canvas)",
  border: "1px solid var(--border)",
  borderRadius: 7,
  padding: "6px 9px",
  color: "var(--fg1)",
  outline: "none",
  fontFamily: FONT_MONO,
  fontSize: 12,
};

type ScreenshotInfo = {
  imageHandle: string;
  byteCount: number;
  format: string;
};

type LastAction =
  | "navigate"
  | "snapshot"
  | "screenshot"
  | "click"
  | "type"
  | "evaluate"
  | null;

export function BrowserSurfacePanel({
  client,
  surfaceId,
}: {
  client: ControlClient;
  surfaceId: string;
}) {
  const [url, setUrl] = useState("https://example.invalid");
  const [selector, setSelector] = useState("#q");
  const [text, setText] = useState("agentmux");
  const [script, setScript] = useState("document.title");

  const [currentUrl, setCurrentUrl] = useState<string | null>(null);
  const [lastAction, setLastAction] = useState<LastAction>(null);
  const [snapshot, setSnapshot] = useState<string | null>(null);
  const [evalValue, setEvalValue] = useState<string | null>(null);
  const [screenshotInfo, setScreenshotInfo] = useState<ScreenshotInfo | null>(null);
  const [error, setError] = useState<string | null>(null);

  async function handleNavigate() {
    try {
      const result = await client.browserNavigate(surfaceId, url);
      setCurrentUrl(result.url);
      setLastAction("navigate");
      setError(null);
    } catch (e) {
      setError(e instanceof Error ? e.message : String(e));
    }
  }

  async function handleSnapshot() {
    try {
      const result = await client.browserDomSnapshot(surfaceId);
      setSnapshot(result.html);
      setLastAction("snapshot");
      setError(null);
    } catch (e) {
      setError(e instanceof Error ? e.message : String(e));
    }
  }

  async function handleScreenshot() {
    try {
      const result = await client.browserScreenshot(surfaceId, null);
      setScreenshotInfo({
        imageHandle: result.imageHandle,
        byteCount: result.byteCount,
        format: result.format,
      });
      setLastAction("screenshot");
      setError(null);
    } catch (e) {
      setError(e instanceof Error ? e.message : String(e));
    }
  }

  async function handleClick() {
    try {
      await client.browserClick(surfaceId, { selector });
      setLastAction("click");
      setError(null);
    } catch (e) {
      setError(e instanceof Error ? e.message : String(e));
    }
  }

  async function handleType() {
    try {
      await client.browserType(surfaceId, selector, text);
      setLastAction("type");
      setError(null);
    } catch (e) {
      setError(e instanceof Error ? e.message : String(e));
    }
  }

  async function handleEvaluate() {
    try {
      const result = await client.browserEvaluate(surfaceId, script);
      setEvalValue(result.valueJson);
      setLastAction("evaluate");
      setError(null);
    } catch (e) {
      setError(e instanceof Error ? e.message : String(e));
    }
  }

  return (
    <div
      style={{
        height: "100%",
        display: "flex",
        flexDirection: "column",
        minHeight: 0,
        background: "var(--term)",
        color: "var(--fg2)",
        fontFamily: FONT_SANS,
        fontSize: 12,
      }}
    >
      {/* Address row */}
      <div
        style={{
          display: "flex",
          gap: 6,
          padding: "8px 10px",
          borderBottom: "1px solid var(--border)",
          alignItems: "center",
          flexWrap: "wrap",
        }}
      >
        <input
          style={{ ...inputStyle, flex: 1, minWidth: 120 }}
          value={url}
          onChange={(e) => setUrl(e.target.value)}
          onKeyDown={(e) => {
            if (e.key === "Enter") {
              void handleNavigate();
            }
          }}
          placeholder="URL"
        />
        <button style={buttonStyle} onClick={() => { void handleNavigate(); }}>
          이동
        </button>
        <button style={buttonStyle} onClick={() => { void handleSnapshot(); }}>
          스냅샷
        </button>
        <button style={buttonStyle} onClick={() => { void handleScreenshot(); }}>
          스크린샷
        </button>
      </div>

      {/* Controls row */}
      <div
        style={{
          display: "flex",
          gap: 6,
          padding: "8px 10px",
          borderBottom: "1px solid var(--border)",
          alignItems: "center",
          flexWrap: "wrap",
        }}
      >
        <input
          style={{ ...inputStyle, width: 100 }}
          value={selector}
          onChange={(e) => setSelector(e.target.value)}
          placeholder="셀렉터"
        />
        <input
          style={{ ...inputStyle, flex: 1, minWidth: 80 }}
          value={text}
          onChange={(e) => setText(e.target.value)}
          placeholder="입력할 텍스트"
        />
        <button style={buttonStyle} onClick={() => { void handleClick(); }}>
          클릭
        </button>
        <button style={buttonStyle} onClick={() => { void handleType(); }}>
          입력
        </button>
        <input
          style={{ ...inputStyle, flex: 1, minWidth: 100 }}
          value={script}
          onChange={(e) => setScript(e.target.value)}
          placeholder="스크립트"
        />
        <button style={buttonStyle} onClick={() => { void handleEvaluate(); }}>
          실행
        </button>
      </div>

      {/* Output area */}
      <div
        className="agentmux-scroll"
        style={{
          flex: 1,
          overflow: "auto",
          padding: "10px 12px",
          fontFamily: FONT_MONO,
          fontSize: 12,
          display: "flex",
          flexDirection: "column",
          gap: 8,
        }}
      >
        {error !== null && (
          <div style={{ color: "var(--red, #F87171)", wordBreak: "break-all" }}>
            <strong>오류:</strong> {error}
          </div>
        )}

        {lastAction === "navigate" && currentUrl !== null && (
          <div>
            <strong style={{ color: "var(--fg1)" }}>이동 완료:</strong>{" "}
            <span>{currentUrl}</span>
          </div>
        )}

        {lastAction === "click" && error === null && (
          <div>
            <strong style={{ color: "var(--fg1)" }}>클릭 완료</strong>{" "}
            <span style={{ color: "var(--fg2)" }}>셀렉터: {selector}</span>
          </div>
        )}

        {lastAction === "type" && error === null && (
          <div>
            <strong style={{ color: "var(--fg1)" }}>입력 완료</strong>{" "}
            <span style={{ color: "var(--fg2)" }}>
              셀렉터: {selector}, 텍스트: {text}
            </span>
          </div>
        )}

        {lastAction === "screenshot" && screenshotInfo !== null && (
          <div>
            <strong style={{ color: "var(--fg1)" }}>스크린샷:</strong>{" "}
            <span>
              핸들: {screenshotInfo.imageHandle} | 포맷: {screenshotInfo.format}{" "}
              | 크기: {screenshotInfo.byteCount.toLocaleString()} bytes
            </span>
          </div>
        )}

        {lastAction === "evaluate" && evalValue !== null && (
          <div>
            <strong style={{ color: "var(--fg1)" }}>실행 결과:</strong>{" "}
            <span>{evalValue}</span>
          </div>
        )}

        {lastAction === "snapshot" && snapshot !== null && (
          <div>
            <strong style={{ color: "var(--fg1)" }}>DOM 스냅샷:</strong>
            <pre
              style={{
                marginTop: 6,
                padding: 8,
                background: "var(--surface)",
                border: "1px solid var(--border)",
                borderRadius: 6,
                overflow: "auto",
                maxHeight: 300,
                whiteSpace: "pre-wrap",
                wordBreak: "break-all",
                color: "var(--fg2)",
                fontFamily: FONT_MONO,
                fontSize: 11,
              }}
            >
              {snapshot}
            </pre>
          </div>
        )}

        {lastAction === null && error === null && (
          <div style={{ color: "var(--fg2)", opacity: 0.5 }}>
            URL을 입력하고 이동 버튼을 눌러 시작하세요.
          </div>
        )}
      </div>
    </div>
  );
}

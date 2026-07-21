const CODEX_TRANSCRIPT_TITLE = "T R A N S C R I P T";
const CODEX_MODEL_FOOTER = /\bgpt-[\w.-]+\s+(?:default|low|medium|high|xhigh)\s+[\u00b7\u2022]\s+\S/i;

/** Detect Codex from the rendered terminal surface when restored telemetry is absent. */
export function detectCodexTerminalScreen(lines: readonly string[]): boolean {
  const screen = lines.join("\n");
  if (
    /\bOpenAI Codex\s+\(v[\w.-]+\)/i.test(screen) ||
    screen.includes(CODEX_TRANSCRIPT_TITLE)
  ) {
    return true;
  }

  const hasComposer = lines.some((line) => /^\s*[>\u203a]\s+\S/.test(line));
  const hasModelFooter = lines.some((line) => CODEX_MODEL_FOOTER.test(line));
  if (hasComposer && hasModelFooter) {
    return true;
  }

  return (
    lines.some((line) => /^\s*model:\s+gpt-/i.test(line)) &&
    lines.some((line) => /^\s*directory:\s+\S/i.test(line))
  );
}

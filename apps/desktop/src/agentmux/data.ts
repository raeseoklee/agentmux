// Demo data model for the agentmux Terminal design prototype.
// Content (Korean copy, agent telemetry, layouts) is ported verbatim from the
// "agentmux Terminal.dc.html" Claude Design prototype.
import type { ThemeTokens } from "./theme";

export type WindowStatus = "input" | "running" | "done" | "idle";
export type SplitMode = "single" | "two" | "mosaic";
export type TabKind = "agent" | "shell";

export type LineKind =
  | "user"
  | "h"
  | "sec"
  | "li"
  | "dim"
  | "recap"
  | "tool"
  | "res"
  | "cmd"
  | "boot"
  | "ok"
  | "text"
  | "run";

export type RawLine = [LineKind, string];

export interface OmcStatus {
  state: "thinking" | "building" | "running" | "done";
  session: string;
  cost: string;
  tokens: string;
  cache: string;
  rate: string;
  ctx: string;
}

export interface WindowModel {
  type: TabKind;
  title: string;
  status: WindowStatus;
  model?: string;
  agent?: string;
  user?: string;
  tty?: string;
  omc?: OmcStatus;
  lines: RawLine[];
}

export interface Workspace {
  id: string;
  name: string;
  branch: string;
  path: string;
  agent: string;
  status: WindowStatus;
  statusText: string;
  tabs: string[];
}

export interface TabModel {
  title: string;
  kind: TabKind;
  split: SplitMode;
  panes: string[];
}

export interface Profile {
  name: string;
  host: string;
  user: string;
  dot: string;
}

export const WORKSPACES: Workspace[] = [
  {
    id: "haechi",
    name: "haechi-gateway",
    branch: "main",
    path: "~/work/haechi/gateway",
    agent: "Claude",
    status: "input",
    statusText: "Claude가 입력을 기다리는 중",
    tabs: ["t_review", "t_tests", "t_serve"]
  },
  {
    id: "coretext",
    name: "core-text-cpp",
    branch: "develop*",
    path: "~/work/core-text-cpp",
    agent: "Codex",
    status: "running",
    statusText: "Codex 작업 중 · 빌드 실행",
    tabs: ["t_core", "t_build"]
  },
  {
    id: "p2p",
    name: "p2p-engine",
    branch: "feat/sync",
    path: "~/work/p2p-engine",
    agent: "Claude",
    status: "running",
    statusText: "Claude 작업 중 · 동기화 테스트",
    tabs: ["t_sync", "t_logs"]
  },
  {
    id: "dash",
    name: "web-dashboard",
    branch: "fix/grid",
    path: "~/work/web-dashboard",
    agent: "Claude",
    status: "done",
    statusText: "완료 · PR #218 머지됨",
    tabs: ["t_grid"]
  }
];

export const WINDOWS: Record<string, WindowModel> = {
  w1: {
    type: "agent",
    title: "security gateway 리뷰",
    status: "input",
    model: "sonnet-4.5",
    agent: "Claude",
    omc: { state: "thinking", session: "9m", cost: "~$0.5627", tokens: "361.9k", cache: "99.7%", rate: "$3.60/h", ctx: "36%" },
    lines: [
      ["user", "암호화 게이트웨이 하드닝 — nonce 예산 가드와 명명된 보안 CI 게이트를 추가해줘"],
      ["dim", "✳ 스코핑이 그림을 바꿨습니다: 헤드라인 갭(GAP-P0-001, AAD 정규화)은 이미 닫혀 있었고, 빈 곳은 nonce 절반(GAP-P0-002)이었습니다."],
      ["h", "G1 — 키별 GCM nonce 예산 (fail-closed)"],
      ["li", "랜덤 96-bit IV는 키당 ~2³² 까지 안전(NIST SP 800-38D §8.3) → kid별 카운터, 2³²에서 fail-closed"],
      ["li", "카운터를 키 파일 usage에 예약 윈도우 단위로 미리 차감 후 영속화 → 재사용 과소집계 불가, 50%에서 1회 경고"],
      ["li", "읽기 전용 키 파일은 프로세스 단위 한도로 degrade(NONCE_BUDGET_NOPERSIST), 운영은 KMS 위임"],
      ["h", "GAP-P0-012 — 명명된 보안 CI 게이트"],
      ["li", "scripts/security-gate.mjs + 전용 security-gate 잡을 독립 required check로 승격"],
      ["li", "보안 테스트 파일이 사라지면 요란하게 실패(커버리지 은닐 축소 방지)"],
      ["sec", "검증"],
      ["li", "tests/nonce-budget.test.mjs 5개 통과 + 결합 mutation으로 KILL → 게이트 load-bearing 입증"],
      ["li", "전체 830/838 (8 env-skip), check:types 0, release:preflight green, gate:security green"],
      ["li", "문서 EN+KO(threat-model §3, operations-runbook §9), wiki key-management 갱신"],
      ["sec", "상태 / 잔여"],
      ["li", "core 코드 변경이며 미출시(버전 bump 없음) — 다음 core 릴리스에 함께 배포"],
      ["li", "잔여: G2 NFKC-on-AAD(v2 봉투), G3 스트리밍 시퀀스 AAD/replay, G4 봉투 freshness"],
      ["recap", "✺ recap: 암호화/인증 게이트웨이 하드닝 — 키별 GCM nonce 예산 가드 + 명명된 보안 CI 게이트를 main에 머지(PR #123)."],
      ["dim", "✳ Cooked for 40m 24s"]
    ]
  },
  w2: {
    type: "shell",
    title: "irae@mbp — zsh",
    status: "idle",
    tty: "ttys011",
    user: "irae@mbp",
    lines: [
      ["cmd", "git status -sb"],
      ["res", "## main...origin/main"],
      ["boot", "Last login: Thu Jun 18 19:09:54 on ttys011"]
    ]
  },
  w3: {
    type: "shell",
    title: "irae@mbp — zsh",
    status: "idle",
    tty: "ttys012",
    user: "irae@mbp",
    lines: [["boot", "Last login: Thu Jun 18 19:09:58 on ttys012"]]
  },
  w4: {
    type: "shell",
    title: "irae@mbp — zsh",
    status: "idle",
    tty: "ttys013",
    user: "irae@mbp",
    lines: [["boot", "Last login: Thu Jun 18 15:29:11 on ttys013"]]
  },
  w5: {
    type: "agent",
    title: "core 텍스트 정규화",
    status: "running",
    model: "gpt-5-codex",
    agent: "Codex",
    omc: { state: "building", session: "4m", cost: "~$0.2104", tokens: "98.2k", cache: "96.1%", rate: "$2.80/h", ctx: "22%" },
    lines: [
      ["user", "core 텍스트 정규화 경로를 NFKC 기준으로 통일하고 회귀 테스트 추가"],
      ["tool", "Edit(src/normalize.cpp)"],
      ["res", "수정됨 · 추가 34 · 삭제 11"],
      ["cmd", "cmake --build build -j8"],
      ["ok", "[100%] Built target core_text"],
      ["run", "ctest --output-on-failure 실행 중"]
    ]
  },
  w6: {
    type: "shell",
    title: "irae@mbp — zsh",
    status: "idle",
    tty: "ttys021",
    user: "irae@mbp",
    lines: [["boot", "Last login: Thu Jun 18 18:02:40 on ttys021"]]
  },
  w7: {
    type: "agent",
    title: "P2P 동기화 테스트",
    status: "running",
    model: "sonnet-4.5",
    agent: "Claude",
    omc: { state: "running", session: "12m", cost: "~$0.8810", tokens: "512.4k", cache: "98.9%", rate: "$4.10/h", ctx: "44%" },
    lines: [
      ["user", "피어 재연결 시 시퀀스 갭이 생기는 문제를 재현하고 고쳐줘"],
      ["tool", "Read(src/sync/replica.ts)"],
      ["res", "188 lines 읽음"],
      ["text", "재연결 핸드셰이크에서 last-acked 시퀀스를 보내지 않아 갭이 발생합니다. 핸드셰이크 페이로드를 확장하겠습니다."],
      ["cmd", "pnpm test sync --runInBand"],
      ["run", "3 suites 실행 중"]
    ]
  },
  w8: {
    type: "shell",
    title: "irae@mbp — zsh",
    status: "idle",
    tty: "ttys031",
    user: "irae@mbp",
    lines: [["boot", "Last login: Thu Jun 18 17:44:03 on ttys031"]]
  },
  w9: {
    type: "agent",
    title: "그리드 레이아웃 수정",
    status: "done",
    model: "sonnet-4.5",
    agent: "Claude",
    omc: { state: "done", session: "6m", cost: "~$0.3380", tokens: "142.0k", cache: "97.5%", rate: "$3.10/h", ctx: "18%" },
    lines: [
      ["user", "대시보드 그리드가 880px 아래에서 깨지는 문제 수정"],
      ["tool", "Update(src/components/Grid.tsx)"],
      ["res", "수정됨 · 추가 9 · 삭제 4"],
      ["ok", "24 passed (24) · lint clean"],
      ["text", "auto-fit minmax 로 전환했습니다. PR #218 로 머지 완료."]
    ]
  },
  w_test: {
    type: "shell",
    title: "jest --watch",
    status: "idle",
    user: "irae@mbp",
    lines: [
      ["cmd", "pnpm test --watch"],
      ["ok", "PASS  tests/nonce-budget.test.mjs"],
      ["ok", "PASS  tests/aad.test.mjs"],
      ["boot", "Watching for file changes…"]
    ]
  },
  w_lint: {
    type: "shell",
    title: "lint --watch",
    status: "idle",
    user: "irae@mbp",
    lines: [
      ["cmd", "pnpm lint --watch"],
      ["ok", "✓ 0 problems (0 errors, 0 warnings)"]
    ]
  },
  w_serve: {
    type: "shell",
    title: "dev server",
    status: "idle",
    user: "irae@mbp",
    lines: [
      ["cmd", "pnpm dev"],
      ["res", "VITE v5 ready in 412 ms"],
      ["res", "➜ Local: http://localhost:5173/"],
      ["boot", "press h + enter to show help"]
    ]
  },
  w_build: {
    type: "shell",
    title: "cmake build",
    status: "idle",
    user: "irae@mbp",
    lines: [
      ["cmd", "cmake --build build -j8"],
      ["res", "[ 84%] Building CXX core_text.dir/normalize.cpp.o"],
      ["boot", "…"]
    ]
  },
  w_logs: {
    type: "shell",
    title: "tail sync.log",
    status: "idle",
    user: "irae@mbp",
    lines: [
      ["cmd", "tail -f var/log/sync.log"],
      ["res", "19:10:02 peer reconnect seq=4471"],
      ["res", "19:10:02 gap-detected resend=12"],
      ["boot", "…"]
    ]
  }
};

export const TABS: Record<string, TabModel> = {
  t_review: { title: "security gateway 리뷰", kind: "agent", split: "mosaic", panes: ["w1", "w2", "w3", "w4"] },
  t_tests: { title: "테스트 워치", kind: "shell", split: "two", panes: ["w_test", "w_lint"] },
  t_serve: { title: "로컬 서버", kind: "shell", split: "single", panes: ["w_serve"] },
  t_core: { title: "core 텍스트 정규화", kind: "agent", split: "two", panes: ["w5", "w6"] },
  t_build: { title: "빌드 로그", kind: "shell", split: "single", panes: ["w_build"] },
  t_sync: { title: "P2P 동기화", kind: "agent", split: "two", panes: ["w7", "w8"] },
  t_logs: { title: "로그 tail", kind: "shell", split: "single", panes: ["w_logs"] },
  t_grid: { title: "그리드 수정", kind: "agent", split: "single", panes: ["w9"] }
};

export const PROFILES: Profile[] = [
  { name: "prod-server", host: "10.0.4.12", user: "deploy", dot: "#4ADE80" },
  { name: "staging-db", host: "10.0.7.3", user: "ops", dot: "#FBBF24" },
  { name: "gpu-box", host: "gpu.lan", user: "ml", dot: "#6B6B73" }
];

export const KEYMAPS: { k: string; v: string }[] = [
  { k: "커맨드 팔레트", v: "⌘ K" },
  { k: "활성 창 검색", v: "⌘ F" },
  { k: "새 창", v: "⌘ T" },
  { k: "창 닫기", v: "⌘ W" },
  { k: "2-분할", v: "⌘ D" },
  { k: "모자이크", v: "⌘ G" },
  { k: "테마 전환", v: "⌘⇧ L" },
  { k: "설정", v: "⌘ ," }
];

export function statusColor(theme: ThemeTokens, status: WindowStatus): string {
  if (status === "running") return "var(--accent)";
  if (status === "done") return theme.green;
  if (status === "input") return theme.warn;
  return theme.fg4;
}

export function statusLabel(status: WindowStatus): string {
  if (status === "running") return "실행 중";
  if (status === "done") return "완료";
  if (status === "input") return "입력 대기";
  return "대기";
}

export interface LineStyle {
  glyph: string;
  gc: string;
  tc: string;
  t: string;
  w: string;
  fs: string;
  mt: number;
  indent: number;
}

export function mapLine(theme: ThemeTokens, line: RawLine): LineStyle {
  const [kind, t] = line;
  const base: LineStyle = { glyph: "", gc: theme.fg4, tc: theme.fg2, t, w: "400", fs: "normal", mt: 0, indent: 0 };
  switch (kind) {
    case "user":
      return { ...base, glyph: "›", gc: "var(--accent)", tc: theme.fg1, w: "600", mt: 2 };
    case "h":
      return { ...base, tc: theme.fg1, w: "700", mt: 12 };
    case "sec":
      return { ...base, tc: theme.fg1, w: "700", mt: 14 };
    case "li":
      return { ...base, glyph: "–", gc: theme.fg4, tc: theme.fg2, indent: 2 };
    case "dim":
      return { ...base, tc: theme.fg4, mt: 8 };
    case "recap":
      return { ...base, tc: theme.fg4, fs: "italic", mt: 10 };
    case "tool":
      return { ...base, glyph: "⏺", gc: "var(--accent)", tc: theme.fg1, w: "500", mt: 2 };
    case "res":
      return { ...base, glyph: "⎿", gc: theme.fg4, tc: theme.fg3, indent: 14 };
    case "cmd":
      return { ...base, glyph: "$", gc: theme.fg4, tc: theme.fg1, w: "500", mt: 2 };
    case "boot":
      return { ...base, tc: theme.fg4 };
    case "ok":
      return { ...base, glyph: "✓", gc: theme.green, tc: theme.fg2, indent: 14 };
    case "text":
      return { ...base, tc: theme.fg2, indent: 14, mt: 2 };
    case "run":
      return { ...base, tc: theme.fg2, indent: 14, mt: 2 };
    default:
      return base;
  }
}

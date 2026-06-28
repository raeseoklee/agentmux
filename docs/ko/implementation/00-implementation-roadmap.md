# Implementation Roadmap

Status: Draft
Date: 2026-06-18

이 로드맵은 AgentMux를 당분간 Windows-only AI-agent multiplexer로 구현하기 위한 단계별 실행 계획이다. Linux/macOS 네이티브 데스크톱 지원은 플랫폼 backlog로 분리하고, WSL은 Windows 제품의 핵심 Linux 개발 환경 통합으로 다룬다. 각 단계는 독립적으로 검증 가능한 산출물을 가져야 하며, 다음 단계는 이전 단계의 exit criteria가 충족된 뒤 진행한다.

## Phase 0: Repository Foundation

목표: 코드 작성 전에 반복 가능한 빌드, 테스트, 문서, release hygiene을 준비한다.

작업:

- Rust workspace와 desktop app workspace를 만든다.
- formatter, linter, unit test, integration test entrypoint를 만든다.
- Windows 개발 환경 bootstrap 문서를 추가한다.
- CI skeleton을 만든다.
- requirement ID와 implementation issue를 연결하는 convention을 정한다.
- `docs/implementation` 문서와 코드 구조를 맞춘다.

Exit criteria:

- `cargo test` 또는 동등한 core test command가 빈 테스트라도 실행된다.
- desktop app dev command가 placeholder 화면을 띄운다.
- CI가 checkout, install, build placeholder, test placeholder를 수행한다.
- 저장소 루트에 개발자 bootstrap 문서가 있다.

관련 요구사항:

- PR-011, PR-012
- RR-010

## Phase 1: Single Native Terminal Slice

목표: Windows native shell 하나를 pane에 띄우고 입력, 출력, resize, 종료를 end-to-end로 검증한다.

작업:

- core runtime process를 만든다.
- ConPTY backend adapter를 만든다.
- terminal session model을 만든다.
- UI terminal surface를 만든다.
- keyboard input을 focused session으로 라우팅한다.
- resize event를 backend로 전달한다.
- session 종료와 exit code를 UI에 표시한다.

Exit criteria:

- PowerShell 또는 cmd session을 앱에서 시작할 수 있다.
- 사용자가 명령을 입력하고 결과를 볼 수 있다.
- pane resize가 terminal dimensions에 반영된다.
- session 종료 상태가 UI와 API에 표시된다.
- 기본 keystroke latency benchmark가 실행된다.

관련 요구사항:

- FR-004, FR-011, FR-012, FR-034
- PR-003, PR-007

## Phase 2: WSL Direct Shell Slice

목표: 선택한 WSL distribution에서 shell과 agent process를 실행할 수 있게 한다.

작업:

- WSL distribution discovery를 구현한다.
- WSL launch command builder를 만든다.
- Windows path와 WSL path 처리 정책을 구현한다.
- project working directory resolution을 구현한다.
- WSL session diagnostics를 추가한다.

Exit criteria:

- 사용자가 WSL distribution을 선택할 수 있다.
- WSL shell이 project directory에서 열린다.
- WSL 미설치, distribution 없음, working directory 변환 실패가 구분된 오류로 표시된다.
- WSL direct shell도 input, output, resize, 종료가 동작한다.

관련 요구사항:

- FR-005, FR-031, FR-033, FR-034
- CR-001, CR-002

## Phase 3: Durable WSL Session Backend

목표: WSL 내부에서 durable session을 만들고 UI restart 후 재연결할 수 있게 한다.

작업:

- tmux-control process launcher를 구현한다.
- tmux-control parser fixture를 만든다.
- session create, attach, detach, list, recover command를 구현한다.
- backend pane id와 AgentMux session id mapping을 저장한다.
- UI restart 후 duplicate process 없이 reattach한다.
- scrollback 또는 recent output snapshot 복구를 구현한다.

Exit criteria:

- WSL durable session이 앱 종료 후에도 유지된다.
- 앱 재시작 후 기존 session에 재연결된다.
- duplicate shell 또는 duplicate agent process가 생기지 않는다.
- tmux-control parser가 fixture 기반 unit test를 통과한다.

관련 요구사항:

- FR-006, FR-007, FR-008, FR-009, FR-010
- RR-001, RR-002, RR-003

## Phase 4: Workspaces, Panes, and Persistence

목표: 여러 session을 workspace와 layout으로 관리한다.

작업:

- workspace CRUD를 구현한다.
- split pane layout tree를 구현한다.
- surface와 session을 분리한 data model을 구현한다.
- layout persistence를 SQLite에 저장한다.
- safe close flow를 구현한다.
- workspace switch latency를 측정한다.

Exit criteria:

- 사용자가 workspace를 만들고 이름을 바꿀 수 있다.
- vertical/horizontal split이 가능하다.
- session을 다른 pane에 mount하거나 unmount할 수 있다.
- 앱 재시작 후 layout이 복구된다.
- hidden pane은 terminal renderer가 unmounted된다.

관련 요구사항:

- FR-001, FR-002, FR-003, FR-008, FR-035
- PR-005, PR-006, PR-009

## Phase 5: Control Plane API and CLI

목표: 외부 자동화가 workspace, pane, session을 안정적으로 제어할 수 있게 한다.

작업:

- local IPC server를 구현한다.
- request/response envelope와 error code를 고정한다.
- workspace, pane, session API를 구현한다.
- event subscription 또는 polling API를 구현한다.
- CLI wrapper를 구현한다.
- API compatibility test를 만든다.

Exit criteria:

- CLI로 workspace와 session을 생성할 수 있다.
- CLI로 focused pane에 text/key를 보낼 수 있다.
- API로 recent output을 읽을 수 있다.
- event stream으로 session state 변화를 받을 수 있다.
- 잘못된 token, 잘못된 id, 지원하지 않는 backend 오류가 구분된다.

관련 요구사항:

- FR-014, FR-015, FR-016, FR-017, FR-018
- SR-001, SR-002, SR-003

## Phase 6: Agent Lifecycle and Notifications

목표: 여러 AI agent를 동시에 돌릴 때 attention, completion, failure를 UI와 API에서 빠르게 파악하게 한다.

작업:

- agent lifecycle state machine을 구현한다.
- shell hook 또는 marker protocol을 정의한다.
- output heuristic은 optional detector로 분리한다.
- desktop notification adapter를 만든다.
- notification history panel을 만든다.
- API event에 agent state transition을 포함한다.

Exit criteria:

- running, waiting_for_input, completed, failed, detached 상태가 표시된다.
- 여러 session 중 attention 필요한 session이 workspace overview에 표시된다.
- notification history에서 최근 event를 확인할 수 있다.
- lifecycle detector 오탐이 session 제어를 직접 수행하지 않는다.

관련 요구사항:

- FR-020, FR-021, FR-022, FR-023, FR-024, FR-025
- UR-001, UR-002

## Phase 7: Browser Surface and Automation

목표: agent workflow에서 browser surface를 pane으로 다룰 수 있게 한다.

작업:

- browser surface model을 추가한다.
- browser process/profile ownership 정책을 정한다.
- navigation, screenshot, DOM snapshot, click, type, evaluate API를 구현한다.
- browser surface가 terminal session과 같은 pane layout에 mount되게 한다.
- local development server workflow를 검증한다.

Exit criteria:

- 사용자가 pane에 browser surface를 만들 수 있다.
- API로 browser를 탐색하고 screenshot을 받을 수 있다.
- browser automation이 지정된 surface를 벗어나지 않는다.
- browser crash가 core runtime 전체를 죽이지 않는다.

관련 요구사항:

- FR-026, FR-027, FR-028
- SR-008, RR-008

## Phase 8: Performance Hardening and Release Candidate

목표: 다중 session 부하에서 사용자 경험과 안정성을 release 가능한 수준으로 만든다.

작업:

- 20 idle session benchmark를 CI 또는 release gate에 넣는다.
- 50 idle session local benchmark를 만든다.
- high-output stress test를 만든다.
- frame scheduling과 output batching을 조정한다.
- crash recovery와 diagnostics를 검증한다.
- installer, updater, signing, packaging checklist를 작성한다.

Exit criteria:

- 기준 Windows laptop에서 20 idle session을 안정적으로 유지한다.
- visible pane input latency와 workspace switch latency가 budget 안에 들어온다.
- high-output session이 UI 전체를 멈추게 하지 않는다.
- release checklist가 모두 통과한다.

관련 요구사항:

- PR-001 ~ PR-012
- RR-001 ~ RR-010

## 구현 순서의 핵심 이유

1. Native terminal slice가 없으면 UI, IPC, rendering latency의 baseline을 만들 수 없다.
2. WSL direct slice가 있어야 Windows 사용자에게 즉시 가치가 생긴다.
3. Durable backend는 구조적 핵심이므로 workspace/pane 확장 전에 검증한다.
4. Control plane은 UI 구현 뒤에 붙이는 부가기능이 아니라 agent multiplexer의 제품 표면이다.
5. Performance hardening은 마지막 단계이지만 benchmark harness는 Phase 1부터 존재해야 한다.


# Implementation Documents

Status: Draft
Date: 2026-06-18

이 문서 세트는 AgentMux를 실제 제품 코드로 구현하기 위한 실행 기준이다. 상위 요구사항 문서는 "무엇을 만족해야 하는가"를 정의하고, 이 디렉터리는 "어떤 순서와 경계로 구현할 것인가"를 정의한다.

## 기준 결정

| 영역 | 기준 선택 | 이유 |
|---|---|---|
| Core runtime | Rust | PTY, IPC, 스트림 처리, 백프레셔, Windows API 연동에서 예측 가능한 성능과 메모리 제어가 필요하다. |
| Desktop shell | Tauri 계열 구조 | Windows 배포 크기와 native API 접근성을 우선한다. UI는 웹 기술을 쓰되 core는 Rust에 둔다. |
| UI | TypeScript + React | 복잡한 pane layout, 상태 관리, terminal renderer 통합을 빠르게 구현하기 좋다. |
| Terminal renderer | xterm.js adapter 우선 | MVP에서 완전한 터미널 에뮬레이터를 직접 구현하지 않고 검증된 렌더러를 감싼다. |
| Async runtime | Tokio | 세션별 IO task, bounded channel, timer, IPC 서버를 일관되게 처리한다. |
| Local IPC | Windows named pipe + JSON-RPC 스타일 envelope | 로컬 제어 API를 안정적으로 versioning하고 CLI/MCP 연결을 단순화한다. |
| Persistent state | SQLite WAL | workspace, pane, session metadata, event cursor를 crash-safe하게 저장한다. 대용량 terminal output은 별도 ring/snapshot 전략을 둔다. |
| Windows native shell | ConPTY | PowerShell, cmd, native CLI 실행을 지원한다. |
| WSL shell | WSL launcher + optional tmux-control backend | Linux 개발 환경과 durable session을 Windows 데스크톱에서 직접 제어한다. |

## 읽는 순서

1. [00-implementation-roadmap.md](./00-implementation-roadmap.md)
2. [01-runtime-architecture.md](./01-runtime-architecture.md)
3. [02-windows-wsl-session-backends.md](./02-windows-wsl-session-backends.md)
4. [03-control-plane-api.md](./03-control-plane-api.md)
5. [04-ui-terminal-rendering.md](./04-ui-terminal-rendering.md)
6. [05-performance-and-observability.md](./05-performance-and-observability.md)
7. [06-testing-and-release-plan.md](./06-testing-and-release-plan.md)
8. [07-repo-scaffold-and-first-tasks.md](./07-repo-scaffold-and-first-tasks.md)
9. [08-goal-groups.md](./08-goal-groups.md)
10. [09-goal-1-native-terminal-slice-status.md](./09-goal-1-native-terminal-slice-status.md)
11. [10-goal-2-persistence-status.md](./10-goal-2-persistence-status.md)
12. [11-goal-3-wsl-direct-status.md](./11-goal-3-wsl-direct-status.md)
13. [12-goal-4-durable-tmux-status.md](./12-goal-4-durable-tmux-status.md)
14. [13-goal-5-workspace-pane-ux-status.md](./13-goal-5-workspace-pane-ux-status.md)
15. [14-goal-6-control-plane-cli-status.md](./14-goal-6-control-plane-cli-status.md)
16. [15-goal-7-agent-notifications-status.md](./15-goal-7-agent-notifications-status.md)
17. [16-goal-8-browser-surface-automation-status.md](./16-goal-8-browser-surface-automation-status.md)
18. [17-goal-9-performance-diagnostics-status.md](./17-goal-9-performance-diagnostics-status.md)
19. [18-goal-9-release-candidate-checklist.md](./18-goal-9-release-candidate-checklist.md)
20. [19-cmux-windows-parity-gap-analysis.md](./19-cmux-windows-parity-gap-analysis.md)
21. [20-goal-10-setup-config-status.md](./20-goal-10-setup-config-status.md)
22. [21-goal-11-action-registry-status.md](./21-goal-11-action-registry-status.md)
23. [22-goal-12-cli-sidebar-metadata-status.md](./22-goal-12-cli-sidebar-metadata-status.md)
24. [23-overall-completion-goal-groups.md](./23-overall-completion-goal-groups.md)
25. [24-goal-14-tmux-compat-status.md](./24-goal-14-tmux-compat-status.md)
26. [25-goal-15-workspace-groups-status.md](./25-goal-15-workspace-groups-status.md)
27. [26-goal-16-server-mode-web-terminal-status.md](./26-goal-16-server-mode-web-terminal-status.md)
28. [28-goal-18-p2-installed-server-mode-release-gate.md](./28-goal-18-p2-installed-server-mode-release-gate.md)
29. [29-installed-lifecycle-e2e-release-closure.md](./29-installed-lifecycle-e2e-release-closure.md)

## 구현 원칙

- Vertical slice를 우선한다. "단일 terminal을 띄우고 입력/출력/resize/종료가 된다" 같은 end-to-end 경로를 먼저 완성한다.
- backend와 UI는 직접 결합하지 않는다. UI는 core API와 event stream만 본다.
- session은 pane보다 오래 살아야 한다. pane은 표시 객체이고 session은 실행 객체다.
- hidden pane은 렌더링 비용을 내지 않는다. 출력 수집과 scrollback 저장만 bounded하게 유지한다.
- 성능 요구사항은 개발 초기에 자동화한다. 나중에 최적화하려고 남겨두면 구조가 이미 굳는다.
- Windows 전용 문제는 추상화 뒤에 숨기되, 디버깅 가능한 diagnostics를 반드시 노출한다.

## 산출물 기준

구현 단계에서 각 기능 PR은 다음을 남겨야 한다.

- 변경된 requirement ID 또는 roadmap phase.
- 사용자-visible 동작.
- 관련 backend, IPC, UI boundary.
- 테스트 이름 또는 수동 검증 절차.
- 성능 영향이 있으면 benchmark 결과 또는 측정 불가 사유.


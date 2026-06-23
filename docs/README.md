# AgentMux Documentation

Status: Draft
Date: 2026-06-18

이 디렉터리는 AgentMux의 제품 요구사항, 상세 설계, 구현 계획을 관리한다.

## 문서 구조

| 문서 | 목적 |
|---|---|
| [features.md](./features.md) | 사용자 제공 기능 목록과 성숙도(정식/부분/준비중) |
| [ieee-29148-system-design.md](./ieee-29148-system-design.md) | IEEE 29148 형식을 따른 상위 요구사항 및 상세 설계 |
| [implementation/README.md](./implementation/README.md) | 실제 구현 문서의 읽는 순서와 기준 결정 |
| [implementation/00-implementation-roadmap.md](./implementation/00-implementation-roadmap.md) | 단계별 구현 로드맵과 완료 기준 |
| [implementation/01-runtime-architecture.md](./implementation/01-runtime-architecture.md) | 런타임, 프로세스, 모듈, 상태 저장 아키텍처 |
| [implementation/02-windows-wsl-session-backends.md](./implementation/02-windows-wsl-session-backends.md) | Windows, WSL, tmux-control 세션 백엔드 설계 |
| [implementation/03-control-plane-api.md](./implementation/03-control-plane-api.md) | 로컬 IPC, CLI, 이벤트 API 설계 |
| [implementation/04-ui-terminal-rendering.md](./implementation/04-ui-terminal-rendering.md) | UI, 터미널 렌더링, 레이아웃, 알림 설계 |
| [implementation/05-performance-and-observability.md](./implementation/05-performance-and-observability.md) | 성능 예산, 벤치마크, 관측성, 병목 대응 |
| [implementation/06-testing-and-release-plan.md](./implementation/06-testing-and-release-plan.md) | 테스트 전략, 릴리스 게이트, 검증 환경 |
| [implementation/07-repo-scaffold-and-first-tasks.md](./implementation/07-repo-scaffold-and-first-tasks.md) | 저장소 구조와 최초 구현 작업 목록 |
| [implementation/19-cmux-windows-parity-gap-analysis.md](./implementation/19-cmux-windows-parity-gap-analysis.md) | cmux 공식 문서 기준 Windows parity gap 분석과 후속 goal 그룹 |
| [implementation/20-goal-10-setup-config-status.md](./implementation/20-goal-10-setup-config-status.md) | Windows setup/config foundation 구현 상태 |
| [implementation/21-goal-11-action-registry-status.md](./implementation/21-goal-11-action-registry-status.md) | Action registry, shortcuts, command palette 구현 상태 |
| [implementation/22-goal-12-cli-sidebar-metadata-status.md](./implementation/22-goal-12-cli-sidebar-metadata-status.md) | CLI/sidebar metadata compatibility 구현 상태 |
| [implementation/23-overall-completion-goal-groups.md](./implementation/23-overall-completion-goal-groups.md) | Windows cmux parity 전체 완성을 위한 남은 구현 그룹 |
| [implementation/26-goal-16-server-mode-web-terminal-status.md](./implementation/26-goal-16-server-mode-web-terminal-status.md) | agentmux.exe server mode와 웹 터미널 접근 구현 상태 |

## 갱신 원칙

- 요구사항 변경은 상위 설계 문서의 요구사항 ID를 먼저 갱신한다.
- 구현 방식 변경은 implementation 문서에 근거와 영향 범위를 기록한다.
- 성능 관련 주장은 벤치마크 이름, 기준 하드웨어, 측정 방법을 함께 남긴다.
- 백엔드 동작은 Windows 실제 환경에서 검증된 사실과 설계 가정을 분리해 기록한다.


# AgentMux 기능 개요

Status: Draft

영문 문서가 기준 문서입니다. 이 문서는 사용자가 현재 공개 빌드에서
기대할 수 있는 기능을 한국어로 요약합니다.

AgentMux는 Windows 전용 AI 에이전트 워크플로 터미널 멀티플렉서입니다.
여러 터미널, 에이전트, 브라우저 표면, 워크스페이스 상태를 한 앱에서
관찰하고 복구하는 것을 핵심 가치로 둡니다.

현재 제품 범위는 Windows-only입니다. macOS와 Linux 네이티브 데스크톱
지원은 [platform backlog](../en/backlog/platform-backlog.md)에서 관리합니다.
WSL은 Windows 제품 안에서 Linux 개발 워크플로를 지원하기 위한 기능입니다.

## 터미널과 세션 실행

- ConPTY 기반 Windows PowerShell, Command Prompt, 기타 Windows 셸 실행.
- WSL 배포판 탐색과 Windows 경로를 WSL 경로로 변환하는 WSL 직접 셸 실행.
- 장시간 실행되는 에이전트 작업을 위한 WSL tmux 기반 durable 세션.
- 워크스페이스, 탭, 분할 pane, 세션 메타데이터를 SQLite에 저장.

## 에이전트 워크플로

- 액션, 명령 팔레트, CLI를 통한 에이전트 실행.
- running, waiting for input, completed, failed 상태 감지.
- 입력 대기나 실패처럼 개입이 필요한 에이전트를 워크스페이스, pane,
  알림, OS 알림으로 표시.
- 앱 재시작 후 가능한 경우 에이전트 실행 명령과 작업 디렉터리를 복원.

## 워크스페이스 레이아웃

- 워크스페이스, 상단 탭, split pane 지원.
- 탭은 각자 독립적인 pane 레이아웃을 소유하며, 새 탭 추가가 기존 split
  트리를 변경하지 않음.
- pane split, resize, focus, close, surface mount/unmount.
- 워크스페이스, 탭, pane의 이동과 순서 변경.

## 브라우저와 자동화

- 터미널 옆에 브라우저 surface를 split pane으로 열기.
- 현재 선택 영역, 알림, 팀 메시지, 팀 task, attention reason에서 URL을
  찾아 AgentMux 브라우저 탭으로 여는 `browser.openContextLink` 액션.
- 터미널 출력의 OSC 8 링크와 일반 `http://` / `https://` URL을 Windows에서
  Ctrl-click으로 열기.
- CDP 기반 브라우저 자동화: navigate, screenshot, DOM snapshot, click,
  type, evaluate.

## 제어 평면과 CLI

- `agentmux` CLI로 workspace, session, pane, notification, browser, action,
  diagnostics, config 워크플로 제어.
- 데스크톱 자동화를 위한 Windows named-pipe control plane.
- 에이전트와 외부 도구를 위한 이벤트 polling/subscription API.
- cmux 호환 명령은 AgentMux 전환을 돕는 compatibility surface로 제공되며,
  독립 제품인 cmux를 AgentMux 기능으로 광고하지 않음.

## 설정과 운영

- 설정 파일은 JSON 기반으로 저장하며, 앱 설정 화면에서 언어와 주요 동작을
  변경할 수 있음.
- 기본 언어는 English이고 한국어를 선택할 수 있음.
- GitHub Releases 기반 Windows 업데이트 채널을 사용.
- 운영 문서는 [English documentation](../en/README.md)을 기준으로 관리.

## 알려진 제약과 백로그

- 현재 릴리스는 Windows NSIS 설치 파일만 배포합니다.
- native macOS/Linux 앱, cross-platform release matrix, non-Windows terminal
  backend parity는 백로그입니다.
- SSH, 고급 브라우저 자동화, 세션 복원 UX, 에이전트 협업 대시보드는 계속
  개선 중인 영역입니다.

자세한 최신 범위는 [English feature overview](../en/features.md)와
[user manual](../en/user/manual.md)을 확인하세요.

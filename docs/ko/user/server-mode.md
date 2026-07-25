# AgentMux Server Mode

AgentMux Server mode는 Windows 데스크톱 앱과 같은 React Workspace UI를
인증된 로컬 웹 주소로 제공합니다. 별도의 축소형 웹 UI를 운영하지 않습니다.

## 로컬 Server 실행

```powershell
agentmux server --port 8765
```

기본적으로 loopback에만 연결되며, 실행 결과에 로컬 URL과 인증 token이
표시됩니다. `--cwd`를 지정하지 않으면 명령을 실행한 디렉터리가 첫 프로젝트
경로가 됩니다.

```powershell
agentmux server --port 8765 --cwd D:\work\project
```

## 실행 모드

### Local mode

Server 프로세스가 Session을 직접 소유합니다. WSL direct, PowerShell,
Command Prompt를 지원하며, WSL은 데스크톱과 같은 login-shell bootstrap,
사용자 shell, 작업 디렉터리 추적, zsh/Powerlevel10k 호환 환경을 사용합니다.

Source Control 요청에는 선택한 Pane의 cwd와 backend profile이 포함됩니다.
따라서 Pane을 이동하면 해당 Pane 기준으로 Git branch, commit, repository
경로와 변경 파일 목록이 갱신됩니다.

### Desktop-bridge mode

```powershell
agentmux server --desktop-control --port 8765
```

지원되는 요청을 실행 중인 AgentMux 데스크톱 control plane으로 전달합니다.
데스크톱이 소유한 Workspace, durable WSL-tmux Session, 저장된 레이아웃을
웹에서 제어해야 할 때 사용합니다.

## 현재 동일 동작 범위

웹과 데스크톱은 Workspace, Tab, Pane, Terminal, Source Control, 설정,
다국어 UI component를 공유합니다. 다만 Windows 데스크톱 host가 소유해야
하는 기능에는 다음 차이가 있습니다.

- durable WSL-tmux 연결과 저장된 Workspace 레이아웃은 desktop bridge 필요
- 내장 Browser surface는 데스크톱 Browser host가 소유
- tray 알림, updater, native window control은 데스크톱 전용
- local mode 재시작 시 server-owned runtime은 새로 시작

선택한 Server mode가 제공하지 않는 기능은 UI에서 숨기거나 비활성화해야
하며, 다른 backend로 조용히 대체하지 않습니다.

## 보안

- 일반 사용에서는 기본 loopback 연결을 유지하세요.
- bearer token을 로그, 스크린샷, 이슈에 포함하지 마세요.
- 원격 연결은 명시적인 허용 정책과 신뢰할 수 있는 TLS reverse proxy가
  필요합니다.
- MCP HTTP는 allowed host와 origin 검사를 추가로 수행합니다. 자세한 내용은
  [MCP 안내](./mcp.md)를 확인하세요.

영문 기준 문서는 [Server mode guide](../../en/user/server-mode.md)입니다.

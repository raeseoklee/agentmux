# AgentMux MCP 제어 플레인

AgentMux는 Codex, Claude Code 및 기타 MCP 클라이언트가 실행 중인 AgentMux
데스크톱을 제어할 수 있도록 로컬 Model Context Protocol(MCP) 서버를
제공합니다. 로컬 stdio MCP 서버는 인증된 Windows named pipe 제어 플레인에
연결됩니다.

## 사용 가능한 명령

설치된 `agentmux.exe`는 다음 명령을 제공합니다.

```powershell
agentmux mcp serve --help
agentmux mcp doctor --help
agentmux mcp setup --help
```

`mcp serve`와 정상 상태의 `mcp doctor`는 AgentMux 데스크톱이 실행 중이어야
합니다. 도움말과 `mcp setup` 미리보기는 데스크톱을 실행하지 않아도 됩니다.

## 최소 권한 프로필

작업에 필요한 가장 낮은 권한부터 사용합니다.

| 프로필 | 권한 | 권장 용도 |
| --- | --- | --- |
| `read` | 워크스페이스, 세션, 에이전트 주의 상태, 팀 메시지와 태스크 조회, 터미널 출력, 브라우저 스냅샷, 이벤트, 컨텍스트 및 진단 읽기 | 모니터링과 최초 설정 |
| `standard` | `read` 권한과 함께 페인 포커스, 터미널 열기/분할/입력, 브라우저 열기/이동/클릭/입력, 팀 메시지와 태스크 갱신, 에이전트 상태 갱신. 임의 명령 실행과 외부 시스템 변경이 가능한 신뢰된 쓰기 프로필 | 명령 실행이나 쓰기가 필요한 신뢰된 대화형 에이전트 작업 |
| `full` | `standard` 권한과 함께 워크스페이스/페인/서피스 닫기, 세션 종료, 설정 변경, 브라우저 JavaScript 실행, 액션 실행, 알림 삭제 | 파괴적이거나 영향이 큰 작업이 필요한 신뢰된 자동화 |

`standard`는 안전하거나 비파괴적인 프로필이 아닙니다. 명령 실행과 터미널,
브라우저, 공유 협업 상태 변경을 허용할 수 있는 클라이언트에만 부여하세요.
명령·입력 및 잠재적으로 파괴적인 외부 변경 도구에는 위험 annotation이
표시되지만, 이 annotation 자체가 샌드박스를 제공하지는 않습니다. 편의를
이유로 `full`을 기본값으로 사용하지 마세요. 변경 MCP 호출은 제어 플레인
진단 감사 기록에 MCP 프로필과 함께 남습니다.

## 로컬 stdio 서버 실행

```powershell
agentmux mcp serve --profile read
```

기본 제어 pipe와 토큰 파일은 자동으로 검색됩니다. 필요하면 다음 옵션으로
재정의할 수 있습니다.

```powershell
agentmux mcp serve --profile standard `
  --pipe agentmux-control `
  --token-file "$env:LOCALAPPDATA\AgentMux\control.token"
```

`--token`과 `--token-file`은 동시에 사용할 수 없습니다. 환경 변수
`AGENTMUX_CONTROL_PIPE`, `AGENTMUX_CONTROL_TOKEN`,
`AGENTMUX_CONTROL_TOKEN_PATH`도 지원합니다. 토큰 값을 저장소의 클라이언트
설정에 직접 기록하지 않는 것이 좋습니다.

## 연결 진단

데스크톱 실행 후 다음 명령을 사용합니다.

```powershell
agentmux mcp doctor --profile read --json
```

결과에는 stdio 전송, 프로필, 제어 pipe, 토큰 출처, 제어 플레인 연결 상태,
스키마와 오류가 포함됩니다. 종료 코드가 0이 아니면 데스크톱 실행 및 현재
Windows 계정의 제어 토큰 접근 권한을 확인하세요.

## Codex 설정

Codex는 기본적으로 `%USERPROFILE%\.codex\config.toml` 또는
`%CODEX_HOME%\config.toml`의 TOML 설정을 사용합니다.

```powershell
agentmux mcp setup --client codex --profile read --json
agentmux mcp setup --client codex --profile standard --install --json
```

첫 번째 명령은 변경 내용을 미리보기만 하고, 두 번째 명령은 검토한 설정을
적용합니다.

## Claude Code 설정

Claude Code는 기본적으로 `%USERPROFILE%\.claude.json` JSON 설정을
사용합니다.

```powershell
agentmux mcp setup --client claude --profile read --json
agentmux mcp setup --client claude --profile standard --install --json
```

`--install`이 없으면 설정 파일을 수정하지 않습니다. 설치 시 기존의 관련
없는 설정을 유지하고 `agentmux` 이름 충돌을 거부합니다. 미리보기 이후
Codex나 Claude가 설정을 변경했을 수 있으므로 최신 파일을 다시 읽어
재병합한 뒤 비교·교체하며, 실제 교체되는 최신 스냅샷을 타임스탬프 백업으로
남깁니다. 비교·교체 중 파일이 계속 변경되면 다른 클라이언트의 내용을
덮어쓰지 않고 설치를 중단합니다.

기본 위치가 아닌 설정 파일에는 `--config <path>`를 사용하고, 다른 설치본을
등록하려면 `--executable <agentmux.exe의 절대 경로>`를 사용합니다.

## 원격 Streamable HTTP

먼저 데스크톱을 실행한 뒤 desktop-bridge 서버 모드에서 인증형 Streamable
HTTP 엔드포인트를 명시적으로 활성화합니다.

```powershell
agentmux server --desktop-control --port 8765 `
  --mcp-http --mcp-port 8766 --mcp-profile standard --json
```

시작 결과 JSON에는 `/mcp` URL과 생성된 `auth_token`이 포함됩니다. MCP
클라이언트는 `Authorization: Bearer <auth_token>` 헤더를 전송합니다. 새 MCP
세션은 기본적으로 `read`이며, `X-AgentMux-Mcp-Profile` 헤더로
`--mcp-profile` 상한 이내의 `standard` 또는 `full`을 요청할 수 있습니다.

기본 바인딩은 루프백입니다. 외부 주소에는 `--allow-remote`와 하나 이상의
정확한 `--mcp-allowed-host`가 필요하고, 브라우저 Origin을 사용하는
클라이언트는 반복 가능한 `--mcp-allowed-origin`도 지정해야 합니다. MCP
세션 ID는 인증된 토큰 주체와 프로필에 묶여 다른 권한으로 재사용할 수 없습니다.

## 설치 파일 검증

릴리스 CI는 NSIS 설치 파일을 격리된 경로에 무인 설치한 뒤 패키지에 포함된
`agentmux.exe`의 MCP 도움말과 Codex/Claude 설정 미리보기를 검사합니다.
이 검사는 데스크톱을 실행하지 않습니다.

수동으로 다음 비파괴 검사를 수행할 수 있습니다.

```powershell
agentmux mcp help
agentmux mcp doctor --help
agentmux mcp setup --client codex --profile read --json
```

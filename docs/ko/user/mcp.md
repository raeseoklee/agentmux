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

`mcp serve`는 AgentMux 데스크톱이 없어도 초기화되고 `initialize`, `tools/list`
같은 프로토콜 탐색 요청에 응답할 수 있습니다. 데스크톱 상태를 조회하거나
변경하는 도구는 데스크톱 제어 플레인이 실행 중이어야 합니다. 정상 상태의
`mcp doctor`에도 이 연결이 필요합니다. 도움말과 `mcp setup` 미리보기는
데스크톱을 실행하지 않아도 됩니다.

## 최소 권한 프로필

작업에 필요한 가장 낮은 권한부터 사용합니다.

| 프로필 | 권한 | 권장 용도 |
| --- | --- | --- |
| `read` | 워크스페이스, 세션, 에이전트 주의 상태, 페인 워커, 통합 준비 상태, worktree 작업, Git 상태/diff/review, 개발 서버 후보, 팀 메시지와 태스크 조회, 터미널 출력, 브라우저 스냅샷, 이벤트, 컨텍스트 및 진단 읽기 | 모니터링, 검토와 최초 설정 |
| `standard` | `read` 권한과 함께 페인 포커스, 터미널 열기/분할/입력, 페인 워커 시작/전달, worktree 생성/복구, 호출자의 고정된 페인·저장소 범위에서 Git 스테이징/스테이징 해제/non-amend 커밋, 호출자 소유 리뷰 작성/전달/댓글 삭제, 개발 서버 분할 열기, 브라우저 조작, 팀 메시지와 태스크 갱신, 에이전트 상태 갱신 | 명령 실행이나 호출자의 고정된 컨텍스트에 제한된 쓰기가 필요한 신뢰된 대화형 에이전트 작업 |
| `full` | `standard` 권한과 함께 워커 회수, worktree 제거, 저장소 전체 Git 스테이징/스테이징 해제, 변경 폐기, 관리형 커밋 권한, 리뷰 스레드 삭제, 통합 shim 설치, 워크스페이스/페인/서피스 닫기, 세션 종료, 설정 변경, 브라우저 JavaScript 실행, 액션 실행, 알림 삭제 | 파괴적이거나 컨텍스트를 넘나들거나 영향이 큰 작업이 필요한 신뢰된 자동화 |

`standard`는 안전하거나 비파괴적인 프로필이 아닙니다. 명령 실행과 터미널,
브라우저, 공유 협업 상태 변경을 허용할 수 있는 클라이언트에만 부여하세요.
프로필은 모든 MCP 도구 호출에서 강제되지만, 같은 Windows 사용자 권한으로
실행되는 다른 프로세스를 막는 보안 샌드박스가 아니라 MCP 도구 표면 정책입니다.
임의 터미널 명령을 실행할 수 있는 클라이언트는 해당 사용자의 권한으로 다른
프로그램을 호출할 수 있습니다. 도구 annotation은 위험을 설명할 뿐 격리를
제공하지 않습니다. 편의를 이유로 `full`을 기본값으로 사용하지 마세요.

데스크톱 제어 플레인 호출은 로컬 토큰을 사용하며, 이 경로로 전달된 변경은
진단 감사 기록에 MCP 프로필과 함께 남습니다. `agent_integration_setup`은 MCP
서버가 직접 수행하는 예외 작업이므로 데스크톱 제어 플레인 감사 기록에는
남지 않습니다. 이 도구는 `AGENTMUX_CMUXTERM_HOME`, 호환 변수인
`CMUXTERM_HOME`, 또는 기본 사용자 디렉터리로 선택된 통합 디렉터리에 shim을
기록합니다. `add_to_user_path: true`이면 Windows 사용자 `PATH`도 변경합니다.

## Git, worktree, 리뷰와 개발 서버 도구

5개 트랙 워크플로는 다음 MCP 표면을 추가합니다. 읽기 도구는 모니터링에
사용할 수 있지만 모든 쓰기는 선택한 프로필과 데스크톱 제어 플레인 감사
기록의 적용을 받습니다.

| 워크플로 | `read` | `standard` | `full` |
| --- | --- | --- | --- |
| Git | `git_status_summary`, `git_status_page`, `git_diff` | 호출자의 고정된 페인·저장소 범위로 제한된 `git_stage`, `git_unstage`, non-amend `git_commit` | `git_stage_all`, `git_unstage_all`, `git_discard`, amend 및 컨텍스트 간 커밋 권한 |
| 에이전트 worktree | `agent_worktree_list` | `agent_worktree_create`, `agent_worktree_recover` | `agent_worktree_remove` |
| Diff 리뷰 | `git_review_thread_list`, `git_review_comment_list` | 호출자 소유 thread/comment 생성·수정, 소유 thread stale 표시, 허용된 대상에 소유 thread 전달, 소유 comment 삭제 | thread 삭제 및 호출자 소유 범위를 벗어난 리뷰 관리 |
| 개발 서버 | `development_server_candidate_list` | 후보 무시 또는 브라우저 분할로 열기 | - |
| Markdown 아티팩트 | `markdown_read` | 호출자 워크스페이스의 프로젝트 루트 내부 파일로 제한된 `markdown_open` | - |

worktree 생성은 복구 가능한 saga입니다. Git worktree, AgentMux 워크스페이스,
터미널과 에이전트 실행 중 뒤 단계가 실패하면 앞 단계를 역순으로 보상합니다.
리뷰 전달은 에이전트 mailbox나 터미널에 입력하므로 명시적으로 수행합니다.
`standard`는 호출자의 고정된 페인 컨텍스트가 허용하는 대상에 호출자 소유 리뷰만
전달할 수 있고, `full`은 컨텍스트 간 관리 권한을 유지합니다. 예제와 복구 절차는
[고급 에이전트 워크플로](./advanced-agent-workflows.md)를 참고하십시오.

## 에이전트 페인 워커와 tmux 통합

AgentMux는 에이전트용 페인을 위한 typed MCP 도구를 제공합니다.

| 도구 | 프로필 | 용도 |
| --- | --- | --- |
| `agent_worker_list` | `read` | AgentMux가 관리하는 tmux 통합 워커와 독립 Codex 페인 워커 조회 |
| `agent_integration_status` | `read` | 선택한 워크스페이스의 기본 WSL 배포판을 우선하여 Claude Teams, OMO, OMX, OMC wrapper와 WSL 준비 상태 진단 |
| `agent_team_list` | `read` | 저장된 에이전트 텔레메트리에서 적응형 팀과 현재 구성원 조회 |
| `agent_worker_start` | `standard` | 현재 페인을 분할하거나 새 탭을 만들고 `claude-teams`, `omo`, `omx`, `omc`, `codex-pane` 시작 |
| `agent_team_start` | `standard` | 현재 터미널을 중심으로 적응형 팀 매니페스트 생성. 초기 워커는 선택 사항 |
| `agent_team_spawn` | `standard` | 세대 및 멱등성 보호를 적용해 워커 한 개를 추가하고 관리 레이아웃 재배치 |
| `agent_team_release` | `full` | 선택한 팀이 소유한 워커 한 개를 종료·회수하고 남은 워커 재배치 |
| `agent_team_reflow` | `standard` | 외부 페인을 이동하지 않고 관리 영역 비율 재계산. `dry_run` 지원 |
| `agent_worker_send` | `standard` | 워커에 리터럴 지시를 보내고 선택적으로 Enter 입력 |
| `agent_worker_stop` | `full` | 워커 세션 종료 |
| `agent_integration_setup` | `full` | 공용 tmux 호환 wrapper 설치 및 선택적으로 Windows 사용자 PATH 등록 |

### 화면에서 동시에 확인하는 적응형 팀

일반적인 흐름에서는 에이전트 수를 미리 등록하지 않습니다. 리드 터미널을 중심으로
빈 적응형 팀을 시작한 뒤, 작업 그래프가 바뀔 때 리드 에이전트가 워커를 추가하거나
회수합니다. AgentMux는 수명 주기, 화면 가시성, 최대 용량과 충돌 제어를 담당하고,
새 워커가 필요한지는 현재 프로젝트를 이해하는 리드 모델이 판단합니다.

```json
{
  "workspace_id": "workspace-id",
  "pane_id": "main-pane-id",
  "mode": "adaptive",
  "layout": "main-left-workers-right",
  "main_ratio": 0.55,
  "max_workers": 6,
  "default_worker_kind": "codex-pane",
  "distribution": "Ubuntu",
  "idempotency_key": "release-0.2.0-analysis",
  "workers": []
}
```

`workers`는 생략할 수 있습니다. `max_workers`는 요청 개수가 아니라 안전 상한이며,
tmux에서 자동 편입된 descendant를 포함한 모든 관리 대상 non-main 멤버를 계산합니다.
응답의 `team_id`와 `generation`은 저장되므로 후속 변경 전에 `agent_team_list`로 최신
팀 상태를 읽고 사용합니다.

데스크톱 host는 main session 소유권과 generation을 하나의 제어 평면 잠금 안에서
선점합니다. 동시에 시작한 다른 팀이 기존 소유권을 덮어쓸 수 없고, 같은 비어 있지 않은
`idempotency_key` 재시도는 진행 중 claim을 재사용하여 초기 워커를 중복 생성하지 않습니다.

각 MCP 서버는 자체 팀 변경을 직렬화하고, 데스크톱 host는 예약 소유자를 저장한 뒤 generation과
mutation ID를 함께 비교하는 CAS 방식으로 완료를 반영합니다. 살아 있는 같은 예약을 재호출하면
분할·리사이즈·종료를 반복하지 않고 `provisioning`을 반환합니다. 예약을 소유한 MCP 프로세스가
종료되면 다음 클라이언트가 해당 예약을 `layout_dirty`로 복구하고 화면의 페인을 확인한 뒤 계속할
수 있습니다. 살아 있는 소유자의 예약은 인수할 수 없습니다.

독립적으로 진행할 작업이 생기면 워커를 정확히 한 개 추가합니다.

```json
{
  "team_id": "team-id",
  "expected_generation": 1,
  "idempotency_key": "release-0.2.0-docs",
  "name": "docs",
  "args": ["릴리스 문서를 검토하고 누락을 보고해줘."]
}
```

AgentMux는 토폴로지를 바꾸기 전에 다음 세대를 원자적으로 예약합니다. 오래된
`expected_generation`은 페인을 열지 않고 `generation_conflict`를 반환합니다.
같은 `idempotency_key`를 재전송하면 중복 생성 대신 기존 워커를 반환합니다.
성공하면 메인 터미널은 왼쪽, 워커는 오른쪽 동일 높이 스택에 배치됩니다.

완료된 워커는 `full` 프로필을 사용하는 클라이언트에서 회수할 수 있습니다.

```json
{
  "team_id": "team-id",
  "expected_generation": 2,
  "name": "docs",
  "mode": "soft"
}
```

`agent_team_release`는 선택한 팀이 소유한 구성원만 받지만 프로세스 종료와 페인 닫기를
수행하므로 `full` 프로필이 필요합니다. 세션 종료에 성공한 뒤에만 팀 멤버십을 제거합니다.
종료가 실패하면 페인과 멤버십을 그대로 유지하므로 안전하게 다시 시도할 수 있습니다.
완료 반영은 CAS로 보호되어 오래된 요청이 더 새로운 팀 generation을 덮어쓸 수 없습니다.

### 레이아웃과 tmux 자동 편입

각 워커 페인은 독립적인 라이브 터미널 출력 구독을 가지므로 포커스되지 않아도
렌더링과 주의 상태 보고를 계속합니다. 팀 구성은 `agent_team_list`, 프로세스 상태는
`agent_worker_list`, 입력 대기는 `agent_attention_list`, 공유 진행은
`team_task_list`로 확인합니다.

`auto_adopt_tmux`가 활성화되면 AgentMux tmux shim의 관리 대상 `split-window` 하위
워커가 팀 ID와 메인 세션 기준점을 이어받습니다. 팀 환경만 있으면 별도 integration
marker 없이 편입됩니다. 첫 자식은 메인 오른쪽, 이후 자식은 워커 스택에 추가되고
자동으로 같은 높이로 재배치됩니다. 새 탭과 새 세션은 관리 레이아웃에 조용히 편입하지
않습니다. 따라서 Claude Code Agent Teams와 OMO/OMX/OMC 통합은 최종 워커 수를
미리 알 필요가 없습니다.

tmux shim 프로세스가 generation을 예약한 뒤 페인을 등록하기 전에 종료되면, 다음 관리
대상 split은 기록된 소유자 프로세스가 종료됐는지 확인하고 중단된 예약을
`layout_dirty`로 회수합니다. 이어서 최신 토폴로지를 다시 읽고 같은 호출 안에서 split을
계속합니다. 살아 있는 소유자는 충돌로 보호되므로 진행 중인 워커가 중복 생성되지
않습니다.

AgentMux는 관리되는 하위 트리만 크기를 바꿉니다. 사용자 페인이 섞였거나 split 축이
관리 레이아웃과 다르면 `layout_conflict`를 반환하고 어떤 비율도 변경하지 않으며,
명시적인 운영자 결정을 위해 레이아웃을 dirty 상태로 표시합니다. 변경 없이 계획만
확인하려면 다음과 같이 호출합니다.

```json
{
  "team_id": "team-id",
  "expected_generation": 3,
  "dry_run": true
}
```

실제 reflow는 비율을 적용하기 전에 다음 generation을 예약하므로 MCP spawn/release와
tmux 자동 편입 사이에서도 직렬화됩니다. `dry_run`은 generation을 예약하지 않고
토폴로지도 변경하지 않습니다.

### 고정 초기 팀과 보상 정책

작업 수가 완전히 정해진 배치는 `mode`를 `fixed`로 설정하고 1-8개의 명명된 초기
워커를 전달할 수 있습니다. 적응형 팀에도 초기 워커를 넣은 뒤 추가 확장할 수 있습니다.
워커 이름은 1-64자이며 팀 안에서 고유해야 합니다.

여러 제어 호출을 데이터베이스 트랜잭션처럼 가장하지 않고 보상 정책을 사용합니다.
MCP로 만든 워커가 실패하면 세션을 종료하고 페인을 닫으며, 초기 워커는 역순으로
정리합니다. tmux가 만든 하위 워커는 자동 재배치만 실패한 경우 종료하지 않고
레이아웃을 dirty로 보고합니다.

`codex-pane`은 독립 Codex CLI 프로세스이며 Codex 내장 `/agent` 스레드는 아닙니다.
`claude-teams`, `omo`, `omx`, `omc`는 하위 프로세스를 자동 편입할 수 있는 tmux
호환 lead를 시작합니다. `agent_worker_send`와 범용 `agent_worker_stop`은 일반
터미널 세션을 거부합니다.

shim은 WSL에서 Windows `agentmux.exe`로 넘어갈 때 Linux 쪽 `PATH`를 별도
변수로 캡처하며 Windows 프로세스의 `PATH`를 Linux 자식에 복사하지 않습니다.
데스크톱은 재시작 뒤 캡처한 WSL 경로와 저장된 팀 텔레메트리를 복원하므로 중첩 split도
같은 팀에 귀속됩니다.

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

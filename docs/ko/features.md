# AgentMux 기능 목록 (User-Facing Features)

Status: Draft
Date: 2026-06-23

이 문서는 AgentMux가 **사용자에게 제공하는 기능**을 사용 관점에서 정리한다. 내부 구현이 아니라 "사용자가 무엇을 할 수 있는가"를 기준으로 묶었고, 각 기능에는 현재 빌드의 성숙도를 표기했다. 정확도는 2026-06-23 기준 전체 코드 검토(빌드/테스트 그린, 블로커 0)와 `docs/implementation/`의 Goal 상태 문서(09–26)를 근거로 한다.

AgentMux의 정체성은 **Windows에서 여러 AI 에이전트 세션·셸·브라우저 워크플로를 병렬로 돌리는 터미널 멀티플렉서**다. 에이전트가 패널·복원·프로세스 연속성을 필요로 할 때 WSL과 tmux를 durable 실행 기반으로 사용한다.

범위 기준:

- **Goal 0–9** = MVP / 릴리스 후보 베이스라인 (대부분 정식 제공).
- **Goal 10–18** = cmux 패리티 트랙 (일부 구현, 다수 계획). 자세한 갭은 [cmux 패리티 갭 분석](./implementation/19-cmux-windows-parity-gap-analysis.md) 참고.

## 성숙도 표기

| 표기 | 의미 |
|---|---|
| ✅ 정식 제공 | UI → 컨트롤 플레인 → 백엔드/저장소까지 end-to-end로 동작하며 테스트로 검증됨 |
| 🟡 부분 제공 | 핵심 경로는 동작하지만 일부 진입점·피드백·세부 동작이 미완 또는 재구축 대기 |
| ⛔ 준비 중 | 설계·배선만 있고 사용자 경험은 아직 제공되지 않음 |

---

## 1. 터미널 / 세션 실행

| 기능 | 성숙도 | 설명 |
|---|---|---|
| 네이티브 Windows 셸 (ConPTY) | ✅ | 앱에서 Windows 셸을 열어 입력·출력·리사이즈·종료 상태까지 처리. 리다이렉트된 부모 핸들을 자식이 상속하지 않도록 처리해 출력 캡처 안정화 |
| WSL 직접 셸 | ✅ | 설치된 WSL 배포판 셸을 지정한 디렉터리에서 열기. 배포판 자동 탐색, `wslpath` 기반 Windows→WSL 경로 변환(+`/mnt/<drive>` 폴백) |
| Durable WSL-tmux 세션 | ✅ | tmux control mode(`tmux -CC`) 기반 영속 세션. 앱 재시작 시 실행 중이던 durable 세션에 best-effort 재연결, 장기 프로세스 중복 생성 없음 (Ubuntu + tmux 3.2a 실측 검증) |
| 세션 영속 / 복원 | ✅ | 워크스페이스·패널·서피스·세션 메타데이터가 SQLite(WAL)에 저장되어 재시작 후 유지. 복구 시 durable 세션은 `recovering`, 비durable은 `disconnected`로 정규화되어 중복 생성 방지 |
| 민감 정보 리댁션 | ✅ | 저장 시 token/secret/password 포함 키와 `_KEY`로 끝나는 환경변수 값을 `redacted`로 치환 |
| WSL/tmux 진단 | ✅ | 5종 타입 진단(`wsl.exe` 없음 / 배포판 없음 / 선택 배포판 없음 / 잘못된 cwd / 런치 타임아웃). Settings의 Diagnostics 탭에서 WSL tmux 프로브 직접 실행, 설치 안내 메시지 제공 |

> 관련 문서: [Goal 1](./implementation/09-goal-1-native-terminal-slice-status.md) · [Goal 2](./implementation/10-goal-2-persistence-status.md) · [Goal 3](./implementation/11-goal-3-wsl-direct-status.md) · [Goal 4](./implementation/12-goal-4-durable-tmux-status.md)

## 2. AI 에이전트 실행

| 기능 | 성숙도 | 설명 |
|---|---|---|
| 에이전트 런치 (명령 팔레트 / 액션) | ✅ | durable WSL-tmux 패널로 에이전트를 실행. 새 상단 탭으로 분리되어 현재 split 레이아웃을 건드리지 않음 |
| 워크스페이스 기본 에이전트 명령 | ✅ | 워크스페이스별 기본 에이전트 실행 명령·기본 WSL 배포판을 설정해 런치에 사용 |
| 에이전트 생명주기 상태 감지 | ✅ | 셸 마커(`::agentmux-agent {...}`)·OSC 777(`ESC]777;agentmux;{...}`)로 상태 감지, 옵트인 휴리스틱 감지(기본 비활성). 정상 종료는 `completed`, 비정상/백엔드 실패는 `failed` |
| 통합 에이전트 워커 표식 | ✅ | tmux-compat 래퍼로 생성된 워커 세션을 agent state·사이드바 status/log로 표식, 종료 시 completed/failed 자동 전이 |
| 원클릭 에이전트 런치 버튼 (타이틀바/빈 패널) | 🟡 | 불안정하여 제거됨, 더 깔끔한 형태로 재구축 대기. 현재는 명령 팔레트·액션 레지스트리·CLI로 실행 가능 |

> 관련 문서: [Goal 7](./implementation/15-goal-7-agent-notifications-status.md) · [Goal 14](./implementation/24-goal-14-tmux-compat-status.md) · [전체 완료 현황 G2](./implementation/23-overall-completion-goal-groups.md)

## 3. 워크스페이스 / 패널 / 레이아웃

| 기능 | 성숙도 | 설명 |
|---|---|---|
| 워크스페이스 CRUD | ✅ | 생성·이름변경·포커스·닫기 |
| 상단 탭 + split 패널 | ✅ | 탭이 자신의 패널 레이아웃을 소유. 탭을 닫으면 해당 탭의 패널 서브트리와 마운트된 서피스·세션이 함께 정리됨. split은 탭 스코프 유지 |
| 패널 split / focus / resize | ✅ | 수직·수평 split, 패널 포커스, split 비율 조정(0.1–0.9). 활성 패널에만 xterm 렌더러를 마운트해 stale 세션 입력 방지 |
| 서피스 마운트 / 언마운트 | ✅ | 터미널·브라우저 서피스의 마운트·해제. 한 서피스는 한 패널에만 마운트. 숨겨진 서피스는 능동 렌더링을 멈추되 스크롤백은 유지 |
| 안전한 닫기 정책 | ✅ | 워크스페이스: `fail_if_running` / `detach_sessions` / `terminate_sessions`. 패널: `detach_surface` / `close_surface` / `fail_if_session_running`. 실행 중 세션은 명시적 정책으로 보호 |
| 인라인 이름변경 | ✅ | 워크스페이스 카드에서 즉석 이름변경 |

> 관련 문서: [Goal 5](./implementation/13-goal-5-workspace-pane-ux-status.md) · [전체 완료 현황 G1](./implementation/23-overall-completion-goal-groups.md)

## 4. 워크스페이스 그룹 / 사이드바 조직화

| 기능 | 성숙도 | 설명 |
|---|---|---|
| 워크스페이스 그룹 | ✅ | 관련 워크스페이스를 그룹으로 묶기 (생성/수정/삭제, 멤버 추가·제거). 탭/패널 소유 모델은 바꾸지 않는 사이드바 전용 조직화 |
| 그룹 정렬·핀·접기 | ✅ | 핀 우선 정렬, 그룹 접기/펼치기, 그룹·멤버 이동(버튼 + 포인터 드래그앤드롭) |
| 다중 선택 그룹화 | ✅ | 워크스페이스 카드 체크박스로 여러 개를 선택해 새 그룹 생성 또는 기존 그룹에 추가 |
| 우클릭 컨텍스트 메뉴 | ✅ | 그룹/워크스페이스 우클릭으로 주요 작업 실행. 그룹 앵커 워크스페이스 닫을 때 경고 |
| 사이드바 검색/필터 | ✅ | 그룹·미그룹·멤버를 좁혀 보되 그룹/접힘 상태는 보존 |
| 사이드바 상태 메타데이터 | ✅ | 워크스페이스별 status / progress / log / git 브랜치 표시. 재오픈 후에도 유지. CLI(`set-status`/`set-progress`/`log`)와 동일 채널 |

> 관련 문서: [Goal 15 (워크스페이스 그룹)](./implementation/25-goal-15-workspace-groups-status.md) · [Goal 12 (사이드바 메타데이터)](./implementation/22-goal-12-cli-sidebar-metadata-status.md)

## 5. 알림 / 주의 신호

| 기능 | 성숙도 | 설명 |
|---|---|---|
| 에이전트 주의 알림 | ✅ | 입력 대기(`waiting_for_input`)·실패(`failed`) 등 주의가 필요한 상태를 패널을 일일이 열지 않고도 확인 |
| OS 데스크톱 알림 | ✅ | `tauri-plugin-notification`으로 `agent.needs_input` / `completed` / `failed`를 OS 알림으로 발송, 호스트 실행당 중복 제거 |
| 알림 히스토리 | ✅ | 워크스페이스·심각도별 필터링, 주의 해제(dismiss), 재시작 후에도 유지 |
| 워크스페이스/패널 배지 | ✅ | 사이드바 워크스페이스 주의 카운트, 패널 타이틀 배지 |
| 알림 액션 후크 | ✅ | 알림 타입/심각도에 매칭해 액션 레지스트리 버튼을 Settings에 렌더링. 실행 후 자동 닫기(dismissOnRun) 지원 (호스트 v1은 보안상 액션 레지스트리 ID로 제한) |

> 관련 문서: [Goal 7](./implementation/15-goal-7-agent-notifications-status.md)

## 6. 브라우저 서피스 / 자동화

| 기능 | 성숙도 | 설명 |
|---|---|---|
| 브라우저 서피스 | ✅ | 패널 안에 브라우저 서피스를 새 상단 탭 또는 현재 패널에 생성. `http`/`https`/`data` URL을 임베디드 뷰포트로 렌더, 안전하지 않은 스킴 차단 |
| 실제 브라우저 자동화 (CDP) | ✅ | Chrome DevTools Protocol로 Edge/Chrome/Chromium을 실제 제어(서피스별 격리 프로필). `AGENTMUX_BROWSER_AUTOMATION=auto\|cdp\|memory`, 미설치 시 결정적 in-memory 어댑터로 폴백 |
| 자동화 명령 | ✅ | 지정한 브라우저 서피스에만 navigate / screenshot / DOM 스냅샷 / click / type / evaluate. 다른 서피스로 조용히 리타깃되지 않음 |
| 설정 기반 커스텀 브라우저 액션 | ✅ | config로 새 탭 열기·URL 이동·자동화 레시피를 명령 팔레트·단축키·알림 후크·`actions.run`에서 실행 |
| 자동화 실패 진단 | ✅ | 실패를 `diagnostics.browser` 히스토리와 `browser.action_failed`(error 심각도) 알림으로 노출 |
| 보이는 패널 ↔ CDP 타깃 통합 | 🟡 | 현재 임베디드 뷰포트와 CDP 제어 타깃이 별개. 같은 페이지 인스턴스로 통합하는 작업은 후속(Goal 16) |

> 관련 문서: [Goal 8](./implementation/16-goal-8-browser-surface-automation-status.md) · [전체 완료 현황 G4](./implementation/23-overall-completion-goal-groups.md)

## 7. 원격 (SSH)

| 기능 | 성숙도 | 설명 |
|---|---|---|
| SSH 프로필 관리 | ✅ | Settings UI에서 SSH 프로필 생성·수정·삭제 |
| SSH 세션 실행 | 🟡 | russh 기반 전송 백엔드와 프로필/명령 배선은 존재. 풍부한 런치 피드백·`agentmux ssh` CLI·원격 브라우저 프록시·재연결은 후속(Goal 17) |

> 관련 문서: [전체 완료 현황 G4](./implementation/23-overall-completion-goal-groups.md) · [갭 분석 SSH](./implementation/19-cmux-windows-parity-gap-analysis.md)

## 8. 컨트롤 플레인 / CLI

| 기능 | 성숙도 | 설명 |
|---|---|---|
| `agentmux` CLI | ✅ | 실행 중인 데스크톱 인스턴스에 Windows named pipe(`\\.\pipe\agentmux-control`)로 접속. 워크스페이스·세션·패널·그룹·에이전트·알림·이벤트·진단 명령. `--json` 출력, 파괴적 명령은 `--yes`/`--confirm` 필요 |
| `cmux` 호환 CLI | ✅ | cmux 스타일 별칭(`list-workspaces`, `new-workspace`, `current-workspace`, `new-split`, `send`, `send-key`, `notify`, `sidebar-state`, `ping`, `capabilities`, `identify` 등). `--socket` / `CMUX_SOCKET_PATH` 별칭 지원 |
| 인증 토큰 | ✅ | 사용자별 32바이트 랜덤 hex 토큰을 시작 시 생성/로드. 파일 권한은 Unix `0600`, Windows는 Owner Rights DACL. `AGENTMUX_CONTROL_TOKEN(_PATH)` / `AGENTMUX_CONTROL_PIPE`로 재정의 |
| 이벤트 구독/폴링 | ✅ | 단조 커서 기반 `events.poll`(워크스페이스/세션/타입/최대수 필터 + 누적 드롭 카운트) 및 `events.subscribe`(`after_event_id` 리플레이). `agentmux events watch`는 마지막 커서로 재연결 |
| 관리형 세션 환경변수 | ✅ | 관리형 터미널에 `AGENTMUX_*` / `CMUX_*`(workspace/pane/surface), `TMUX` / `TMUX_PANE` 주입, `WSLENV`로 WSL까지 전달 |
| tmux 호환 명령 변환 | 🟡 | `agentmux __tmux-compat`가 `display-message`, `capture-pane`, `list-panes/-sessions/-windows`, `has-session`, `new-session/-window`, `rename-*`, `select-*`, `send-keys`, `split-window`, `switch-client` 등 다수 변환. 단 `send-keys` 멀티토큰/`-l` 충실도와 일부 키 조합(C-c 등)은 미완 (검토 메이저/마이너) |
| 광고된 미구현 CLI 패밀리 | ⛔ | `browser` / `pane` / `surface` / `system` 패밀리는 usage에 노출되나 동작은 no-op (검토 마이너, 정리 예정) |

> 관련 문서: [Goal 6](./implementation/14-goal-6-control-plane-cli-status.md) · [Goal 14 (tmux-compat)](./implementation/24-goal-14-tmux-compat-status.md)

## 9. 액션 레지스트리 / 단축키 / 명령 팔레트

| 기능 | 성숙도 | 설명 |
|---|---|---|
| 액션 레지스트리 (`actions.list` / `actions.run`) | ✅ | 빌트인 + 워크스페이스 스코프 커스텀 액션 메타데이터 노출, CLI·관리형 에이전트가 control-safe 액션 실행 (`AGENTMUX_WORKSPACE_ID`/`CMUX_WORKSPACE_ID` 폴백) |
| 커스텀 액션 (`custom.*`) | ✅ | global/project config로 명령 팔레트 액션 추가. durable WSL/tmux 에이전트·WSL 터미널·브라우저 액션 정책 지원 |
| 단축키 재바인딩 | ✅ | 사용자 재바인딩, 2단계 코드(chord) 입력, 해제, 중복 바인딩 진단. cmux `cmd` 기본값을 Windows `ctrl`로 매핑 |
| UI 액션 트리거 | ✅ | 사이드바 + 버튼, 서피스 탭 + 버튼, 탭 우측 액션 버튼이 모두 동일 액션 레지스트리로 실행 |

> 관련 문서: [Goal 11](./implementation/21-goal-11-action-registry-status.md)

## 10. 설정 / 구성 관리

| 기능 | 성숙도 | 설명 |
|---|---|---|
| 워크스페이스 설정 | ✅ | 이름·프로젝트 루트·설명·아이콘·색상·기본 WSL 배포판·기본 에이전트 명령 편집. 카드에 아이콘/색상/설명 렌더링 |
| 앱 config export/import/reset/reload | ✅ | 컨트롤 플레인·CLI·Settings에서 재시작 없이 JSON 내보내기/붙여넣기 가져오기/전역 리셋/리로드 |
| 프로젝트 config | ✅ | `<projectRoot>/.agentmux/agentmux.json` 단축키 바인딩을 전역 config 위에 병합. 내보내기/가져오기/리셋 |
| cmux config 호환/마이그레이션 | ✅ | `.agentmux/agentmux.json`이 없으면 호환 `.cmux/cmux.json` 필드를 읽기. `config.migrate_project` / `agentmux config migrate-cmux` / Settings로 마이그레이션(우발적 덮어쓰기 거부) |
| config 진단 + 스키마 | ✅ | global/AgentMux/cmux config 존재·유효성·활성 소스·경로를 진단(깨진 config여도). `agentmux config schema`로 JSON Schema 내보내기 |

> 관련 문서: [Goal 10](./implementation/20-goal-10-setup-config-status.md) · [전체 완료 현황 G3](./implementation/23-overall-completion-goal-groups.md)

## 11. 에이전트 통합 (claude-teams / omo / omx / omc)

| 기능 | 성숙도 | 설명 |
|---|---|---|
| 통합 래퍼 실행 | ✅ | `cmux claude-teams` / `omo` / `omx` / `omc`가 래퍼별 tmux shim 디렉터리를 준비하고 shim-first PATH로 에이전트 실행 |
| WSL 인지 실행 | ✅ | WSL 또는 명시적 배포판 override 시 해당 WSL 배포판 안에서 에이전트를 실행하고 tmux shim 콜백을 Windows `cmux.exe`(`CMUX_EXE`)로 라우팅 |
| 통합 셋업/환경 점검 | ✅ | `cmux integrations setup/env <kind>`로 에이전트를 띄우지 않고 환경 준비·점검 |
| OMO 섀도 config + 패키지 설치 | ✅ | 사용자 OpenCode config를 건드리지 않는 섀도 config에 `oh-my-opencode` 등록·tmux 활성화(JSONC 주석 보존), `bun`/`npm`(또는 WSL 내부)으로 패키지 설치, `node_modules` 격리 보고 |
| 영속 shim + PATH 등록 | ✅ | `install-shims`가 영속 엔트리포인트와 PowerShell/POSIX PATH 스니펫 작성, `--user-path`로 Windows 사용자 PATH(HKCU) 등록 |
| 통합 doctor | ✅ | `cmux integrations doctor [kind]`가 래퍼·tmux shim·PATH·섀도 config·restore 모듈·실행파일 준비 상태를 변경 없이 보고. `--distribution`으로 WSL 컨텍스트 점검 |

> 관련 문서: [Goal 14](./implementation/24-goal-14-tmux-compat-status.md) · [전체 완료 현황 G3](./implementation/23-overall-completion-goal-groups.md)

## 12. 성능 / 진단 / 패키징

| 기능 | 성숙도 | 설명 |
|---|---|---|
| 진단 export | ✅ | `diagnostics.export`(+`agentmux diagnostics export`)가 백엔드 health(백엔드별 active/recovering/failed), 큐 압력(depth/capacity/dropped), 브라우저 실패 히스토리, 알림 요약, 복구 진단을 한 번에 내보냄 |
| 성능 벤치마크 게이트 | ✅ | 5종 벤치마크: single-terminal-latency, many-idle-sessions, high-output, resize-storm, restart-recovery. 레퍼런스 Windows 환경에서 JSON 증거 기록 |
| Windows 설치 패키지 (NSIS) | ✅ | `agentmux.exe` + `cmux.exe` CLI 사이드카를 포함한 NSIS 설치 프로그램 빌드. 설치 파일 내용 게이트로 사이드카 추출·해시 검증 |
| 클린 머신 릴리스 검증 | 🟡 | 자동 게이트는 그린. preinstall / CLI 포함 installed(-RequireCli) / uninstalled 라이프사이클과 WSL 부재·tmux 부재 매트릭스의 클린 머신 패스가 최종 릴리스 전 남음 |

> 관련 문서: [Goal 9 (성능/진단)](./implementation/17-goal-9-performance-diagnostics-status.md) · [릴리스 체크리스트](./implementation/18-goal-9-release-candidate-checklist.md) · [전체 완료 현황 G6](./implementation/23-overall-completion-goal-groups.md)

## 13. 서버 모드 / 웹 접근

| 기능 | 성숙도 | 설명 |
|---|---|---|
| `agentmux server` (로컬 웹 서버) | ✅ | CLI가 로컬 HTTP 서버(기본 `127.0.0.1:8765`)를 띄워 **공유 데스크톱 React UI를 브라우저로** 제공. 워크스페이스/탭/패널/터미널 컴포넌트를 데스크톱과 동일하게 사용 |
| local / desktop-bridge 모드 | ✅ | `local`(기본): CLI 프로세스가 자체 `RuntimeControlPlane`/`TerminalRuntime` 소유, `conpty`(기본) 또는 `--backend wsl-direct --distribution <name>`. `--mode desktop-bridge`: 실행 중인 데스크톱 앱 상태를 named pipe로 노출 |
| HTTP API | ✅ | spawn / send / key / resize / terminate / recent / state / sessions JSON 엔드포인트 |
| 로컬 바인딩 안전장치 | 🟡 | 기본은 루프백. 비루프백은 `--allow-remote` 필요. **아직 브라우저 인증 토큰이 없어 원격 바인딩은 개발 전용**. 출력은 폴링(WebSocket 스트리밍은 후속), 설치형 exe에 UI 번들 임베드는 후속 |

> 관련 문서: [Goal 16 (서버 모드)](./implementation/26-goal-16-server-mode-web-terminal-status.md)

## 14. 에이전트 스킬 (Codex)

| 기능 | 성숙도 | 설명 |
|---|---|---|
| `agentmux-control` 스킬 | ✅ | AgentMux 제어 워크플로 Codex 스킬(`skills/agentmux-control/`: SKILL.md + control-workflows 레퍼런스 + `agents/openai.yaml`). `npm run skills:install`로 `CODEX_HOME`(또는 `~/.codex`)의 skills 디렉터리에 설치. 에이전트가 CLI/컨트롤 워크플로를 자가 학습 |

> 관련 문서: [갭 분석 Skills](./implementation/19-cmux-windows-parity-gap-analysis.md)

---

## 아직 제공하지 않는 기능 (cmux 패리티 갭)

정직한 범위 표기를 위해, cmux가 제공하지만 AgentMux가 **아직 제공하지 않는** 주요 항목 (출처: [갭 분석](./implementation/19-cmux-windows-parity-gap-analysis.md)):

- **First-run 셋업 마법사** — WSL 설치/배포판 선택/프로젝트 루트를 안내하는 폴리시된 첫 실행 마법사. (현재는 진단 + 설치 안내 메시지 수준)
- **자동 업데이트 채널** — winget/MSIX/Tauri updater 등 업데이트 UX 없음.
- **다중 윈도우 / 워크스페이스 스위처** — 멀티 윈도우 모델, 포커스 히스토리, 완전한 키보드 내비게이션 미구현.
- **TextBox 복원/고급 동작** — 활성 터미널로 보내는 기본 composer, 세션별
  draft 보존, `ui.text_box_max_lines` 설정은 구현됨. 세션 restore 연계와
  shell/agent별 paste semantics는 미구현.
- **커스텀 워크스페이스 명령(레이아웃 DSL / worktree 템플릿)** — JSON 정의 레이아웃·worktree 미구현.
- **고급 세션 복원** — 버전드 스냅샷, 스크롤백 리플레이, 브라우저 히스토리 복원, 수동 "이전 세션 복원", resume 바인딩 신뢰 정책 미구현. (현재는 메타데이터 + best-effort tmux attach)
- **확장 브라우저 명령군 / 포커스 모드 / DevTools / React Grab** — wait, back/forward, frames, dialog, download, cookies/storage 등 다수 미구현.
- **Dock (우측 사이드바 TUI 제어)** — `dock.json` 로딩, 우측 컨트롤 패널,
  backend-auditable 프로젝트 Dock 신뢰 승인 저장, 컨트롤별 Dock 내부 WSL 터미널 슬롯,
  restart/close lifecycle, 컨트롤별 높이 조절/저장이 구현됨.
- **SSH 풀 패리티** — `agentmux ssh`, 딥링크, 원격 브라우저 프록시, scp 드래그앤드롭, 릴레이 데몬, 재연결 미구현.

> 참고: AgentMux Codex 스킬(`agentmux-control`)은 이미 제공된다(§14 참고). cmux의 광범위한 스킬 카탈로그 패리티는 별개 항목.

## 알려진 제약 (현재 빌드 버그/품질)

전체 코드 검토에서 확인한, 사용자 경험에 영향을 주는 미해결 항목:

- **기동 속도**: durable 세션 복구가 창이 뜨기 전에 동기로 실행되어, 복구 세션마다 WSL 프로브가 직렬로 지연될 수 있음 (수정 예정).
- **콘솔 깜빡임**: WSL 프로브가 `CREATE_NO_WINDOW` 없이 실행되어 WSL/tmux 작업 시 콘솔 창이 잠깐 깜빡일 수 있음 (수정 예정).
- **spawn↔persist 불일치**: 세션이 실제로 떴는데 사후 저장 실패 시 UI엔 실패로 표시되어 살아있는 프로세스가 고아가 될 수 있음 (수정 예정).
- **CLI tmux send-keys 충실도**: 멀티토큰 텍스트와 `-l` 리터럴 플래그 처리에 갭 존재.
- **광고된 일부 CLI 패밀리**(browser/pane/surface/system): 아직 no-op.

성숙도와 제약은 향후 빌드에서 갱신된다. 상세 근거는 [전체 완료 현황](./implementation/23-overall-completion-goal-groups.md), [갭 분석](./implementation/19-cmux-windows-parity-gap-analysis.md)과 각 Goal 상태 문서를 참고한다.

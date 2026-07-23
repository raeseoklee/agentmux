# 고급 에이전트 워크플로

AgentMux의 데스크톱 UI, CLI, 서버 모드, MCP는 같은 제어 계약을 사용합니다.
아래 기능은 별도 표기가 없다면 실행 중인 AgentMux 제어 플레인이 필요합니다.

## 대규모 Git 저장소

소스 제어 패널은 변경 파일을 제한된 페이지로 가져오고 화면에 보이는 행만
렌더링합니다. 데스크톱 모드는 저장소 변경 이벤트로 갱신하며, 서버 모드는
인증된 제어 브리지와 저빈도 보조 폴링을 사용합니다.

```powershell
agentmux git status --workspace <workspace-id> --json
agentmux git page --workspace <workspace-id> --limit 200 --json
agentmux git diff --workspace <workspace-id> --path src/app.ts --json
```

로컬 변경을 버리는 명령은 반드시 `--yes`를 요구합니다.

## 에이전트별 worktree 격리

worktree, 워크스페이스, 터미널, 에이전트 명령을 하나의 복구 가능한 작업으로
생성합니다. 재시도 시 중복 생성을 막으려면 안정적인 idempotency key를
지정하십시오.

```powershell
agentmux agent worktree create `
  --workspace <source-workspace-id> `
  --branch agent/fix-login `
  --destination D:\worktrees\fix-login `
  --base main --create-branch `
  --idempotency-key issue-142-fix-login `
  -- claude
```

```powershell
agentmux agent worktree list --include-completed --json
agentmux agent worktree recover --operation-id <operation-id> --json
agentmux agent worktree remove <worktree-id> --yes
```

삭제는 AgentMux가 소유 기록을 남긴 worktree만 허용하며 Git 브랜치는 삭제하지
않습니다. 주 worktree나 임의 경로는 삭제할 수 없습니다.

## Diff 리뷰 피드백

Git diff의 줄 또는 hunk에 리뷰 스레드를 만들고 선택한 에이전트의 mailbox나
터미널로 전달할 수 있습니다. 리뷰 생성만으로 에이전트 입력을 보내지는 않으며,
전달은 사용자가 명시적으로 실행해야 합니다.

```powershell
agentmux git review thread create `
  --workspace <workspace-id> `
  --path src/app.ts --side right --line 42 `
  --body "상태를 갱신하기 전에 취소 경로를 처리해 주세요."

agentmux git review thread deliver <thread-id> `
  --target mailbox --session <agent-session-id> --include-context
```

## Claude/Codex Hook

설치 전에 반드시 미리보기를 확인하십시오.

```powershell
agentmux agent hooks preview --provider all
agentmux agent hooks install --provider all --yes
```

설치기는 관련 없는 설정을 보존하고 기존 파일을 백업한 뒤 원자적으로 교체합니다.
기존 Codex `notify` 명령을 발견하면 자동으로 덮어쓰지 않고 중단합니다. Hook은
Windows 사용자 권한으로 실행되므로 Claude와 Codex의 hook 검토 화면에서도
설정을 확인하십시오.

## 개발 서버 링크

AgentMux는 터미널 출력에서 로컬 HTTP/HTTPS 개발 서버 주소를 찾습니다. 감지만
으로 브라우저를 열지 않으며, 사용자가 승인하면 임베디드 브라우저 분할을 만듭니다.

```powershell
agentmux dev-server candidate list --workspace <workspace-id> --json
agentmux dev-server candidate open <candidate-id> --axis vertical --ratio 0.4
agentmux dev-server candidate dismiss <candidate-id> --reason ignored
```

## 터미널 Warm-retain

최근 탭은 짧은 유예 시간 동안 xterm 인스턴스와 viewport를 유지합니다. 오래
숨겨진 탭만 직렬화하고 해제하며, 활성 탭은 제거하지 않습니다. GPU 렌더러는
보이는 페인에만 사용해 WebView2 컨텍스트 고갈을 방지합니다.

자세한 MCP 권한은 [MCP 제어 플레인](./mcp.md)을 참고하십시오.

# UI and Terminal Rendering

Status: Draft
Date: 2026-06-18

이 문서는 desktop UI, pane layout, terminal rendering, browser surfaces, notifications의 구현 기준을 정의한다.

## UI Goals

- 많은 session을 빠르게 스캔하고 전환할 수 있어야 한다.
- terminal pane은 작업 도구처럼 조밀하고 안정적으로 동작해야 한다.
- visible pane만 비싼 렌더링 비용을 내야 한다.
- session 상태와 agent attention은 사용자가 별도 탐색 없이 볼 수 있어야 한다.
- UI는 core API client이며 backend 세부 구현을 모르면 된다.

## Primary Views

| View | Purpose |
|---|---|
| Workspace shell | 전체 app frame, sidebar, active workspace area |
| Workspace overview | workspace 목록, running/attention/failed 상태 요약 |
| Pane canvas | split layout과 mounted surfaces |
| Terminal surface | terminal rendering, input, selection, scrollback |
| Browser surface | local browser automation view |
| Notification panel | session/agent/browser event history |
| Diagnostics panel | backend health, queue pressure, recent errors |
| Command palette | workspace/session/pane commands |

## Layout Model

UI layout mirrors core pane tree.

```ts
type PaneNode =
  | {
      kind: "split";
      paneId: string;
      axis: "horizontal" | "vertical";
      ratio: number;
      first: PaneNode;
      second: PaneNode;
    }
  | {
      kind: "leaf";
      paneId: string;
      mountedSurfaceId?: string;
    };
```

Rules:

- Layout state comes from core.
- Drag resize sends layout patch, not a full app state mutation.
- Split ratio is stable across window resize.
- Leaf pane has stable min dimensions.
- Closing a pane must not kill a running session unless policy says so.

## Surface Mounting

Surface can be mounted, unmounted, hidden, or closed.

| State | Meaning |
|---|---|
| `mounted_visible` | Surface has active renderer and receives visible output |
| `mounted_hidden` | Surface is mounted in inactive workspace or occluded tab |
| `unmounted` | Surface exists but no renderer is attached |
| `closing` | Close flow started |
| `error` | Renderer or surface adapter failed |

Terminal session must continue when surface is unmounted if backend durability allows it.

## Terminal Renderer Adapter

MVP uses a renderer adapter boundary:

```ts
interface TerminalRenderer {
  mount(element: HTMLElement, initialState: TerminalSnapshot): void;
  unmount(): void;
  write(batch: Uint8Array): void;
  resize(columns: number, rows: number): void;
  focus(): void;
  dispose(): void;
}
```

Initial implementation may wrap xterm.js. The rest of UI must not depend on xterm.js-specific APIs directly.

Adapter responsibilities:

- render terminal output
- report dimensions
- emit keyboard input
- emit paste
- expose selection/copy
- support theme changes
- provide renderer health

Non-responsibilities:

- process lifecycle
- backend protocol parsing
- persistent session metadata
- durable recovery

## Rendering Performance

Required behavior:

- Only visible terminal surfaces are mounted with active renderer.
- Hidden surfaces receive summary state, not continuous DOM updates.
- Output batches are scheduled per animation frame.
- Large output bursts are chunked.
- Scrolling a terminal should not trigger full workspace re-render.
- Workspace overview uses lightweight session summaries.

Recommended implementation:

- Keep terminal output outside global React state.
- Use an imperative renderer adapter for byte streams.
- Store session summaries in React state.
- Use memoized pane tree rendering.
- Use explicit surface lifecycle hooks.

## Input Routing

Input path:

1. User focuses a terminal surface.
2. UI records focused pane and surface.
3. Renderer emits text/key/paste event.
4. UI sends `session.send_text`, `session.send_key`, or `session.paste`.
5. Core routes input to backend.

Rules:

- Input always targets explicit session id.
- Focus changes must be acknowledged by UI state before input dispatch.
- Paste above configurable byte threshold should show a confirmation or safe paste affordance.
- Keyboard shortcuts must distinguish app command shortcuts from terminal pass-through keys.

## Resize Handling

Resize path:

1. Pane dimensions change.
2. Terminal adapter computes columns/rows.
3. UI sends `session.resize`.
4. Core coalesces and forwards to backend.

Rules:

- Resize events are debounced or coalesced.
- Last size wins.
- Hidden terminal surfaces do not emit continuous resize events.
- On remount, renderer sends current dimensions immediately.

## Agent Attention UI

Session summary fields:

- title
- backend kind
- process state
- agent state
- last output time
- attention count
- failure indicator
- detached/recovering indicator

Attention display:

- workspace sidebar badge
- pane title badge
- notification history entry
- command palette filter

Rules:

- Attention indicator must be visible without opening the pane.
- False-positive heuristic attention must be dismissible.
- Completed/failed state must include timestamp and exit code where available.

## Notification Panel

Notification types:

- agent needs input
- agent completed
- agent failed
- backend disconnected
- output truncated
- browser action failed
- workspace recovered

Notification fields:

- id
- type
- severity
- workspace id
- session/surface id
- title
- message
- created time
- read/dismissed state

The panel must support filtering by workspace and severity.

## Browser Surface

Browser surface is a first-class surface type, not a modal-only tool.

Browser UI responsibilities:

- show current URL
- show loading/error state
- render browser viewport
- expose screenshot/action state
- surface automation failures in diagnostics

Browser automation should be scoped to the surface id. UI must make it clear which browser surface is being automated.

## Accessibility and Usability

Minimum requirements:

- Keyboard navigation for workspace, pane focus, command palette.
- Visible focus state.
- Terminal font size setting.
- High contrast theme support.
- Clear error states for disconnected sessions.
- No hidden destructive close behavior.

## UI Test Targets

Automated UI tests should cover:

- create workspace
- open native shell
- split pane
- focus pane and type
- resize pane
- close pane with running session warning
- recover layout after restart
- show attention badge
- open notification panel
- open diagnostics panel


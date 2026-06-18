# tmux-control Fixtures

Fixture categories:

- simple command response
- output with escape sequences
- high-volume output split across reads
- pane death
- server exit
- unknown line
- malformed output payload
- attach to existing session

Current fixtures:

- `simple-command.txt`: `%begin`/response/`%end` command correlation.
- `output-escapes.txt`: `%output` payload escape decoding.
- `topology-events.txt`: window, pane, layout, session, exit, and unknown events.

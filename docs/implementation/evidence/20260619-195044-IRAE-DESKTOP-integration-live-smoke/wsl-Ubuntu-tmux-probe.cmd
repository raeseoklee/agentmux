@echo off
cd /d D:\Workspace\irae\agentmux
C:\Windows\system32\wsl.exe --distribution Ubuntu --exec sh -lc "command -v tmux >/dev/null 2>&1 && tmux -V" > D:\Workspace\irae\agentmux\docs\implementation\evidence\20260619-195044-IRAE-DESKTOP-integration-live-smoke\wsl-Ubuntu-tmux-probe.stdout.txt 2> D:\Workspace\irae\agentmux\docs\implementation\evidence\20260619-195044-IRAE-DESKTOP-integration-live-smoke\wsl-Ubuntu-tmux-probe.stderr.txt
exit /b %ERRORLEVEL%

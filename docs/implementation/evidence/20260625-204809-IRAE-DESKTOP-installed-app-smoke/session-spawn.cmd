@echo off
cd /d D:\Workspace\irae\agentmux
C:\Users\irae\AppData\Local\AgentMux\agentmux.exe session spawn --workspace ws_00000001 --backend conpty --cwd D:\Workspace\irae\agentmux --durability ephemeral --json --pipe \\.\pipe\agentmux-installed-smoke-079b9e77a3484a6f83e15792dc9b3cf6 --token-path D:\Workspace\irae\agentmux\docs\implementation\evidence\20260625-204809-IRAE-DESKTOP-installed-app-smoke\runtime\control.token -- cmd.exe /d /q /c "echo AGENTMUX_INSTALLED_SMOKE_bb47f9aa96484c10a01d47e20525aa63" > D:\Workspace\irae\agentmux\docs\implementation\evidence\20260625-204809-IRAE-DESKTOP-installed-app-smoke\session-spawn.stdout.txt 2> D:\Workspace\irae\agentmux\docs\implementation\evidence\20260625-204809-IRAE-DESKTOP-installed-app-smoke\session-spawn.stderr.txt
exit /b %ERRORLEVEL%

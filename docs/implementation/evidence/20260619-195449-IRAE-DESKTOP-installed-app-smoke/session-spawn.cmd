@echo off
cd /d D:\Workspace\irae\agentmux
D:\Workspace\irae\agentmux\target\debug\agentmux.exe session spawn --workspace ws_00000001 --backend conpty --cwd D:\Workspace\irae\agentmux --durability ephemeral --json --pipe \\.\pipe\agentmux-installed-smoke-d40404c3b70649f4a8cc2c3039ab9b44 --token-path D:\Workspace\irae\agentmux\docs\implementation\evidence\20260619-195449-IRAE-DESKTOP-installed-app-smoke\runtime\control.token -- cmd.exe /d /q /c "echo AGENTMUX_INSTALLED_SMOKE_753a2219992e4e2e8339e94d9b96fa13" > D:\Workspace\irae\agentmux\docs\implementation\evidence\20260619-195449-IRAE-DESKTOP-installed-app-smoke\session-spawn.stdout.txt 2> D:\Workspace\irae\agentmux\docs\implementation\evidence\20260619-195449-IRAE-DESKTOP-installed-app-smoke\session-spawn.stderr.txt
exit /b %ERRORLEVEL%

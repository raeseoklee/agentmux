@echo off
cd /d D:\Workspace\irae\agentmux
D:\Workspace\irae\agentmux\target\debug\agentmux.exe diagnostics export --json --pipe \\.\pipe\agentmux-installed-smoke-d40404c3b70649f4a8cc2c3039ab9b44 --token-path D:\Workspace\irae\agentmux\docs\implementation\evidence\20260619-195449-IRAE-DESKTOP-installed-app-smoke\runtime\control.token > D:\Workspace\irae\agentmux\docs\implementation\evidence\20260619-195449-IRAE-DESKTOP-installed-app-smoke\diagnostics-export.stdout.txt 2> D:\Workspace\irae\agentmux\docs\implementation\evidence\20260619-195449-IRAE-DESKTOP-installed-app-smoke\diagnostics-export.stderr.txt
exit /b %ERRORLEVEL%

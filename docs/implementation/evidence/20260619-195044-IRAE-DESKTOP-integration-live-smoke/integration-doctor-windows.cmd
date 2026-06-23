@echo off
cd /d D:\Workspace\irae\agentmux
set "PATH=D:\Workspace\irae\agentmux\docs\implementation\evidence\20260619-195044-IRAE-DESKTOP-integration-live-smoke\runtime\cmuxterm\bin;%PATH%"
D:\Workspace\irae\agentmux\target\debug\cmux.exe integrations doctor --json --base-dir D:\Workspace\irae\agentmux\docs\implementation\evidence\20260619-195044-IRAE-DESKTOP-integration-live-smoke\runtime\cmuxterm --bin-dir D:\Workspace\irae\agentmux\docs\implementation\evidence\20260619-195044-IRAE-DESKTOP-integration-live-smoke\runtime\cmuxterm\bin > D:\Workspace\irae\agentmux\docs\implementation\evidence\20260619-195044-IRAE-DESKTOP-integration-live-smoke\integration-doctor-windows.stdout.txt 2> D:\Workspace\irae\agentmux\docs\implementation\evidence\20260619-195044-IRAE-DESKTOP-integration-live-smoke\integration-doctor-windows.stderr.txt
exit /b %ERRORLEVEL%

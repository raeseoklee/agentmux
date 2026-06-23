@echo off
cd /d D:\Workspace\irae\agentmux
D:\Workspace\irae\agentmux\target\debug\cmux.exe integrations install-shims --json --base-dir D:\Workspace\irae\agentmux\docs\implementation\evidence\20260619-195044-IRAE-DESKTOP-integration-live-smoke\runtime\cmuxterm --bin-dir D:\Workspace\irae\agentmux\docs\implementation\evidence\20260619-195044-IRAE-DESKTOP-integration-live-smoke\runtime\cmuxterm\bin > D:\Workspace\irae\agentmux\docs\implementation\evidence\20260619-195044-IRAE-DESKTOP-integration-live-smoke\integration-install-shims.stdout.txt 2> D:\Workspace\irae\agentmux\docs\implementation\evidence\20260619-195044-IRAE-DESKTOP-integration-live-smoke\integration-install-shims.stderr.txt
exit /b %ERRORLEVEL%

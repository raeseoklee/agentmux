import type { AppLocaleLanguage } from "../control/ControlClient";

export type I18nKey =
  | "app.sidebar.toggle"
  | "app.search.activeWindow"
  | "app.search.placeholder"
  | "app.commandPalette.open"
  | "app.commandPalette.noResults"
  | "app.commandPalette.placeholder"
  | "app.commandPalette.shortcutClose"
  | "app.commandPalette.shortcutMove"
  | "app.commandPalette.shortcutRun"
  | "app.panes.balance"
  | "app.settings.open"
  | "app.version"
  | "app.version.current"
  | "app.window.minimize"
  | "app.window.maximize"
  | "app.window.restore"
  | "app.window.close"
  | "appearance.dark"
  | "appearance.light"
  | "common.active"
  | "common.cancel"
  | "common.clear"
  | "common.close"
  | "common.connect"
  | "common.dismiss"
  | "common.edit"
  | "common.empty"
  | "common.idle"
  | "common.invalid"
  | "common.loading"
  | "common.ok"
  | "common.refresh"
  | "common.reload"
  | "common.reset"
  | "common.save"
  | "common.settings"
  | "common.unassigned"
  | "browser.dialog.allow"
  | "browser.dialog.confirmTitle"
  | "browser.dialog.expiredDescription"
  | "browser.dialog.expiredTitle"
  | "browser.dialog.promptLabel"
  | "browser.dialog.promptTitle"
  | "browser.dialog.submit"
  | "config.configuration"
  | "config.export"
  | "config.exportProject"
  | "config.globalPath"
  | "config.import"
  | "config.importProject"
  | "config.jsonOnlyHint"
  | "config.migrateCmux"
  | "config.projectPath"
  | "config.reload"
  | "config.resetGlobalConfirm"
  | "config.resetProject"
  | "config.resetProjectConfirm"
  | "config.scopeGlobal"
  | "config.scopeProject"
  | "dialog.dismissToast"
  | "dialog.confirm"
  | "dialog.pressKey"
  | "dialog.pressSecondKey"
  | "dialog.required"
  | "language.english"
  | "language.korean"
  | "language.label"
  | "language.savedGlobally"
  | "notifications.empty"
  | "notifications.focus"
  | "notifications.open"
  | "notifications.severity.error"
  | "notifications.severity.info"
  | "notifications.severity.warning"
  | "notifications.summary"
  | "notifications.title"
  | "pane.empty"
  | "pane.invalidLayout"
  | "pane.restoring"
  | "settings.appearance"
  | "settings.advanced"
  | "settings.diagnostics"
  | "settings.general"
  | "settings.keys"
  | "settings.profiles"
  | "settings.project"
  | "settings.tabs.appearance"
  | "settings.tabs.advanced"
  | "settings.tabs.diagnostics"
  | "settings.tabs.general"
  | "settings.tabs.keys"
  | "settings.tabs.profiles"
  | "settings.tabs.workspace"
  | "settings.theme"
  | "settings.accentColor"
  | "settings.uiFontSize"
  | "settings.terminalInnerMargin"
  | "settings.terminalGpuAcceleration"
  | "settings.terminalGpuAccelerationHint"
  | "settings.terminalGpuAcceleration.auto"
  | "settings.terminalGpuAcceleration.on"
  | "settings.terminalGpuAcceleration.off"
  | "settings.terminalStartDirectory"
  | "settings.terminalStartDirectoryHint"
  | "settings.terminalStartDirectory.home"
  | "settings.terminalStartDirectory.workspace"
  | "settings.terminalStartDirectory.custom"
  | "settings.terminalStartCustomCwd"
  | "settings.terminalStartCustomCwdPlaceholder"
  | "settings.terminalSplitBehavior"
  | "settings.terminalSplitBehaviorHint"
  | "settings.terminalSplitBehavior.cloneCurrent"
  | "settings.terminalSplitBehavior.empty"
  | "settings.terminalLinkOpen"
  | "settings.terminalLinkOpenHint"
  | "settings.terminalLinkOpen.system"
  | "settings.terminalLinkOpen.inApp"
  | "session.status.attention"
  | "session.status.running"
  | "session.status.starting"
  | "session.status.recovering"
  | "session.status.detached"
  | "session.status.disconnected"
  | "session.status.exited"
  | "session.status.failed"
  | "session.status.lost"
  | "surface.tab.actions"
  | "surface.tab.nameLabel"
  | "surface.tab.rename"
  | "surface.tab.renameTitle"
  | "surface.tab.reset"
  | "workspace.status.needsInput"
  | "workspace.status.running"
  | "workspace.status.sessionCount"
  | "workspace.status.idle"
  | "workspace.settings"
  | "statusbar.openPath"
  | "statusbar.openPathFailed"
  | "statusbar.surfaceSummary"
  | "sourceControl.changes"
  | "sourceControl.clean"
  | "sourceControl.commit"
  | "sourceControl.commitCreated"
  | "sourceControl.commitPlaceholder"
  | "sourceControl.diff"
  | "sourceControl.enterCommitMessage"
  | "sourceControl.filterPlaceholder"
  | "sourceControl.loading"
  | "sourceControl.loadMore"
  | "sourceControl.noBranch"
  | "sourceControl.noDiff"
  | "sourceControl.noMatchingChanges"
  | "sourceControl.noStagedChanges"
  | "sourceControl.notRepository"
  | "sourceControl.notRepositoryDescription"
  | "sourceControl.resizeFileList"
  | "sourceControl.selectFile"
  | "sourceControl.shownCount"
  | "sourceControl.stageAll"
  | "sourceControl.stageFile"
  | "sourceControl.stagedChanges"
  | "sourceControl.syncState"
  | "sourceControl.title"
  | "sourceControl.truncated"
  | "sourceControl.unavailableServer"
  | "sourceControl.unstageAll"
  | "sourceControl.unstageFile"
  | "sourceControl.updating"
  | "sourceControl.worktreeBase"
  | "sourceControl.worktreeBranch"
  | "sourceControl.worktreeCommand"
  | "sourceControl.worktreeCreateConfirm"
  | "sourceControl.worktreeCreateDescription"
  | "sourceControl.worktreeCreateTitle"
  | "sourceControl.worktreeDestination"
  | "sourceControl.worktreeList"
  | "sourceControl.worktreeRecover"
  | "sourceControl.worktreeRemove"
  | "sourceControl.worktreeRemoveDescription"
  | "sourceControl.worktreeRemoveTitle"
  | "sourceControl.worktreeStarted"
  | "sourceControl.worktreeStateCompleted"
  | "sourceControl.worktreeStateFailed"
  | "sourceControl.worktreeStatePrepared"
  | "sourceControl.worktreeStateRemoved"
  | "sourceControl.worktreeStateRolledBack"
  | "sourceControl.worktreeStateRollingBack"
  | "sourceControl.worktreeStateSessionCreated"
  | "sourceControl.worktreeStateUnknown"
  | "sourceControl.worktreeStateWorkspaceCreated"
  | "sourceControl.worktreeStateWorktreeCreated"
  | "sourceControl.reviewAdd"
  | "sourceControl.reviewCommentOn"
  | "sourceControl.reviewDelete"
  | "sourceControl.reviewDeleteDescription"
  | "sourceControl.reviewDeleteTitle"
  | "sourceControl.reviewDoNotDeliver"
  | "sourceControl.reviewMailbox"
  | "sourceControl.reviewPlaceholder"
  | "sourceControl.reviewReopen"
  | "sourceControl.reviewResolve"
  | "sourceControl.reviewSideContext"
  | "sourceControl.reviewSideLeft"
  | "sourceControl.reviewSideRight"
  | "sourceControl.reviewSideUnknown"
  | "sourceControl.reviewTerminal"
  | "devServer.detectedTitle"
  | "devServer.detectedDescription"
  | "devServer.openInSplit"
  | "devServer.openFailed"
  | "action.group.agent"
  | "action.group.terminal"
  | "action.group.workspace"
  | "action.group.view"
  | "action.group.remote"
  | "settings.workspace.noActiveProject"
  | "settings.workspace.scope"
  | "settings.workspace.selector"
  | "settings.workspace.name"
  | "settings.workspace.root"
  | "settings.workspace.description"
  | "settings.workspace.icon"
  | "settings.workspace.color"
  | "settings.workspace.defaultTerminal"
  | "settings.workspace.defaultWsl"
  | "settings.workspace.defaultAgentCommand"
  | "settings.workspace.systemDefault"
  | "settings.workspace.saveProject"
  | "settings.workspace.saveFailed"
  | "settings.workspace.title"
  | "settings.workspace.unsavedTitle"
  | "settings.workspace.unsavedDescription"
  | "settings.workspace.discardChanges"
  | "shortcuts.conflictsTitle"
  | "shortcuts.editDescription"
  | "shortcuts.editTitle"
  | "shortcuts.firstStroke"
  | "shortcuts.invalidDescription"
  | "shortcuts.invalidTitle"
  | "shortcuts.replaceAction"
  | "shortcuts.replaceDescription"
  | "shortcuts.replaceDetail"
  | "shortcuts.replaceTitle"
  | "shortcuts.save"
  | "shortcuts.secondStroke"
  | "shortcuts.unassign"
  | "updates.autoCheck"
  | "updates.autoCheckHint"
  | "updates.check"
  | "updates.install"
  | "updates.notification.action"
  | "updates.notification.body"
  | "updates.notification.fallback.failed"
  | "updates.notification.fallback.unsupported"
  | "updates.notification.title"
  | "updates.releaseNotes"
  | "updates.status.available"
  | "updates.status.checking"
  | "updates.status.downloading"
  | "updates.status.error"
  | "updates.status.idle"
  | "updates.status.installed"
  | "updates.status.notAvailable"
  | "updates.status.unsupported"
  | "updates.title"
  | "workspace.addSelectedToGroup"
  | "workspace.add"
  | "workspace.createGroupFromSelection"
  | "workspace.addToGroup"
  | "workspace.clearSelection"
  | "workspace.createGroup"
  | "workspace.filter"
  | "workspace.group.addWorkspace"
  | "workspace.group.moveDown"
  | "workspace.group.moveUp"
  | "workspace.group.colorDescription"
  | "workspace.group.colorLabel"
  | "workspace.group.createAction"
  | "workspace.group.createTitle"
  | "workspace.group.deleteAction"
  | "workspace.group.deleteDescription"
  | "workspace.group.deleteTitle"
  | "workspace.group.editTitle"
  | "workspace.group.iconDescription"
  | "workspace.group.iconLabel"
  | "workspace.group.invalidColor"
  | "workspace.group.nameLabel"
  | "workspace.none"
  | "workspace.section"
  | "workspace.selectedCount";

export type Translator = (
  key: I18nKey,
  values?: Record<string, string | number>,
) => string;

export const SUPPORTED_LANGUAGES: Array<{
  code: AppLocaleLanguage;
  labelKey: I18nKey;
}> = [
  { code: "en", labelKey: "language.english" },
  { code: "ko", labelKey: "language.korean" },
];

const en: Record<I18nKey, string> = {
  "app.sidebar.toggle": "Toggle sidebar",
  "app.search.activeWindow": "Search active window",
  "app.search.placeholder": "Search active window",
  "app.commandPalette.open": "Open command palette",
  "app.commandPalette.noResults": "No results",
  "app.commandPalette.placeholder": "Run a command or search workspaces...",
  "app.commandPalette.shortcutClose": "esc close",
  "app.commandPalette.shortcutMove": "up/down move",
  "app.commandPalette.shortcutRun": "enter run",
  "app.panes.balance": "Balance split panes",
  "app.settings.open": "Open settings",
  "app.version": "Version",
  "app.version.current": "Current AgentMux version",
  "app.window.minimize": "Minimize",
  "app.window.maximize": "Maximize",
  "app.window.restore": "Restore",
  "app.window.close": "Close",
  "appearance.dark": "Dark",
  "appearance.light": "Light",
  "common.active": "active",
  "common.cancel": "Cancel",
  "common.clear": "Clear",
  "common.close": "Close",
  "common.connect": "Connect",
  "common.dismiss": "Dismiss",
  "common.edit": "Edit",
  "common.empty": "empty",
  "common.idle": "idle",
  "common.invalid": "invalid",
  "common.loading": "Loading...",
  "common.ok": "ok",
  "common.refresh": "Refresh",
  "common.reload": "Reload",
  "common.reset": "Reset",
  "common.save": "Save",
  "common.settings": "Settings",
  "common.unassigned": "Unassigned",
  "browser.dialog.allow": "Allow",
  "browser.dialog.confirmTitle": "{source} asks for confirmation",
  "browser.dialog.expiredDescription": "The browser dialog could not be completed.",
  "browser.dialog.expiredTitle": "Browser dialog expired",
  "browser.dialog.promptLabel": "Value",
  "browser.dialog.promptTitle": "{source} requests input",
  "browser.dialog.submit": "Submit",
  "config.configuration": "Configuration",
  "config.export": "Export",
  "config.exportProject": "Export workspace",
  "config.globalPath": "global {path}",
  "config.import": "Import",
  "config.importProject": "Import workspace",
  "config.jsonOnlyHint": "Saved in agentmux.json. Edit the JSON file directly or use import/export.",
  "config.migrateCmux": "Migrate .cmux",
  "config.projectPath": "workspace {path}",
  "config.reload": "Reload",
  "config.resetGlobalConfirm": "Reset global AgentMux config?",
  "config.resetProject": "Reset workspace",
  "config.resetProjectConfirm": "Reset workspace AgentMux config?",
  "config.scopeGlobal": "Global",
  "config.scopeProject": "Workspace",
  "dialog.dismissToast": "Dismiss {title}",
  "dialog.confirm": "OK",
  "dialog.pressKey": "Press a key combination",
  "dialog.pressSecondKey": "Press a second key combination",
  "dialog.required": "{label} is required.",
  "language.english": "English",
  "language.korean": "Korean",
  "language.label": "Language",
  "language.savedGlobally": "Language is saved globally and applies to every workspace.",
  "notifications.empty": "No active notifications.",
  "notifications.focus": "Focus",
  "notifications.open": "Open notifications",
  "notifications.severity.error": "Error",
  "notifications.severity.info": "Info",
  "notifications.severity.warning": "Warning",
  "notifications.summary": "{count} active",
  "notifications.title": "Notifications",
  "pane.empty": "Empty pane",
  "pane.invalidLayout": "Invalid pane layout",
  "pane.restoring": "Restoring",
  "settings.appearance": "Appearance",
  "settings.advanced": "Advanced",
  "settings.diagnostics": "Diagnostics",
  "settings.general": "General",
  "settings.keys": "Keyboard shortcuts",
  "settings.profiles": "Profiles",
  "settings.project": "Workspace",
  "settings.tabs.appearance": "Appearance",
  "settings.tabs.advanced": "Advanced",
  "settings.tabs.diagnostics": "Diagnostics",
  "settings.tabs.general": "General",
  "settings.tabs.keys": "Shortcuts",
  "settings.tabs.profiles": "Profiles and SSH",
  "settings.tabs.workspace": "Workspace",
  "settings.theme": "Theme",
  "settings.accentColor": "Accent color",
  "settings.uiFontSize": "UI font size",
  "settings.terminalInnerMargin": "Terminal inner margin",
  "settings.terminalGpuAcceleration": "Terminal GPU acceleration",
  "settings.terminalGpuAccelerationHint":
    "Auto uses WebGL when supported and stable. Only the focused terminal uses GPU rendering.",
  "settings.terminalGpuAcceleration.auto": "Auto",
  "settings.terminalGpuAcceleration.on": "On",
  "settings.terminalGpuAcceleration.off": "Off",
  "settings.terminalStartDirectory": "Terminal start directory",
  "settings.terminalStartDirectoryHint":
    "New empty terminals start here. Empty custom paths fall back to the home directory.",
  "settings.terminalStartDirectory.home": "Home directory",
  "settings.terminalStartDirectory.workspace": "Workspace project",
  "settings.terminalStartDirectory.custom": "Custom path",
  "settings.terminalStartCustomCwd": "Custom terminal start path",
  "settings.terminalStartCustomCwdPlaceholder": "D:\\Workspace\\project or /mnt/d/Workspace/project",
  "settings.terminalSplitBehavior": "Split pane behavior",
  "settings.terminalSplitBehaviorHint":
    "Choose whether split commands open a matching terminal immediately or leave an empty pane.",
  "settings.terminalSplitBehavior.cloneCurrent": "Clone current terminal",
  "settings.terminalSplitBehavior.empty": "Create empty pane",
  "settings.terminalLinkOpen": "Open terminal links in",
  "settings.terminalLinkOpenHint":
    "System browser is required for OAuth/login flows (e.g. Claude Code) to complete their localhost callback.",
  "settings.terminalLinkOpen.system": "System browser",
  "settings.terminalLinkOpen.inApp": "In-app browser",
  "surface.tab.actions": "Tab actions",
  "surface.tab.nameLabel": "Tab name",
  "surface.tab.rename": "Rename tab",
  "surface.tab.renameTitle": "Rename tab",
  "surface.tab.reset": "Restore automatic title",
  "session.status.attention": "Waiting for input",
  "session.status.running": "Running",
  "session.status.starting": "Starting",
  "session.status.recovering": "Recovering",
  "session.status.detached": "Detached",
  "session.status.disconnected": "Disconnected",
  "session.status.exited": "Exited",
  "session.status.failed": "Failed",
  "session.status.lost": "Lost",
  "workspace.status.needsInput": "Agent waiting for input",
  "workspace.status.running": "Session running",
  "workspace.status.sessionCount": "{count} sessions",
  "workspace.status.idle": "Idle",
  "statusbar.openPath": "Open {path} in File Explorer",
  "statusbar.openPathFailed": "Could not open folder",
  "sourceControl.changes": "Changes",
  "sourceControl.clean": "Working tree is clean.",
  "sourceControl.commit": "Commit staged changes",
  "sourceControl.commitCreated": "Commit created",
  "sourceControl.commitPlaceholder": "Commit message (Ctrl+Enter)",
  "sourceControl.diff": "Diff preview",
  "sourceControl.enterCommitMessage": "Enter a commit message.",
  "sourceControl.filterPlaceholder": "Filter changed files",
  "sourceControl.loading": "Loading repository status...",
  "sourceControl.loadMore": "Show {count} more",
  "sourceControl.noBranch": "No branch",
  "sourceControl.noDiff": "No textual diff is available.",
  "sourceControl.noMatchingChanges": "No matching changes.",
  "sourceControl.noStagedChanges": "Stage at least one change before committing.",
  "sourceControl.notRepository": "No Git repository",
  "sourceControl.notRepositoryDescription": "Open a terminal inside a Git repository or set the workspace root.",
  "sourceControl.resizeFileList": "Resize file list and diff preview",
  "sourceControl.selectFile": "Select a changed file to inspect its diff.",
  "sourceControl.shownCount": "Showing {shown} of {total}",
  "sourceControl.stageAll": "Stage all",
  "sourceControl.stageFile": "Stage {path}",
  "sourceControl.stagedChanges": "Staged changes",
  "sourceControl.syncState": "{ahead} ahead / {behind} behind",
  "sourceControl.title": "Source Control",
  "sourceControl.truncated": "Preview truncated",
  "sourceControl.unavailableServer": "Source control is not available in server mode yet.",
  "sourceControl.unstageAll": "Unstage all",
  "sourceControl.unstageFile": "Unstage {path}",
  "sourceControl.updating": "Updating repository...",
  "sourceControl.worktreeBase": "Base revision",
  "sourceControl.worktreeBranch": "Branch",
  "sourceControl.worktreeCommand": "Agent command",
  "sourceControl.worktreeCreateConfirm": "Create worktree",
  "sourceControl.worktreeCreateDescription": "Creates an AgentMux-owned worktree, workspace, and optional agent session as one recoverable operation.",
  "sourceControl.worktreeCreateTitle": "Create isolated agent worktree",
  "sourceControl.worktreeDestination": "Destination",
  "sourceControl.worktreeList": "Agent worktrees",
  "sourceControl.worktreeRecover": "Recover operation",
  "sourceControl.worktreeRemove": "Remove",
  "sourceControl.worktreeRemoveDescription": "Only AgentMux-owned worktree resources will be removed.",
  "sourceControl.worktreeRemoveTitle": "Remove isolated worktree?",
  "sourceControl.worktreeStarted": "Worktree operation started",
  "sourceControl.worktreeStateCompleted": "Completed",
  "sourceControl.worktreeStateFailed": "Failed",
  "sourceControl.worktreeStatePrepared": "Preparing",
  "sourceControl.worktreeStateRemoved": "Removed",
  "sourceControl.worktreeStateRolledBack": "Rolled back",
  "sourceControl.worktreeStateRollingBack": "Rolling back",
  "sourceControl.worktreeStateSessionCreated": "Session created",
  "sourceControl.worktreeStateUnknown": "Unknown status",
  "sourceControl.worktreeStateWorkspaceCreated": "Workspace created",
  "sourceControl.worktreeStateWorktreeCreated": "Worktree created",
  "sourceControl.reviewAdd": "Add comment",
  "sourceControl.reviewCommentOn": "Comment on {side} line {line}",
  "sourceControl.reviewDelete": "Delete",
  "sourceControl.reviewDeleteDescription": "This removes stored review comments.",
  "sourceControl.reviewDeleteTitle": "Delete review thread?",
  "sourceControl.reviewDoNotDeliver": "Do not deliver yet",
  "sourceControl.reviewMailbox": "Mailbox",
  "sourceControl.reviewPlaceholder": "Write review feedback",
  "sourceControl.reviewReopen": "Reopen",
  "sourceControl.reviewResolve": "Resolve",
  "sourceControl.reviewSideContext": "context",
  "sourceControl.reviewSideLeft": "left",
  "sourceControl.reviewSideRight": "right",
  "sourceControl.reviewSideUnknown": "unknown side",
  "sourceControl.reviewTerminal": "Terminal",
  "devServer.detectedTitle": "Development server detected",
  "devServer.detectedDescription": "{url} is ready to open beside the terminal.",
  "devServer.openInSplit": "Open in split",
  "devServer.openFailed": "Could not open the development server",
  "statusbar.surfaceSummary": "{surfaces} surfaces · {terminals} terminals · {running} running",
  "action.group.agent": "Agent",
  "action.group.terminal": "Terminal",
  "action.group.workspace": "Workspace",
  "action.group.view": "View",
  "action.group.remote": "Remote · WSL",
  "settings.workspace.noActiveProject": "No active workspace.",
  "settings.workspace.scope": "Workspace to edit",
  "settings.workspace.selector": "Workspace to edit",
  "settings.workspace.name": "Name",
  "settings.workspace.root": "Workspace root",
  "settings.workspace.description": "Description",
  "settings.workspace.icon": "Icon",
  "settings.workspace.color": "Color",
  "settings.workspace.defaultTerminal": "Default terminal",
  "settings.workspace.defaultWsl": "Default WSL distribution",
  "settings.workspace.defaultAgentCommand": "Default agent command",
  "settings.workspace.systemDefault": "System default",
  "settings.workspace.saveProject": "Save workspace",
  "settings.workspace.saveFailed": "Could not save workspace settings",
  "settings.workspace.title": "Workspace",
  "settings.workspace.unsavedTitle": "Discard unsaved workspace changes?",
  "settings.workspace.unsavedDescription":
    "Switching workspaces will discard changes you have not saved.",
  "settings.workspace.discardChanges": "Discard changes",
  "shortcuts.conflictsTitle": "Shortcut conflicts",
  "shortcuts.editDescription":
    "Focus a key field and press the combination. Use the second field only for a two-step chord.",
  "shortcuts.editTitle": "Edit shortcut: {action}",
  "shortcuts.firstStroke": "Shortcut",
  "shortcuts.invalidDescription":
    "Press one key combination or an optional two-step chord.",
  "shortcuts.invalidTitle": "Invalid shortcut",
  "shortcuts.replaceAction": "Replace",
  "shortcuts.replaceDescription": "{binding} is already assigned to {actions}.",
  "shortcuts.replaceDetail":
    "Replacing it will unassign the shortcut from those actions.",
  "shortcuts.replaceTitle": "Replace existing shortcut?",
  "shortcuts.save": "Save shortcut",
  "shortcuts.secondStroke": "Second key (optional chord)",
  "shortcuts.unassign": "Unassign",
  "updates.autoCheck": "Check for updates automatically",
  "updates.autoCheckHint": "AgentMux checks GitHub Releases at startup and periodically while it is running. Installation still requires your approval.",
  "updates.check": "Check for updates",
  "updates.install": "Download and install",
  "updates.notification.action": "Open update",
  "updates.notification.body": "AgentMux {version} is ready to download from Settings.",
  "updates.notification.fallback.failed": "Windows could not show the notification. The update remains available here.",
  "updates.notification.fallback.unsupported": "Native notifications are unavailable. The update remains available here.",
  "updates.notification.title": "AgentMux update available",
  "updates.releaseNotes": "Release notes",
  "updates.status.available": "Version {version} is available.",
  "updates.status.checking": "Checking for updates...",
  "updates.status.downloading": "Downloading update {progress}",
  "updates.status.error": "Update check failed: {message}",
  "updates.status.idle": "No update check has run yet.",
  "updates.status.installed": "Update installed. Relaunching AgentMux...",
  "updates.status.notAvailable": "AgentMux is up to date.",
  "updates.status.unsupported": "Updates are available in the packaged desktop app.",
  "updates.title": "Updates",
  "workspace.addSelectedToGroup": "Add selected workspaces",
  "workspace.add": "Add workspace",
  "workspace.createGroupFromSelection": "Create group from selection",
  "workspace.addToGroup": "Add workspace to group",
  "workspace.clearSelection": "Clear selection",
  "workspace.createGroup": "Create group",
  "workspace.filter": "Filter workspaces",
  "workspace.group.addWorkspace": "Add workspace to group",
  "workspace.group.moveDown": "Move group down",
  "workspace.group.moveUp": "Move group up",
  "workspace.group.colorDescription": "Hex color in #RRGGBB format.",
  "workspace.group.colorLabel": "Group color",
  "workspace.group.createAction": "Create",
  "workspace.group.createTitle": "Create workspace group",
  "workspace.group.deleteAction": "Delete group",
  "workspace.group.deleteDescription": "The workspaces in this group will remain.",
  "workspace.group.deleteTitle": "Delete {name}?",
  "workspace.group.editTitle": "Edit workspace group",
  "workspace.group.iconDescription": "One or two letters.",
  "workspace.group.iconLabel": "Group icon",
  "workspace.group.invalidColor": "Use a color such as #58A6FF.",
  "workspace.group.nameLabel": "Group name",
  "workspace.none": "No workspace",
  "workspace.section": "Workspaces",
  "workspace.settings": "Workspace settings",
  "workspace.selectedCount": "{count} selected",
};

const ko: Record<I18nKey, string> = {
  "app.sidebar.toggle": "사이드바 열기/닫기",
  "app.search.activeWindow": "활성 창 검색",
  "app.search.placeholder": "활성 창 검색",
  "app.commandPalette.open": "명령 팔레트 열기",
  "app.commandPalette.noResults": "결과 없음",
  "app.commandPalette.placeholder": "명령 실행 또는 워크스페이스 검색...",
  "app.commandPalette.shortcutClose": "esc 닫기",
  "app.commandPalette.shortcutMove": "위/아래 이동",
  "app.commandPalette.shortcutRun": "enter 실행",
  "app.panes.balance": "분할창 균등 정렬",
  "app.settings.open": "설정 열기",
  "app.version": "버전",
  "app.version.current": "현재 AgentMux 버전",
  "app.window.minimize": "최소화",
  "app.window.maximize": "최대화",
  "app.window.restore": "이전 크기로 복원",
  "app.window.close": "닫기",
  "appearance.dark": "다크",
  "appearance.light": "라이트",
  "common.active": "활성",
  "common.cancel": "취소",
  "common.clear": "지우기",
  "common.close": "닫기",
  "common.connect": "연결",
  "common.dismiss": "해제",
  "common.edit": "편집",
  "common.empty": "비어 있음",
  "common.idle": "대기",
  "common.invalid": "잘못됨",
  "common.ok": "정상",
  "common.reload": "다시 불러오기",
  "common.reset": "초기화",
  "common.save": "저장",
  "common.settings": "설정",
  "common.unassigned": "미지정",
  "config.configuration": "설정 파일",
  "config.export": "내보내기",
  "config.exportProject": "워크스페이스 내보내기",
  "config.globalPath": "전역 {path}",
  "config.import": "가져오기",
  "config.importProject": "워크스페이스 가져오기",
  "config.jsonOnlyHint": "agentmux.json에 저장됩니다. JSON 파일을 직접 편집하거나 가져오기/내보내기를 사용하세요.",
  "config.migrateCmux": ".cmux 마이그레이션",
  "config.projectPath": "워크스페이스 {path}",
  "config.reload": "다시 불러오기",
  "config.resetGlobalConfirm": "전역 AgentMux 설정을 초기화할까요?",
  "config.resetProject": "워크스페이스 초기화",
  "config.resetProjectConfirm": "워크스페이스 AgentMux 설정을 초기화할까요?",
  "config.scopeGlobal": "전역",
  "config.scopeProject": "워크스페이스",
  "language.english": "영어",
  "language.korean": "한국어",
  "language.label": "언어",
  "language.savedGlobally": "언어는 전역으로 저장되며 모든 워크스페이스에 적용됩니다.",
  "notifications.empty": "활성 알림이 없습니다.",
  "notifications.focus": "포커스",
  "notifications.open": "알림 열기",
  "notifications.severity.error": "오류",
  "notifications.severity.info": "정보",
  "notifications.severity.warning": "경고",
  "notifications.summary": "활성 {count}개",
  "notifications.title": "알림",
  "pane.empty": "빈 페인",
  "pane.invalidLayout": "잘못된 페인 레이아웃",
  "pane.restoring": "복원 중",
  "settings.appearance": "모양",
  "settings.advanced": "고급",
  "settings.diagnostics": "진단",
  "settings.general": "일반",
  "settings.keys": "키보드 단축키",
  "settings.profiles": "프로필",
  "settings.project": "워크스페이스",
  "settings.tabs.appearance": "모양",
  "settings.tabs.advanced": "고급",
  "settings.tabs.diagnostics": "진단",
  "settings.tabs.general": "일반",
  "settings.tabs.keys": "단축키",
  "settings.tabs.profiles": "프로필 및 SSH",
  "settings.tabs.workspace": "워크스페이스",
  "settings.theme": "테마",
  "settings.accentColor": "강조 색상",
  "settings.uiFontSize": "UI 글자 크기",
  "settings.terminalInnerMargin": "터미널 내부 여백",
  "settings.terminalGpuAcceleration": "터미널 GPU 가속",
  "settings.terminalGpuAccelerationHint":
    "자동은 WebGL을 사용할 수 있고 안정적인 환경에서 활성화합니다. 포커스된 터미널만 GPU 렌더링을 사용합니다.",
  "settings.terminalGpuAcceleration.auto": "자동",
  "settings.terminalGpuAcceleration.on": "켜기",
  "settings.terminalGpuAcceleration.off": "끄기",
  "settings.terminalLinkOpen": "터미널 링크 열기",
  "settings.terminalLinkOpenHint":
    "OAuth/로그인 흐름(예: Claude Code)이 localhost 콜백을 완료하려면 시스템 브라우저가 필요합니다.",
  "settings.terminalLinkOpen.system": "시스템 브라우저",
  "settings.terminalLinkOpen.inApp": "앱 내부 브라우저",
  "surface.tab.actions": "탭 작업",
  "surface.tab.nameLabel": "탭 이름",
  "surface.tab.rename": "탭 이름 바꾸기",
  "surface.tab.renameTitle": "탭 이름 바꾸기",
  "surface.tab.reset": "자동 이름으로 복원",
  "settings.workspace.noActiveProject": "활성 워크스페이스가 없습니다.",
  "settings.workspace.scope": "편집할 워크스페이스",
  "settings.workspace.selector": "편집할 워크스페이스",
  "settings.workspace.name": "이름",
  "settings.workspace.root": "워크스페이스 루트",
  "settings.workspace.description": "설명",
  "settings.workspace.icon": "아이콘",
  "settings.workspace.color": "색상",
  "settings.workspace.defaultTerminal": "기본 터미널",
  "settings.workspace.defaultWsl": "기본 WSL 배포판",
  "settings.workspace.defaultAgentCommand": "기본 에이전트 명령",
  "settings.workspace.systemDefault": "시스템 기본값",
  "settings.workspace.saveProject": "워크스페이스 저장",
  "settings.workspace.saveFailed": "워크스페이스 설정을 저장하지 못했습니다",
  "settings.workspace.title": "워크스페이스",
  "settings.workspace.unsavedTitle": "저장하지 않은 변경사항을 버릴까요?",
  "settings.workspace.unsavedDescription":
    "다른 워크스페이스로 전환하면 저장하지 않은 변경사항이 사라집니다.",
  "settings.workspace.discardChanges": "변경사항 버리기",
  "updates.autoCheck": "자동으로 업데이트 확인",
  "updates.autoCheckHint": "AgentMux가 시작될 때와 실행 중에 주기적으로 GitHub Release를 확인합니다. 설치는 사용자가 승인해야 진행됩니다.",
  "updates.check": "업데이트 확인",
  "updates.install": "다운로드 및 설치",
  "updates.notification.action": "업데이트 열기",
  "updates.notification.body": "설정에서 AgentMux {version}을 다운로드할 수 있습니다.",
  "updates.notification.fallback.failed": "Windows 알림을 표시하지 못했습니다. 여기에서 업데이트를 계속 진행할 수 있습니다.",
  "updates.notification.fallback.unsupported": "네이티브 알림을 사용할 수 없습니다. 여기에서 업데이트를 계속 진행할 수 있습니다.",
  "updates.notification.title": "AgentMux 업데이트 사용 가능",
  "updates.releaseNotes": "릴리스 노트",
  "updates.status.available": "버전 {version} 업데이트가 있습니다.",
  "updates.status.checking": "업데이트 확인 중...",
  "updates.status.downloading": "업데이트 다운로드 중 {progress}",
  "updates.status.error": "업데이트 확인 실패: {message}",
  "updates.status.idle": "아직 업데이트를 확인하지 않았습니다.",
  "updates.status.installed": "업데이트를 설치했습니다. AgentMux를 다시 시작합니다...",
  "updates.status.notAvailable": "AgentMux가 최신 상태입니다.",
  "updates.status.unsupported": "업데이트는 패키징된 데스크톱 앱에서 사용할 수 있습니다.",
  "updates.title": "업데이트",
  "workspace.addSelectedToGroup": "선택한 워크스페이스 추가",
  "workspace.add": "워크스페이스 추가",
  "workspace.createGroupFromSelection": "선택 항목으로 그룹 만들기",
  "workspace.addToGroup": "그룹에 워크스페이스 추가",
  "workspace.clearSelection": "선택 해제",
  "workspace.createGroup": "그룹 만들기",
  "workspace.filter": "워크스페이스 필터",
  "workspace.group.addWorkspace": "그룹에 워크스페이스 추가",
  "workspace.group.moveDown": "그룹 아래로 이동",
  "workspace.group.moveUp": "그룹 위로 이동",
  "workspace.none": "워크스페이스 없음",
  "workspace.section": "워크스페이스",
  "workspace.settings": "워크스페이스 설정",
  "workspace.selectedCount": "{count}개 선택",
  "settings.terminalStartDirectory": "터미널 시작 위치",
  "settings.terminalStartDirectoryHint":
    "새 빈 터미널이 이 위치에서 시작됩니다. 사용자 지정 경로가 비어 있으면 홈 디렉터리를 사용합니다.",
  "settings.terminalStartDirectory.home": "홈 디렉터리",
  "settings.terminalStartDirectory.workspace": "워크스페이스 프로젝트",
  "settings.terminalStartDirectory.custom": "사용자 지정 경로",
  "settings.terminalStartCustomCwd": "사용자 지정 터미널 시작 경로",
  "settings.terminalStartCustomCwdPlaceholder": "D:\\Workspace\\project 또는 /mnt/d/Workspace/project",
  "settings.terminalSplitBehavior": "분할창 동작",
  "settings.terminalSplitBehaviorHint":
    "분할 명령에서 현재 터미널을 같은 위치로 바로 열지, 빈 pane을 만들지 선택합니다.",
  "settings.terminalSplitBehavior.cloneCurrent": "현재 터미널 복제",
  "settings.terminalSplitBehavior.empty": "빈 pane 만들기",
  "session.status.attention": "입력 대기",
  "session.status.running": "실행 중",
  "session.status.starting": "시작 중",
  "session.status.recovering": "복구 중",
  "session.status.detached": "분리됨",
  "session.status.disconnected": "연결 끊김",
  "session.status.exited": "종료됨",
  "session.status.failed": "실패",
  "session.status.lost": "손실됨",
  "workspace.status.needsInput": "에이전트가 입력을 기다리는 중",
  "workspace.status.running": "세션 실행 중",
  "workspace.status.sessionCount": "{count}개 세션",
  "workspace.status.idle": "대기 중",
  "statusbar.openPath": "파일 탐색기에서 {path} 열기",
  "statusbar.openPathFailed": "폴더를 열 수 없습니다",
  "statusbar.surfaceSummary": "{surfaces} surface · {terminals} 터미널 · {running} 실행",
  "action.group.agent": "에이전트",
  "action.group.terminal": "터미널",
  "action.group.workspace": "워크스페이스",
  "action.group.view": "보기",
  "action.group.remote": "원격 · WSL",
  "dialog.dismissToast": "{title} 알림 닫기",
  "dialog.confirm": "확인",
  "dialog.pressKey": "키 조합을 누르세요",
  "dialog.pressSecondKey": "두 번째 키 조합을 누르세요",
  "dialog.required": "{label} 항목은 필수입니다.",
  "browser.dialog.allow": "허용",
  "browser.dialog.confirmTitle": "{source}에서 확인을 요청합니다",
  "browser.dialog.expiredDescription": "브라우저 대화 상자를 완료하지 못했습니다.",
  "browser.dialog.expiredTitle": "브라우저 대화 상자 만료",
  "browser.dialog.promptLabel": "값",
  "browser.dialog.promptTitle": "{source}에서 입력을 요청합니다",
  "browser.dialog.submit": "제출",
  "shortcuts.conflictsTitle": "단축키 충돌",
  "shortcuts.editDescription":
    "키 입력란에 포커스를 두고 조합을 누르세요. 두 단계 단축키일 때만 두 번째 입력란을 사용합니다.",
  "shortcuts.editTitle": "단축키 편집: {action}",
  "shortcuts.firstStroke": "단축키",
  "shortcuts.invalidDescription":
    "하나의 키 조합 또는 선택적인 두 단계 단축키를 입력하세요.",
  "shortcuts.invalidTitle": "잘못된 단축키",
  "shortcuts.replaceAction": "교체",
  "shortcuts.replaceDescription": "{binding} 단축키는 이미 {actions}에 할당되어 있습니다.",
  "shortcuts.replaceDetail": "교체하면 해당 작업에서 기존 단축키 할당이 해제됩니다.",
  "shortcuts.replaceTitle": "기존 단축키를 교체할까요?",
  "shortcuts.save": "단축키 저장",
  "shortcuts.secondStroke": "두 번째 키(선택적 두 단계 단축키)",
  "shortcuts.unassign": "할당 해제",
  "workspace.group.colorDescription": "#RRGGBB 형식의 16진수 색상입니다.",
  "workspace.group.colorLabel": "그룹 색상",
  "workspace.group.createAction": "만들기",
  "workspace.group.createTitle": "워크스페이스 그룹 만들기",
  "workspace.group.deleteAction": "그룹 삭제",
  "workspace.group.deleteDescription": "그룹에 포함된 워크스페이스는 유지됩니다.",
  "workspace.group.deleteTitle": "{name} 그룹을 삭제할까요?",
  "workspace.group.editTitle": "워크스페이스 그룹 편집",
  "workspace.group.iconDescription": "한두 글자를 입력하세요.",
  "workspace.group.iconLabel": "그룹 아이콘",
  "workspace.group.invalidColor": "#58A6FF 같은 색상을 사용하세요.",
  "workspace.group.nameLabel": "그룹 이름",
  "common.loading": "불러오는 중...",
  "common.refresh": "새로 고침",
  "sourceControl.changes": "변경 사항",
  "sourceControl.clean": "작업 트리가 깨끗합니다.",
  "sourceControl.commit": "스테이징된 변경 커밋",
  "sourceControl.commitCreated": "커밋 생성됨",
  "sourceControl.commitPlaceholder": "커밋 메시지 (Ctrl+Enter)",
  "sourceControl.diff": "Diff 미리 보기",
  "sourceControl.enterCommitMessage": "커밋 메시지를 입력하세요.",
  "sourceControl.filterPlaceholder": "변경된 파일 필터",
  "sourceControl.loading": "저장소 상태를 불러오는 중...",
  "sourceControl.loadMore": "{count}개 더 보기",
  "sourceControl.noBranch": "브랜치 없음",
  "sourceControl.noDiff": "표시할 텍스트 Diff가 없습니다.",
  "sourceControl.noMatchingChanges": "일치하는 변경 사항이 없습니다.",
  "sourceControl.noStagedChanges": "변경 사항을 하나 이상 스테이징한 뒤 커밋하세요.",
  "sourceControl.notRepository": "Git 저장소가 아닙니다",
  "sourceControl.notRepositoryDescription": "Git 저장소 안에서 터미널을 열거나 워크스페이스 루트를 설정하세요.",
  "sourceControl.resizeFileList": "파일 목록과 Diff 미리 보기 크기 조절",
  "sourceControl.selectFile": "변경된 파일을 선택하면 Diff를 확인할 수 있습니다.",
  "sourceControl.shownCount": "{total}개 중 {shown}개 표시",
  "sourceControl.stageAll": "모두 스테이징",
  "sourceControl.stageFile": "{path} 스테이징",
  "sourceControl.stagedChanges": "스테이징된 변경 사항",
  "sourceControl.syncState": "앞섬 {ahead} / 뒤처짐 {behind}",
  "sourceControl.title": "소스 제어",
  "sourceControl.truncated": "미리 보기 일부만 표시됨",
  "sourceControl.unavailableServer": "서버 모드의 소스 제어는 아직 지원되지 않습니다.",
  "sourceControl.unstageAll": "모두 스테이징 해제",
  "sourceControl.unstageFile": "{path} 스테이징 해제",
  "sourceControl.updating": "저장소 변경 적용 중...",
  "sourceControl.worktreeBase": "기준 리비전",
  "sourceControl.worktreeBranch": "브랜치",
  "sourceControl.worktreeCommand": "에이전트 명령",
  "sourceControl.worktreeCreateConfirm": "worktree 만들기",
  "sourceControl.worktreeCreateDescription": "AgentMux 소유 worktree, 워크스페이스, 선택적 에이전트 세션을 하나의 복구 가능한 작업으로 만듭니다.",
  "sourceControl.worktreeCreateTitle": "격리된 에이전트 worktree 만들기",
  "sourceControl.worktreeDestination": "대상 경로",
  "sourceControl.worktreeList": "에이전트 worktree",
  "sourceControl.worktreeRecover": "작업 복구",
  "sourceControl.worktreeRemove": "제거",
  "sourceControl.worktreeRemoveDescription": "AgentMux가 소유한 worktree 리소스만 제거됩니다.",
  "sourceControl.worktreeRemoveTitle": "격리된 worktree를 제거할까요?",
  "sourceControl.worktreeStarted": "worktree 작업 시작됨",
  "sourceControl.worktreeStateCompleted": "완료됨",
  "sourceControl.worktreeStateFailed": "실패함",
  "sourceControl.worktreeStatePrepared": "준비 중",
  "sourceControl.worktreeStateRemoved": "제거됨",
  "sourceControl.worktreeStateRolledBack": "되돌림 완료",
  "sourceControl.worktreeStateRollingBack": "되돌리는 중",
  "sourceControl.worktreeStateSessionCreated": "세션 생성됨",
  "sourceControl.worktreeStateUnknown": "알 수 없는 상태",
  "sourceControl.worktreeStateWorkspaceCreated": "워크스페이스 생성됨",
  "sourceControl.worktreeStateWorktreeCreated": "worktree 생성됨",
  "sourceControl.reviewAdd": "코멘트 추가",
  "sourceControl.reviewCommentOn": "{side} {line}번 줄에 코멘트",
  "sourceControl.reviewDelete": "삭제",
  "sourceControl.reviewDeleteDescription": "저장된 리뷰 코멘트를 제거합니다.",
  "sourceControl.reviewDeleteTitle": "리뷰 thread를 삭제할까요?",
  "sourceControl.reviewDoNotDeliver": "아직 전달하지 않음",
  "sourceControl.reviewMailbox": "메일박스",
  "sourceControl.reviewPlaceholder": "리뷰 피드백 작성",
  "sourceControl.reviewReopen": "다시 열기",
  "sourceControl.reviewResolve": "해결",
  "sourceControl.reviewSideContext": "문맥",
  "sourceControl.reviewSideLeft": "왼쪽",
  "sourceControl.reviewSideRight": "오른쪽",
  "sourceControl.reviewSideUnknown": "알 수 없는 쪽",
  "sourceControl.reviewTerminal": "터미널",
  "devServer.detectedTitle": "개발 서버 감지됨",
  "devServer.detectedDescription": "{url}을(를) 터미널 옆에서 열 수 있습니다.",
  "devServer.openInSplit": "분할로 열기",
  "devServer.openFailed": "개발 서버를 열지 못했습니다",
};

const resources: Record<AppLocaleLanguage, Record<I18nKey, string>> = {
  en,
  ko,
};

export function normalizeLanguage(value: string | null | undefined): AppLocaleLanguage {
  const normalized = value?.trim().toLowerCase();
  return normalized === "ko" || normalized === "ko-kr" || normalized === "ko_kr"
    ? "ko"
    : "en";
}

export function createTranslator(language: AppLocaleLanguage): Translator {
  return (key, values) => {
    let text = resources[language][key] ?? resources.en[key] ?? key;
    if (values) {
      for (const [name, value] of Object.entries(values)) {
        text = text.replaceAll(`{${name}}`, String(value));
      }
    }
    return text;
  };
}

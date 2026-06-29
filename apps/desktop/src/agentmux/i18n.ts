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
  | "common.ok"
  | "common.reload"
  | "common.reset"
  | "common.save"
  | "common.settings"
  | "common.unassigned"
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
  | "language.english"
  | "language.korean"
  | "language.label"
  | "language.savedGlobally"
  | "notifications.empty"
  | "pane.empty"
  | "pane.invalidLayout"
  | "pane.restoring"
  | "settings.appearance"
  | "settings.diagnostics"
  | "settings.general"
  | "settings.keys"
  | "settings.profiles"
  | "settings.project"
  | "settings.tabs.appearance"
  | "settings.tabs.diagnostics"
  | "settings.tabs.general"
  | "settings.tabs.keys"
  | "settings.tabs.profiles"
  | "settings.tabs.workspace"
  | "settings.theme"
  | "settings.accentColor"
  | "settings.uiFontSize"
  | "settings.terminalInnerMargin"
  | "settings.terminalLinkOpen"
  | "settings.terminalLinkOpenHint"
  | "settings.terminalLinkOpen.system"
  | "settings.terminalLinkOpen.inApp"
  | "settings.workspace.noActiveProject"
  | "settings.workspace.saveProject"
  | "settings.workspace.title"
  | "updates.autoCheck"
  | "updates.autoCheckHint"
  | "updates.check"
  | "updates.install"
  | "updates.notification.body"
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
  "common.ok": "ok",
  "common.reload": "Reload",
  "common.reset": "Reset",
  "common.save": "Save",
  "common.settings": "Settings",
  "common.unassigned": "Unassigned",
  "config.configuration": "Configuration",
  "config.export": "Export",
  "config.exportProject": "Export project",
  "config.globalPath": "global {path}",
  "config.import": "Import",
  "config.importProject": "Import project",
  "config.jsonOnlyHint": "Saved in agentmux.json. Edit the JSON file directly or use import/export.",
  "config.migrateCmux": "Migrate .cmux",
  "config.projectPath": "project {path}",
  "config.reload": "Reload",
  "config.resetGlobalConfirm": "Reset global AgentMux config?",
  "config.resetProject": "Reset project",
  "config.resetProjectConfirm": "Reset project AgentMux config?",
  "config.scopeGlobal": "Global",
  "config.scopeProject": "Project",
  "language.english": "English",
  "language.korean": "Korean",
  "language.label": "Language",
  "language.savedGlobally": "Language is saved globally and applies to every workspace.",
  "notifications.empty": "No active notifications.",
  "pane.empty": "Empty pane",
  "pane.invalidLayout": "Invalid pane layout",
  "pane.restoring": "Restoring",
  "settings.appearance": "Appearance",
  "settings.diagnostics": "Diagnostics",
  "settings.general": "General and notifications",
  "settings.keys": "Keyboard shortcuts",
  "settings.profiles": "Profiles",
  "settings.project": "Project",
  "settings.tabs.appearance": "Appearance",
  "settings.tabs.diagnostics": "Diagnostics",
  "settings.tabs.general": "General",
  "settings.tabs.keys": "Shortcuts",
  "settings.tabs.profiles": "Profiles and SSH",
  "settings.tabs.workspace": "Project",
  "settings.theme": "Theme",
  "settings.accentColor": "Accent color",
  "settings.uiFontSize": "UI font size",
  "settings.terminalInnerMargin": "Terminal inner margin",
  "settings.terminalLinkOpen": "Open terminal links in",
  "settings.terminalLinkOpenHint":
    "System browser is required for OAuth/login flows (e.g. Claude Code) to complete their localhost callback.",
  "settings.terminalLinkOpen.system": "System browser",
  "settings.terminalLinkOpen.inApp": "In-app browser",
  "settings.workspace.noActiveProject": "No active project.",
  "settings.workspace.saveProject": "Save project",
  "settings.workspace.title": "Project",
  "updates.autoCheck": "Check for updates automatically",
  "updates.autoCheckHint": "AgentMux checks GitHub Releases at startup. Installation still requires your approval.",
  "updates.check": "Check for updates",
  "updates.install": "Download and install",
  "updates.notification.body": "AgentMux {version} is ready to download from Settings.",
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
  "workspace.none": "No workspace",
  "workspace.section": "Workspaces",
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
  "config.exportProject": "프로젝트 내보내기",
  "config.globalPath": "전역 {path}",
  "config.import": "가져오기",
  "config.importProject": "프로젝트 가져오기",
  "config.jsonOnlyHint": "agentmux.json에 저장됩니다. JSON 파일을 직접 편집하거나 가져오기/내보내기를 사용하세요.",
  "config.migrateCmux": ".cmux 마이그레이션",
  "config.projectPath": "프로젝트 {path}",
  "config.reload": "다시 불러오기",
  "config.resetGlobalConfirm": "전역 AgentMux 설정을 초기화할까요?",
  "config.resetProject": "프로젝트 초기화",
  "config.resetProjectConfirm": "프로젝트 AgentMux 설정을 초기화할까요?",
  "config.scopeGlobal": "전역",
  "config.scopeProject": "프로젝트",
  "language.english": "영어",
  "language.korean": "한국어",
  "language.label": "언어",
  "language.savedGlobally": "언어는 전역으로 저장되며 모든 워크스페이스에 적용됩니다.",
  "notifications.empty": "활성 알림이 없습니다.",
  "pane.empty": "빈 페인",
  "pane.invalidLayout": "잘못된 페인 레이아웃",
  "pane.restoring": "복원 중",
  "settings.appearance": "모양",
  "settings.diagnostics": "진단",
  "settings.general": "일반 및 알림",
  "settings.keys": "키보드 단축키",
  "settings.profiles": "프로필",
  "settings.project": "프로젝트",
  "settings.tabs.appearance": "모양",
  "settings.tabs.diagnostics": "진단",
  "settings.tabs.general": "일반",
  "settings.tabs.keys": "단축키",
  "settings.tabs.profiles": "프로필 및 SSH",
  "settings.tabs.workspace": "프로젝트",
  "settings.theme": "테마",
  "settings.accentColor": "강조 색상",
  "settings.uiFontSize": "UI 글자 크기",
  "settings.terminalInnerMargin": "터미널 내부 여백",
  "settings.terminalLinkOpen": "터미널 링크 열기",
  "settings.terminalLinkOpenHint":
    "OAuth/로그인 흐름(예: Claude Code)이 localhost 콜백을 완료하려면 시스템 브라우저가 필요합니다.",
  "settings.terminalLinkOpen.system": "시스템 브라우저",
  "settings.terminalLinkOpen.inApp": "앱 내부 브라우저",
  "settings.workspace.noActiveProject": "활성 프로젝트가 없습니다.",
  "settings.workspace.saveProject": "프로젝트 저장",
  "settings.workspace.title": "프로젝트",
  "updates.autoCheck": "자동으로 업데이트 확인",
  "updates.autoCheckHint": "AgentMux가 시작될 때 GitHub Release를 확인합니다. 설치는 사용자가 승인해야 진행됩니다.",
  "updates.check": "업데이트 확인",
  "updates.install": "다운로드 및 설치",
  "updates.notification.body": "설정에서 AgentMux {version}을 다운로드할 수 있습니다.",
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
  "workspace.selectedCount": "{count}개 선택",
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

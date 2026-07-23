import {
  useCallback,
  useEffect,
  useMemo,
  useRef,
  useState,
  type KeyboardEvent as ReactKeyboardEvent,
  type PointerEvent as ReactPointerEvent,
} from "react";
import type {
  ControlClient,
  GitDiff,
  GitFileChange,
  GitStatus,
  WorkspaceSummary,
} from "../control/ControlClient";
import { useAppDialogs } from "./dialogs";
import { IconBranch, IconClose, IconPlus, IconReset, IconSearch } from "./icons";
import type { Translator } from "./i18n";

interface SourceControlPanelProps {
  client: ControlClient;
  workspace: WorkspaceSummary;
  onClose: () => void;
  onRepositoryChanged: () => void | Promise<void>;
  t: Translator;
}

interface GitSelection {
  path: string;
  staged: boolean;
  untracked: boolean;
}

interface StatusRequest {
  workspaceId: string;
  promise: Promise<GitStatus>;
}

const MAX_RENDERED_CHANGES_PER_SECTION = 500;
const STATUS_POLL_INTERVAL_MS = 15_000;
const STATUS_CACHE_TTL_MS = STATUS_POLL_INTERVAL_MS;
const FILTER_VISIBILITY_THRESHOLD = 12;
const DEFAULT_CHANGES_SPLIT_RATIO = 0.52;
const MIN_CHANGES_HEIGHT = 116;
const MIN_DIFF_HEIGHT = 140;
const SPLIT_KEYBOARD_STEP = 24;

interface CachedGitStatus {
  status: GitStatus;
  updatedAt: number;
}

const statusCacheByClient = new WeakMap<ControlClient, Map<string, CachedGitStatus>>();
const statusRequestsByClient = new WeakMap<ControlClient, Map<string, StatusRequest>>();

function statusCacheFor(client: ControlClient): Map<string, CachedGitStatus> {
  let cache = statusCacheByClient.get(client);
  if (!cache) {
    cache = new Map();
    statusCacheByClient.set(client, cache);
  }
  return cache;
}

function statusRequestsFor(client: ControlClient): Map<string, StatusRequest> {
  let requests = statusRequestsByClient.get(client);
  if (!requests) {
    requests = new Map();
    statusRequestsByClient.set(client, requests);
  }
  return requests;
}

function selectionKey(selection: GitSelection): string {
  return `${selection.staged ? "staged" : "working"}:${selection.path}`;
}

function pathsForChange(change: GitFileChange): string[] {
  return change.originalPath
    ? [change.path, change.originalPath]
    : [change.path];
}

function selectionExists(status: GitStatus, selection: GitSelection): boolean {
  return status.files.some(
    (change) =>
      change.path === selection.path &&
      (selection.staged ? change.staged : change.unstaged),
  );
}

function changeBadge(change: GitFileChange, staged: boolean): string {
  if (change.conflict) return "!";
  if (change.untracked) return "?";
  const status = staged ? change.indexStatus : change.worktreeStatus;
  return status === "." ? "M" : status;
}

function GitChangeRow({
  change,
  staged,
  selected,
  disabled,
  onSelect,
  onToggleStage,
  t,
}: {
  change: GitFileChange;
  staged: boolean;
  selected: boolean;
  disabled: boolean;
  onSelect: () => void;
  onToggleStage: () => void;
  t: Translator;
}) {
  const segments = change.path.replaceAll("\\", "/").split("/");
  const name = segments.pop() ?? change.path;
  const directory = segments.join("/");
  const actionLabel = staged
    ? t("sourceControl.unstageFile", { path: change.path })
    : t("sourceControl.stageFile", { path: change.path });
  return (
    <div
      className="agentmux-source-control__change"
      data-selected={selected ? "true" : undefined}
      data-testid={`source-control-change-${staged ? "staged" : "working"}`}
    >
      <button
        type="button"
        className="agentmux-source-control__change-main"
        onClick={onSelect}
        title={change.path}
      >
        <span
          className="agentmux-source-control__badge"
          data-status={changeBadge(change, staged)}
        >
          {changeBadge(change, staged)}
        </span>
        <span className="agentmux-source-control__change-label">
          <span className="agentmux-source-control__filename">{name}</span>
          {directory ? (
            <span className="agentmux-source-control__directory">{directory}</span>
          ) : null}
        </span>
      </button>
      <button
        type="button"
        className="agentmux-source-control__row-action"
        onClick={onToggleStage}
        disabled={disabled}
        title={actionLabel}
        aria-label={actionLabel}
      >
        {staged ? <IconClose size={10} /> : <IconPlus size={11} />}
      </button>
    </div>
  );
}

export function SourceControlPanel({
  client,
  workspace,
  onClose,
  onRepositoryChanged,
  t,
}: SourceControlPanelProps) {
  const dialogs = useAppDialogs();
  const [status, setStatus] = useState<GitStatus | null>(null);
  const [selection, setSelection] = useState<GitSelection | null>(null);
  const [diff, setDiff] = useState<GitDiff | null>(null);
  const [commitMessage, setCommitMessage] = useState("");
  const [error, setError] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);
  const [statusLoading, setStatusLoading] = useState(true);
  const [filter, setFilter] = useState("");
  const [diffRevision, setDiffRevision] = useState(0);
  const [changesSplitRatio, setChangesSplitRatio] = useState(
    DEFAULT_CHANGES_SPLIT_RATIO,
  );
  const [resizingSplit, setResizingSplit] = useState(false);
  const [stagedRenderLimit, setStagedRenderLimit] = useState(
    MAX_RENDERED_CHANGES_PER_SECTION,
  );
  const [workingRenderLimit, setWorkingRenderLimit] = useState(
    MAX_RENDERED_CHANGES_PER_SECTION,
  );
  const busyRef = useRef(false);
  const statusSequenceRef = useRef(0);
  const diffSequenceRef = useRef(0);
  const workspaceIdRef = useRef(workspace.workspaceId);
  const splitContainerRef = useRef<HTMLDivElement>(null);
  const splitHandleRef = useRef<HTMLDivElement>(null);

  useEffect(() => {
    workspaceIdRef.current = workspace.workspaceId;
    statusSequenceRef.current += 1;
    diffSequenceRef.current += 1;
    setStatus(null);
    setSelection(null);
    setDiff(null);
    setCommitMessage("");
    setError(null);
    setStatusLoading(true);
    setStagedRenderLimit(MAX_RENDERED_CHANGES_PER_SECTION);
    setWorkingRenderLimit(MAX_RENDERED_CHANGES_PER_SECTION);
  }, [workspace.workspaceId]);

  const setChangesHeight = useCallback((requestedHeight: number) => {
    const container = splitContainerRef.current;
    if (!container) return;
    const handleHeight = splitHandleRef.current?.getBoundingClientRect().height ?? 7;
    const availableHeight = Math.max(1, container.clientHeight - handleHeight);
    const minimum = Math.min(MIN_CHANGES_HEIGHT, availableHeight);
    const maximum = Math.max(minimum, availableHeight - MIN_DIFF_HEIGHT);
    const nextHeight = Math.min(maximum, Math.max(minimum, requestedHeight));
    setChangesSplitRatio(nextHeight / availableHeight);
  }, []);

  const resizeFromPointer = useCallback(
    (clientY: number) => {
      const container = splitContainerRef.current;
      if (!container) return;
      const handleHeight = splitHandleRef.current?.getBoundingClientRect().height ?? 7;
      const bounds = container.getBoundingClientRect();
      setChangesHeight(clientY - bounds.top - handleHeight / 2);
    },
    [setChangesHeight],
  );

  const finishSplitResize = useCallback(
    (event?: ReactPointerEvent<HTMLDivElement>) => {
      if (
        event &&
        event.currentTarget.hasPointerCapture(event.pointerId)
      ) {
        event.currentTarget.releasePointerCapture(event.pointerId);
      }
      setResizingSplit(false);
    },
    [],
  );

  const handleSplitPointerDown = useCallback(
    (event: ReactPointerEvent<HTMLDivElement>) => {
      if (event.button !== 0) return;
      event.preventDefault();
      event.currentTarget.setPointerCapture(event.pointerId);
      setResizingSplit(true);
      resizeFromPointer(event.clientY);
    },
    [resizeFromPointer],
  );

  const handleSplitKeyDown = useCallback(
    (event: ReactKeyboardEvent<HTMLDivElement>) => {
      const container = splitContainerRef.current;
      if (!container) return;
      const handleHeight = splitHandleRef.current?.getBoundingClientRect().height ?? 7;
      const availableHeight = Math.max(1, container.clientHeight - handleHeight);
      const currentHeight = changesSplitRatio * availableHeight;
      let nextHeight: number | null = null;
      if (event.key === "ArrowUp") nextHeight = currentHeight - SPLIT_KEYBOARD_STEP;
      if (event.key === "ArrowDown") nextHeight = currentHeight + SPLIT_KEYBOARD_STEP;
      if (event.key === "Home") nextHeight = MIN_CHANGES_HEIGHT;
      if (event.key === "End") nextHeight = availableHeight - MIN_DIFF_HEIGHT;
      if (nextHeight === null) return;
      event.preventDefault();
      setChangesHeight(nextHeight);
    },
    [changesSplitRatio, setChangesHeight],
  );

  const refreshStatus = useCallback(
    async (options: { force?: boolean; showLoading?: boolean } = {}) => {
      const workspaceId = workspace.workspaceId;
      const { force = false, showLoading = false } = options;
      const sequence = ++statusSequenceRef.current;
      const cache = statusCacheFor(client);
      const cached = cache.get(workspaceId);
      if (
        !force &&
        cached &&
        Date.now() - cached.updatedAt < STATUS_CACHE_TTL_MS
      ) {
        if (
          workspaceId === workspaceIdRef.current &&
          sequence === statusSequenceRef.current
        ) {
          setStatus(cached.status);
          setError(null);
          setSelection((current) =>
            current && selectionExists(cached.status, current) ? current : null,
          );
          if (showLoading) setStatusLoading(false);
        }
        return cached.status;
      }

      if (showLoading) setStatusLoading(true);
      const requests = statusRequestsFor(client);
      let statusRequest = !force ? requests.get(workspaceId) : undefined;
      if (!statusRequest) {
        const promise = client.getGitStatus(workspaceId);
        statusRequest = { workspaceId, promise };
        requests.set(workspaceId, statusRequest);
        void promise.then(
          () => {
            if (requests.get(workspaceId) === statusRequest) {
              requests.delete(workspaceId);
            }
          },
          () => {
            if (requests.get(workspaceId) === statusRequest) {
              requests.delete(workspaceId);
            }
          },
        );
      }
      try {
        const next = await statusRequest.promise;
        if (
          workspaceId !== workspaceIdRef.current ||
          sequence !== statusSequenceRef.current
        ) {
          return next;
        }
        cache.set(workspaceId, {
          status: next,
          updatedAt: Date.now(),
        });
        setStatus(next);
        setError(null);
        setSelection((current) =>
          current && selectionExists(next, current)
            ? current
            : null,
        );
        return next;
      } catch (cause) {
        if (
          workspaceId === workspaceIdRef.current &&
          sequence === statusSequenceRef.current
        ) {
          setError(cause instanceof Error ? cause.message : String(cause));
        }
        throw cause;
      } finally {
        if (
          showLoading &&
          workspaceId === workspaceIdRef.current &&
          sequence === statusSequenceRef.current
        ) {
          setStatusLoading(false);
        }
      }
    },
    [client, workspace.workspaceId],
  );

  useEffect(() => {
    void refreshStatus({ showLoading: true }).catch(() => undefined);
    const timer = window.setInterval(() => {
      if (document.visibilityState === "visible") {
        void refreshStatus().catch(() => undefined);
      }
    }, STATUS_POLL_INTERVAL_MS);
    const refreshOnFocus = () => {
      void refreshStatus().catch(() => undefined);
    };
    window.addEventListener("focus", refreshOnFocus);
    return () => {
      window.clearInterval(timer);
      window.removeEventListener("focus", refreshOnFocus);
    };
  }, [refreshStatus]);

  useEffect(() => {
    if (!selection) {
      diffSequenceRef.current += 1;
      setDiff(null);
      return;
    }
    const sequence = ++diffSequenceRef.current;
    setDiff(null);
    void client
      .getGitDiff(workspace.workspaceId, selection.path, {
        staged: selection.staged,
        untracked: selection.untracked,
      })
      .then((result) => {
        if (sequence === diffSequenceRef.current) setDiff(result);
      })
      .catch((cause) => {
        if (sequence === diffSequenceRef.current) {
          setError(cause instanceof Error ? cause.message : String(cause));
        }
      });
  }, [client, diffRevision, selection, workspace.workspaceId]);

  useEffect(() => {
    const handleKeyDown = (event: KeyboardEvent) => {
      if (event.key === "Escape") onClose();
    };
    window.addEventListener("keydown", handleKeyDown);
    return () => window.removeEventListener("keydown", handleKeyDown);
  }, [onClose]);

  const stagedChanges = useMemo(
    () => status?.files.filter((change) => change.staged) ?? [],
    [status],
  );
  const workingChanges = useMemo(
    () => status?.files.filter((change) => change.unstaged) ?? [],
    [status],
  );
  const showFilter = (status?.files.length ?? 0) > FILTER_VISIBILITY_THRESHOLD;
  const commitDisabledReason = busy
    ? t("sourceControl.updating")
    : stagedChanges.length === 0
      ? t("sourceControl.noStagedChanges")
      : !commitMessage.trim()
        ? t("sourceControl.enterCommitMessage")
        : null;
  const normalizedFilter = showFilter ? filter.trim().toLocaleLowerCase() : "";

  useEffect(() => {
    setStagedRenderLimit(MAX_RENDERED_CHANGES_PER_SECTION);
    setWorkingRenderLimit(MAX_RENDERED_CHANGES_PER_SECTION);
  }, [normalizedFilter]);

  const matchesFilter = useCallback(
    (change: GitFileChange) => {
      if (!normalizedFilter) return true;
      return [change.path, change.originalPath]
        .filter((value): value is string => Boolean(value))
        .some((value) => value.toLocaleLowerCase().includes(normalizedFilter));
    },
    [normalizedFilter],
  );
  const filteredStagedChanges = useMemo(
    () => stagedChanges.filter(matchesFilter),
    [matchesFilter, stagedChanges],
  );
  const filteredWorkingChanges = useMemo(
    () => workingChanges.filter(matchesFilter),
    [matchesFilter, workingChanges],
  );
  const renderedDiff = useMemo(() => {
    const lines = diff?.patch.split("\n") ?? [];
    return lines.slice(0, 4_000).map((line, index) => {
      const kind = line.startsWith("+") && !line.startsWith("+++")
        ? "add"
        : line.startsWith("-") && !line.startsWith("---")
          ? "remove"
          : line.startsWith("@@")
            ? "hunk"
            : "context";
      return (
        <span key={`${index}:${line}`} data-kind={kind}>
          {line || " "}{"\n"}
        </span>
      );
    });
  }, [diff]);

  const mutate = useCallback(
    async (operation: () => Promise<void>) => {
      if (busyRef.current) return;
      busyRef.current = true;
      setBusy(true);
      try {
        await operation();
        await refreshStatus({ force: true });
        setDiffRevision((value) => value + 1);
        await onRepositoryChanged();
        setError(null);
      } catch (cause) {
        setError(cause instanceof Error ? cause.message : String(cause));
      } finally {
        busyRef.current = false;
        setBusy(false);
      }
    },
    [onRepositoryChanged, refreshStatus],
  );

  const commit = useCallback(async () => {
    const message = commitMessage.trim();
    if (busyRef.current || !message || stagedChanges.length === 0) return;
    busyRef.current = true;
    setBusy(true);
    try {
      const result = await client.commitGitChanges(workspace.workspaceId, message);
      setCommitMessage("");
      await refreshStatus({ force: true });
      setDiffRevision((value) => value + 1);
      await onRepositoryChanged();
      dialogs.toast({
        title: t("sourceControl.commitCreated"),
        description: result.summary || result.commit,
        tone: "success",
      });
      setError(null);
    } catch (cause) {
      setError(cause instanceof Error ? cause.message : String(cause));
    } finally {
      busyRef.current = false;
      setBusy(false);
    }
  }, [
    client,
    commitMessage,
    dialogs,
    onRepositoryChanged,
    refreshStatus,
    stagedChanges.length,
    t,
    workspace.workspaceId,
  ]);

  const renderSection = (
    title: string,
    changes: GitFileChange[],
    matchingChanges: GitFileChange[],
    staged: boolean,
    renderLimit: number,
    onLoadMore: () => void,
  ) => {
    const renderedChanges = matchingChanges.slice(0, renderLimit);
    const remainingChanges = matchingChanges.length - renderedChanges.length;
    return (
      <section className="agentmux-source-control__section">
        <div className="agentmux-source-control__section-header">
          <span>{title}</span>
          <span className="agentmux-source-control__count">{changes.length}</span>
          {changes.length > 0 ? (
            <button
              type="button"
              className="agentmux-source-control__section-action"
              onClick={() =>
                void mutate(() =>
                  staged
                    ? client.unstageGitFiles(workspace.workspaceId)
                    : client.stageGitFiles(workspace.workspaceId),
                )
              }
              disabled={busy}
            >
              {staged
                ? t("sourceControl.unstageAll")
                : t("sourceControl.stageAll")}
            </button>
          ) : null}
        </div>
        {remainingChanges > 0 ? (
          <div className="agentmux-source-control__shown-count">
            <span>
              {t("sourceControl.shownCount", {
                shown: renderedChanges.length,
                total: matchingChanges.length,
              })}
            </span>
            <button
              type="button"
              className="agentmux-source-control__load-more"
              onClick={onLoadMore}
              disabled={busy}
            >
              {t("sourceControl.loadMore", {
                count: Math.min(
                  remainingChanges,
                  MAX_RENDERED_CHANGES_PER_SECTION,
                ),
              })}
            </button>
          </div>
        ) : null}
        {matchingChanges.length === 0 && changes.length > 0 ? (
          <div className="agentmux-source-control__no-matches">
            {t("sourceControl.noMatchingChanges")}
          </div>
        ) : null}
        {renderedChanges.map((change) => {
          const nextSelection = {
            path: change.path,
            staged,
            untracked: !staged && change.untracked,
          };
          return (
            <GitChangeRow
              key={`${staged ? "staged" : "working"}:${change.path}`}
              change={change}
              staged={staged}
              selected={
                selection !== null &&
                selectionKey(selection) === selectionKey(nextSelection)
              }
              disabled={busy}
              onSelect={() => setSelection(nextSelection)}
              onToggleStage={() =>
                void mutate(() =>
                  staged
                    ? client.unstageGitFiles(
                        workspace.workspaceId,
                        pathsForChange(change),
                      )
                    : client.stageGitFiles(
                        workspace.workspaceId,
                        pathsForChange(change),
                      ),
                )
              }
              t={t}
            />
          );
        })}
      </section>
    );
  };

  return (
    <aside
      className="agentmux-source-control"
      aria-label={t("sourceControl.title")}
      aria-busy={busy || statusLoading}
      data-testid="source-control-panel"
    >
      <header className="agentmux-source-control__header">
        <div className="agentmux-source-control__title">
          <IconBranch size={14} />
          <span>{t("sourceControl.title")}</span>
        </div>
        <div className="agentmux-source-control__header-actions">
          <button
            type="button"
            onClick={() => void refreshStatus({ force: true, showLoading: true }).catch(() => undefined)}
            disabled={busy || statusLoading}
            title={t("common.refresh")}
            aria-label={t("common.refresh")}
          >
            <IconReset size={13} />
          </button>
          <button
            type="button"
            onClick={onClose}
            title={t("common.close")}
            aria-label={t("common.close")}
          >
            <IconClose size={12} />
          </button>
        </div>
      </header>

      <div className="agentmux-source-control__repository">
        <strong>{status?.branch ?? t("sourceControl.noBranch")}</strong>
        {status?.upstream ? <span>{status.upstream}</span> : null}
        {status && (status.ahead > 0 || status.behind > 0) ? (
          <span>
            {t("sourceControl.syncState", {
              ahead: status.ahead,
              behind: status.behind,
            })}
          </span>
        ) : null}
        <small title={status?.repositoryRoot ?? undefined}>
          {status?.repositoryRoot ?? workspace.projectRoot ?? workspace.name}
        </small>
      </div>

      {error ? (
        <div className="agentmux-source-control__error" role="status">
          {error}
        </div>
      ) : null}

      {busy ? (
        <div className="agentmux-source-control__activity" role="status">
          <span className="agentmux-term-booting-spinner" aria-hidden="true" />
          <span>{t("sourceControl.updating")}</span>
        </div>
      ) : null}

      {statusLoading && status === null ? (
        <div className="agentmux-source-control__loading" role="status">
          {t("sourceControl.loading")}
        </div>
      ) : status && !status.isRepository ? (
        <div className="agentmux-source-control__empty">
          <IconBranch size={24} />
          <strong>{t("sourceControl.notRepository")}</strong>
          <span>{t("sourceControl.notRepositoryDescription")}</span>
        </div>
      ) : (
        <>
          <div className="agentmux-source-control__content" ref={splitContainerRef}>
            <div
              className="agentmux-source-control__changes agentmux-scroll"
              data-filter-visible={showFilter ? "true" : undefined}
              style={{ flexBasis: `${changesSplitRatio * 100}%` }}
            >
              {showFilter ? (
                <label className="agentmux-source-control__filter">
                  <IconSearch size={13} />
                  <input
                    type="search"
                    value={filter}
                    onChange={(event) => setFilter(event.currentTarget.value)}
                    placeholder={t("sourceControl.filterPlaceholder")}
                    aria-label={t("sourceControl.filterPlaceholder")}
                  />
                </label>
              ) : null}
              {renderSection(
                t("sourceControl.stagedChanges"),
                stagedChanges,
                filteredStagedChanges,
                true,
                stagedRenderLimit,
                () =>
                  setStagedRenderLimit((current) =>
                    current + MAX_RENDERED_CHANGES_PER_SECTION,
                  ),
              )}
              {renderSection(
                t("sourceControl.changes"),
                workingChanges,
                filteredWorkingChanges,
                false,
                workingRenderLimit,
                () =>
                  setWorkingRenderLimit((current) =>
                    current + MAX_RENDERED_CHANGES_PER_SECTION,
                  ),
              )}
              {status && status.files.length === 0 ? (
                <div className="agentmux-source-control__clean">
                  {t("sourceControl.clean")}
                </div>
              ) : null}
            </div>

            <div
              ref={splitHandleRef}
              className="agentmux-source-control__splitter"
              data-resizing={resizingSplit ? "true" : undefined}
              role="separator"
              aria-label={t("sourceControl.resizeFileList")}
              aria-orientation="horizontal"
              aria-valuemin={0}
              aria-valuemax={100}
              aria-valuenow={Math.round(changesSplitRatio * 100)}
              tabIndex={0}
              title={t("sourceControl.resizeFileList")}
              onPointerDown={handleSplitPointerDown}
              onPointerMove={(event) => {
                if (resizingSplit) resizeFromPointer(event.clientY);
              }}
              onPointerUp={finishSplitResize}
              onPointerCancel={finishSplitResize}
              onLostPointerCapture={() => setResizingSplit(false)}
              onKeyDown={handleSplitKeyDown}
            />

            <div className="agentmux-source-control__diff">
              <div className="agentmux-source-control__diff-header">
                <span title={selection?.path}>{selection?.path ?? t("sourceControl.diff")}</span>
                {diff?.truncated ? <em>{t("sourceControl.truncated")}</em> : null}
              </div>
              <pre className="agentmux-scroll">
                {diff
                  ? renderedDiff.length > 0
                    ? renderedDiff
                    : t("sourceControl.noDiff")
                  : selection
                    ? t("common.loading")
                    : t("sourceControl.selectFile")}
              </pre>
            </div>
          </div>

          <div className="agentmux-source-control__commit">
            <textarea
              value={commitMessage}
              disabled={busy}
              onChange={(event) => setCommitMessage(event.target.value)}
              onKeyDown={(event) => {
                if ((event.ctrlKey || event.metaKey) && event.key === "Enter") {
                  event.preventDefault();
                  void commit();
                }
              }}
              rows={2}
              placeholder={t("sourceControl.commitPlaceholder")}
              aria-label={t("sourceControl.commitPlaceholder")}
            />
            <button
              type="button"
              onClick={() => void commit()}
              disabled={commitDisabledReason !== null}
              title={commitDisabledReason ?? t("sourceControl.commit")}
            >
              {t("sourceControl.commit")}
            </button>
          </div>
        </>
      )}
    </aside>
  );
}

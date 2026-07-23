import { useCallback, useEffect, useMemo, useRef, useState, type ReactNode } from "react";
import { useVirtualizer } from "@tanstack/react-virtual";
import type {
  AgentWorktreeOperation,
  ControlClient,
  GitChangeSummary,
  GitPagedDiff,
  GitReviewLineAnchor,
  GitReviewThread,
  GitStatusSummary,
  TerminalSession,
  WorkspaceSummary,
} from "../control/ControlClient";
import {
  GIT_EVENT_COALESCE_MS,
  SERVER_GIT_REFRESH_MS,
  nextGitRefreshDelay,
  shouldRefreshForGitEvent,
  shouldReloadGitPage,
} from "../control/GitRefreshPolicy";
import {
  createAgentWorktreeIdempotencyKey,
  parseAgentWorktreeCommand,
} from "../control/GitWorktreeForm";
import { useAppDialogs } from "./dialogs";
import { IconBranch, IconClose, IconPlus, IconReset, IconSearch } from "./icons";
import type { I18nKey, Translator } from "./i18n";
import "./SourceControlPanel.css";

interface Props {
  client: ControlClient;
  workspace: WorkspaceSummary;
  activePaneId: string | null;
  activeSessionId: string | null;
  activeCwd: string | null;
  onClose: () => void;
  onRepositoryChanged: () => void | Promise<void>;
  t: Translator;
}

type Stage = "staged" | "working" | "untracked";
interface Selection { path: string; stage: Stage; }
interface PanelTarget { key: string; generation: number; }
type Row =
  | { kind: "header"; key: string; title: string; count: number; action: "stage" | "unstage" | null }
  | { kind: "change"; key: string; change: GitChangeSummary }
  | { kind: "more"; key: string }
  | { kind: "empty"; key: string; text: string };

const PAGE_SIZE = 250;
const MIN_CHANGE_HEIGHT = 120;
const MIN_DIFF_HEIGHT = 180;
const SPLIT_RATIO_STORAGE_KEY = "agentmux.sourceControl.changeListRatio.v1";
const WORKTREE_STATE_KEYS: Record<string, I18nKey> = {
  prepared: "sourceControl.worktreeStatePrepared",
  worktree_created: "sourceControl.worktreeStateWorktreeCreated",
  workspace_created: "sourceControl.worktreeStateWorkspaceCreated",
  session_created: "sourceControl.worktreeStateSessionCreated",
  completed: "sourceControl.worktreeStateCompleted",
  failed: "sourceControl.worktreeStateFailed",
  rolling_back: "sourceControl.worktreeStateRollingBack",
  rolled_back: "sourceControl.worktreeStateRolledBack",
  removed: "sourceControl.worktreeStateRemoved",
};

function readSplitRatio(): number {
  try {
    const stored = Number(window.localStorage.getItem(SPLIT_RATIO_STORAGE_KEY));
    return Number.isFinite(stored) && stored >= 0.2 && stored <= 0.8 ? stored : 0.5;
  } catch {
    return 0.5;
  }
}

function stageFor(change: GitChangeSummary): Stage {
  return change.staged ? "staged" : change.untracked ? "untracked" : "working";
}

function statusBadge(change: GitChangeSummary): string {
  if (change.conflicted) return "!";
  if (change.untracked) return "?";
  return change.status === "." ? "M" : change.status;
}

function splitPath(path: string): { name: string; directory: string } {
  const segments = path.replaceAll("\\", "/").split("/");
  return { name: segments.pop() ?? path, directory: segments.join("/") };
}

function worktreeStateLabel(t: Translator, state: string): string {
  return t(WORKTREE_STATE_KEYS[state] ?? "sourceControl.worktreeStateUnknown");
}

function reviewAnchorSideLabel(t: Translator, side: GitReviewLineAnchor["side"]): string {
  switch (side) {
    case "left": return t("sourceControl.reviewSideLeft");
    case "right": return t("sourceControl.reviewSideRight");
    case "context": return t("sourceControl.reviewSideContext");
    default: return t("sourceControl.reviewSideUnknown");
  }
}

function diffLineAnchors(diff: GitPagedDiff | null, path: string): Array<{ text: string; kind: string; anchor: GitReviewLineAnchor | null }> {
  let left = 0;
  let right = 0;
  let hunkHeader: string | null = null;
  return (diff?.patch ?? "").split("\n").slice(0, 8_000).map((text) => {
    if (text.startsWith("@@")) {
      const match = /@@ -(\d+)(?:,\d+)? \+(\d+)(?:,\d+)? @@/.exec(text);
      left = Number(match?.[1] ?? 0);
      right = Number(match?.[2] ?? 0);
      hunkHeader = text;
      return { text, kind: "hunk", anchor: null };
    }
    if (text.startsWith("+") && !text.startsWith("+++")) {
      const anchor = right ? { path, side: "right", line: right, hunkHeader, diffHash: diff?.diffHash || null } : null;
      right += 1;
      return { text, kind: "add", anchor };
    }
    if (text.startsWith("-") && !text.startsWith("---")) {
      const anchor = left ? { path, side: "left", line: left, hunkHeader, diffHash: diff?.diffHash || null } : null;
      left += 1;
      return { text, kind: "remove", anchor };
    }
    if (text.startsWith(" ")) {
      const anchor = right ? { path, side: "right", line: right, hunkHeader, diffHash: diff?.diffHash || null } : null;
      left += 1;
      right += 1;
      return { text, kind: "context", anchor };
    }
    return { text, kind: "context", anchor: null };
  });
}

export function SourceControlPanel({ client, workspace, activePaneId, activeSessionId, activeCwd, onClose, onRepositoryChanged, t }: Props) {
  const dialogs = useAppDialogs();
  const [summary, setSummary] = useState<GitStatusSummary | null>(null);
  const [changes, setChanges] = useState<GitChangeSummary[]>([]);
  const [nextCursor, setNextCursor] = useState<string | null>(null);
  const [selection, setSelection] = useState<Selection | null>(null);
  const [diff, setDiff] = useState<GitPagedDiff | null>(null);
  const [threads, setThreads] = useState<GitReviewThread[]>([]);
  const [reviewAnchor, setReviewAnchor] = useState<GitReviewLineAnchor | null>(null);
  const [reviewBody, setReviewBody] = useState("");
  const [delivery, setDelivery] = useState<"mailbox" | "terminal">("mailbox");
  const [deliverySession, setDeliverySession] = useState("");
  const [sessions, setSessions] = useState<TerminalSession[]>([]);
  const [worktrees, setWorktrees] = useState<AgentWorktreeOperation[]>([]);
  const [commitMessage, setCommitMessage] = useState("");
  const [filter, setFilter] = useState("");
  const [serverQuery, setServerQuery] = useState("");
  const [filteredCount, setFilteredCount] = useState<number | null>(null);
  const [loading, setLoading] = useState(true);
  const [loadingMore, setLoadingMore] = useState(false);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [ratio, setRatio] = useState(readSplitRatio);
  const [resizing, setResizing] = useState(false);
  const scrollRef = useRef<HTMLDivElement>(null);
  const splitRef = useRef<HTMLDivElement>(null);
  const refreshSequence = useRef(0);
  const threadSequence = useRef(0);
  const loadedGeneration = useRef<number | null>(null);
  const loadedRepositoryId = useRef<string | null>(null);
  const lastRefresh = useRef(0);
  const eventTimer = useRef<ReturnType<typeof window.setTimeout> | null>(null);
  const targetKey = `${workspace.workspaceId}:${activePaneId ?? "none"}:${activeSessionId ?? "none"}:${activeCwd ?? ""}`;
  const targetKeyRef = useRef(targetKey);
  const targetGenerationRef = useRef(0);
  if (targetKeyRef.current !== targetKey) {
    targetKeyRef.current = targetKey;
    targetGenerationRef.current += 1;
  }
  const serverQueryRef = useRef(serverQuery);
  serverQueryRef.current = serverQuery;
  const captureTarget = useCallback((): PanelTarget => ({ key: targetKeyRef.current, generation: targetGenerationRef.current }), []);
  const isCurrentTarget = useCallback((target: PanelTarget) => target.key === targetKeyRef.current && target.generation === targetGenerationRef.current, []);

  useEffect(() => {
    const timeout = window.setTimeout(() => setServerQuery(filter.trim()), 180);
    return () => window.clearTimeout(timeout);
  }, [filter]);

  useEffect(() => {
    try { window.localStorage.setItem(SPLIT_RATIO_STORAGE_KEY, String(ratio)); } catch { /* storage may be unavailable in restricted webviews */ }
  }, [ratio]);

  const loadWorktrees = useCallback(async () => {
    try { setWorktrees(await client.listAgentWorktrees(workspace.workspaceId, true)); } catch { /* host may still be upgrading */ }
  }, [client, workspace.workspaceId]);

  const loadThreads = useCallback(async (path?: string | null, repositoryId?: string | null, expectedTarget = captureTarget()) => {
    const sequence = ++threadSequence.current;
    if (!repositoryId) {
      if (isCurrentTarget(expectedTarget)) setThreads([]);
      return;
    }
    try {
      const nextThreads = await client.listGitReviewThreads(workspace.workspaceId, {
        repositoryId, path: path ?? null, includeResolved: true, includeStale: true,
      });
      if (sequence === threadSequence.current && isCurrentTarget(expectedTarget)) setThreads(nextThreads);
    } catch (cause) {
      if (sequence === threadSequence.current && isCurrentTarget(expectedTarget)) setError(cause instanceof Error ? cause.message : String(cause));
    }
  }, [captureTarget, client, isCurrentTarget, workspace.workspaceId]);

  const refresh = useCallback(async (showLoading = false, reuseLoadedSnapshot = false) => {
    const sequence = ++refreshSequence.current;
    const expectedTarget = captureTarget();
    const query = serverQueryRef.current;
    const requestedGeneration = reuseLoadedSnapshot ? loadedGeneration.current : null;
    const requestedRepositoryId = reuseLoadedSnapshot ? loadedRepositoryId.current : null;
    if (showLoading) setLoading(true);
    try {
      let page = await client.getGitStatusPage(workspace.workspaceId, {
        paneId: activePaneId, limit: PAGE_SIZE, query,
        repositoryId: requestedRepositoryId,
        generation: requestedGeneration,
      });
      if (sequence !== refreshSequence.current || !isCurrentTarget(expectedTarget) || query !== serverQueryRef.current) return;
      loadedGeneration.current = page.generation;
      loadedRepositoryId.current = page.repositoryId;
      if (!page.summary) {
        setSummary(null);
        setChanges(page.changes);
        setNextCursor(null);
        setFilteredCount(null);
        setError(null);
        setLoading(false);
        await new Promise<void>((resolve) => window.requestAnimationFrame(() => resolve()));
        if (sequence !== refreshSequence.current || !isCurrentTarget(expectedTarget) || query !== serverQueryRef.current) return;
        page = await client.getGitStatusPage(workspace.workspaceId, {
          repositoryId: page.repositoryId,
          generation: page.generation,
          paneId: activePaneId,
          limit: PAGE_SIZE,
          query,
        });
      }
      const nextSummary = page.summary ?? await client.getGitStatusSummary(
        workspace.workspaceId,
        page.repositoryId,
        activePaneId,
      );
      if (sequence !== refreshSequence.current || !isCurrentTarget(expectedTarget) || query !== serverQueryRef.current) return;
      if (shouldReloadGitPage(nextSummary.generation, page.generation)) {
        loadedGeneration.current = null;
        loadedRepositoryId.current = null;
        void refresh(showLoading, false);
        return;
      }
      const scrollTop = scrollRef.current?.scrollTop ?? 0;
      setSummary(nextSummary);
      setChanges(page.changes);
      setNextCursor(page.nextCursor ?? null);
      setFilteredCount(page.totalCount ?? null);
      loadedGeneration.current = page.generation;
      loadedRepositoryId.current = page.repositoryId;
      lastRefresh.current = Date.now();
      setError(null);
      void loadWorktrees();
      requestAnimationFrame(() => { if (scrollRef.current) scrollRef.current.scrollTop = scrollTop; });
    } catch (cause) {
      if (sequence === refreshSequence.current && isCurrentTarget(expectedTarget) && query === serverQueryRef.current) {
        setError(cause instanceof Error ? cause.message : String(cause));
      }
    } finally {
      if (sequence === refreshSequence.current && isCurrentTarget(expectedTarget) && query === serverQueryRef.current) {
        setLoading(false);
      }
    }
  }, [activePaneId, captureTarget, client, isCurrentTarget, loadWorktrees, workspace.workspaceId]);

  const loadMore = useCallback(async () => {
    if (!summary || !nextCursor || loadingMore) return;
    const operationTarget = captureTarget();
    const query = serverQueryRef.current;
    const sequence = refreshSequence.current;
    setLoadingMore(true);
    try {
      const page = await client.getGitStatusPage(workspace.workspaceId, {
        repositoryId: summary.repositoryId, cursor: nextCursor, limit: PAGE_SIZE, generation: summary.generation,
        paneId: activePaneId, query,
      });
      if (!isCurrentTarget(operationTarget) || query !== serverQueryRef.current || sequence !== refreshSequence.current) return;
      if (shouldReloadGitPage(summary.generation, page.generation)) { await refresh(); return; }
      setChanges((current) => [...current, ...page.changes]);
      setNextCursor(page.nextCursor ?? null);
      loadedGeneration.current = page.generation;
    } catch (cause) {
      if (isCurrentTarget(operationTarget) && query === serverQueryRef.current && sequence === refreshSequence.current) setError(cause instanceof Error ? cause.message : String(cause));
    } finally {
      if (isCurrentTarget(operationTarget) && query === serverQueryRef.current) setLoadingMore(false);
    }
  }, [activePaneId, captureTarget, client, isCurrentTarget, loadingMore, nextCursor, refresh, summary, workspace.workspaceId]);

  useEffect(() => {
    threadSequence.current++;
    const expectedTarget = captureTarget();
    loadedGeneration.current = null;
    loadedRepositoryId.current = null;
    setLoadingMore(false);
    setSummary(null); setChanges([]); setNextCursor(null); setFilteredCount(null); setSelection(null); setDiff(null); setThreads([]); setReviewAnchor(null); setCommitMessage(""); setError(null);
    void refresh(true, false);
    void client.getWorkspace(workspace.workspaceId)
      .then((detail) => { if (isCurrentTarget(expectedTarget)) setSessions(detail.sessions); })
      .catch(() => { if (isCurrentTarget(expectedTarget)) setSessions([]); });
  }, [captureTarget, client, isCurrentTarget, refresh, targetKey, workspace.workspaceId]);

  const previousQueryRef = useRef(serverQuery);
  useEffect(() => {
    if (previousQueryRef.current === serverQuery) return;
    previousQueryRef.current = serverQuery;
    setLoadingMore(false);
    setChanges([]);
    setNextCursor(null);
    setFilteredCount(null);
    void refresh(true, true);
  }, [refresh, serverQuery]);

  useEffect(() => {
    const events = (window as Window & { __TAURI__?: { event?: { listen?: (name: string, handler: (event: { payload?: unknown }) => void) => Promise<() => void> } } }).__TAURI__?.event;
    let unlisten: (() => void) | undefined;
    let disposed = false;
    if (events?.listen) {
      void events.listen("agentmux://git-repository-changed", (event) => {
        if (disposed) return;
        const payload = event.payload as { workspace_id?: string; repository_id?: string; generation?: number } | undefined;
        if (!shouldRefreshForGitEvent(payload, workspace.workspaceId, summary?.repositoryId, loadedGeneration.current)) return;
        if (eventTimer.current) window.clearTimeout(eventTimer.current);
        eventTimer.current = window.setTimeout(() => { eventTimer.current = null; void refresh(); }, nextGitRefreshDelay(Date.now(), lastRefresh.current, GIT_EVENT_COALESCE_MS));
      }).then((stop) => {
        if (disposed) stop();
        else unlisten = stop;
      }).catch(() => undefined);
    }
    const timer = events?.listen ? undefined : window.setInterval(() => {
      if (document.visibilityState === "visible") void refresh();
    }, SERVER_GIT_REFRESH_MS);
    return () => { disposed = true; if (eventTimer.current) window.clearTimeout(eventTimer.current); if (timer) window.clearInterval(timer); unlisten?.(); };
  }, [refresh, summary?.repositoryId, workspace.workspaceId]);

  useEffect(() => {
    const isPending = worktrees.some((operation) => !["completed", "failed", "removed", "rolled_back"].includes(operation.state));
    if (!isPending) return;
    const timer = window.setInterval(() => { void loadWorktrees(); }, 1_500);
    return () => window.clearInterval(timer);
  }, [loadWorktrees, worktrees]);

  useEffect(() => {
    if (!summary || !selection) { setDiff(null); return; }
    let current = true;
    setDiff(null);
    void client.getGitPagedDiff(workspace.workspaceId, selection.path, {
      repositoryId: summary.repositoryId, stage: selection.stage, generation: summary.generation, contextLines: 4,
      paneId: activePaneId,
    }).then((result) => { if (current) setDiff(result); }).catch((cause) => { if (current) setError(cause instanceof Error ? cause.message : String(cause)); });
    void loadThreads(selection.path, summary.repositoryId, captureTarget());
    return () => { current = false; };
  }, [activePaneId, captureTarget, client, loadThreads, selection, summary, workspace.workspaceId]);

  const visibleChanges = changes;
  useEffect(() => {
    if (!selection || visibleChanges.some((change) => change.path === selection.path && stageFor(change) === selection.stage)) return;
    setSelection(null);
  }, [selection, visibleChanges]);
  const rows = useMemo<Row[]>(() => {
    const staged = visibleChanges.filter((change) => change.staged);
    const working = visibleChanges.filter((change) => !change.staged);
    const next: Row[] = [
      { kind: "header", key: "staged", title: t("sourceControl.stagedChanges"), count: summary?.stagedCount ?? 0, action: staged.length ? "unstage" : null },
      ...staged.map((change) => ({ kind: "change" as const, key: `staged:${change.path}`, change })),
      { kind: "header", key: "working", title: t("sourceControl.changes"), count: (summary?.unstagedCount ?? 0) + (summary?.untrackedCount ?? 0), action: working.length ? "stage" : null },
      ...working.map((change) => ({ kind: "change" as const, key: `working:${change.path}`, change })),
    ];
    if (!visibleChanges.length && !nextCursor) next.push({ kind: "empty", key: "empty", text: serverQuery ? t("sourceControl.noMatchingChanges") : t("sourceControl.clean") });
    if (nextCursor) next.push({ kind: "more", key: "more" });
    return next;
  }, [nextCursor, serverQuery, summary?.stagedCount, summary?.unstagedCount, summary?.untrackedCount, t, visibleChanges]);
  const virtualizer = useVirtualizer({ count: rows.length, getScrollElement: () => scrollRef.current, estimateSize: (index) => rows[index]?.kind === "change" ? 44 : 36, getItemKey: (index) => rows[index]?.key ?? index, overscan: 16, useFlushSync: false });
  const diffLines = useMemo(() => diffLineAnchors(diff, selection?.path ?? ""), [diff, selection?.path]);
  const markers = useMemo(() => new Map(threads.map((thread) => [`${thread.anchor.side}:${thread.anchor.line}`, thread])), [threads]);

  const mutate = useCallback(async (operation: () => Promise<unknown>) => {
    if (busy) return;
    const operationTarget = captureTarget();
    setBusy(true);
    try {
      await operation();
      await onRepositoryChanged();
      if (isCurrentTarget(operationTarget)) await refresh();
    }
    catch (cause) { if (isCurrentTarget(operationTarget)) setError(cause instanceof Error ? cause.message : String(cause)); }
    finally { setBusy(false); }
  }, [busy, captureTarget, isCurrentTarget, onRepositoryChanged, refresh]);

  const runPanelAction = useCallback(async (action: (target: PanelTarget) => Promise<void>) => {
    const operationTarget = captureTarget();
    try {
      await action(operationTarget);
      if (isCurrentTarget(operationTarget)) setError(null);
    } catch (cause) {
      if (isCurrentTarget(operationTarget)) setError(cause instanceof Error ? cause.message : String(cause));
    }
  }, [captureTarget, isCurrentTarget]);

  const stagedCount = summary?.stagedCount ?? 0;
  const commitDisabledReason = busy
    ? t("sourceControl.updating")
    : stagedCount === 0
      ? t("sourceControl.noStagedChanges")
      : !commitMessage.trim()
        ? t("sourceControl.enterCommitMessage")
        : null;
  const commit = useCallback(async () => {
    const message = commitMessage.trim();
    if (busy || !message || stagedCount === 0) return;
    setBusy(true);
    const operationTarget = captureTarget();
    try {
      const result = await client.commitGitChanges(workspace.workspaceId, message, { paneId: activePaneId });
      await onRepositoryChanged();
      if (isCurrentTarget(operationTarget)) {
        setCommitMessage("");
        await refresh();
        dialogs.toast({ title: t("sourceControl.commitCreated"), description: result.summary || result.commit, tone: "success" });
        setError(null);
      }
    } catch (cause) {
      if (isCurrentTarget(operationTarget)) setError(cause instanceof Error ? cause.message : String(cause));
    } finally {
      setBusy(false);
    }
  }, [activePaneId, busy, captureTarget, client, commitMessage, dialogs, isCurrentTarget, onRepositoryChanged, refresh, stagedCount, t, workspace.workspaceId]);

  const createReview = useCallback(async () => {
    if (busy || !summary || !reviewAnchor || !reviewBody.trim()) return;
    const operationTarget = captureTarget();
    setBusy(true);
    try {
      const thread = await client.createGitReviewThread({ workspaceId: workspace.workspaceId, repositoryId: summary.repositoryId, anchor: reviewAnchor, body: reviewBody });
      if (deliverySession) await client.deliverGitReviewThread(thread.threadId, { target: delivery, targetSessionId: deliverySession, includeContext: true });
      if (!isCurrentTarget(operationTarget)) return;
      setReviewBody("");
      setReviewAnchor(null);
      await loadThreads(selection?.path, summary.repositoryId, operationTarget);
    } catch (cause) {
      if (isCurrentTarget(operationTarget)) setError(cause instanceof Error ? cause.message : String(cause));
    } finally {
      setBusy(false);
    }
  }, [busy, captureTarget, client, delivery, deliverySession, isCurrentTarget, loadThreads, reviewAnchor, reviewBody, selection?.path, summary, workspace.workspaceId]);

  const createWorktree = useCallback(async () => {
    const values = await dialogs.form({ title: t("sourceControl.worktreeCreateTitle"), description: t("sourceControl.worktreeCreateDescription"), confirmLabel: t("sourceControl.worktreeCreateConfirm"), testId: "source-control-worktree-form", fields: [
      { id: "branch", label: t("sourceControl.worktreeBranch"), placeholder: "agent/review-api", required: true },
      { id: "base", label: t("sourceControl.worktreeBase"), initialValue: "HEAD" },
      { id: "destination", label: t("sourceControl.worktreeDestination"), placeholder: "D:\\workspace\\agent-review", required: true },
      { id: "command", label: t("sourceControl.worktreeCommand"), placeholder: "claude --dangerously-skip-permissions" },
    ] });
    if (!values) return;
    await mutate(async () => {
      const branch = String(values.branch).trim();
      const destination = String(values.destination).trim();
      const result = await client.createAgentWorktree({ workspaceId: workspace.workspaceId, branch, destination, baseRevision: String(values.base).trim() || null, createBranch: true, command: parseAgentWorktreeCommand(String(values.command)), cwd: destination, idempotencyKey: createAgentWorktreeIdempotencyKey(workspace.workspaceId, branch, destination) });
      await loadWorktrees();
      dialogs.toast({ title: t("sourceControl.worktreeStarted"), description: `${result.branch} · ${worktreeStateLabel(t, result.state)}`, tone: "success" });
    });
  }, [client, dialogs, loadWorktrees, mutate, t, workspace.workspaceId]);

  const updateRatio = (clientY: number) => {
    const node = splitRef.current; if (!node) return;
    const available = Math.max(1, node.clientHeight - 7);
    const proposed = clientY - node.getBoundingClientRect().top;
    const height = Math.min(Math.max(proposed, Math.min(MIN_CHANGE_HEIGHT, available)), Math.max(MIN_CHANGE_HEIGHT, available - MIN_DIFF_HEIGHT));
    setRatio(height / available);
  };

  const path = summary?.repositoryRoot ?? workspace.projectRoot ?? workspace.name;
  return <aside className="agentmux-source-control" aria-label={t("sourceControl.title")} aria-busy={loading || busy} data-testid="source-control-panel">
    <header className="agentmux-source-control__header"><div className="agentmux-source-control__title"><IconBranch size={14} /><span>{t("sourceControl.title")}</span></div><div className="agentmux-source-control__header-actions"><button type="button" title={t("sourceControl.worktreeCreateTitle")} aria-label={t("sourceControl.worktreeCreateTitle")} onClick={() => void createWorktree()}><IconPlus size={13} /></button><button type="button" title={t("common.refresh")} aria-label={t("common.refresh")} disabled={loading || busy} onClick={() => void refresh(true)}><IconReset size={13} /></button><button type="button" title={t("common.close")} aria-label={t("common.close")} onClick={onClose}><IconClose size={12} /></button></div></header>
    <div className="agentmux-source-control__repository"><strong>{summary?.branch ?? t("sourceControl.noBranch")}</strong>{summary?.upstream ? <span>{summary.upstream}</span> : null}{summary && (summary.ahead || summary.behind) ? <span>{t("sourceControl.syncState", { ahead: summary.ahead, behind: summary.behind })}</span> : null}<small title={path}>{path}</small></div>
    {error ? <div className="agentmux-source-control__error" role="status">{error}</div> : null}
    {worktrees.length ? <div className="agentmux-source-control__worktrees"><span>{t("sourceControl.worktreeList")}</span>{worktrees.map((operation) => <div className="agentmux-source-control__worktree" key={operation.worktreeId}><button type="button" title={t("sourceControl.worktreeRecover")} onClick={() => void runPanelAction(async () => { await client.recoverAgentWorktree({ operationId: operation.operationId }); await loadWorktrees(); })}>{operation.branch}</button><small>{worktreeStateLabel(t, operation.state)}</small><button type="button" onClick={() => void dialogs.confirm({ title: t("sourceControl.worktreeRemoveTitle"), description: operation.path, detail: t("sourceControl.worktreeRemoveDescription"), confirmLabel: t("sourceControl.worktreeRemove"), tone: "danger" }).then((confirmed) => { if (confirmed) void runPanelAction(async () => { await client.removeAgentWorktree({ worktreeId: operation.worktreeId }); await loadWorktrees(); }); })}>{t("sourceControl.worktreeRemove")}</button></div>)}</div> : null}
    {loading && !summary ? <div className="agentmux-source-control__loading">{t("sourceControl.loading")}</div> : <div ref={splitRef} className="agentmux-source-control__content">
      <section className="agentmux-source-control__changes" style={{ flexBasis: `${ratio * 100}%` }} data-filtered-count={filteredCount ?? undefined}><label className="agentmux-source-control__filter"><IconSearch size={13} /><input type="search" value={filter} onChange={(event) => setFilter(event.currentTarget.value)} placeholder={t("sourceControl.filterPlaceholder")} aria-label={t("sourceControl.filterPlaceholder")} /></label><div ref={scrollRef} className="agentmux-source-control__virtual-scroll agentmux-scroll" onScroll={(event) => { const target = event.currentTarget; if (nextCursor && target.scrollTop + target.clientHeight > target.scrollHeight - 180) void loadMore(); }}><div className="agentmux-source-control__virtual-list" style={{ height: `${virtualizer.getTotalSize()}px` }}>{virtualizer.getVirtualItems().map((item) => { const row = rows[item.index]; if (!row) return null; let content: ReactNode; if (row.kind === "header") content = <div className="agentmux-source-control__section-header"><span>{row.title}</span><span className="agentmux-source-control__count">{row.count}</span>{row.action ? <button type="button" className="agentmux-source-control__section-action" onClick={() => void mutate(() => row.action === "stage" ? client.stageAllGitFiles(workspace.workspaceId, { repositoryId: summary?.repositoryId, paneId: activePaneId }) : client.unstageAllGitFiles(workspace.workspaceId, { repositoryId: summary?.repositoryId, paneId: activePaneId }))}>{row.action === "stage" ? t("sourceControl.stageAll") : t("sourceControl.unstageAll")}</button> : null}</div>; else if (row.kind === "more") content = <button type="button" className="agentmux-source-control__load-more" disabled={loadingMore} onClick={() => void loadMore()}>{loadingMore ? t("common.loading") : t("sourceControl.loadMore", { count: PAGE_SIZE })}</button>; else if (row.kind === "empty") content = <div className="agentmux-source-control__clean">{row.text}</div>; else { const next = { path: row.change.path, stage: stageFor(row.change) }; const selected = selection?.path === next.path && selection.stage === next.stage; const label = splitPath(row.change.path); content = <div className="agentmux-source-control__change" data-testid={`source-control-change-${row.change.staged ? "staged" : "working"}`} data-selected={selected || undefined}><button type="button" className="agentmux-source-control__change-main" onClick={() => setSelection(next)}><span className="agentmux-source-control__badge" data-status={statusBadge(row.change)}>{statusBadge(row.change)}</span><span className="agentmux-source-control__change-label"><span className="agentmux-source-control__filename">{label.name}</span>{label.directory ? <span className="agentmux-source-control__directory">{label.directory}</span> : null}</span></button><button type="button" className="agentmux-source-control__row-action" onClick={() => void mutate(() => row.change.staged ? client.unstageGitFiles(workspace.workspaceId, [row.change.path], { repositoryId: summary?.repositoryId, paneId: activePaneId }) : client.stageGitFiles(workspace.workspaceId, [row.change.path], { repositoryId: summary?.repositoryId, paneId: activePaneId }))}>{row.change.staged ? <IconClose size={10} /> : <IconPlus size={11} />}</button></div>; } return <div key={row.key} className="agentmux-source-control__virtual-row" data-index={item.index} ref={virtualizer.measureElement} style={{ transform: `translateY(${item.start}px)` }}>{content}</div>; })}</div></div></section>
      <div className="agentmux-source-control__splitter" role="separator" aria-orientation="horizontal" aria-label={t("sourceControl.resizeFileList")} aria-valuemin={20} aria-valuemax={80} aria-valuenow={Math.round(ratio * 100)} data-resizing={resizing || undefined} tabIndex={0} onPointerDown={(event) => { if (event.button !== 0) return; event.currentTarget.setPointerCapture(event.pointerId); setResizing(true); updateRatio(event.clientY); }} onPointerMove={(event) => { if (event.currentTarget.hasPointerCapture(event.pointerId)) updateRatio(event.clientY); }} onPointerUp={(event) => { if (event.currentTarget.hasPointerCapture(event.pointerId)) event.currentTarget.releasePointerCapture(event.pointerId); setResizing(false); }} onPointerCancel={() => setResizing(false)} onKeyDown={(event) => { if (event.key === "ArrowUp") { setRatio((current) => Math.max(.2, current - .05)); event.preventDefault(); } if (event.key === "ArrowDown") { setRatio((current) => Math.min(.8, current + .05)); event.preventDefault(); } }} />
      <section className="agentmux-source-control__diff"><div className="agentmux-source-control__diff-header"><span title={selection?.path}>{selection?.path ?? t("sourceControl.diff")}</span>{diff?.truncated ? <em>{t("sourceControl.truncated")}</em> : null}</div><div className="agentmux-source-control__diff-lines agentmux-scroll">{diff ? diffLines.map((line, index) => { const marked = line.anchor ? markers.get(`${line.anchor.side}:${line.anchor.line}`) : null; const selectedLine = reviewAnchor?.line === line.anchor?.line && reviewAnchor?.side === line.anchor?.side; return <button key={`${index}:${line.text}`} type="button" className="agentmux-source-control__diff-line" data-kind={line.kind} data-selected={selectedLine || undefined} disabled={!line.anchor} onClick={() => setReviewAnchor(line.anchor)}><span className="agentmux-source-control__diff-line-number">{line.anchor?.line ?? ""}</span><code>{line.text || " "}</code>{marked ? <span className="agentmux-source-control__thread-marker">1</span> : null}</button>; }) : <div className="agentmux-source-control__diff-empty">{selection ? t("common.loading") : t("sourceControl.selectFile")}</div>}</div>{reviewAnchor ? <div className="agentmux-source-control__review-composer"><small>{t("sourceControl.reviewCommentOn", { side: reviewAnchorSideLabel(t, reviewAnchor.side), line: reviewAnchor.line })}</small><textarea value={reviewBody} onChange={(event) => setReviewBody(event.currentTarget.value)} placeholder={t("sourceControl.reviewPlaceholder")} /><div><select value={delivery} onChange={(event) => setDelivery(event.currentTarget.value as "mailbox" | "terminal")}><option value="mailbox">{t("sourceControl.reviewMailbox")}</option><option value="terminal">{t("sourceControl.reviewTerminal")}</option></select><select value={deliverySession} onChange={(event) => setDeliverySession(event.currentTarget.value)}><option value="">{t("sourceControl.reviewDoNotDeliver")}</option>{sessions.map((session) => <option key={session.sessionId} value={session.sessionId}>{session.backendKind} · {session.sessionId.slice(-8)}</option>)}</select><button type="button" disabled={!reviewBody.trim() || busy} onClick={() => void createReview()}>{t("sourceControl.reviewAdd")}</button></div></div> : null}{threads.length ? <div className="agentmux-source-control__threads">{threads.map((thread) => <div key={thread.threadId}><span>{reviewAnchorSideLabel(t, thread.anchor.side)} {thread.anchor.line}</span><p>{thread.comments.at(-1)?.body}</p><button type="button" onClick={() => void runPanelAction(async (operationTarget) => { await client.updateGitReviewThread(thread.threadId, { resolved: !thread.resolved }); await loadThreads(selection?.path, summary?.repositoryId, operationTarget); })}>{thread.resolved ? t("sourceControl.reviewReopen") : t("sourceControl.reviewResolve")}</button><button type="button" onClick={() => void dialogs.confirm({ title: t("sourceControl.reviewDeleteTitle"), description: t("sourceControl.reviewDeleteDescription"), confirmLabel: t("sourceControl.reviewDelete"), tone: "danger" }).then((confirmed) => { if (confirmed) void runPanelAction(async (operationTarget) => { await client.deleteGitReviewThread(thread.threadId); await loadThreads(selection?.path, summary?.repositoryId, operationTarget); }); })}>{t("sourceControl.reviewDelete")}</button></div>)}</div> : null}</section>
    </div>}
    <div className="agentmux-source-control__commit">
      <textarea value={commitMessage} disabled={busy} onChange={(event) => setCommitMessage(event.currentTarget.value)} onKeyDown={(event) => { if ((event.ctrlKey || event.metaKey) && event.key === "Enter") { event.preventDefault(); void commit(); } }} rows={2} placeholder={t("sourceControl.commitPlaceholder")} aria-label={t("sourceControl.commitPlaceholder")} />
      <button type="button" onClick={() => void commit()} disabled={commitDisabledReason !== null} title={commitDisabledReason ?? t("sourceControl.commit")}>{t("sourceControl.commit")}</button>
    </div>
  </aside>;
}

export function parseAgentWorktreeCommand(value: string): string[] {
  return (value.trim().match(/(?:[^\s"]+|"[^"]*")+/g) ?? [])
    .map((word) => word.replace(/^"|"$/g, ""));
}

export function createAgentWorktreeIdempotencyKey(
  workspaceId: string,
  branch: string,
  destination: string,
): string {
  const normalized = [workspaceId, branch, destination]
    .map((value) => value.trim().replace(/\s+/g, "-"))
    .join(":");
  return `ui-worktree:${normalized}`;
}

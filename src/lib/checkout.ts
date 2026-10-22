import type { SessionEntry, Workspace } from "@/types/session";

const canon = (path: string) => path.replace(/\/+$/, "");

/**
 * What a session's own worktree has checked out now, as the last workspace
 * listing read it. An agent can switch the worktree to another branch, or to
 * a bare commit, in the middle of a conversation; the branch the session was
 * made on is then only where it started. Undefined for a session in the
 * project's shared checkout, or before the listing has the worktree.
 */
export function worktreeCheckout(session: SessionEntry, workspaces: Record<string, Workspace[]>): Workspace | undefined {
  if (!session.worktreeName || session.worktreeRemoved) return undefined;
  return (workspaces[session.projectPath] ?? []).find((workspace) => !workspace.isMain && canon(workspace.path) === canon(session.cwd));
}

/**
 * Whether a session's worktree is still on the branch name drawn for it
 * (`calm-ember-panda`). Until somebody names a branch the conversation goes
 * by its title, as a chat does; it becomes a branch once it is on a real one.
 */
export function onPlaceholder(session: SessionEntry, workspaces: Record<string, Workspace[]>): boolean {
  return !!worktreeCheckout(session, workspaces)?.placeholder;
}

/** A checkout in a few words: its branch, else the commit it is detached at. */
export function checkoutLabel(checkout: Pick<Workspace, "branch" | "head">): string | null {
  if (checkout.branch) return checkout.branch;
  return checkout.head ? `detached at ${checkout.head.slice(0, 7)}` : null;
}

/**
 * Whether the work on a checkout's branch is in the default branch. The
 * listing says `merged` only for a squash or rebase merge: a branch merged
 * with a merge commit reads `clean`, as does a new one with nothing of its
 * own yet, since either way its tip is already in the default branch's
 * history. Where a session made the worktree, the commit it started from
 * tells them apart: a clean checkout that has moved on from it had work of
 * its own, and that work is merged.
 */
export function branchMerged(workspace: Workspace, sessions: SessionEntry[]): boolean {
  if (workspace.isMain || !workspace.branch) return false;
  if (workspace.state === "merged") return true;
  if (workspace.state !== "clean" || !workspace.head) return false;
  const made = sessions.find((session) => session.worktreeBase?.commit && canon(session.cwd) === canon(workspace.path));
  return !!made?.worktreeBase && made.worktreeBase.commit !== workspace.head;
}

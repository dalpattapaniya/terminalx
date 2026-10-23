import { gh, type PrAssociation, type PullRequest } from "@/lib/api";
import type { GitSource } from "@/lib/gitSource";

export interface SessionPr {
  identity: string;
  association?: PrAssociation;
  pr?: PullRequest;
  /** Present only when returned by discovery for the current checkout branch. */
  current: boolean;
  error?: string;
}

export function prIdentity(url: string): string {
  return url.toLowerCase().replace(/\/$/, "");
}

/** Keep only explicitly remembered identities when refreshing an old branch. */
export async function sessionPrs(source: GitSource, sessionId: string | undefined, branch: string | null, sessionBranch?: string | null, isCurrent: () => boolean = () => true): Promise<{ rows: SessionPr[]; error?: string }> {
  if (!isCurrent()) return { rows: [] };
  // The persisted session branch is explicit legacy association evidence. It
  // can differ from the live checkout after an agent creates a follow-up.
  const discover = async () => {
    const read = (name: string | null | undefined) => (name ? source.prs(name) : Promise.resolve([]))
      .then((prs) => ({ prs, error: undefined as string | undefined }), (error: unknown) => ({ prs: [] as PullRequest[], error: source.errorMessage(error) }));
    const [current, initial] = await Promise.all([read(branch), read(sessionId && sessionBranch !== branch ? sessionBranch : null)]);
    return { current: current.prs, initial: initial.prs, error: [current.error, initial.error].filter(Boolean).join("; ") || undefined };
  };
  const [remembered, discovery] = await Promise.all([
    sessionId ? gh.associations(sessionId) : Promise.resolve([]),
    discover(),
  ]);
  // A superseded read may finish after another session takes over the same
  // checkout. It must not attach that session's newly discovered PRs here.
  if (!isCurrent()) return { rows: [] };
  const current = discovery.current;
  // A shared checkout is not session ownership evidence. Only its saved
  // branch and explicit identities may contribute rows to this session.
  const owned = new Set(remembered.map((entry) => prIdentity(entry.url)));
  const discovered = [...discovery.initial, ...current.filter((pr) => !sessionId || branch === sessionBranch || owned.has(prIdentity(pr.url)))];
  const additions = source.prScope ? discovered.map((pr): PrAssociation => ({ url: pr.url, number: pr.number, head: pr.head, scope: source.prScope! })) : [];
  const associations = sessionId && additions.length ? await gh.associations(sessionId, additions) : remembered;
  const rows = new Map<string, SessionPr>();
  for (const association of associations) rows.set(prIdentity(association.url), { identity: prIdentity(association.url), association, current: false });
  for (const pr of discovered) {
    const identity = prIdentity(pr.url);
    rows.set(identity, { identity, pr, association: associations.find((entry) => prIdentity(entry.url) === identity), current: current.some((entry) => prIdentity(entry.url) === identity) });
  }
  await Promise.all([...rows.values()].filter((row) => !row.pr).map(async (row) => {
    try {
      if (!source.prAccess || !row.association) throw new Error("This PR's repository is unavailable.");
      const pr = await source.prAccess(row.association).read();
      if (prIdentity(pr.url) !== row.identity) throw new Error("GitHub returned a different pull request.");
      row.pr = pr;
    } catch (error) {
      row.error = source.errorMessage(error);
    }
  }));
  return { error: discovery.error, rows: [...rows.values()].sort((a, b) => Number(b.pr?.state === "OPEN") - Number(a.pr?.state === "OPEN") || (b.pr?.number ?? 0) - (a.pr?.number ?? 0)) };
}

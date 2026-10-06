import { useMemo, useState } from "react";
import { Archive, ChevronDown, Ellipsis, FolderInput, FolderOpen, FolderPlus, GitBranch, ListTree, MessagesSquare, Pin, Search, Settings, SquarePen, Trash2 } from "lucide-react";
import { open as openDialog } from "@tauri-apps/plugin-dialog";
import { Button } from "@/components/ui/button";
import { WithTooltip } from "@/components/ui/tooltip";
import { DropdownMenu, DropdownMenuContent, DropdownMenuItem, DropdownMenuLabel, DropdownMenuSeparator, DropdownMenuSub, DropdownMenuSubContent, DropdownMenuSubTrigger, DropdownMenuTrigger } from "@/components/ui/menu";
import { RowActions, actionRow, yieldsToRowActions } from "@/components/layout/RowActions";
import { AccountSidebarEntry } from "@/components/account/AccountSidebarEntry";
import { cn } from "@/lib/cn";
import { errorMessage } from "@/lib/api";
import { setPrefs } from "@/lib/prefs";
import { useShortcutKeys } from "@/lib/hotkeys";
import { addProject, archiveSession, pinSession, selectSession, startChat, startSessionIn, useSessionStore } from "@/lib/sessions";
import { moveChatToProject } from "@/lib/continuation";
import { confirmDeleteSession } from "@/lib/deleteSessionFlow";
import type { Project } from "@/types/session";
import { sessionStatus, type SessionEntry } from "@/types/session";
import { StatusStripe } from "./SidebarRows";
import { AppDestinations, type RailProps } from "./ProjectRail";
import { TITLEBAR_INSET } from "./AppShell";

const canon = (path: string) => path.replace(/\/+$/, "");
const DAY = 24 * 60 * 60 * 1000;

/** A conversation's name: its agent's title for it once there is one. */
export function conversationTitle(session: SessionEntry): string {
  const tab = session.tabs.find((entry) => entry.id === session.activeTab) ?? session.tabs[0];
  return tab?.title?.trim() || session.title;
}

/** Pinned first, then by when each was last touched, in the buckets a chat app uses. */
export function groupByRecency(sessions: SessionEntry[], now = Date.now()): { label: string; sessions: SessionEntry[] }[] {
  const midnight = new Date(now);
  midnight.setHours(0, 0, 0, 0);
  const today = midnight.getTime();
  const buckets: { label: string; since: number; sessions: SessionEntry[] }[] = [
    { label: "Today", since: today, sessions: [] },
    { label: "Yesterday", since: today - DAY, sessions: [] },
    { label: "Previous 7 days", since: today - 7 * DAY, sessions: [] },
    { label: "Older", since: -Infinity, sessions: [] },
  ];
  const pinned: SessionEntry[] = [];
  for (const session of [...sessions].sort((a, b) => b.modified.localeCompare(a.modified))) {
    if (session.pinned) {
      pinned.push(session);
      continue;
    }
    const at = Date.parse(session.modified);
    (buckets.find((bucket) => (Number.isNaN(at) ? -Infinity : at) >= bucket.since) ?? buckets[buckets.length - 1]).sessions.push(session);
  }
  return [{ label: "Pinned", sessions: pinned }, ...buckets].filter((group) => group.sessions.length > 0);
}

/**
 * The sidebar as a chat app draws it: start a conversation, find one, and
 * every conversation newest first, each a single line. Where it runs (a
 * project, or nowhere in particular) is a quiet tag on the row rather than a
 * level of a tree; the tree is one click away as the detailed sidebar.
 */
export function SessionSidebar({ onOpenSettings, onOpenAccount, onSearch, ...destinations }: RailProps) {
  const store = useSessionStore();
  const [error, setError] = useState<string | null>(null);
  const chatsPath = store.chatsPath ? canon(store.chatsPath) : null;
  const projectNames = useMemo(() => new Map(store.projects.map((project) => [canon(project.path), project.name])), [store.projects]);
  const projects = store.projects.filter((project) => !project.archived && canon(project.path) !== chatsPath).sort((a, b) => a.name.localeCompare(b.name));
  const groups = useMemo(
    () => groupByRecency(store.sessions.filter((session) => !session.archived || session.id === store.selectedSessionId)),
    [store.sessions, store.selectedSessionId],
  );

  const run = (action: () => Promise<unknown>) => () => {
    action().then(() => setError(null), (cause) => setError(errorMessage(cause)));
  };
  const newChat = run(() => startChat());
  const addAndStart = run(async () => {
    const dir = await openDialog({ directory: true, multiple: false, title: "Add a project" });
    if (typeof dir === "string") startSessionIn((await addProject(dir)).path, null);
  });

  return (
    <div className="flex h-full min-h-0 w-full shrink-0 flex-col overflow-hidden border-r border-hairline" data-testid="sidebar-rail">
      <div data-tauri-drag-region="deep" className="h-(--titlebar-h) shrink-0" style={{ paddingLeft: TITLEBAR_INSET }} />

      <div className="flex shrink-0 flex-col gap-0.5 px-2">
        <div className="flex items-center gap-1">
          <Button variant="ghost" className="min-w-0 flex-1 justify-start gap-2 px-2 text-foreground" onClick={newChat}>
            <SquarePen />
            New chat
          </Button>
          <DropdownMenu>
            <WithTooltip label="Start in a project">
              <DropdownMenuTrigger asChild>
                <Button variant="ghost" size="icon-sm" aria-label="Start in a project">
                  <ChevronDown />
                </Button>
              </DropdownMenuTrigger>
            </WithTooltip>
            <DropdownMenuContent align="end" className="w-60">
              <DropdownMenuItem onSelect={newChat}>
                <MessagesSquare /> New chat
              </DropdownMenuItem>
              {projects.length > 0 && (
                <>
                  <DropdownMenuSeparator />
                  <DropdownMenuLabel>Start in a project</DropdownMenuLabel>
                  {projects.map((project) => (
                    <DropdownMenuItem key={project.path} onSelect={() => startSessionIn(project.path, null)}>
                      <FolderOpen /> <span className="truncate">{project.name}</span>
                    </DropdownMenuItem>
                  ))}
                </>
              )}
              <DropdownMenuSeparator />
              <DropdownMenuItem onSelect={addAndStart}>
                <FolderPlus /> Add a project…
              </DropdownMenuItem>
            </DropdownMenuContent>
          </DropdownMenu>
        </div>
        <Button variant="ghost" className="justify-start gap-2 px-2" onClick={onSearch}>
          <Search />
          Search
          <Keys />
        </Button>
      </div>

      {error && <div className="mx-2 mt-1 shrink-0 rounded-md bg-destructive/10 px-2 py-1 text-xs text-destructive">{error}</div>}

      <nav className="mt-2 min-h-0 flex-1 basis-0 overflow-y-auto overscroll-contain scrollbar-thin px-2 pb-2" aria-label="Conversations" data-testid="conversation-list">
        {groups.length === 0 ? (
          <p className="px-2 py-6 text-center text-xs text-muted-foreground">No conversations yet.</p>
        ) : (
          groups.map((group) => (
            <section key={group.label} aria-label={group.label} className="mt-3 first:mt-0">
              <h2 className="px-2 pb-1 text-[11px] font-medium text-faint">{group.label}</h2>
              <ul className="flex flex-col gap-px">
                {group.sessions.map((session) => (
                  <ConversationRow
                    key={session.id}
                    session={session}
                    selected={store.selectedSessionId === session.id}
                    place={placeOf(session, chatsPath, projectNames)}
                    projects={canon(session.projectPath) === chatsPath ? projects : null}
                    onMove={(project, worktree) => run(async () => {
                      const moved = await moveChatToProject({ ...session, title: conversationTitle(session) }, project.path, worktree);
                      if (!moved.delivered) throw new Error("Moved, but the chat's context could not be confirmed as sent. It is in the new conversation's composer.");
                    })()}
                    onAddProject={(worktree) => run(async () => {
                      const dir = await openDialog({ directory: true, multiple: false, title: "Move this chat into a project" });
                      if (typeof dir !== "string") return;
                      const project = await addProject(dir);
                      const moved = await moveChatToProject({ ...session, title: conversationTitle(session) }, project.path, worktree && project.kind !== "folder");
                      if (!moved.delivered) throw new Error("Moved, but the chat's context could not be confirmed as sent. It is in the new conversation's composer.");
                    })()}
                  />
                ))}
              </ul>
            </section>
          ))
        )}
      </nav>

      <div className="shrink-0 border-t border-hairline px-2 pt-1">
        <AppDestinations {...destinations} />
      </div>
      <div className="flex shrink-0 items-center gap-1 px-2 pb-2" data-testid="sidebar-account-row">
        <div className="min-w-0 flex-1">
          <AccountSidebarEntry onOpenAccount={onOpenAccount} />
        </div>
        <WithTooltip label="Detailed sidebar: projects, workspaces and tabs" side="top">
          <Button variant="ghost" size="icon-sm" className="shrink-0" aria-label="Detailed sidebar" onClick={() => setPrefs({ sidebarLayout: "detailed" })}>
            <ListTree />
          </Button>
        </WithTooltip>
        <WithTooltip label="Settings" shortcut="app.settings" side="top">
          <Button variant="ghost" size="icon-sm" className="shrink-0" aria-label="Settings" onClick={() => onOpenSettings()}>
            <Settings />
          </Button>
        </WithTooltip>
      </div>
    </div>
  );
}

/** Where a conversation runs, as a row's quiet tag: nothing for a chat, else the project and any worktree branch. */
function placeOf(session: SessionEntry, chatsPath: string | null, projectNames: Map<string, string>): string | null {
  if (canon(session.projectPath) === chatsPath) return null;
  const project = projectNames.get(canon(session.projectPath)) ?? session.projectPath.split("/").pop() ?? null;
  return session.worktreeName && !session.worktreeRemoved && session.branch ? `${project} · ${session.branch}` : project;
}

function ConversationRow({
  session,
  selected,
  place,
  projects,
  onMove,
  onAddProject,
}: {
  session: SessionEntry;
  selected: boolean;
  place: string | null;
  /** The projects a chat may move into; null for a conversation already in one. */
  projects: Project[] | null;
  onMove: (project: Project, worktree: boolean) => void;
  onAddProject: (worktree: boolean) => void;
}) {
  const title = conversationTitle(session);
  return (
    <li>
      <DropdownMenu>
        <div
          className={cn(actionRow, "relative flex h-8 items-center gap-2 rounded-md pl-2 pr-1", selected ? "bg-selected text-foreground" : "text-foreground/85 hover:bg-selected/50")}
          title={place ? `${title} · ${place}` : title}
        >
          <StatusStripe status={sessionStatus(session)} size="row" />
          <button
            type="button"
            onClick={() => selectSession(session.id)}
            aria-current={selected ? "true" : undefined}
            className="flex min-w-0 flex-1 items-center gap-2 rounded-sm text-left outline-none focus-visible:ring-2 focus-visible:ring-ring/40"
          >
            {session.pinned && <Pin className="size-3 shrink-0 text-faint" />}
            <span className="min-w-0 flex-1 truncate text-[13px]">{title}</span>
            {place && <span className={cn("max-w-[45%] shrink-0 truncate text-[11px] text-faint", yieldsToRowActions)}>{place}</span>}
          </button>
          <RowActions>
            <DropdownMenuTrigger asChild>
              <Button variant="ghost" size="icon-xs" aria-label={`Menu for ${title}`}>
                <Ellipsis />
              </Button>
            </DropdownMenuTrigger>
          </RowActions>
        </div>
        <DropdownMenuContent align="end" className="w-56">
          {projects && (
            <>
              <DropdownMenuSub>
                <DropdownMenuSubTrigger>
                  <FolderInput /> Move to a project
                </DropdownMenuSubTrigger>
                <DropdownMenuSubContent className="w-60">
                  {projects.map((project) =>
                    project.kind === "folder" ? (
                      <DropdownMenuItem key={project.path} onSelect={() => onMove(project, false)}>
                        <FolderOpen /> <span className="truncate">{project.name}</span>
                      </DropdownMenuItem>
                    ) : (
                      <DropdownMenuSub key={project.path}>
                        <DropdownMenuSubTrigger>
                          <FolderOpen /> <span className="truncate">{project.name}</span>
                        </DropdownMenuSubTrigger>
                        <DropdownMenuSubContent>
                          <DropdownMenuItem onSelect={() => onMove(project, false)}>
                            <FolderOpen /> In the project
                          </DropdownMenuItem>
                          <DropdownMenuItem onSelect={() => onMove(project, true)}>
                            <GitBranch /> On a new branch (worktree)
                          </DropdownMenuItem>
                        </DropdownMenuSubContent>
                      </DropdownMenuSub>
                    ),
                  )}
                  {projects.length > 0 && <DropdownMenuSeparator />}
                  <DropdownMenuItem onSelect={() => onAddProject(false)}>
                    <FolderPlus /> Choose a folder…
                  </DropdownMenuItem>
                  <DropdownMenuItem onSelect={() => onAddProject(true)}>
                    <GitBranch /> Choose a repo, new branch…
                  </DropdownMenuItem>
                </DropdownMenuSubContent>
              </DropdownMenuSub>
              <DropdownMenuSeparator />
            </>
          )}
          <DropdownMenuItem onSelect={() => void pinSession(session.id, !session.pinned)}>
            <Pin /> {session.pinned ? "Unpin" : "Pin"}
          </DropdownMenuItem>
          <DropdownMenuItem onSelect={() => void archiveSession(session.id, !session.archived)}>
            <Archive /> {session.archived ? "Unarchive" : "Archive"}
          </DropdownMenuItem>
          <DropdownMenuSeparator />
          <DropdownMenuItem destructive onSelect={() => void confirmDeleteSession(session)}>
            <Trash2 /> Delete…
          </DropdownMenuItem>
        </DropdownMenuContent>
      </DropdownMenu>
    </li>
  );
}

function Keys() {
  const keys = useShortcutKeys("app.commandPalette");
  return (
    <span className="ml-auto flex gap-0.5 text-[10px] text-faint">
      {keys.map((key) => (
        <kbd key={key} className="rounded-sm bg-veil-raised px-1 font-sans">
          {key}
        </kbd>
      ))}
    </span>
  );
}

import { useMemo } from "react";
import { ChevronDown, Folder, FolderGit2, GitBranch, Pin, Plus } from "@/icons";
import { Button } from "@/components/ui/button";
import { WithTooltip } from "@/components/ui/tooltip";
import { RowActions, actionRow, yieldsToRowActions } from "@/components/layout/RowActions";
import { StatusMark } from "@/components/ui/StatusMark";
import { cn } from "@/lib/cn";
import { useAccount } from "@/lib/account";
import { catalogErrorText, placeCloudProjects, useCloudCatalog } from "@/lib/cloudCatalog";
import { cloudSessionStatus, useCloudWorkspaceSessions, type CloudSessionRow } from "@/lib/cloudSessions";
import { mayStartCloudSessions } from "@/lib/multiOrg";
import { usePrefs } from "@/lib/prefs";
import { selectCloudSession, selectCloudWorkspace, startCloudSessionIn, useSessionStore } from "@/lib/sessions";
import type { OrganizationSummary } from "@/lib/api";
import type { CloudProject, CloudWorkspaceNode } from "@/types/target";
import { sectionName, useCloudSections } from "./cloud/CloudSections";
import { RowTime } from "./SidebarRows";
import { ListSection, useFold } from "./SessionSidebar";

/**
 * The cloud organizations under the local projects, drawn as the sidebar draws
 * projects: a section per organization, named after it, listing its cloud
 * projects (a repository with workspaces, or a blank project); each project
 * folds to its sessions, and a project with several workspaces names each
 * workspace over its sessions. `+` on a project starts a session there, as it
 * does on a local project. Looking costs nothing: everything here is read from
 * the catalog and this desktop's caches; nothing connects or wakes a workspace.
 */
export function CloudOrganizations() {
  const cloud = useCloudSections();
  if (!cloud.orgs.length) return null;
  return (
    <>
      {cloud.orgs.map((org) => (
        <OrganizationSection key={org.id} org={org} live={cloud.live.has(org.id)} />
      ))}
    </>
  );
}

/** Whether the sidebar has an organization section, so the local projects are labelled "Local". */
export function useHasCloudOrganizations(): boolean {
  return useCloudSections().orgs.length > 0;
}

function OrganizationSection({ org, live }: { org: OrganizationSummary; live: boolean }) {
  const catalog = useCloudCatalog();
  const prefs = usePrefs();
  const { status } = useAccount();
  const cached = catalog.orgs[org.id];
  const placed = useMemo(
    () => placeCloudProjects(cached ?? { orgId: org.id, workspaces: [], repositories: null }, catalog.createMemory, { pinned: prefs.cloudPinned[org.id], added: prefs.cloudProjects[org.id], blank: prefs.cloudBlankProjects[org.id] }),
    [cached, org.id, catalog.createMemory, prefs.cloudPinned, prefs.cloudProjects, prefs.cloudBlankProjects],
  );
  const mayStart = mayStartCloudSessions(status, org.id);
  // Never listed yet: loading, or failed with nothing to show.
  const unloaded = !cached || (cached.fetchedAt === null && !cached.workspaces.length && !placed.projects.length);
  return (
    <ListSection id={`org:${org.id}`} label={sectionName(org)} data-testid="cloud-org-list" data-org={org.id}>
      {!live ? (
        <p className="px-2 py-1 text-[12px] text-faint">Its cloud sessions show when it is the default organization.</p>
      ) : unloaded ? (
        <p className="px-2 py-1 text-[12px] text-faint" role="status">{cached?.error ? catalogErrorText(cached.error) : "Loading cloud workspaces…"}</p>
      ) : placed.projects.length === 0 ? (
        <p className="px-2 py-1 text-[12px] text-faint">No cloud workspaces yet.</p>
      ) : (
        placed.projects.map((project) => <CloudProjectGroup key={project.key} project={project} startable={mayStart === true && project.selected} />)
      )}
    </ListSection>
  );
}

/** A cloud project, then its sessions along a guide line, like a local project's branches. */
function CloudProjectGroup({ project, startable }: { project: CloudProject; startable: boolean }) {
  const store = useSessionStore();
  const { animations } = usePrefs();
  const [open, toggle] = useFold(`cloud-project:${project.key}`, true);
  const drafting = store.cloudSessionPreset?.projectKey === project.key;
  const grouped = project.workspaces.length > 1;
  return (
    <div role="group" aria-label={project.fullName} className="mb-1" data-testid="cloud-project-group" data-project={project.key}>
      <div className={cn(actionRow, "relative flex h-8 items-center gap-2 rounded-md px-2", drafting ? "bg-selected" : "hover:bg-selected/50")} title={project.blank ? `${project.fullName} · no repository` : project.identity}>
        <button
          type="button"
          onClick={toggle}
          aria-expanded={open}
          aria-label={open ? `Collapse ${project.fullName}` : `Expand ${project.fullName}`}
          className="flex h-full min-w-0 flex-1 items-center gap-2 rounded-md text-left outline-none focus-visible:ring-2 focus-visible:ring-ring/40"
        >
          <span className="relative size-4 shrink-0">
            <span className={cn("absolute inset-0 transition-opacity duration-150", open ? "group-hover/row:opacity-0 group-focus-within/row:opacity-0" : "opacity-0")} aria-hidden>
              {project.blank ? <Folder className="size-4 text-faint" /> : <FolderGit2 className="size-4 text-faint" />}
            </span>
            <ChevronDown
              className={cn(
                "absolute inset-0 size-4 text-muted-foreground transition-[opacity,transform] duration-200",
                open ? "opacity-0 group-hover/row:opacity-100 group-focus-within/row:opacity-100" : "-rotate-90 opacity-100",
              )}
            />
          </span>
          <span className="min-w-0 flex-1 truncate text-[13px]">{project.fullName}</span>
          {project.pinned && <Pin className="size-3 shrink-0 text-faint" />}
          {project.blank && <span className="shrink-0 rounded-sm bg-veil-raised px-1 text-[9px] text-faint">no repo</span>}
          {!project.selected && <span className="shrink-0 rounded-sm bg-veil-raised px-1 text-[9px] text-faint">not accessible</span>}
        </button>
        {startable && (
          <RowActions>
            <WithTooltip label={`New session in ${project.fullName}`}>
              <Button variant="ghost" size="icon-xs" aria-label={`New session in ${project.fullName}`} onClick={() => startCloudSessionIn(project.key)}>
                <Plus />
              </Button>
            </WithTooltip>
          </RowActions>
        )}
      </div>
      <div
        className={cn("grid", animations && "transition-[grid-template-rows,opacity] duration-200 ease-out", open ? "grid-rows-[1fr] opacity-100" : "grid-rows-[0fr] opacity-0")}
        aria-hidden={!open}
        inert={!open}
        data-testid="cloud-project-sessions"
      >
        <div className="min-h-0 overflow-hidden">
          <ul className="ml-[15px] flex flex-col gap-px border-l border-hairline pl-1.5">
            {project.workspaces.length === 0 && <li className="px-2 py-1 text-[12px] text-faint">{project.blank ? "Not created yet." : "No workspace yet."}</li>}
            {project.workspaces.map((node) => (
              <WorkspaceRows key={node.key} node={node} named={grouped} open={open} />
            ))}
          </ul>
        </div>
      </div>
    </div>
  );
}

/** A workspace's sessions; with several workspaces in the project, under a row that names the workspace. */
function WorkspaceRows({ node, named, open }: { node: CloudWorkspaceNode; named: boolean; open: boolean }) {
  const store = useSessionStore();
  const { sessions, known } = useCloudWorkspaceSessions(node, { load: open, showArchived: store.showArchived, selectedKey: store.selectedSessionId });
  const { state } = node.item.workspace;
  const openable = state === "ready" || state === "suspended";
  const rows = (
    <>
      {sessions.map((row) => (
        <CloudSessionItem key={row.key} row={row} selected={store.selectedSessionId === row.key} />
      ))}
      {sessions.length === 0 && (
        <li className="px-2 py-1 text-[12px] text-faint" data-testid="cloud-workspace-empty" data-workspace={node.item.workspace.id}>
          {known || !openable ? (
            "No sessions yet."
          ) : (
            <button type="button" className="underline-offset-2 hover:text-muted-foreground hover:underline" onClick={() => selectCloudWorkspace(node.key)}>
              Open to load sessions
            </button>
          )}
        </li>
      )}
    </>
  );
  if (!named) return rows;
  return (
    <li data-testid="cloud-workspace-group" data-workspace={node.item.workspace.id}>
      <div className="flex h-8 items-center gap-2 rounded-md pl-2 pr-1 text-foreground/85" title={node.item.workspace.name}>
        <GitBranch className="size-3.5 shrink-0 text-faint" />
        <span className="min-w-0 flex-1 truncate font-mono text-[12px]">{node.item.workspace.name}</span>
        <span className="shrink-0 text-[11px] tabular-nums text-faint">{sessions.length}</span>
      </div>
      <ul className="ml-[13px] flex flex-col gap-px border-l border-hairline pl-1.5">{rows}</ul>
    </li>
  );
}

/** A cloud session as the sidebar draws a conversation: its name, its state at the end, and its branch or when it was last active. */
function CloudSessionItem({ row, selected }: { row: CloudSessionRow; selected: boolean }) {
  const status = cloudSessionStatus(row);
  return (
    <li>
      <div
        className={cn(actionRow, "relative flex h-8 items-center gap-2 rounded-md pl-2 pr-1", selected ? "bg-selected text-foreground" : "text-foreground/85 hover:bg-selected/50")}
        title={row.branch ? `${row.branch} · ${row.title}` : row.title}
        data-testid="cloud-session-row"
        data-session={row.key}
      >
        <button
          type="button"
          onClick={() => selectCloudSession(row.key)}
          aria-current={selected ? "true" : undefined}
          className="flex min-w-0 flex-1 items-center gap-2 rounded-sm text-left outline-none focus-visible:ring-2 focus-visible:ring-ring/40"
        >
          {row.pinned && <Pin className="size-3 shrink-0 text-faint" />}
          <span className="min-w-0 flex-1 truncate text-[13px]">{row.title}</span>
          <StatusMark status={status} />
          {status === "in_progress" ? null : row.branch ? (
            <span className={cn("flex max-w-[45%] shrink-0 items-center gap-1 font-mono text-[10px] text-faint", yieldsToRowActions)}>
              <GitBranch className="size-3 shrink-0" />
              <span className="truncate">{row.branch}</span>
            </span>
          ) : (
            <RowTime at={row.modified} />
          )}
        </button>
      </div>
    </li>
  );
}

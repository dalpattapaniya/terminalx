import { act, cleanup, fireEvent, render, screen, within } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { TooltipProvider } from "@/components/ui/tooltip";
import type { Project, SessionEntry } from "@/types/session";

const mocks = vi.hoisted(() => ({ invoke: vi.fn() }));
vi.mock("@tauri-apps/api/core", () => ({ invoke: mocks.invoke, convertFileSrc: (path: string) => path }));
vi.mock("@tauri-apps/api/event", () => ({ listen: vi.fn(async () => () => {}) }));
vi.mock("@tauri-apps/plugin-dialog", () => ({ ask: vi.fn(), open: vi.fn() }));
vi.mock("@tauri-apps/plugin-opener", () => ({ revealItemInDir: vi.fn() }));
vi.mock("./AppShell", () => ({ TITLEBAR_INSET: 78 }));
vi.mock("@/components/account/AccountSidebarEntry", () => ({ AccountSidebarEntry: () => null }));
vi.mock("@/lib/automations", () => ({ useAutomationStore: () => ({ automations: [] }) }));

const { ProjectRail } = await import("./ProjectRail");
const { orderConversations } = await import("./SessionSidebar");
const store = await import("@/lib/sessions");
const prefs = await import("@/lib/prefs");

const chatsPath = "/store/Chats";
const projects: Project[] = [{ path: "/repos/web", name: "web", kind: "git" }];
const session = (id: string, projectPath: string, modified: string, extra: Partial<SessionEntry> = {}): SessionEntry => ({
  id, projectPath, cwd: projectPath, title: `Session ${id}`, created: modified, modified, pinned: false, archived: false, worktreeRemoved: false, activeTab: `${id}-tab`,
  tabs: [{ id: `${id}-tab`, title: `Talk ${id}`, harness: "claude", model: "", permissionMode: "auto", status: "idle", created: "", modified: "" }],
  ...extra,
});
let sessions: SessionEntry[];

beforeEach(async () => {
  // Folds are remembered; each test starts from the defaults.
  prefs.setPrefs({ sidebarLayout: "simple", sidebarSections: {} });
  mocks.invoke.mockReset().mockImplementation(async (command: string) => {
    if (command === "list_projects") return { projects, lastSelected: null };
    if (command === "list_sessions") return sessions;
    if (command === "list_harnesses") return [];
    if (command === "chats_dir") return chatsPath;
    if (command === "list_workspaces") return [{ path: "/repos/web", name: "web", branch: "main", head: "abc", isMain: true, managed: false, uncommitted: 0, additions: 0, deletions: 0, unpushed: 0, ahead: 0, behind: 0 }];
    return undefined;
  });
});
afterEach(cleanup);

const mount = () => render(<TooltipProvider><ProjectRail onOpenSettings={() => {}} onOpenAccount={() => {}} onOpenIssues={() => {}} onOpenAgents={() => {}} onOpenStats={() => {}} onOpenAutomations={() => {}} onOpenSkills={() => {}} onSearch={() => {}} /></TooltipProvider>);
const rowsIn = (element: HTMLElement) => within(element).getAllByRole("listitem").map((row) => row.querySelector("[title]")?.getAttribute("title"));

async function boot(list: SessionEntry[]) {
  sessions = list;
  await act(async () => {
    await store.bootSessions();
    await store.refreshEverything();
  });
}

describe("the conversation list", () => {
  it("puts each conversation under the project it is linked to, a chat by the repo its agent works in, and the rest under Chats", async () => {
    const at = (n: number) => new Date(Date.UTC(2026, 9, n)).toISOString();
    await boot([
      session("chat", chatsPath, at(3)),
      session("work", "/repos/web", at(2), { worktreeName: "fix-login", branch: "raccoon/fix-login" }),
      session("here", "/repos/web", at(1)),
      session("gone", chatsPath, at(4), { archived: true }),
      session("follow", chatsPath, at(0), { link: { path: "/repos/web", branch: "feature/new-work" } }),
    ]);
    mount();

    // Under a project, a conversation alone on its branch is named by the branch.
    const web = screen.getByRole("group", { name: "web" });
    expect(within(web).getAllByRole("button", { name: /, Talk/ }).map((row) => row.getAttribute("aria-label"))).toEqual([
      "raccoon/fix-login, Talk work",
      "main, Talk here",
      "feature/new-work, Talk follow",
    ]);
    expect(rowsIn(screen.getByRole("region", { name: "Chats" }))).toEqual(["Talk chat"]);
  });

  it("names the branch a conversation in the shared checkout worked on, not where another one has since switched it", async () => {
    const at = (n: number) => new Date(Date.UTC(2026, 9, n)).toISOString();
    // The checkout is on main now; one conversation started on release, one last worked on dev.
    await boot([
      session("kept", "/repos/web", at(2), { branch: "release" }),
      session("moved", "/repos/web", at(1), { branch: "release", link: { path: "/repos/web", branch: "dev" } }),
    ]);
    mount();
    const web = screen.getByRole("group", { name: "web" });
    expect(within(web).getAllByRole("button", { name: /, Talk/ }).map((row) => row.getAttribute("aria-label"))).toEqual(["release, Talk kept", "dev, Talk moved"]);
  });

  it("starts another session on a branch from its row, in the folder where that branch is checked out", async () => {
    const at = (n: number) => new Date(Date.UTC(2026, 9, n)).toISOString();
    await boot([session("here", "/repos/web", at(1)), session("gone-branch", "/repos/web", at(2), { branch: "old" })]);
    const base = mocks.invoke.getMockImplementation()!;
    mocks.invoke.mockImplementation(async (command: string, args?: unknown) => {
      if (command === "list_harnesses") return [{ id: "claude", name: "Claude", available: true }];
      if (command === "create_session") return session("new", "/repos/web", at(3));
      return base(command, args);
    });
    await act(async () => { await store.refreshHarnesses(); });
    mount();
    const web = screen.getByRole("group", { name: "web" });
    // A branch checked out nowhere has nowhere to start one.
    expect(within(web).queryByRole("button", { name: "New session on old" })).toBeNull();
    await act(async () => { fireEvent.click(within(web).getByRole("button", { name: "New session on main" })); });
    const created = mocks.invoke.mock.calls.find(([command]) => command === "create_session");
    expect(created?.[1]).toMatchObject({ req: { projectPath: "/repos/web", cwd: "/repos/web", useWorktree: false } });
  });

  it("removes a project from its menu after asking, archiving its conversations rather than deleting them", async () => {
    const at = (n: number) => new Date(Date.UTC(2026, 9, n)).toISOString();
    await boot([session("here", "/repos/web", at(1)), session("there", "/repos/web", at(2), { branch: "old" }), session("done", "/repos/web", at(3), { archived: true })]);
    const dialog = await import("@tauri-apps/plugin-dialog");
    vi.mocked(dialog.ask).mockResolvedValue(true);
    mount();
    fireEvent.pointerDown(screen.getByRole("button", { name: "Project menu for web" }), { button: 0, ctrlKey: false, pointerType: "mouse" });
    await act(async () => { fireEvent.click(await screen.findByRole("menuitem", { name: "Remove from TerminalX…" })); });
    expect(vi.mocked(dialog.ask).mock.calls[0][0]).toBe("Remove web from TerminalX? Its 2 conversations are archived, and the folder on disk is left as it is.");
    const calls = mocks.invoke.mock.calls.filter(([command]) => command === "set_session_archived" || command === "remove_project").map(([command, args]) => [command, args]);
    expect(calls).toEqual([["set_session_archived", { sessionId: "there", archived: true }], ["set_session_archived", { sessionId: "here", archived: true }], ["remove_project", { path: "/repos/web" }]]);
    expect(screen.queryByRole("group", { name: "web" })).toBeNull();
  });

  it("marks each row with its agent only once the conversations use more than one", async () => {
    const at = (n: number) => new Date(Date.UTC(2026, 9, n)).toISOString();
    const codex = (entry: SessionEntry) => ({ ...entry, tabs: entry.tabs.map((tab) => ({ ...tab, harness: "codex" })) });
    await boot([session("one", chatsPath, at(1)), session("two", chatsPath, at(2))]);
    mount();
    expect(document.querySelectorAll("[data-agent-id]")).toHaveLength(0);
    cleanup();

    await boot([session("one", chatsPath, at(1)), codex(session("two", chatsPath, at(2)))]);
    mount();
    const chats = screen.getByRole("region", { name: "Chats" });
    expect([...chats.querySelectorAll("[data-agent-id]")].map((mark) => mark.getAttribute("data-agent-id"))).toEqual(["codex", "claude"]);
  });

  it("gathers the conversations on one branch under it, and shows a project's latest few branches before View all", async () => {
    const at = (n: number) => new Date(Date.UTC(2026, 9, n)).toISOString();
    await boot([
      session("m1", "/repos/web", at(9)),
      session("m2", "/repos/web", at(8)),
      ...[1, 2, 3].map((n) => session(`w${n}`, "/repos/web", at(n), { worktreeName: `b${n}`, branch: `b${n}` })),
    ]);
    mount();
    const web = screen.getByRole("group", { name: "web" });
    const main = within(web).getByRole("button", { name: /^main/ });
    expect(main.textContent).toContain("2");
    expect(within(web).queryByText("Talk m1")).toBeNull();
    fireEvent.click(main);
    expect(within(web).getByText("Talk m1")).toBeTruthy();
    expect(within(web).getByText("Talk m2")).toBeTruthy();

    expect(within(web).queryByRole("button", { name: "b1, Talk w1" })).toBeNull();
    fireEvent.click(within(web).getByRole("button", { name: "View all" }));
    expect(within(web).getByRole("button", { name: "b1, Talk w1" })).toBeTruthy();
  });

  it("invites a first chat when there is none", async () => {
    await boot([]);
    mount();
    expect(within(screen.getByTestId("conversation-list")).getByRole("button", { name: /Start a chat/ })).toBeTruthy();
  });

  it("names the app's destinations above the conversations, the dashboard first", async () => {
    await boot([]);
    mount();
    const destinations = screen.getByRole("toolbar", { name: "Destinations" });
    expect(within(destinations).getAllByRole("button").map((button) => button.textContent)).toEqual(["Dashboard", "Issues", "Automations", "Skills", "Usage"]);
    expect(destinations.compareDocumentPosition(screen.getByTestId("conversation-list")) & Node.DOCUMENT_POSITION_FOLLOWING).toBeTruthy();
  });
});

describe("ordering conversations", () => {
  it("puts pinned ones first, then the newest, and keeps the open one even when archived", () => {
    const at = (n: number) => new Date(Date.UTC(2026, 9, n)).toISOString();
    const ordered = orderConversations([
      session("old", chatsPath, at(1)),
      session("new", "/a", at(5)),
      session("pin", "/b", at(2), { pinned: true }),
      session("arch", chatsPath, at(6), { archived: true }),
      session("open", chatsPath, at(3), { archived: true }),
    ], "open");
    expect(ordered.map((entry) => entry.id)).toEqual(["pin", "new", "open", "old"]);
  });
});

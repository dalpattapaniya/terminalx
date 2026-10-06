import { act, cleanup, render, screen, within } from "@testing-library/react";
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
const { groupByRecency } = await import("./SessionSidebar");
const store = await import("@/lib/sessions");
(await import("@/lib/prefs")).setPrefs({ sidebarLayout: "simple" });

const chatsPath = "/store/Chats";
const projects: Project[] = [{ path: "/repos/web", name: "web", kind: "git" }];
const session = (id: string, projectPath: string, modified: string, extra: Partial<SessionEntry> = {}): SessionEntry => ({
  id, projectPath, cwd: projectPath, title: `Session ${id}`, created: modified, modified, pinned: false, archived: false, worktreeRemoved: false, activeTab: `${id}-tab`,
  tabs: [{ id: `${id}-tab`, title: `Talk ${id}`, harness: "claude", model: "", permissionMode: "auto", status: "idle", created: "", modified: "" }],
  ...extra,
});
let sessions: SessionEntry[];

beforeEach(async () => {
  mocks.invoke.mockReset().mockImplementation(async (command: string) => {
    if (command === "list_projects") return { projects, lastSelected: null };
    if (command === "list_sessions") return sessions;
    if (command === "list_harnesses") return [];
    if (command === "chats_dir") return chatsPath;
    if (command === "list_workspaces") return [];
    return undefined;
  });
});
afterEach(cleanup);

describe("the conversation list", () => {
  it("lists chats and project work as one list, newest first, the place a quiet tag", async () => {
    const now = new Date();
    const today = now.toISOString();
    const lastMonth = new Date(now.getTime() - 40 * 24 * 60 * 60 * 1000).toISOString();
    sessions = [
      session("chat", chatsPath, today),
      session("work", "/repos/web", today, { worktreeName: "fix-login", branch: "raccoon/fix-login" }),
      session("old", "/repos/web", lastMonth),
      session("gone", chatsPath, today, { archived: true }),
    ];
    await act(async () => {
      await store.bootSessions();
      await store.refreshEverything();
    });
    render(<TooltipProvider><ProjectRail onOpenSettings={() => {}} onOpenAccount={() => {}} onOpenIssues={() => {}} onOpenAgents={() => {}} onOpenStats={() => {}} onOpenAutomations={() => {}} onOpenSkills={() => {}} onSearch={() => {}} /></TooltipProvider>);

    const list = screen.getByRole("navigation", { name: "Conversations" });
    expect(within(list).getAllByRole("heading").map((h) => h.textContent)).toEqual(["Today", "Older"]);
    const rows = within(list).getAllByRole("listitem").map((row) => row.textContent);
    expect(rows).toHaveLength(3);
    expect(rows).toContain("Talk chat");
    expect(rows).toContain("Talk workweb · raccoon/fix-login");
    expect(rows.at(-1)).toBe("Talk oldweb");
    expect(screen.queryByText("Projects")).toBeNull();
  });
});

describe("recency groups", () => {
  it("puts pinned conversations first, then today, yesterday, the week, and the rest", () => {
    const now = new Date("2026-10-06T15:00:00").getTime();
    const at = (iso: string) => new Date(iso).toISOString();
    const groups = groupByRecency([
      session("a", chatsPath, at("2026-10-06T09:00:00")),
      session("b", chatsPath, at("2026-10-05T23:00:00")),
      session("c", chatsPath, at("2026-10-01T12:00:00")),
      session("d", chatsPath, at("2026-08-01T12:00:00")),
      session("p", chatsPath, at("2026-07-01T12:00:00"), { pinned: true }),
    ], now);
    expect(groups.map((group) => [group.label, group.sessions.map((s) => s.id)])).toEqual([
      ["Pinned", ["p"]], ["Today", ["a"]], ["Yesterday", ["b"]], ["Previous 7 days", ["c"]], ["Older", ["d"]],
    ]);
  });
});

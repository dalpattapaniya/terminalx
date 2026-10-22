import { act, cleanup, render, screen, within } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { TooltipProvider } from "@/components/ui/tooltip";
import type { ProjectOverview as Overview } from "@/lib/api";

const mocks = vi.hoisted(() => ({ invoke: vi.fn() }));
vi.mock("@tauri-apps/api/core", () => ({ invoke: mocks.invoke, convertFileSrc: (path: string) => path }));
vi.mock("@tauri-apps/api/event", () => ({ listen: vi.fn(async () => () => {}) }));
vi.mock("@tauri-apps/plugin-opener", () => ({ openUrl: vi.fn(), revealItemInDir: vi.fn() }));

const { ProjectOverview } = await import("./ProjectOverview");
const store = await import("@/lib/sessions");

const at = (daysAgo: number) => new Date(Date.now() - daysAgo * 86_400_000).toISOString();
const overview: Overview = {
  defaultBranch: "main",
  recentCommits: 91,
  branches: [
    { name: "feat/login", committedAt: at(1), author: "Ada", subject: "Add login", ahead: 3, behind: 2 },
    { name: "main", committedAt: at(2), author: "Bo", subject: "Release", ahead: 0, behind: 0 },
  ],
  contributors: [{ name: "Ada Lovelace", email: "ada@x.dev", login: null, commits: 59, days: [0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 1, 3] }],
  latest: [{ sha: "abc1234", author: "Bo", email: "bo@x.dev", at: at(2), subject: "Release" }],
  pullRequests: { open: 4, merged: 12, recent: [{ number: 102, title: "Privacy options", url: "https://github.com/a/b/pull/102", state: "OPEN", isDraft: false, author: "janedoe", at: at(0) }] },
  pullRequestNote: null,
};

beforeEach(async () => {
  mocks.invoke.mockReset().mockImplementation(async (command: string) => {
    if (command === "list_projects") return { projects: [{ path: "/repos/web", name: "web", kind: "git" }], lastSelected: null };
    if (command === "list_sessions") return [];
    if (command === "project_overview") return overview;
    if (command === "github_identity") return { login: null, source: null, email: null, repo: "acme/web" };
    return undefined;
  });
  await act(async () => { await store.bootSessions(); });
});
afterEach(cleanup);

describe("a project's overview", () => {
  it("shows the numbers, who works on it, its pull requests, branches and latest commits, and how to start", async () => {
    render(<TooltipProvider><ProjectOverview path="/repos/web" /></TooltipProvider>);
    const glance = await screen.findByRole("region", { name: "At a glance" });
    await within(glance).findByText("91");
    expect(glance.textContent).toBe("91Commits4Open PRs12Merged PRs2Branches");
    expect(within(screen.getByRole("region", { name: "Top contributors" })).getByText("59 commits")).toBeTruthy();
    expect(screen.getByLabelText("4 commits in the last two weeks")).toBeTruthy();
    expect(within(screen.getByRole("region", { name: "Pull requests" })).getByText(/Privacy options/)).toBeTruthy();
    const branches = screen.getByRole("region", { name: "Branches" });
    expect(within(branches).getByText("↑3 ↓2")).toBeTruthy();
    expect(within(branches).getByRole("button", { name: "New chat on feat/login" })).toBeTruthy();
    expect(within(screen.getByRole("region", { name: "Latest" })).getByText("abc1234")).toBeTruthy();
    expect(screen.getByRole("button", { name: "New chat" })).toBeTruthy();
    expect(screen.getByRole("button", { name: "New branch" })).toBeTruthy();
  });

  it("says why there are no pull requests", async () => {
    mocks.invoke.mockImplementation(async (command: string) => (command === "project_overview" ? { ...overview, pullRequests: null, pullRequestNote: "Install the GitHub CLI (gh) to see pull requests." } : undefined));
    render(<TooltipProvider><ProjectOverview path="/repos/web" /></TooltipProvider>);
    expect(await screen.findByText("Install the GitHub CLI (gh) to see pull requests.")).toBeTruthy();
  });
});

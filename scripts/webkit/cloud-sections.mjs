#!/usr/bin/env node
// WebKit check for what is left of the PRO-23 sidebar gaps: Search lists the
// projects of the same name in two organizations, each with its
// organization, and a local one marked Local.
//
// Not checked since the chat-first sidebar (#484): the reader's own named
// sections in an organization (a long name truncating before the count and
// menu, the rename field fitting its row, "Move to section", a new section
// opening with its name field focused), an organization whose list failed
// (worded, with Retry) and one still loading, and the tree not scrolling
// sideways. Sections, organization headers and their lines were rows of the
// cloud tree, which the sidebar no longer mounts; nothing else renders
// `cloud-section-row`, `cloud-org-section`, `cloud-org-failed` or
// `cloud-org-loading`.
//
// With SCREENSHOTS=<dir> it also saves the screen, in light and dark.
//
//   pnpm build && npx playwright install webkit && node scripts/webkit/cloud-sections.mjs
import { launch, open, report, serve, sidebar } from "./harness.mjs";

const shots = process.env.SCREENSHOTS ?? null;
const server = await serve();
const browser = await launch();
let failed = 0;

// Two live organizations with a project called "accone" each, one whose list
// fails, one that never answers; the reader has two sections in the first.
const extra = `(() => {
  const stub = window.__PW_STUB__;
  const now = Date.now();
  const org = (id, name, role) => ({ id, name, role, isPersonal: false, cloud: { enabled: true, flags: {} } });
  stub.answers.account_status = { ...stub.answers.account_status, identity: { name: "Alice", email: "a@b.c", organization: "Share Lab", organizationId: "org-a" }, multiOrg: true,
    organizations: [org("org-a", "Share Lab", "owner"), org("org-b", "Second Lab", "owner"), org("org-c", "Third Lab", "member"), org("org-d", "Fourth Lab", "member")] };
  const api = { identity: "github.com/acme/api", fullName: "acme/api", cloneUrl: "https://github.com/acme/api.git", primary: true };
  const ws = (orgId, id, name, repositories = []) => ({ workspace: { id, orgId, name, provider: "local-docker", state: "ready", accessMode: "organization", createdAt: 1, updatedAt: now, lastActivityAt: now, releaseDisposition: null, repositories }, latestOperation: null });
  const lists = {
    "org-a": [ws("org-a", "w1", "accone"), ws("org-a", "w2", "acctwo"), ws("org-a", "w3", "api-fix", [api])],
    "org-b": [ws("org-b", "w4", "accone")],
  };
  stub.answers.cloud_catalog_load = { version: 1, createMemory: {}, orgs: {} };
  stub.answers.cloud_workspaces = ({ orgId }) => {
    if (orgId === "org-c") throw { code: "cloud_workspace_unavailable", message: "unavailable" };
    if (orgId === "org-d") return new Promise(() => undefined);
    return { workspaces: lists[orgId] ?? [], quota: { used: 1, limit: 2, running: { used: 1, limit: 2 } } };
  };
  stub.answers.cloud_workspace_repositories = ({ orgId }) => ({ configured: true, repositories: orgId === "org-a" ? [{ fullName: "acme/api", cloneUrl: "https://github.com/acme/api.git" }] : [] });
  stub.answers.list_projects = { projects: [{ path: "/Users/alice/code/accone", name: "accone" }], lastSelected: "/Users/alice/code/accone" };
  try {
    const saved = JSON.parse(localStorage.getItem("raccoon.prefs") || "{}");
    localStorage.setItem("raccoon.prefs", JSON.stringify({ ...saved, cloudSections: { "a@b.c": { "org-a": [
      { id: "s1", name: "Active", projects: ["blank/accone", "github.com/acme/api"] },
      { id: "s2", name: "Parked until the quarterly release review is done", projects: ["blank/acctwo"] },
    ] } } }));
  } catch {}
})();`;

for (const colorScheme of shots ? ["light", "dark"] : ["light"]) {
  const page = await open(browser, server.url, { cloud: true, localProjects: 1 }, { width: 1100, height: 760 }, { extra, colorScheme });
  await sidebar(page);
  const shot = async (name) => shots && (await page.screenshot({ path: `${shots}/${name}-${colorScheme}.png` }));

  // Search: three projects called accone, each saying where it is.
  await page.getByRole("button", { name: /^Search/ }).click();
  await page.getByLabel("Command palette search").fill("accone");
  await page.locator('[role="option"][id^="cloud-project:"]').nth(1).waitFor();
  const projectRows = await page.locator('[role="option"][id^="cloud-project:"], [role="option"][id^="project:"]').allTextContents();
  failed += report(`search (${colorScheme})`, {
    "lists both organizations' accone and the local one, each labelled": ["Share Lab cloud · no repository", "Second Lab cloud · no repository", "Local · /Users/alice/code/accone"].every((label) => projectRows.some((row) => row.startsWith("accone") && row.endsWith(label))),
  }, projectRows);
  await shot("search-projects");
  await page.close();
}

await browser.close();
server.close();
process.exit(failed ? 1 : 0);

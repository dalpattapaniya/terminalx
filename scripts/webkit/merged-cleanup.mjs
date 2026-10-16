#!/usr/bin/env node
// WebKit check for the removal that follows a merged pull request (#432), in
// the built app (`pnpm build`) with a stubbed backend:
//
// - the notices (about to go, with Keep; gone, with Undo) are inside the
//   window, and Keep reaches the backend and withdraws its notice;
// - Settings → Storage shows the setting, each project's own answer, GitHub's
//   own head-branch setting and what was removed, at 960 px and at 720 px,
//   with nothing leaving its column or running into its neighbour.
//
// Not checked since the chat-first sidebar (#484): a workspace's row saying
// why it is kept or when it goes (`merged-note`: its words inside the sidebar
// on one line, a long reason cut short, none under its button, and the row
// saying "Stop keeping" after Keep), since the sidebar lists conversations
// and has no workspace rows; what stands where is in Settings → Storage.
//
//   pnpm build && npx playwright install webkit && node scripts/webkit/merged-cleanup.mjs [screenshot-dir]
import { launch, open, report, serve, sidebar } from "./harness.mjs";

const shots = process.argv[2];
const server = await serve();
const browser = await launch();
let failed = 0;
const SLACK = 0.5;

/** The elements of `selector` under `root` that leave it sideways. */
const leaving = (page, root, selector) =>
  page.evaluate(
    ([root, selector, slack]) => {
      const box = document.querySelector(root).getBoundingClientRect();
      return [...document.querySelector(root).querySelectorAll(selector)]
        .filter((element) => {
          const rect = element.getBoundingClientRect();
          return rect.width && (rect.left < box.left - slack || rect.right > box.right + slack);
        })
        .map((element) => element.textContent.slice(0, 50));
    },
    [root, selector, SLACK],
  );

for (const width of [960, 720]) {
  const page = await open(browser, server.url, { cloud: true, localProjects: 3, mergedCleanup: true }, { width, height: 760 });
  await sidebar(page);
  const notices = page.locator('[data-testid="merged-notice"]');
  await notices.nth(1).waitFor();
  // The notices fade in; measure and picture them once they have.
  await page.waitForTimeout(400);
  if (shots) await page.screenshot({ path: `${shots}/merged-${width}-notices.png` });

  const shown = await notices.evaluateAll((elements) => {
    const boxes = elements.map((notice) => notice.getBoundingClientRect()).map((rect) => ({ top: rect.top, bottom: rect.bottom, inside: rect.left >= 0 && rect.right <= innerWidth && rect.top >= 0 && rect.bottom <= innerHeight }));
    return { notices: boxes, scroll: document.scrollingElement.scrollWidth - document.scrollingElement.clientWidth };
  });
  failed += report(`notices at ${width}px`, {
    "the notices are inside the window and apart": shown.notices.length === 2 && shown.notices.every((notice) => notice.inside) && shown.notices[0].bottom <= shown.notices[1].top + SLACK,
    "nothing pushes the page sideways": shown.scroll === 0,
  }, shown);

  // Keep, from the notice: the backend is told, and the notice goes.
  const keep = notices.getByRole("button", { name: "Keep workspace" });
  await keep.click();
  await keep.waitFor({ state: "detached" });
  failed += report(`keep at ${width}px`, {
    "Keep reaches the backend with the workspace's path": (await page.evaluate(() => window.__PW_MERGED__.held)).join() === "/repos/p0/.raccoon/worktrees/quiet-amber-fox",
    "its notice is withdrawn and the removal's stays": (await notices.count()) === 1 && (await notices.getByRole("button", { name: "Undo" }).count()) === 1,
  });

  await page.locator('[data-testid="sidebar-rail"]').getByRole("button", { name: "Settings", exact: true }).click();
  await page.getByRole("button", { name: "Storage", exact: true }).click();
  const section = '[data-testid="merged-cleanup-settings"]';
  await page.locator(section).getByRole("list", { name: "Workspaces removed after their pull request merged" }).waitFor();
  await page.locator(section).getByRole("button", { name: /Automatically delete head branches/ }).waitFor();
  await page.locator(section).scrollIntoViewIfNeeded();
  if (shots) await page.screenshot({ path: `${shots}/merged-${width}-settings.png`, fullPage: false });
  const settings = await page.evaluate(
    ([section, slack]) => {
      const root = document.querySelector(section);
      // A row is a line of columns: its text may not run under what follows it.
      const overlapping = [...root.querySelectorAll("li")].filter((row) => {
        const line = getComputedStyle(row).display === "flex" && getComputedStyle(row).flexDirection === "row" ? row : row.firstElementChild;
        const cells = [...line.children].map((cell) => cell.getBoundingClientRect()).filter((rect) => rect.width);
        return cells.some((rect, index) => cells.slice(index + 1).some((next) => Math.abs(next.top - rect.top) < rect.height && rect.right > next.left + slack));
      }).map((row) => row.textContent.slice(0, 50));
      return { overlapping, projects: [...root.querySelectorAll('ul[aria-label="Projects"] > li')].length, removed: [...root.querySelectorAll('ul[aria-label^="Workspaces removed"] > li')].length, restore: [...root.querySelectorAll('button[aria-label^="Restore "]')].length, scroll: document.scrollingElement.scrollWidth - document.scrollingElement.clientWidth };
    },
    [section, SLACK],
  );
  const outside = await leaving(page, section, "li, button, p, span, [role=radiogroup], [role=switch]");
  failed += report(`settings at ${width}px`, {
    "the projects and the cloud workspaces are listed": settings.projects === 5,
    "the removals are listed, and only what can be brought back offers Restore": settings.removed === 4 && settings.restore === 2,
    "nothing leaves the section": outside.length === 0 && settings.scroll === 0,
    "no row runs into its control": settings.overlapping.length === 0,
  }, { settings, outside });

  // The head-branch offer, and a restore that has something to say.
  await page.locator(section).getByRole("button", { name: /Automatically delete head branches/ }).click();
  await page.locator(section).getByText(/GitHub deletes the merged branches of a-rather-long-organization-name/).waitFor();
  await page.locator(section).getByRole("button", { name: "Restore gone-blue-jay" }).click();
  await page.locator(section).getByText(/has been used since and stays under the project/).waitFor();
  await page.locator(section).getByRole("list", { name: "Workspaces removed after their pull request merged" }).scrollIntoViewIfNeeded();
  if (shots) await page.screenshot({ path: `${shots}/merged-${width}-removed.png` });
  const after = await leaving(page, section, "li, button, p, span");
  failed += report(`after a restore at ${width}px`, {
    "the head-branch setting was turned on once": (await page.evaluate(() => window.__PW_MERGED__.headBranch)) === 1,
    "the restore reached the backend": (await page.evaluate(() => window.__PW_MERGED__.restored)).join() === "removal-0",
    "nothing leaves the section": after.length === 0,
  }, after);
  await page.close();
}

await browser.close();
server.close();
process.exit(failed ? 1 : 0);

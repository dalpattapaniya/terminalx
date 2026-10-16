#!/usr/bin/env node
// WebKit layout check for the new-workspace dialog (PRO-68), measured at
// 1000x520 since jsdom draws nothing: the dialog fits the window and its
// overlay covers all of it, the sidebar included.
//
// The dialog is reached through the command palette ("New cloud workspace…"):
// the chat-first sidebar has no organization menu.
//
// Not checked since the chat-first sidebar (#484): the notice for a workspace
// deleted elsewhere (wrapping inside the sidebar, Dismiss inside it), the
// archived rows with their three lines (a failed archive's reason wrapping,
// what the final save did, the actions inside the sidebar) and the archived
// section's billing note. They were rows and notices of the cloud tree,
// which the sidebar no longer mounts; nothing else renders them.
//
//   pnpm build && npx playwright install webkit && pnpm test:webkit-layout
import { launch, open, report, serve, sidebar } from "./harness.mjs";

const SLACK = 1;
const server = await serve();
const browser = await launch();
let failed = 0;

const page = await open(browser, server.url, { cloud: true, localProjects: 1, archived: true }, { width: 1000, height: 520 });
await sidebar(page);

// The new-workspace dialog, from the command palette.
await page.keyboard.press("Meta+K");
const palette = page.getByRole("dialog");
await palette.locator("input").fill("New cloud workspace");
await palette.getByRole("option", { name: /^New cloud workspace/ }).click();
await page.getByTestId("cloud-new-workspace-dialog").waitFor();
const dialog = await page.evaluate(() => {
  const box = document.querySelector('[data-testid="cloud-new-workspace-dialog"]').getBoundingClientRect();
  const overlay = document.querySelector("[data-dialog-overlay]").getBoundingClientRect();
  const rail = document.querySelector('[data-testid="sidebar-rail"]').getBoundingClientRect();
  const top = document.elementFromPoint(rail.left + 20, rail.top + 20);
  return { box: { left: box.left, right: box.right, top: box.top, bottom: box.bottom }, overlay: { left: overlay.left, right: overlay.right, top: overlay.top, bottom: overlay.bottom }, viewport: { width: innerWidth, height: innerHeight }, sidebarCovered: !!top?.closest("[data-dialog-overlay]") };
});
failed += report(
  "new cloud workspace dialog",
  {
    "the dialog is inside the window": dialog.box.left >= -SLACK && dialog.box.top >= -SLACK && dialog.box.right <= dialog.viewport.width + SLACK && dialog.box.bottom <= dialog.viewport.height + SLACK,
    "the overlay covers the whole window": dialog.overlay.left <= SLACK && dialog.overlay.top <= SLACK && dialog.overlay.right >= dialog.viewport.width - SLACK && dialog.overlay.bottom >= dialog.viewport.height - SLACK,
    "the sidebar is under the overlay": dialog.sidebarCovered,
  },
  dialog,
);

await page.close();
await browser.close();
server.close();
process.exit(failed ? 1 : 0);

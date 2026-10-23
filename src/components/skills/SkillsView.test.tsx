import { cleanup, fireEvent, render, screen, waitFor } from "@testing-library/react";
import { afterEach, expect, it, vi } from "vitest";
import { TooltipProvider } from "@/components/ui/tooltip";
import { SkillsView } from "./SkillsView";

const list = vi.hoisted(() => vi.fn(async () => []));
vi.mock("@/lib/api", async (original) => ({ ...(await original<typeof import("@/lib/api")>()), skills: { list } }));
afterEach(cleanup);

it("offers a refresh in the empty reader using the existing discovery flow", async () => {
  render(<TooltipProvider><SkillsView projectPath="/repo" initialAgent={null} /></TooltipProvider>);
  const heading = screen.getByRole("heading", { name: "Skills", level: 2 });
  expect(heading).toBeTruthy();
  await waitFor(() => expect(list).toHaveBeenCalledWith("/repo", false));
  const pane = heading.parentElement!;
  fireEvent.click(pane.querySelector("button")!);
  await waitFor(() => expect(list).toHaveBeenCalledWith("/repo", true));
});

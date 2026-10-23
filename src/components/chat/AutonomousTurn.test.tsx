import { cleanup, fireEvent, render, screen } from "@testing-library/react";
import { afterEach, expect, it, vi } from "vitest";
import { TurnBlock } from "./TurnBlock";
import { buildTranscript } from "@terminalx/portable/transcript";
import type { AgentEvent } from "@terminalx/portable/events";

vi.mock("@tauri-apps/api/core", () => ({ convertFileSrc: (path: string) => path, invoke: vi.fn() }));
afterEach(cleanup);

it("discloses the original initiating text without drawing a reader message (#566)", () => {
  const text = '<task-notification>\n<summary>Job finished</summary>\n<script>literal text</script>\n</task-notification>';
  const event: AgentEvent = { id: "e1", seq: 1, ts: "2026-10-24T12:00:00Z", sessionId: "synthetic", tabId: "tab", harness: "claude", payload: { type: "turn_started", startedBy: "Job finished", initiatingPrompt: text } };
  const turn = buildTranscript(JSON.parse(JSON.stringify([event])), false).turns[0];
  expect(turn.prompt).toBeUndefined();
  const { container } = render(<TurnBlock turn={turn} sessionId="synthetic" stream={[]} working={false} />);
  expect(screen.getByText("Job finished")).toBeTruthy();
  const summary = screen.getByText("Initiating prompt");
  expect(container.querySelector("details")?.open).toBe(false);
  summary.focus();
  expect(document.activeElement).toBe(summary);
  fireEvent.click(summary);
  expect(container.querySelector("details")?.open).toBe(true);
  expect(container.querySelector("details")?.textContent).toContain(text);
  expect(container.querySelector("script")).toBeNull();
  fireEvent.click(summary);
  expect(container.querySelector("details")?.open).toBe(false);
});

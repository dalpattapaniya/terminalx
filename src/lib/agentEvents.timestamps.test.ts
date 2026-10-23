import { expect, it, vi } from "vitest";
import type { AgentEvent, Payload } from "@/types/events";
vi.mock("@/lib/sessions", () => ({ getSessions: () => ({ sessions: [] }), patchTab: vi.fn() }));
vi.mock("@/lib/notify", () => ({ noteStatusChange: vi.fn() }));
import { applyEvent, dropTabLog, getTabLog, mergeTabEvents } from "./agentEvents";

it("keeps first-chunk time through streaming, authoritative commit, synchronization and reload", () => {
  const block = { messageId: "synthetic", index: 0 };
  const first = "2026-10-22T23:59:00Z";
  const event = (seq: number, ts: string, payload: Payload): AgentEvent => ({ seq, ts, id: `e${seq}`, sessionId: "timestamp-test", tabId: "tab", harness: "claude", payload });
  applyEvent(event(1, "2026-10-22T23:58:00Z", { type: "delta", delta: "block_start", block, blockType: "text" }));
  applyEvent(event(2, first, { type: "delta", delta: "text_delta", block, text: "Hello" }));
  applyEvent(event(3, "2026-10-23T00:01:00Z", { type: "delta", delta: "text_delta", block, text: " world" }));
  expect(getTabLog("timestamp-test", "tab").stream[0]).toMatchObject({ ts: first, text: "Hello world" });
  applyEvent(event(4, "2026-10-23T00:01:30Z", { type: "user_message", queued: true, text: "Synthetic queued input" }));
  expect(getTabLog("timestamp-test", "tab").stream[0].ts).toBe(first);
  const committed = event(5, first, { type: "assistant_text", block, text: "Hello world" });
  applyEvent(committed);
  expect(getTabLog("timestamp-test", "tab").stream).toEqual([]);
  mergeTabEvents("timestamp-test", "tab", [committed]);
  const persisted = JSON.parse(JSON.stringify(getTabLog("timestamp-test", "tab").events));
  dropTabLog("timestamp-test", "tab");
  mergeTabEvents("timestamp-test", "tab", persisted);
  expect(getTabLog("timestamp-test", "tab").events.find((ev) => ev.id === committed.id)?.ts).toBe(first);
  dropTabLog("timestamp-test", "tab");
});

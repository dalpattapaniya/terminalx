import { expect, it } from "vitest";
import { compareMessageActivity, conversationTime, keepMessageTime } from "./messageActivity";

it("compares instants, falls back only for absent/invalid message time, and merges monotonically", () => {
  const old = { created: "2026-10-01T00:00:00Z", pinned: false, lastMessageAt: "2026-10-04T04:00:00+04:00" };
  const equal = { ...old, lastMessageAt: "2026-10-04T00:00:00Z" };
  expect(compareMessageActivity(old, equal, "a", "b")).toBeLessThan(0);
  expect(conversationTime({ ...old, lastMessageAt: "invalid" })).toBe(old.created);
  expect(conversationTime(old)).toBe(old.lastMessageAt);
  expect(keepMessageTime({ ...old, lastMessageAt: "2026-10-02T00:00:00Z" }, old).lastMessageAt).toBe(old.lastMessageAt);
  expect(keepMessageTime({ created: old.created, pinned: false, lastMessageAt: undefined }, old).lastMessageAt).toBe(old.lastMessageAt);
});

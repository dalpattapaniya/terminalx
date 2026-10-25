import { act, cleanup, renderHook } from "@testing-library/react";
import { afterEach, expect, it } from "vitest";
import { beginComposerSend, clearComposerSendError, finishComposerSend, useComposerSend } from "./composerSends";
import { setDraft } from "./drafts";

afterEach(cleanup);

it("retains request ownership without subscribers, isolates scopes and ignores stale settlement", () => {
  const key = "store-owner";
  setDraft("store-tab", "P");
  const first = beginComposerSend(key, "store-tab")!;
  expect(beginComposerSend(key, "store-tab")).toBeNull();
  const other = beginComposerSend("other-owner", "other-tab")!;
  setDraft("store-tab", "P");
  expect(first.ownsDraft()).toBe(false);
  expect(other.ownsDraft()).toBe(true);
  const failure = new Error("disconnected");
  finishComposerSend(key, first, failure);
  const view = renderHook(() => useComposerSend(key));
  expect(view.result.current.error).toBe(failure);
  let second!: NonNullable<ReturnType<typeof beginComposerSend>>;
  act(() => { second = beginComposerSend(key, "store-tab")!; });
  expect(view.result.current.error).toBeNull();
  act(() => finishComposerSend(key, first, failure));
  expect(view.result.current.pending).toBe(second);
  act(() => finishComposerSend(key, second));
  finishComposerSend("other-owner", other);
  expect(view.result.current.pending).toBeNull();
});

it("keeps failure for remount and clears it on explicit recovery", () => {
  const request = beginComposerSend("recovery-owner", "recovery-tab")!;
  finishComposerSend("recovery-owner", request, new Error("disconnected"));
  const first = renderHook(() => useComposerSend("recovery-owner"));
  expect(first.result.current.error).toBeInstanceOf(Error);
  first.unmount();
  const second = renderHook(() => useComposerSend("recovery-owner"));
  expect(second.result.current.error).toBeInstanceOf(Error);
  act(() => clearComposerSendError("recovery-owner"));
  expect(second.result.current.error).toBeNull();
});

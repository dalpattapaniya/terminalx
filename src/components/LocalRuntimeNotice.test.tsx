import { act, cleanup, fireEvent, render, screen } from "@testing-library/react";
import { afterEach, beforeEach, expect, it, vi } from "vitest";

const native = vi.hoisted(() => ({ status: vi.fn(), reconnect: vi.fn(), listeners: new Map<string, () => void>() }));
vi.mock("@/lib/api", () => ({ localRuntime: native, errorMessage: (error: unknown) => String(error) }));
vi.mock("@tauri-apps/api/event", () => ({
  listen: vi.fn(async (event: string, callback: () => void) => {
    native.listeners.set(event, callback);
    return () => native.listeners.delete(event);
  }),
}));

import { LocalRuntimeNotice } from "./LocalRuntimeNotice";

const connected = { state: "connected", epoch: "owner", runningAgents: 1, terminals: 1 };
const unavailable = { state: "unavailable", message: "Live terminal output in window main is disconnected. Agents and terminal commands may still be running." };
beforeEach(() => {
  vi.useFakeTimers();
  native.status.mockReset().mockResolvedValue(connected);
  native.reconnect.mockReset().mockResolvedValue(connected);
  native.listeners.clear();
});
afterEach(() => { cleanup(); vi.useRealTimers(); });

async function mounted(reload = vi.fn()) {
  let view!: ReturnType<typeof render>;
  await act(async () => { view = render(<LocalRuntimeNotice reload={reload} />); });
  return { ...view, reload };
}
async function disconnect(event = "local_runtime_disconnected") {
  await act(async () => native.listeners.get(event)?.());
}
async function advance(ms: number) {
  await act(async () => vi.advanceTimersByTimeAsync(ms));
}

it("ignores another window's failure and delayed disconnects when current transport is healthy", async () => {
  const { reload } = await mounted();
  await disconnect("local_runtime_view_disconnected");
  await disconnect();
  await advance(30_000);
  expect(screen.queryByRole("status")).toBeNull();
  expect(native.reconnect).not.toHaveBeenCalled();
  expect(reload).not.toHaveBeenCalled();
});

it("automatically reattaches once and clears the notice without moving focus", async () => {
  const input = document.createElement("input");
  document.body.appendChild(input);
  input.focus();
  const { reload } = await mounted();
  native.status.mockResolvedValue(unavailable);
  await disconnect("local_runtime_view_disconnected");
  expect(screen.getByRole("status").textContent).toContain("window main");
  expect(document.activeElement).toBe(input);
  await advance(1_000);
  expect(native.reconnect).toHaveBeenCalledOnce();
  expect(reload).toHaveBeenCalledOnce();
  expect(screen.queryByRole("status")).toBeNull();
  await disconnect();
  await advance(30_000);
  expect(native.reconnect).toHaveBeenCalledOnce();
  input.remove();
});

it("bounds retries, keeps checking health, and clears after independent recovery", async () => {
  native.status.mockResolvedValue(unavailable);
  native.reconnect.mockRejectedValue("Owner unavailable");
  const { reload } = await mounted();
  await advance(60_000);
  expect(native.reconnect).toHaveBeenCalledTimes(3);
  expect(screen.getByRole("button", { name: "Reconnect" })).toBeTruthy();
  native.status.mockResolvedValue(connected);
  await advance(5_000);
  expect(screen.queryByRole("status")).toBeNull();
  expect(reload).not.toHaveBeenCalled();
  expect(native.reconnect).toHaveBeenCalledTimes(3);
});

it("allows manual recovery after exhausting automatic attempts", async () => {
  native.status.mockResolvedValue(unavailable);
  native.reconnect.mockRejectedValue("Owner unavailable");
  const { reload } = await mounted();
  await advance(60_000);
  native.reconnect.mockResolvedValue(connected);
  await act(async () => fireEvent.click(screen.getByRole("button", { name: "Reconnect" })));
  expect(native.reconnect).toHaveBeenCalledTimes(4);
  expect(reload).toHaveBeenCalledOnce();
});

it("does not let an older healthy status erase a newer failure", async () => {
  let resolve!: (state: unknown) => void;
  native.status.mockImplementationOnce(() => new Promise((done) => { resolve = done; }));
  await mounted();
  native.status.mockResolvedValue(unavailable);
  await disconnect();
  await act(async () => resolve(connected));
  expect(screen.getByRole("status").textContent).toContain("Live connection unavailable");
  expect(native.reconnect).not.toHaveBeenCalled();
});

it("serializes recovery and reloads once when the event precedes its reply", async () => {
  native.status.mockResolvedValue(unavailable);
  let resolve!: (state: unknown) => void;
  native.reconnect.mockImplementation(() => new Promise((done) => { resolve = done; }));
  const { reload } = await mounted();
  await advance(1_000);
  expect((screen.getByRole("button", { name: "Reconnecting…" }) as HTMLButtonElement).disabled).toBe(true);
  await disconnect();
  await disconnect("local_runtime_reattached");
  await act(async () => resolve(connected));
  expect(reload).toHaveBeenCalledOnce();
  expect(native.reconnect).toHaveBeenCalledOnce();
});

it("cancels timers and ignores an in-flight recovery after unmount", async () => {
  native.status.mockResolvedValue(unavailable);
  let resolve!: (state: unknown) => void;
  native.reconnect.mockImplementation(() => new Promise((done) => { resolve = done; }));
  const { unmount, reload } = await mounted();
  await advance(1_000);
  unmount();
  await act(async () => resolve(connected));
  await advance(60_000);
  expect(native.listeners.size).toBe(0);
  expect(native.reconnect).toHaveBeenCalledOnce();
  expect(reload).not.toHaveBeenCalled();
});

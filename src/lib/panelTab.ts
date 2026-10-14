import { getPrefs, setPrefs } from "@/lib/prefs";

/** The tabs of a session's right panel. Ports is a cloud session's only, for someone who may use previews. */
export type PanelTab = "changes" | "repo" | "pr" | "files" | "ports";

let pending: PanelTab | null = null;
const listeners = new Set<(tab: PanelTab) => void>();

/**
 * Opens the right panel on `tab`, from a menu elsewhere in the window. The
 * panel takes the request when it is next drawn, or at once when it is
 * already on screen.
 */
export function showPanelTab(tab: PanelTab) {
  pending = tab;
  if (!getPrefs().panelOpen) setPrefs({ panelOpen: true });
  for (const listener of listeners) listener(tab);
}

/** The tab asked for before the panel was drawn, once. */
export function takePanelTab(): PanelTab | null {
  const tab = pending;
  pending = null;
  return tab;
}

export function onPanelTab(listener: (tab: PanelTab) => void): () => void {
  listeners.add(listener);
  return () => listeners.delete(listener);
}

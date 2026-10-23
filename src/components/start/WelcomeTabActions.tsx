import { useRef } from "react";
import { useShortcut } from "@/lib/hotkeys";
import { getSessionStore } from "@/lib/sessions";
import { workspaceRow } from "@/lib/sessionTabs";
import { reopenClosedTab } from "@/lib/tabActions";

/** The session's tab handlers unmount at welcome; retain only Reopen here. */
export function WelcomeTabActions() {
  const busy = useRef(false);
  useShortcut("session.reopenTab", () => {
    const store = getSessionStore();
    const session = store.sessions.find((item) => item.id === store.closedWorkspaceSessionId);
    if (store.view !== "welcome" || store.selectedSessionId || !session || session.archived || session.worktreeRemoved) return false;
    if (busy.current) return;
    busy.current = true;
    // Restore the tab before selecting the session, so its empty-checkout shell never opens.
    void reopenClosedTab(workspaceRow(session))
      .catch((error) => console.error("reopening a tab failed", error))
      .finally(() => { busy.current = false; });
  });
  return null;
}

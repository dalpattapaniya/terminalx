import { invoke } from "@tauri-apps/api/core";

/**
 * Put text on the clipboard. The desktop app writes it through the system's
 * clipboard: WebKit refuses the webview's own write when it does not count
 * the click as the reader's gesture, which a choice in a context menu often
 * is not. Outside the app (no backend to ask), the webview's clipboard.
 */
export async function copyText(text: string): Promise<void> {
  try {
    await invoke("copy_text", { text });
  } catch {
    await navigator.clipboard.writeText(text);
  }
}

import { Code, Eye, FolderOpen, Save } from "@/icons";
import { revealItemInDir } from "@tauri-apps/plugin-opener";
import { Button } from "@/components/ui/button";
import { Segmented } from "@/components/ui/controls";
import type { ViewMode } from "@/lib/editors";

export function EditorToolbar({ path, relativePath, markdown, viewMode, onViewModeChange, cloud, readOnly, dirty, truncated, saveDisabled, saveKeys, onSave }: {
  path: string;
  relativePath: string;
  markdown: boolean;
  viewMode: ViewMode;
  onViewModeChange: (mode: ViewMode) => void;
  cloud?: boolean;
  readOnly: boolean;
  dirty: boolean;
  truncated: boolean;
  saveDisabled: boolean;
  saveKeys: string[];
  onSave: () => void;
}) {
  const slash = relativePath.lastIndexOf("/") + 1;
  const notes = [truncated && "first 4 MB shown", cloud && "cloud workspace", readOnly && "read-only", dirty && "unsaved"].filter((note): note is string => typeof note === "string");
  return (
    <div className="@container/editor-toolbar h-8 shrink-0 border-b border-hairline" data-testid="editor-toolbar">
      {/* Below the minimum icon width, scroll instead of crushing or overlapping controls. */}
      <div className="flex h-full items-center gap-2 overflow-x-auto px-3 text-xs">
        <span className="flex min-w-0 flex-1 text-muted-foreground" title={path}>
          <span className="truncate">{relativePath.slice(0, slash)}</span>
          <span className="min-w-0 shrink-0 truncate max-w-full">{relativePath.slice(slash)}</span>
        </span>
        <span className="ml-auto flex shrink-0 items-center gap-1">
          {notes.length > 0 && (
            <span title={notes.join(" · ")} aria-label={notes.join(" · ")} className="text-faint" data-testid="editor-notes">
              <span className="hidden max-w-20 truncate @min-[260px]/editor-toolbar:block">{notes.map((note, index) => <span key={note}>{index > 0 && " · "}<span className={note === "first 4 MB shown" ? "text-warning" : undefined}>{note}</span></span>)}</span>
              <span aria-hidden className="block size-1.5 rounded-full bg-current @min-[260px]/editor-toolbar:hidden" />
              {cloud && <span className="sr-only" data-testid="editor-cloud-file">cloud workspace</span>}
            </span>
          )}
          {markdown && (
            <Segmented<ViewMode>
              aria-label="View"
              value={viewMode}
              onChange={onViewModeChange}
              options={[
                { value: "preview", "aria-label": "Preview", title: "Preview", label: <><Eye aria-hidden className="size-3.5 @min-[360px]/editor-toolbar:hidden" /><span className="hidden @min-[360px]/editor-toolbar:inline">Preview</span></> },
                { value: "source", "aria-label": "Source", title: "Source", label: <><Code aria-hidden className="size-3.5 @min-[360px]/editor-toolbar:hidden" /><span className="hidden @min-[360px]/editor-toolbar:inline">Source</span></> },
              ]}
            />
          )}
          {!cloud && (
            <Button variant="ghost" size="icon-xs" aria-label="Reveal in Finder" title="Reveal in Finder" onClick={() => void revealItemInDir(path).catch(() => {})}>
              <FolderOpen />
            </Button>
          )}
          <Button variant="ghost" size="xs" onClick={onSave} disabled={saveDisabled} aria-label="Save" title={`Save ${saveKeys.join("")}`}>
            <Save className="size-3.5" />
            <span className="hidden @min-[440px]/editor-toolbar:inline">Save</span>
            <kbd className="ml-1 hidden text-[10px] text-faint @min-[520px]/editor-toolbar:inline">{saveKeys.join("")}</kbd>
          </Button>
        </span>
      </div>
    </div>
  );
}

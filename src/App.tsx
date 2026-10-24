import { ActivityPanel } from "./components/ActivityPanel";
import { ChatPanel } from "./components/ChatPanel";
import { CommandPalette } from "./components/CommandPalette";
import { ExportModal } from "./components/ExportModal";
import { ModelDownloads } from "./components/ModelDownloads";
import { SessionsDrawer } from "./components/SessionsDrawer";
import { SourceCompareTable } from "./components/SourceCompareTable";
import { SourcesPanel } from "./components/SourcesPanel";
import { StatusBar } from "./components/StatusBar";
import { TopBar } from "./components/TopBar";
import { useEventBridge } from "./lib/useEventBridge";
import { useSessionStore } from "./stores/sessionStore";

export default function App() {
  useEventBridge();
  const exportModalOpen = useSessionStore((state) => state.exportModalOpen);
  const exportEntryId = useSessionStore((state) => state.exportEntryId);
  const closeExport = useSessionStore((state) => state.closeExport);

  return (
    <div className="flex h-screen flex-col overflow-hidden bg-[#050a13] text-slate-100 selection:bg-sky-400/25">
      <TopBar />

      <div className="flex min-h-0 flex-1">
        <SourcesPanel />
        <ChatPanel />
        <ActivityPanel />
      </div>

      <StatusBar />
      <ModelDownloads />
      <SessionsDrawer />
      <CommandPalette />
      <SourceCompareTable />
      <ExportModal isOpen={exportModalOpen} onClose={closeExport} targetEntryId={exportEntryId} />
    </div>
  );
}

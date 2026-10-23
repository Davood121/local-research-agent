import { ActivityPanel } from "./components/ActivityPanel";
import { ChatPanel } from "./components/ChatPanel";
import { ModelDownloads } from "./components/ModelDownloads";
import { SourcesPanel } from "./components/SourcesPanel";
import { StatusBar } from "./components/StatusBar";
import { TopBar } from "./components/TopBar";
import { useEventBridge } from "./lib/useEventBridge";

export default function App() {
  useEventBridge();

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
    </div>
  );
}

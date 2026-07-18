import { useState } from "react";
import { AppShell } from "./app/AppShell";
import type { AppModule } from "./app/AppShell";
import { CleanupModule } from "./features/cleanup/CleanupModule";
import { PreviewModule } from "./features/preview/PreviewModule";
import { WatermarkModule } from "./features/watermark/WatermarkModule";
import type {
  WatermarkTransferDraft,
  WatermarkTransferIntent,
} from "./features/watermark/types";

function App() {
  const [activeModule, setActiveModule] = useState<AppModule>("preview");
  const [watermarkTransfer, setWatermarkTransfer] = useState<WatermarkTransferDraft | null>(null);

  function sendToWatermark(draft: WatermarkTransferIntent) {
    setWatermarkTransfer({ ...draft, transferId: crypto.randomUUID() });
    setActiveModule("watermark");
  }

  return (
    <AppShell activeModule={activeModule} onModuleChange={setActiveModule}>
      <div className="module-panel" hidden={activeModule !== "preview"}>
        <PreviewModule active={activeModule === "preview"} onSendToWatermark={sendToWatermark} />
      </div>
      <div className="module-panel" hidden={activeModule !== "cleanup"}>
        <CleanupModule active={activeModule === "cleanup"} />
      </div>
      <div className="module-panel" hidden={activeModule !== "watermark"}>
        <WatermarkModule
          active={activeModule === "watermark"}
          transfer={watermarkTransfer}
        />
      </div>
    </AppShell>
  );
}

export default App;

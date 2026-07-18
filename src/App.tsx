import { isTauri } from "@tauri-apps/api/core";
import { useEffect, useRef, useState } from "react";
import { AppShell } from "./app/AppShell";
import type { AppModule } from "./app/AppShell";
import { CleanupModule } from "./features/cleanup/CleanupModule";
import { PreviewModule } from "./features/preview/PreviewModule";
import { WatermarkModule } from "./features/watermark/WatermarkModule";
import { WatermarkLeaveDialog } from "./features/watermark/WatermarkLeaveDialog";
import type { WatermarkUnsavedWork } from "./features/watermark/WatermarkLeaveDialog";
import type {
  WatermarkTransferDraft,
  WatermarkTransferIntent,
} from "./features/watermark/types";

function App() {
  const [activeModule, setActiveModule] = useState<AppModule>("preview");
  const [watermarkTransfer, setWatermarkTransfer] = useState<WatermarkTransferDraft | null>(null);
  const [watermarkUnsaved, setWatermarkUnsaved] = useState<WatermarkUnsavedWork>({
    dirtyTemplate: false,
    unexportedChanges: false,
  });
  const [pendingDestination, setPendingDestination] = useState<AppModule | "close" | null>(null);
  const [watermarkDiscardToken, setWatermarkDiscardToken] = useState(0);
  const closeBypass = useRef(false);
  const hasUnsavedWatermark = watermarkUnsaved.dirtyTemplate || watermarkUnsaved.unexportedChanges;

  useEffect(() => {
    if (!isTauri()) return;
    let disposed = false;
    let unlisten: (() => void) | undefined;
    void import("@tauri-apps/api/window")
      .then(async ({ getCurrentWindow }) => {
        const stopListening = await getCurrentWindow().onCloseRequested((event) => {
          if (closeBypass.current) {
            closeBypass.current = false;
            return;
          }
          if (!hasUnsavedWatermark) return;
          event.preventDefault();
          setPendingDestination("close");
        });
        if (disposed) stopListening();
        else unlisten = stopListening;
      })
      .catch(() => undefined);
    return () => {
      disposed = true;
      unlisten?.();
    };
  }, [hasUnsavedWatermark]);

  function requestModuleChange(module: AppModule) {
    if (module === activeModule) return;
    if (activeModule === "watermark" && hasUnsavedWatermark) {
      setPendingDestination(module);
      return;
    }
    setActiveModule(module);
  }

  function confirmLeaveWatermark() {
    const destination = pendingDestination;
    if (!destination) return;
    setPendingDestination(null);
    setWatermarkDiscardToken((current) => current + 1);
    setWatermarkUnsaved({ dirtyTemplate: false, unexportedChanges: false });
    if (destination === "close") {
      closeBypass.current = true;
      void import("@tauri-apps/api/window")
        .then(({ getCurrentWindow }) => getCurrentWindow().close())
        .catch(() => { closeBypass.current = false; });
      return;
    }
    setActiveModule(destination);
  }

  function sendToWatermark(draft: WatermarkTransferIntent) {
    setWatermarkTransfer({ ...draft, transferId: crypto.randomUUID() });
    setActiveModule("watermark");
  }

  return (
    <AppShell activeModule={activeModule} onModuleChange={requestModuleChange}>
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
          discardToken={watermarkDiscardToken}
          onUnsavedWorkChange={setWatermarkUnsaved}
        />
      </div>
      <WatermarkLeaveDialog
        open={pendingDestination !== null}
        reason={pendingDestination === "close" ? "close" : "navigate"}
        unsaved={watermarkUnsaved}
        onCancel={() => setPendingDestination(null)}
        onConfirm={confirmLeaveWatermark}
      />
    </AppShell>
  );
}

export default App;

import { invoke, isTauri } from "@tauri-apps/api/core";
import { open } from "@tauri-apps/plugin-dialog";
import { FolderOpen, Stamp } from "lucide-react";
import { useEffect, useRef, useState } from "react";
import { errorMessage } from "../../utils";
import { WatermarkSourcePanel } from "./WatermarkSourcePanel";
import type {
  WatermarkSourceInput,
  WatermarkSourceOrigin,
  WatermarkSourceSnapshot,
  WatermarkTransferDraft,
} from "./types";
import "./watermark.css";

interface WatermarkModuleProps {
  active: boolean;
  transfer: WatermarkTransferDraft | null;
}

export function WatermarkModule({ active, transfer }: WatermarkModuleProps) {
  const [snapshot, setSnapshot] = useState<WatermarkSourceSnapshot | null>(null);
  const [busy, setBusy] = useState(false);
  const [dropActive, setDropActive] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const busyRef = useRef(false);
  const processedTransferId = useRef<string | null>(null);

  async function prepare(origin: WatermarkSourceOrigin, inputs: WatermarkSourceInput[]) {
    if (busyRef.current || inputs.length === 0) return;
    if (!isTauri()) {
      setError("请在 FramePair 桌面应用中载入本地照片");
      return;
    }
    busyRef.current = true;
    setBusy(true);
    setError(null);
    try {
      const result = await invoke<WatermarkSourceSnapshot>("prepare_watermark_source", {
        request: { origin, inputs },
      });
      setSnapshot(result);
    } catch (prepareError) {
      setError(errorMessage(prepareError));
    } finally {
      busyRef.current = false;
      setBusy(false);
    }
  }

  useEffect(() => {
    if (!active || !transfer || processedTransferId.current === transfer.transferId) return;
    if (busyRef.current) return;
    processedTransferId.current = transfer.transferId;
    void prepare(transfer.origin, transfer.inputs);
  }, [active, busy, transfer]);

  useEffect(() => {
    if (!active || !isTauri()) return;
    let disposed = false;
    let unlisten: (() => void) | undefined;

    async function listenForDrops() {
      const { getCurrentWebview } = await import("@tauri-apps/api/webview");
      const stopListening = await getCurrentWebview().onDragDropEvent((event) => {
        if (disposed) return;
        if (event.payload.type === "leave") {
          setDropActive(false);
          return;
        }
        if (event.payload.type === "over") {
          if (!busyRef.current) setDropActive(true);
          return;
        }
        setDropActive(false);
        if (busyRef.current || event.payload.paths.length === 0) return;
        const inputs: WatermarkSourceInput[] = event.payload.paths.map((path) => (
          /\.jpe?g$/i.test(path)
            ? { kind: "file", path }
            : { kind: "directory", path }
        ));
        void prepare("drop", inputs);
      });
      if (disposed) stopListening();
      else unlisten = stopListening;
    }

    void listenForDrops().catch(() => setError("无法启用拖放，请使用目录选择器"));
    return () => {
      disposed = true;
      setDropActive(false);
      unlisten?.();
    };
  }, [active]);

  async function chooseDirectory() {
    try {
      const selected = await open({
        directory: true,
        multiple: false,
        title: "选择需要加水印的 JPG 照片目录",
      });
      if (typeof selected === "string") {
        await prepare("directory", [{ kind: "directory", path: selected }]);
      }
    } catch (chooseError) {
      setError(errorMessage(chooseError));
    }
  }

  return (
    <section className={dropActive ? "watermark-module is-drop-target" : "watermark-module"} aria-label="水印导出">
      <header className="watermark-header">
        <div className="module-heading">
          <Stamp aria-hidden="true" size={20} />
          <div><strong>水印导出</strong><span>边框、署名与发布副本</span></div>
        </div>
        <div className="watermark-header-state">
          {snapshot ? <span>{snapshot.photos.length} 张 JPG/JPEG</span> : <span>尚未添加照片</span>}
          <button className="secondary-command" type="button" onClick={() => void chooseDirectory()} disabled={busy}>
            <FolderOpen aria-hidden="true" size={17} />选择目录
          </button>
        </div>
      </header>
      {busy ? <div className="activity-line" aria-hidden="true"><span /></div> : null}
      <WatermarkSourcePanel
        snapshot={snapshot}
        busy={busy}
        error={error}
        onChooseDirectory={() => void chooseDirectory()}
        onDismissError={() => setError(null)}
      />
      {dropActive ? (
        <div className="watermark-drop-overlay"><FolderOpen aria-hidden="true" size={30} /><strong>松开以添加 JPG 或照片目录</strong></div>
      ) : null}
    </section>
  );
}

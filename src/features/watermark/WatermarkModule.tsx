import { invoke, isTauri } from "@tauri-apps/api/core";
import { open } from "@tauri-apps/plugin-dialog";
import { FolderOpen, Stamp } from "lucide-react";
import { useEffect, useMemo, useRef, useState } from "react";
import { errorMessage } from "../../utils";
import {
  loadPhotoPreviewUrl,
  preloadPreviewRequests,
  type PreviewRequest,
} from "../preview/previewCache";
import { WatermarkCanvas } from "./WatermarkCanvas";
import { WatermarkSourcePanel } from "./WatermarkSourcePanel";
import type {
  WatermarkRenderRequest,
  WatermarkSourcePhoto,
  WatermarkSourceInput,
  WatermarkSourceOrigin,
  WatermarkSourceSnapshot,
  WatermarkTransferDraft,
} from "./types";
import { createDefaultWatermarkTemplate } from "./watermarkUtils";
import {
  loadWatermarkPreview,
  watermarkPreviewRequestKey,
  WatermarkPreviewCache,
  type WatermarkPreviewDescriptor,
  type WatermarkPreviewResult,
} from "./watermarkPreviewCache";
import "./watermark.css";

const WATERMARK_PREVIEW_EDGE = 1400;
const SOURCE_THUMBNAIL_EDGE = 220;
const SOURCE_PRELOAD_CONCURRENCY = 4;

interface WatermarkModuleProps {
  active: boolean;
  transfer: WatermarkTransferDraft | null;
}

export function WatermarkModule({ active, transfer }: WatermarkModuleProps) {
  const [snapshot, setSnapshot] = useState<WatermarkSourceSnapshot | null>(null);
  const [busy, setBusy] = useState(false);
  const [dropActive, setDropActive] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [selectedPhotoId, setSelectedPhotoId] = useState<string | null>(null);
  const [preview, setPreview] = useState<WatermarkPreviewResult | null>(null);
  const [previewBusy, setPreviewBusy] = useState(false);
  const [previewError, setPreviewError] = useState<string | null>(null);
  const busyRef = useRef(false);
  const processedTransferId = useRef<string | null>(null);
  const previousPreviewPhotoId = useRef<string | null>(null);
  const previewCacheRef = useRef<WatermarkPreviewCache | null>(null);
  if (!previewCacheRef.current) {
    previewCacheRef.current = new WatermarkPreviewCache((url) => URL.revokeObjectURL(url));
  }
  const template = useMemo(
    () => createDefaultWatermarkTemplate("framepair-clean", "简洁白边"),
    [],
  );

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
      setSelectedPhotoId(result.photos[0]?.id ?? null);
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

  useEffect(() => () => previewCacheRef.current?.clear(), []);

  useEffect(() => {
    previewCacheRef.current?.clear();
    setPreview(null);
    setPreviewError(null);
    previousPreviewPhotoId.current = null;
  }, [snapshot?.id]);

  useEffect(() => {
    if (!snapshot || snapshot.photos.length === 0) return;
    const controller = new AbortController();
    const requests: PreviewRequest[] = snapshot.photos.map((photo) => ({
      root: photo.root,
      relativePath: photo.relativePath,
      maxEdge: SOURCE_THUMBNAIL_EDGE,
      version: `${snapshot.id}:${photo.sizeBytes}:${photo.modifiedMs}`,
    }));
    void preloadPreviewRequests(requests, loadPhotoPreviewUrl, {
      concurrency: SOURCE_PRELOAD_CONCURRENCY,
      signal: controller.signal,
    });
    return () => controller.abort();
  }, [snapshot]);

  const selectedIndex = useMemo(
    () => snapshot?.photos.findIndex((photo) => photo.id === selectedPhotoId) ?? -1,
    [selectedPhotoId, snapshot],
  );
  const selectedPhoto = selectedIndex >= 0 ? snapshot?.photos[selectedIndex] ?? null : null;

  function requestFor(photo: WatermarkSourcePhoto): WatermarkRenderRequest {
    return {
      schemaVersion: 1,
      source: photo,
      template,
      photoOverride: null,
      colorSpace: "srgb",
      transparentBackground: false,
      jpegFlattenColor: "#ffffff",
    };
  }

  useEffect(() => {
    if (!active || !snapshot || !selectedPhoto || selectedIndex < 0) return;
    const cache = previewCacheRef.current;
    if (!cache) return;
    const keepPhotos = snapshot.photos.slice(
      Math.max(0, selectedIndex - 2),
      Math.min(snapshot.photos.length, selectedIndex + 3),
    );
    cache.retainPhotos(new Set(keepPhotos.map((photo) => photo.id)));

    const request = requestFor(selectedPhoto);
    const key = watermarkPreviewRequestKey(request, WATERMARK_PREVIEW_EDGE);
    const descriptor: WatermarkPreviewDescriptor = {
      key,
      photoId: selectedPhoto.id,
      root: selectedPhoto.root,
      templateId: template.id,
    };
    const token = cache.begin(selectedPhoto.id, key);
    const switchedPhoto = previousPreviewPhotoId.current !== selectedPhoto.id;
    previousPreviewPhotoId.current = selectedPhoto.id;
    let disposed = false;
    setPreviewBusy(true);
    setPreviewError(null);
    const timeout = window.setTimeout(() => {
      void cache.getOrLoad(
        descriptor,
        () => loadWatermarkPreview(request, WATERMARK_PREVIEW_EDGE),
      ).then((result) => {
        if (disposed || !cache.isCurrent(token)) return;
        setPreview(result);
        for (const neighbor of keepPhotos) {
          if (neighbor.id === selectedPhoto.id) continue;
          const neighborRequest = requestFor(neighbor);
          const neighborKey = watermarkPreviewRequestKey(neighborRequest, WATERMARK_PREVIEW_EDGE);
          void cache.getOrLoad({
            key: neighborKey,
            photoId: neighbor.id,
            root: neighbor.root,
            templateId: template.id,
          }, () => loadWatermarkPreview(neighborRequest, WATERMARK_PREVIEW_EDGE)).catch(() => undefined);
        }
      }).catch((renderError) => {
        if (!disposed && cache.isCurrent(token)) setPreviewError(errorMessage(renderError));
      }).finally(() => {
        if (!disposed && cache.isCurrent(token)) setPreviewBusy(false);
      });
    }, switchedPhoto ? 0 : 80);
    return () => {
      disposed = true;
      window.clearTimeout(timeout);
    };
  }, [active, selectedPhoto, selectedIndex, snapshot, template]);

  function selectAt(index: number) {
    const photo = snapshot?.photos[index];
    if (photo) setSelectedPhotoId(photo.id);
  }

  useEffect(() => {
    if (!active || !snapshot) return;
    const handleKeyDown = (event: KeyboardEvent) => {
      const target = event.target as HTMLElement | null;
      if (target?.closest("input, textarea, select, [contenteditable='true']")) return;
      if (event.key === "ArrowLeft" && selectedIndex > 0) {
        event.preventDefault();
        selectAt(selectedIndex - 1);
      } else if (event.key === "ArrowRight" && selectedIndex < snapshot.photos.length - 1) {
        event.preventDefault();
        selectAt(selectedIndex + 1);
      }
    };
    window.addEventListener("keydown", handleKeyDown);
    return () => window.removeEventListener("keydown", handleKeyDown);
  }, [active, selectedIndex, snapshot]);

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
      {snapshot && snapshot.photos.length > 0 ? (
        <div className="watermark-live-workspace">
          <WatermarkSourcePanel
            snapshot={snapshot}
            busy={busy}
            error={error}
            selectedPhotoId={selectedPhotoId}
            onChooseDirectory={() => void chooseDirectory()}
            onDismissError={() => setError(null)}
            onSelectPhoto={setSelectedPhotoId}
          />
          <WatermarkCanvas
            photo={selectedPhoto}
            preview={preview}
            loading={previewBusy}
            error={previewError}
            position={selectedIndex + 1}
            total={snapshot.photos.length}
            onPrevious={() => selectAt(selectedIndex - 1)}
            onNext={() => selectAt(selectedIndex + 1)}
          />
        </div>
      ) : (
        <WatermarkSourcePanel
          snapshot={snapshot}
          busy={busy}
          error={error}
          selectedPhotoId={selectedPhotoId}
          onChooseDirectory={() => void chooseDirectory()}
          onDismissError={() => setError(null)}
          onSelectPhoto={setSelectedPhotoId}
        />
      )}
      {dropActive ? (
        <div className="watermark-drop-overlay"><FolderOpen aria-hidden="true" size={30} /><strong>松开以添加 JPG 或照片目录</strong></div>
      ) : null}
    </section>
  );
}

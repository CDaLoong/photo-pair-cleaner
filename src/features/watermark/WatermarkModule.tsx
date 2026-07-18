import { invoke, isTauri } from "@tauri-apps/api/core";
import { open } from "@tauri-apps/plugin-dialog";
import { FolderOpen, Images, LayoutTemplate } from "lucide-react";
import { useEffect, useMemo, useReducer, useRef, useState } from "react";
import { errorMessage, storedBooleanPreference } from "../../utils";
import {
  loadPhotoPreviewUrl,
  peekPhotoPreviewUrl,
  preloadPreviewRequests,
  type PreloadProgress,
  type PreviewRequest,
} from "../preview/previewCache";
import { WatermarkCanvas } from "./WatermarkCanvas";
import { WatermarkFilmstrip } from "./WatermarkFilmstrip";
import { WatermarkHeader } from "./WatermarkHeader";
import { WatermarkInspector } from "./WatermarkInspector";
import {
  createWatermarkEditorState,
  watermarkEditorReducer,
} from "./watermarkEditorState";
import type { WatermarkUnsavedWork } from "./WatermarkLeaveDialog";
import { WatermarkSourcePanel } from "./WatermarkSourcePanel";
import { WatermarkTemplatePanel } from "./WatermarkTemplatePanel";
import type {
  WatermarkRenderRequest,
  EmbeddedTemplateResource,
  WatermarkFontSummary,
  WatermarkSourcePhoto,
  WatermarkSourceInput,
  WatermarkSourceOrigin,
  WatermarkSourceSnapshot,
  WatermarkTransferDraft,
} from "./types";
import {
  createDefaultWatermarkTemplate,
  createWatermarkExifLayer,
  createWatermarkImageLayer,
  createWatermarkTextLayer,
  defaultWatermarkLayerLayouts,
} from "./watermarkUtils";
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
const LEFT_PANEL_STORAGE_KEY = "framepair.watermark.left-panel-collapsed.v1";
const RIGHT_PANEL_STORAGE_KEY = "framepair.watermark.right-panel-collapsed.v1";
const EMPTY_PRELOAD_PROGRESS: PreloadProgress = { total: 0, completed: 0, failed: 0 };

function storedPanelPreference(key: string): boolean {
  try {
    return storedBooleanPreference(localStorage.getItem(key));
  } catch {
    return false;
  }
}

function persistPanelPreference(key: string, value: boolean): void {
  try {
    localStorage.setItem(key, String(value));
  } catch {
    // The current layout remains usable if preferences cannot be stored.
  }
}

interface WatermarkModuleProps {
  active: boolean;
  transfer: WatermarkTransferDraft | null;
  discardToken: number;
  onUnsavedWorkChange: (work: WatermarkUnsavedWork) => void;
  immersive: boolean;
  onImmersiveChange: (immersive: boolean) => void;
}

export function WatermarkModule({
  active,
  transfer,
  discardToken,
  onUnsavedWorkChange,
  immersive,
  onImmersiveChange,
}: WatermarkModuleProps) {
  const initialTemplate = useMemo(
    () => createDefaultWatermarkTemplate("framepair-clean", "简洁白边"),
    [],
  );
  const [editor, dispatchEditor] = useReducer(
    watermarkEditorReducer,
    initialTemplate,
    createWatermarkEditorState,
  );
  const [snapshot, setSnapshot] = useState<WatermarkSourceSnapshot | null>(null);
  const [busy, setBusy] = useState(false);
  const [dropActive, setDropActive] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [selectedPhotoId, setSelectedPhotoId] = useState<string | null>(null);
  const [preview, setPreview] = useState<WatermarkPreviewResult | null>(null);
  const [previewBusy, setPreviewBusy] = useState(false);
  const [previewError, setPreviewError] = useState<string | null>(null);
  const [originalUrl, setOriginalUrl] = useState<string | null>(null);
  const [compareOriginal, setCompareOriginal] = useState(false);
  const [preloadProgress, setPreloadProgress] = useState<PreloadProgress>(EMPTY_PRELOAD_PROGRESS);
  const [leftTab, setLeftTab] = useState<"photos" | "templates">("photos");
  const [leftCollapsed, setLeftCollapsed] = useState(() => storedPanelPreference(LEFT_PANEL_STORAGE_KEY));
  const [rightCollapsed, setRightCollapsed] = useState(() => storedPanelPreference(RIGHT_PANEL_STORAGE_KEY));
  const [fonts, setFonts] = useState<WatermarkFontSummary[]>([]);
  const busyRef = useRef(false);
  const processedTransferId = useRef<string | null>(null);
  const previousPreviewPhotoId = useRef<string | null>(null);
  const handledDiscardToken = useRef(discardToken);
  const immersiveRestoreRef = useRef({ left: leftCollapsed, right: rightCollapsed });
  const wasImmersiveRef = useRef(immersive);
  const previewCacheRef = useRef<WatermarkPreviewCache | null>(null);
  if (!previewCacheRef.current) {
    previewCacheRef.current = new WatermarkPreviewCache((url) => URL.revokeObjectURL(url));
  }
  const template = editor.present.template;

  useEffect(() => {
    if (wasImmersiveRef.current && !immersive) {
      setLeftCollapsed(immersiveRestoreRef.current.left);
      setRightCollapsed(immersiveRestoreRef.current.right);
    }
    wasImmersiveRef.current = immersive;
  }, [immersive]);

  useEffect(() => {
    const leftQuery = window.matchMedia("(max-width: 999px)");
    const rightQuery = window.matchMedia("(max-width: 880px)");
    const syncResponsivePanels = () => {
      setLeftCollapsed(leftQuery.matches ? true : storedPanelPreference(LEFT_PANEL_STORAGE_KEY));
      setRightCollapsed(rightQuery.matches ? true : storedPanelPreference(RIGHT_PANEL_STORAGE_KEY));
    };
    syncResponsivePanels();
    leftQuery.addEventListener("change", syncResponsivePanels);
    rightQuery.addEventListener("change", syncResponsivePanels);
    return () => {
      leftQuery.removeEventListener("change", syncResponsivePanels);
      rightQuery.removeEventListener("change", syncResponsivePanels);
    };
  }, []);

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
      if (result.photos.length > 0) dispatchEditor({ type: "markSourceChanged" });
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
    if (!active || !isTauri() || fonts.length > 0) return;
    let disposed = false;
    void invoke<WatermarkFontSummary[]>("list_watermark_fonts")
      .then((items) => { if (!disposed) setFonts(items); })
      .catch(() => undefined);
    return () => { disposed = true; };
  }, [active, fonts.length]);

  useEffect(() => {
    onUnsavedWorkChange({
      dirtyTemplate: editor.present.dirtyTemplate,
      unexportedChanges: editor.present.unexportedChanges,
    });
  }, [editor.present.dirtyTemplate, editor.present.unexportedChanges, onUnsavedWorkChange]);

  useEffect(() => {
    if (handledDiscardToken.current === discardToken) return;
    handledDiscardToken.current = discardToken;
    previewCacheRef.current?.clear();
    previousPreviewPhotoId.current = null;
    setSnapshot(null);
    setSelectedPhotoId(null);
    setPreview(null);
    setPreviewError(null);
    setOriginalUrl(null);
    setCompareOriginal(false);
    setPreloadProgress(EMPTY_PRELOAD_PROGRESS);
    setError(null);
    onImmersiveChange(false);
    dispatchEditor({ type: "resetEditor", template: initialTemplate });
  }, [discardToken, initialTemplate, onImmersiveChange]);

  useEffect(() => {
    previewCacheRef.current?.clear();
    setPreview(null);
    setPreviewError(null);
    previousPreviewPhotoId.current = null;
  }, [snapshot?.id]);

  useEffect(() => {
    if (!snapshot || snapshot.photos.length === 0) {
      setPreloadProgress(EMPTY_PRELOAD_PROGRESS);
      return;
    }
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
      onProgress: setPreloadProgress,
    });
    return () => controller.abort();
  }, [snapshot]);

  const selectedIndex = useMemo(
    () => snapshot?.photos.findIndex((photo) => photo.id === selectedPhotoId) ?? -1,
    [selectedPhotoId, snapshot],
  );
  const selectedPhoto = selectedIndex >= 0 ? snapshot?.photos[selectedIndex] ?? null : null;

  useEffect(() => {
    if (!selectedPhoto) return;
    dispatchEditor({ type: "setActiveOrientation", orientation: selectedPhoto.orientation });
  }, [selectedPhoto]);

  useEffect(() => {
    if (!selectedPhoto || !snapshot) {
      setOriginalUrl(null);
      return;
    }
    const request: PreviewRequest = {
      root: selectedPhoto.root,
      relativePath: selectedPhoto.relativePath,
      maxEdge: WATERMARK_PREVIEW_EDGE,
      version: `${snapshot.id}:${selectedPhoto.sizeBytes}:${selectedPhoto.modifiedMs}`,
    };
    let disposed = false;
    setOriginalUrl(peekPhotoPreviewUrl(request));
    void loadPhotoPreviewUrl(request)
      .then((url) => { if (!disposed) setOriginalUrl(url); })
      .catch(() => { if (!disposed) setOriginalUrl(null); });
    return () => { disposed = true; };
  }, [selectedPhoto, snapshot]);

  function requestFor(photo: WatermarkSourcePhoto): WatermarkRenderRequest {
    return {
      schemaVersion: 1,
      source: photo,
      template,
      photoOverride: editor.present.photoOverrides[photo.id] ?? null,
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
  }, [active, editor.present.photoOverrides, selectedPhoto, selectedIndex, snapshot, template]);

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

  function toggleStoredPanel(side: "left" | "right") {
    if (side === "left") {
      setLeftCollapsed((current) => {
        const next = !current;
        persistPanelPreference(LEFT_PANEL_STORAGE_KEY, next);
        return next;
      });
    } else {
      setRightCollapsed((current) => {
        const next = !current;
        persistPanelPreference(RIGHT_PANEL_STORAGE_KEY, next);
        return next;
      });
    }
  }

  function toggleImmersive() {
    if (!immersive) {
      immersiveRestoreRef.current = { left: leftCollapsed, right: rightCollapsed };
      setLeftCollapsed(true);
      setRightCollapsed(true);
      onImmersiveChange(true);
      return;
    }
    onImmersiveChange(false);
  }

  function addTextLayer() {
    const layer = createWatermarkTextLayer(crypto.randomUUID());
    dispatchEditor({ type: "addLayer", layer, layouts: defaultWatermarkLayerLayouts("text") });
  }

  function addExifLayer() {
    const layer = createWatermarkExifLayer(crypto.randomUUID());
    dispatchEditor({ type: "addLayer", layer, layouts: defaultWatermarkLayerLayouts("exifText") });
  }

  async function addImageLayer() {
    try {
      const path = await open({
        multiple: false,
        directory: false,
        title: "选择 PNG 或 JPEG 图片水印",
        filters: [{ name: "图片", extensions: ["png", "jpg", "jpeg"] }],
      });
      if (typeof path !== "string") return;
      const resource = await invoke<EmbeddedTemplateResource>("import_watermark_resource", { path });
      dispatchEditor({ type: "addResource", resource });
      const layer = createWatermarkImageLayer(crypto.randomUUID(), resource.id);
      dispatchEditor({ type: "addLayer", layer, layouts: defaultWatermarkLayerLayouts("image") });
    } catch (resourceError) {
      setPreviewError(errorMessage(resourceError));
    }
  }

  function changeOrientation(orientation: WatermarkSourcePhoto["orientation"]) {
    const target = snapshot?.photos.find((photo) => photo.orientation === orientation);
    if (target) setSelectedPhotoId(target.id);
  }

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

  const previewWarnings = useMemo(() => new Set(
    selectedPhotoId && (previewError || preview?.warnings.length) ? [selectedPhotoId] : [],
  ), [preview?.warnings.length, previewError, selectedPhotoId]);
  const workspaceClass = [
    "watermark-studio",
    leftCollapsed ? "is-left-collapsed" : "",
    rightCollapsed ? "is-right-collapsed" : "",
    immersive ? "is-immersive" : "",
  ].filter(Boolean).join(" ");

  return (
    <section className={dropActive ? "watermark-module is-drop-target" : "watermark-module"} aria-label="水印导出">
      <WatermarkHeader
        photoCount={snapshot?.photos.length ?? 0}
        templateName={template.name}
        orientation={editor.activeOrientation}
        busy={busy}
        canUndo={editor.past.length > 0}
        canRedo={editor.future.length > 0}
        compareOriginal={compareOriginal}
        compareAvailable={Boolean(originalUrl)}
        leftCollapsed={leftCollapsed}
        rightCollapsed={rightCollapsed}
        immersive={immersive}
        workspaceReady={Boolean(snapshot?.photos.length)}
        onChooseDirectory={() => void chooseDirectory()}
        onUndo={() => dispatchEditor({ type: "undo" })}
        onRedo={() => dispatchEditor({ type: "redo" })}
        onCompare={() => setCompareOriginal((current) => !current)}
        onToggleLeft={() => toggleStoredPanel("left")}
        onToggleRight={() => toggleStoredPanel("right")}
        onToggleImmersive={toggleImmersive}
      />
      {busy ? <div className="activity-line" aria-hidden="true"><span /></div> : null}
      {snapshot && snapshot.photos.length > 0 ? (
        <section className={workspaceClass}>
          <div className="watermark-workspace">
            <aside className="watermark-left-panel" data-watermark-tour="sources-templates" aria-label="照片与模板">
              <div className="watermark-left-tabs" role="tablist" aria-label="照片和模板">
                <button type="button" role="tab" aria-selected={leftTab === "photos"} onClick={() => setLeftTab("photos")}><Images aria-hidden="true" size={15} />照片</button>
                <button type="button" role="tab" aria-selected={leftTab === "templates"} onClick={() => setLeftTab("templates")}><LayoutTemplate aria-hidden="true" size={15} />模板</button>
              </div>
              {leftTab === "photos" ? (
                <WatermarkSourcePanel
                  snapshot={snapshot}
                  busy={busy}
                  error={error}
                  selectedPhotoId={selectedPhotoId}
                  onChooseDirectory={() => void chooseDirectory()}
                  onDismissError={() => setError(null)}
                  onSelectPhoto={setSelectedPhotoId}
                />
              ) : <WatermarkTemplatePanel template={template} orientation={editor.activeOrientation} />}
            </aside>
            <main className="watermark-stage" data-watermark-tour="canvas">
              <WatermarkCanvas
                photo={selectedPhoto}
                preview={preview}
                template={template}
                orientation={selectedPhoto?.orientation ?? editor.activeOrientation}
                activeLayerId={editor.activeLayerId}
                loading={previewBusy}
                error={previewError}
                originalUrl={originalUrl}
                compareOriginal={compareOriginal}
                position={selectedIndex + 1}
                total={snapshot.photos.length}
                onPrevious={() => selectAt(selectedIndex - 1)}
                onNext={() => selectAt(selectedIndex + 1)}
                onSelectLayer={(layerId) => dispatchEditor({ type: "setActiveLayer", layerId })}
                onSetLayerPlacement={(layerId, patch, historyGroup) => dispatchEditor({
                  type: "setLayerPlacement",
                  orientation: selectedPhoto?.orientation ?? editor.activeOrientation,
                  layerId,
                  patch,
                  historyGroup,
                })}
                onCloseHistoryGroup={() => dispatchEditor({ type: "closeHistoryGroup" })}
              />
            </main>
            <aside className="watermark-inspector-panel" data-watermark-tour="inspector">
              <WatermarkInspector
                template={template}
                orientation={editor.activeOrientation}
                activeLayerId={editor.activeLayerId}
                photoId={selectedPhoto?.id ?? null}
                photoOverride={selectedPhoto ? editor.present.photoOverrides[selectedPhoto.id] ?? null : null}
                fonts={fonts}
                dispatch={dispatchEditor}
                onOrientationChange={changeOrientation}
                onAddText={addTextLayer}
                onAddExif={addExifLayer}
                onAddImage={() => void addImageLayer()}
              />
            </aside>
          </div>
          <WatermarkFilmstrip
            data-watermark-tour="filmstrip"
            photos={snapshot.photos}
            snapshotId={snapshot.id}
            selectedPhotoId={selectedPhotoId}
            preloadProgress={preloadProgress}
            warningPhotoIds={previewWarnings}
            onSelectPhoto={setSelectedPhotoId}
          />
        </section>
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

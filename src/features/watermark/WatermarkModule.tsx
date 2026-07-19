import { Channel, invoke, isTauri } from "@tauri-apps/api/core";
import { confirm as confirmDialog, open, save } from "@tauri-apps/plugin-dialog";
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
import { WatermarkExportDialog } from "./WatermarkExportDialog";
import { WatermarkFilmstrip } from "./WatermarkFilmstrip";
import { WatermarkHeader } from "./WatermarkHeader";
import { WatermarkGuideDialog } from "./WatermarkGuideDialog";
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
  WatermarkExportEvent,
  WatermarkExportRequest,
  WatermarkOutputSettings,
  EmbeddedTemplateResource,
  WatermarkFontSummary,
  WatermarkSourcePhoto,
  WatermarkSourceInput,
  WatermarkSourceOrigin,
  WatermarkSourceSnapshot,
  WatermarkTemplateEntry,
  WatermarkTransferDraft,
} from "./types";
import {
  createDefaultWatermarkTemplate,
  createWatermarkExifLayer,
  createWatermarkImageLayer,
  createWatermarkTextLayer,
  createWatermarkExportProgress,
  defaultWatermarkOutputDirectory,
  DEFAULT_WATERMARK_OUTPUT,
  defaultWatermarkLayerLayouts,
  reduceWatermarkExportProgress,
  validateWatermarkOutputSettings,
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
const SOURCE_PRELOAD_CONCURRENCY = 3;
const LEFT_PANEL_STORAGE_KEY = "framepair.watermark.left-panel-collapsed.v1";
const RIGHT_PANEL_STORAGE_KEY = "framepair.watermark.right-panel-collapsed.v1";
const WATERMARK_GUIDE_STORAGE_KEY = "framepair.watermark.guide.v1";
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
    () => createDefaultWatermarkTemplate("minimal-signature", "极简署名"),
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
  const [previewPhotoId, setPreviewPhotoId] = useState<string | null>(null);
  const [previewBusy, setPreviewBusy] = useState(false);
  const [previewError, setPreviewError] = useState<string | null>(null);
  const [originalUrl, setOriginalUrl] = useState<string | null>(null);
  const [originalPhotoId, setOriginalPhotoId] = useState<string | null>(null);
  const [compareOriginal, setCompareOriginal] = useState(false);
  const [preloadProgress, setPreloadProgress] = useState<PreloadProgress>(EMPTY_PRELOAD_PROGRESS);
  const [leftTab, setLeftTab] = useState<"photos" | "templates">("photos");
  const [leftCollapsed, setLeftCollapsed] = useState(() => storedPanelPreference(LEFT_PANEL_STORAGE_KEY));
  const [rightCollapsed, setRightCollapsed] = useState(() => storedPanelPreference(RIGHT_PANEL_STORAGE_KEY));
  const [fonts, setFonts] = useState<WatermarkFontSummary[]>([]);
  const [templateEntries, setTemplateEntries] = useState<WatermarkTemplateEntry[]>([]);
  const [templateBusy, setTemplateBusy] = useState(false);
  const [templateError, setTemplateError] = useState<string | null>(null);
  const [exportOpen, setExportOpen] = useState(false);
  const [outputSettings, setOutputSettings] = useState<WatermarkOutputSettings>(DEFAULT_WATERMARK_OUTPUT);
  const [exportProgress, setExportProgress] = useState(createWatermarkExportProgress);
  const [exportError, setExportError] = useState<string | null>(null);
  const [guideOpen, setGuideOpen] = useState(false);
  const busyRef = useRef(false);
  const processedTransferId = useRef<string | null>(null);
  const previousPreviewPhotoId = useRef<string | null>(null);
  const handledDiscardToken = useRef(discardToken);
  const immersiveRestoreRef = useRef({ left: leftCollapsed, right: rightCollapsed });
  const wasImmersiveRef = useRef(immersive);
  const templatesRequestedRef = useRef(false);
  const previewCacheRef = useRef<WatermarkPreviewCache | null>(null);
  const exportTaskIdRef = useRef<string | null>(null);
  const exportChannelRef = useRef<Channel<WatermarkExportEvent> | null>(null);
  const cancelAfterExportStartRef = useRef(false);
  const attemptedGuideRef = useRef(false);
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
    if (exportProgress.phase === "running") {
      setError("请先等待水印导出结束或取消后续任务");
      return;
    }
    if (!isTauri()) {
      setError("请在 FramePair 桌面应用中载入本地照片");
      return;
    }
    busyRef.current = true;
    setBusy(true);
    setError(null);
    const exportTaskId = exportTaskIdRef.current;
    if (exportTaskId && exportProgress.phase === "results") {
      await invoke("acknowledge_watermark_export", { taskId: exportTaskId }).catch(() => undefined);
    }
    exportTaskIdRef.current = null;
    exportChannelRef.current = null;
    setExportOpen(false);
    setExportProgress(createWatermarkExportProgress());
    setExportError(null);
    setOutputSettings(DEFAULT_WATERMARK_OUTPUT);
    try {
      const result = await invoke<WatermarkSourceSnapshot>("prepare_watermark_source", {
        request: { origin, inputs },
      });
      setSnapshot(result);
      setSelectedPhotoId(result.photos[0]?.id ?? null);
      if (result.photos.length > 0) {
        dispatchEditor({ type: "markSourceChanged" });
        if (!attemptedGuideRef.current) {
          attemptedGuideRef.current = true;
          let shouldOpenGuide = true;
          try {
            shouldOpenGuide = localStorage.getItem(WATERMARK_GUIDE_STORAGE_KEY) !== "true";
          } catch {
            // The first-use guide still opens when preferences are unavailable.
          }
          if (shouldOpenGuide) {
            setLeftCollapsed(false);
            setRightCollapsed(false);
            onImmersiveChange(false);
            setGuideOpen(true);
          }
        }
      }
    } catch (prepareError) {
      setError(errorMessage(prepareError));
    } finally {
      busyRef.current = false;
      setBusy(false);
    }
  }

  useEffect(() => {
    if (!active || !transfer || processedTransferId.current === transfer.transferId) return;
    if (busyRef.current || exportProgress.phase === "running") return;
    processedTransferId.current = transfer.transferId;
    void prepare(transfer.origin, transfer.inputs);
  }, [active, busy, exportProgress.phase, transfer]);

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
    if (!active || !isTauri() || templatesRequestedRef.current) return;
    templatesRequestedRef.current = true;
    let disposed = false;
    setTemplateBusy(true);
    setTemplateError(null);
    void invoke<unknown>("list_watermark_templates")
      .then((entries) => {
        if (disposed) return;
        if (!Array.isArray(entries)) throw new Error("无法读取水印模板列表");
        const templateItems = entries as WatermarkTemplateEntry[];
        setTemplateEntries(templateItems);
        const current = templateItems.find((entry) => entry.template.id === template.id) ?? templateItems[0];
        if (current && !editor.present.dirtyTemplate) {
          dispatchEditor({ type: "hydrateTemplate", template: current.template });
        }
      })
      .catch((loadError) => {
        if (disposed) return;
        templatesRequestedRef.current = false;
        setTemplateError(errorMessage(loadError));
      })
      .finally(() => { if (!disposed) setTemplateBusy(false); });
    return () => { disposed = true; };
  }, [active, editor.present.dirtyTemplate, template.id]);

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
    setPreviewPhotoId(null);
    setPreviewError(null);
    setOriginalUrl(null);
    setOriginalPhotoId(null);
    setCompareOriginal(false);
    setPreloadProgress(EMPTY_PRELOAD_PROGRESS);
    setError(null);
    const exportTaskId = exportTaskIdRef.current;
    if (exportProgress.phase === "running") {
      if (exportTaskId) {
        void invoke("cancel_watermark_export", { taskId: exportTaskId }).catch(() => undefined);
      } else {
        cancelAfterExportStartRef.current = true;
      }
      setExportOpen(false);
    } else {
      if (exportTaskId) {
        void invoke("acknowledge_watermark_export", { taskId: exportTaskId }).catch(() => undefined);
      }
      exportTaskIdRef.current = null;
      exportChannelRef.current = null;
      setExportProgress(createWatermarkExportProgress());
      setExportOpen(false);
    }
    setExportError(null);
    setGuideOpen(false);
    setOutputSettings(DEFAULT_WATERMARK_OUTPUT);
    onImmersiveChange(false);
    dispatchEditor({ type: "resetEditor", template: initialTemplate });
  }, [discardToken, exportProgress.phase, initialTemplate, onImmersiveChange]);

  useEffect(() => {
    previewCacheRef.current?.clear();
    setPreview(null);
    setPreviewPhotoId(null);
    setPreviewError(null);
    previousPreviewPhotoId.current = null;
  }, [snapshot?.id]);

  useEffect(() => {
    if (!snapshot?.photos.length) return;
    setOutputSettings({
      ...DEFAULT_WATERMARK_OUTPUT,
      outputDirectory: defaultWatermarkOutputDirectory(snapshot),
    });
    setExportProgress(createWatermarkExportProgress());
    setExportError(null);
    setExportOpen(false);
    exportTaskIdRef.current = null;
    exportChannelRef.current = null;
  }, [snapshot?.id]);

  useEffect(() => {
    if (exportOpen || exportProgress.phase !== "results") return;
    const taskId = exportTaskIdRef.current;
    exportTaskIdRef.current = null;
    exportChannelRef.current = null;
    if (taskId) {
      void invoke("acknowledge_watermark_export", { taskId }).catch(() => undefined);
    }
    setExportProgress(createWatermarkExportProgress());
  }, [exportOpen, exportProgress.phase]);

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
  const availableOrientations = useMemo(
    () => new Set(snapshot?.photos.map((photo) => photo.orientation) ?? []),
    [snapshot],
  );

  useEffect(() => {
    if (!selectedPhoto) return;
    dispatchEditor({ type: "setActiveOrientation", orientation: selectedPhoto.orientation });
  }, [selectedPhoto]);

  useEffect(() => {
    if (!selectedPhoto || !snapshot) {
      setOriginalUrl(null);
      setOriginalPhotoId(null);
      return;
    }
    const thumbnailRequest: PreviewRequest = {
      root: selectedPhoto.root,
      relativePath: selectedPhoto.relativePath,
      maxEdge: SOURCE_THUMBNAIL_EDGE,
      version: `${snapshot.id}:${selectedPhoto.sizeBytes}:${selectedPhoto.modifiedMs}`,
    };
    const request: PreviewRequest = {
      root: selectedPhoto.root,
      relativePath: selectedPhoto.relativePath,
      maxEdge: WATERMARK_PREVIEW_EDGE,
      version: `${snapshot.id}:${selectedPhoto.sizeBytes}:${selectedPhoto.modifiedMs}`,
    };
    let disposed = false;
    const cached = peekPhotoPreviewUrl(request) ?? peekPhotoPreviewUrl(thumbnailRequest);
    setOriginalUrl(cached);
    setOriginalPhotoId(cached ? selectedPhoto.id : null);
    if (!cached) {
      void loadPhotoPreviewUrl(thumbnailRequest)
        .then((url) => {
          if (!disposed) {
            setOriginalUrl(url);
            setOriginalPhotoId(selectedPhoto.id);
          }
        })
        .catch(() => undefined);
    }
    if (compareOriginal) {
      void loadPhotoPreviewUrl(request)
        .then((url) => {
          if (!disposed) {
            setOriginalUrl(url);
            setOriginalPhotoId(selectedPhoto.id);
          }
        })
        .catch(() => undefined);
    }
    return () => { disposed = true; };
  }, [compareOriginal, selectedPhoto, snapshot]);

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
    const cached = cache.peek(key);
    if (cached) {
      setPreview(cached);
      setPreviewPhotoId(selectedPhoto.id);
    }
    setPreviewBusy(true);
    setPreviewError(null);
    const timeout = window.setTimeout(() => {
      void cache.getOrLoad(
        descriptor,
        () => loadWatermarkPreview(request, WATERMARK_PREVIEW_EDGE),
      ).then((result) => {
        if (disposed || !cache.isCurrent(token)) return;
        setPreview(result);
        setPreviewPhotoId(selectedPhoto.id);
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

  const visiblePreview = previewPhotoId === selectedPhoto?.id ? preview : null;
  const visibleOriginalUrl = originalPhotoId === selectedPhoto?.id ? originalUrl : null;

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

  async function refreshTemplates(): Promise<WatermarkTemplateEntry[]> {
    const entries = await invoke<unknown>("list_watermark_templates");
    if (!Array.isArray(entries)) throw new Error("无法读取水印模板列表");
    const templateItems = entries as WatermarkTemplateEntry[];
    setTemplateEntries(templateItems);
    return templateItems;
  }

  async function selectTemplate(entry: WatermarkTemplateEntry) {
    if (entry.template.id === template.id) return;
    if (editor.present.dirtyTemplate) {
      const accepted = await confirmDialog("当前模板包含未保存的调整，切换后这些调整将被放弃。", {
        title: "切换水印模板",
        kind: "warning",
        okLabel: "放弃并切换",
        cancelLabel: "继续编辑",
      });
      if (!accepted) return;
    }
    dispatchEditor({ type: "replaceTemplate", template: entry.template });
    if (!snapshot?.photos.length) dispatchEditor({ type: "markExported" });
  }

  async function saveTemplate(name: string, saveAs: boolean) {
    setTemplateBusy(true);
    setTemplateError(null);
    try {
      const next = structuredClone(template);
      next.name = name.trim();
      const entry = await invoke<WatermarkTemplateEntry>("save_watermark_template", {
        template: next,
        saveAs,
      });
      await refreshTemplates();
      dispatchEditor({ type: "hydrateTemplate", template: entry.template });
    } catch (saveError) {
      setTemplateError(errorMessage(saveError));
    } finally {
      setTemplateBusy(false);
    }
  }

  async function deleteTemplate() {
    const entry = templateEntries.find((candidate) => candidate.template.id === template.id);
    if (!entry || entry.builtIn) return;
    const accepted = await confirmDialog(`删除本地模板“${entry.template.name}”？此操作不会删除已导出的照片。`, {
      title: "删除水印模板",
      kind: "warning",
      okLabel: "删除模板",
      cancelLabel: "取消",
    });
    if (!accepted) return;
    setTemplateBusy(true);
    setTemplateError(null);
    try {
      await invoke("delete_watermark_template", { id: entry.template.id });
      const entries = await refreshTemplates();
      const fallback = entries[0];
      if (fallback) {
        dispatchEditor({ type: "replaceTemplate", template: fallback.template });
        if (!snapshot?.photos.length) dispatchEditor({ type: "markExported" });
      }
    } catch (deleteError) {
      setTemplateError(errorMessage(deleteError));
    } finally {
      setTemplateBusy(false);
    }
  }

  async function importTemplate() {
    if (editor.present.dirtyTemplate) {
      const accepted = await confirmDialog("导入后将切换到新模板，当前未保存的模板调整会被放弃。", {
        title: "导入水印模板",
        kind: "warning",
        okLabel: "继续导入",
        cancelLabel: "取消",
      });
      if (!accepted) return;
    }
    const path = await open({
      multiple: false,
      directory: false,
      title: "导入 FramePair 水印模板",
      filters: [{ name: "FramePair 水印模板", extensions: ["json"] }],
    });
    if (typeof path !== "string") return;
    setTemplateBusy(true);
    setTemplateError(null);
    try {
      const entry = await invoke<WatermarkTemplateEntry>("import_watermark_template", { path });
      await refreshTemplates();
      dispatchEditor({ type: "replaceTemplate", template: entry.template });
      if (!snapshot?.photos.length) dispatchEditor({ type: "markExported" });
    } catch (importError) {
      setTemplateError(errorMessage(importError));
    } finally {
      setTemplateBusy(false);
    }
  }

  async function exportTemplate() {
    const safeName = template.name.replace(/[\\/:*?"<>|]/g, "-").trim() || "FramePair-Template";
    const path = await save({
      title: "导出 FramePair 水印模板",
      defaultPath: `${safeName}.framepair-watermark.json`,
      filters: [{ name: "FramePair 水印模板", extensions: ["json"] }],
    });
    if (typeof path !== "string") return;
    setTemplateBusy(true);
    setTemplateError(null);
    try {
      await invoke("export_watermark_template", { path, template });
    } catch (exportError) {
      setTemplateError(errorMessage(exportError));
    } finally {
      setTemplateBusy(false);
    }
  }

  function changeOrientation(orientation: WatermarkSourcePhoto["orientation"]) {
    const target = snapshot?.photos.find((photo) => photo.orientation === orientation);
    if (target) setSelectedPhotoId(target.id);
  }

  async function chooseDirectory() {
    if (exportProgress.phase === "running") {
      setError("请先等待水印导出结束或取消后续任务");
      return;
    }
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

  function exportBlockingError(): string | null {
    if (previewError) return `当前照片预览失败：${previewError}`;
    const missingResource = template.shared.layers.find((layer) => (
      layer.kind === "image" && !template.resources[layer.resourceId]
    ));
    if (missingResource) return `图片图层“${missingResource.name}”缺少本地资源`;
    const fontWarning = preview?.warnings.find((warning) => warning.includes("字体"));
    return fontWarning ? `请先处理字体警告：${fontWarning}` : null;
  }

  function receiveExportEvent(event: WatermarkExportEvent) {
    if (event.type === "started") exportTaskIdRef.current = event.taskId;
    setExportProgress((current) => reduceWatermarkExportProgress(current, event));
    if (event.type === "finished" && event.summary.failed === 0 && event.summary.cancelled === 0) {
      dispatchEditor({ type: "markExported" });
    }
  }

  async function chooseOutputDirectory() {
    try {
      const selected = await open({
        directory: true,
        multiple: false,
        title: "选择或创建水印副本输出目录",
      });
      if (typeof selected === "string") {
        setOutputSettings((current) => ({ ...current, outputDirectory: selected }));
        setExportError(null);
      }
    } catch (directoryError) {
      setExportError(errorMessage(directoryError));
    }
  }

  async function startExport() {
    if (!snapshot || exportProgress.phase === "running") return;
    const validation = exportBlockingError()
      ?? validateWatermarkOutputSettings(outputSettings, snapshot);
    if (validation) {
      setExportError(validation);
      return;
    }
    if (!isTauri()) {
      setExportError("请在 FramePair 桌面应用中导出本地副本");
      return;
    }
    const channel = new Channel<WatermarkExportEvent>();
    channel.onmessage = receiveExportEvent;
    exportChannelRef.current = channel;
    cancelAfterExportStartRef.current = false;
    setExportError(null);
    setExportProgress({
      ...createWatermarkExportProgress(),
      phase: "running",
      total: snapshot.photos.length,
    });
    const request: WatermarkExportRequest = {
      snapshot,
      settings: outputSettings,
      template,
      photoOverrides: editor.present.photoOverrides,
    };
    try {
      const taskId = await invoke<string>("start_watermark_export", { request, onEvent: channel });
      exportTaskIdRef.current = taskId;
      setExportProgress((current) => ({ ...current, taskId: current.taskId ?? taskId }));
      if (cancelAfterExportStartRef.current) {
        await invoke("cancel_watermark_export", { taskId });
      }
    } catch (startError) {
      exportTaskIdRef.current = null;
      exportChannelRef.current = null;
      setExportProgress(createWatermarkExportProgress());
      setExportError(errorMessage(startError));
    }
  }

  async function cancelExport() {
    const taskId = exportTaskIdRef.current ?? exportProgress.taskId;
    setExportProgress((current) => ({ ...current, cancelRequested: true }));
    if (!taskId) {
      cancelAfterExportStartRef.current = true;
      return;
    }
    try {
      await invoke("cancel_watermark_export", { taskId });
    } catch (cancelError) {
      setExportError(errorMessage(cancelError));
    }
  }

  async function retryExportFailures() {
    const taskId = exportTaskIdRef.current ?? exportProgress.taskId;
    if (!taskId) return;
    const failedCount = exportProgress.results.filter((item) => item.status === "failed").length;
    if (failedCount === 0) return;
    const channel = new Channel<WatermarkExportEvent>();
    channel.onmessage = receiveExportEvent;
    exportChannelRef.current = channel;
    setExportError(null);
    setExportProgress((current) => ({
      ...current,
      phase: "running",
      total: failedCount,
      attemptResults: [],
      summary: null,
      cancelRequested: false,
    }));
    try {
      await invoke("retry_watermark_export_failures", { taskId, onEvent: channel });
    } catch (retryError) {
      setExportProgress((current) => ({ ...current, phase: "results" }));
      setExportError(errorMessage(retryError));
    }
  }

  async function revealExport() {
    const taskId = exportTaskIdRef.current ?? exportProgress.taskId;
    if (!taskId) return;
    try {
      await invoke("reveal_watermark_export", { taskId });
    } catch (revealError) {
      setExportError(errorMessage(revealError));
    }
  }

  async function closeExportDialog() {
    if (exportProgress.phase === "running") {
      const accepted = await confirmDialog(
        "关闭后将停止尚未开始的照片；已完成的副本会保留。",
        {
          title: "停止水印导出？",
          kind: "warning",
          okLabel: "停止并关闭",
          cancelLabel: "继续导出",
        },
      );
      if (!accepted) return;
      await cancelExport();
    }
    setExportOpen(false);
    setExportError(null);
  }

  function openWatermarkGuide() {
    if (!snapshot?.photos.length || exportProgress.phase === "running") return;
    setExportOpen(false);
    setLeftCollapsed(false);
    setRightCollapsed(false);
    onImmersiveChange(false);
    setGuideOpen(true);
  }

  function dismissWatermarkGuide() {
    try {
      localStorage.setItem(WATERMARK_GUIDE_STORAGE_KEY, "true");
    } catch {
      // The guide remains available from the header when storage is unavailable.
    }
    setGuideOpen(false);
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
  const outputBlockingError = exportBlockingError();
  const exportCommandDisabled = !snapshot?.photos.length || exportProgress.phase === "running";

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
        exportDisabled={exportCommandDisabled}
        onChooseDirectory={() => void chooseDirectory()}
        onUndo={() => dispatchEditor({ type: "undo" })}
        onRedo={() => dispatchEditor({ type: "redo" })}
        onCompare={() => setCompareOriginal((current) => !current)}
        onToggleLeft={() => toggleStoredPanel("left")}
        onToggleRight={() => toggleStoredPanel("right")}
        onToggleImmersive={toggleImmersive}
        onExport={() => {
          setExportError(null);
          setExportOpen(true);
        }}
        onGuide={openWatermarkGuide}
      />
      {busy ? <div className="activity-line" aria-hidden="true"><span /></div> : null}
      {snapshot && snapshot.photos.length > 0 ? (
        <section className={workspaceClass}>
          <div className="watermark-workspace">
            <aside className="watermark-left-panel" data-watermark-tour="sources-templates" aria-label="照片与模板">
              <div className="watermark-left-tabs" role="tablist" aria-label="照片和模板">
                <button type="button" role="tab" aria-selected={leftTab === "photos"} onClick={() => setLeftTab("photos")}><Images aria-hidden="true" size={15} />照片</button>
                <button type="button" role="tab" data-watermark-tour="templates" aria-selected={leftTab === "templates"} onClick={() => setLeftTab("templates")}><LayoutTemplate aria-hidden="true" size={15} />模板</button>
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
              ) : (
                <WatermarkTemplatePanel
                  entries={templateEntries}
                  activeTemplateId={template.id}
                  orientation={editor.activeOrientation}
                  busy={templateBusy}
                  error={templateError}
                  onSelect={(entry) => void selectTemplate(entry)}
                  onSave={(name, saveAs) => void saveTemplate(name, saveAs)}
                  onDelete={() => void deleteTemplate()}
                  onImport={() => void importTemplate()}
                  onExport={() => void exportTemplate()}
                />
              )}
            </aside>
            <main className="watermark-stage" data-watermark-tour="canvas">
              <WatermarkCanvas
                photo={selectedPhoto}
                preview={visiblePreview}
                template={template}
                orientation={selectedPhoto?.orientation ?? editor.activeOrientation}
                activeLayerId={editor.activeLayerId}
                loading={previewBusy}
                error={previewError}
                originalUrl={visibleOriginalUrl}
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
                availableOrientations={availableOrientations}
                activeLayerId={editor.activeLayerId}
                photoId={selectedPhoto?.id ?? null}
                photoOverride={selectedPhoto ? editor.present.photoOverrides[selectedPhoto.id] ?? null : null}
                fonts={fonts}
                dispatch={dispatchEditor}
                onOrientationChange={changeOrientation}
                onAddText={addTextLayer}
                onAddExif={addExifLayer}
                onAddImage={() => void addImageLayer()}
                outputSettings={outputSettings}
                exportDisabled={exportCommandDisabled}
                onOpenExport={() => {
                  setExportError(null);
                  setExportOpen(true);
                }}
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
      {snapshot?.photos.length ? (
        <WatermarkExportDialog
          open={exportOpen}
          snapshot={snapshot}
          settings={outputSettings}
          progress={exportProgress}
          error={exportError}
          blockingError={outputBlockingError}
          onSettingsChange={(settings) => {
            setOutputSettings(settings);
            setExportError(null);
          }}
          onChooseDirectory={() => void chooseOutputDirectory()}
          onStart={() => void startExport()}
          onCancel={() => void cancelExport()}
          onRetry={() => void retryExportFailures()}
          onReveal={() => void revealExport()}
          onClose={() => void closeExportDialog()}
        />
      ) : null}
      <WatermarkGuideDialog open={active && guideOpen} onDismiss={dismissWatermarkGuide} />
    </section>
  );
}

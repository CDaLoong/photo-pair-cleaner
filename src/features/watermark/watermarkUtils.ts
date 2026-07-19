import type {
  LayoutVariant,
  ExifTextLayer,
  ImageLayer,
  NormalizedPlacement,
  TextLayer,
  VariantLayerLayout,
  WatermarkLayer,
  WatermarkOrientation,
  WatermarkExportEvent,
  WatermarkExportSummary,
  WatermarkOutputResult,
  WatermarkOutputFormat,
  WatermarkOutputSettings,
  WatermarkTemplate,
} from "./types";

export interface Point2D { x: number; y: number }
export interface Size2D { width: number; height: number }

interface WatermarkLayerGeometryLike {
  anchorRect: { x: number; y: number; width: number; height: number };
  centerX: number;
  centerY: number;
  width: number;
  height: number;
  rotationDeg: number;
}

export function projectWatermarkLayerGeometry<T extends WatermarkLayerGeometryLike>(
  geometry: T,
  initial: NormalizedPlacement,
  current: NormalizedPlacement,
): T {
  const scale = initial.width > 0 ? current.width / initial.width : 1;
  return {
    ...geometry,
    centerX: Math.round(geometry.anchorRect.x + current.x * geometry.anchorRect.width),
    centerY: Math.round(geometry.anchorRect.y + current.y * geometry.anchorRect.height),
    width: Math.max(1, Math.round(geometry.width * scale)),
    height: Math.max(1, Math.round(geometry.height * scale)),
    rotationDeg: current.rotationDeg,
  };
}

export function viewportDeltaToNormalized(delta: { dx: number; dy: number }, anchorSize: Size2D): Point2D {
  if (anchorSize.width <= 0 || anchorSize.height <= 0) {
    throw new Error("锚定区域尺寸必须大于 0");
  }
  return { x: delta.dx / anchorSize.width, y: delta.dy / anchorSize.height };
}

export function clampWatermarkNumber(
  value: number,
  minimum: number,
  maximum: number,
  fallback: number,
): number {
  if (!Number.isFinite(value)) return fallback;
  return Math.max(minimum, Math.min(maximum, value));
}

export function normalizeWatermarkRotation(value: number): number {
  if (!Number.isFinite(value)) return 0;
  return ((value + 180) % 360 + 360) % 360 - 180;
}

export function canManipulateWatermarkLayer(
  layer: Pick<WatermarkLayer, "locked" | "visible">,
): boolean {
  return layer.visible && !layer.locked;
}

export function keyboardNudgeNormalized(
  position: Point2D,
  key: "ArrowLeft" | "ArrowRight" | "ArrowUp" | "ArrowDown",
  anchorSize: Size2D,
  coarse: boolean,
): Point2D {
  const pixels = coarse ? 10 : 1;
  const delta = viewportDeltaToNormalized({
    dx: key === "ArrowLeft" ? -pixels : key === "ArrowRight" ? pixels : 0,
    dy: key === "ArrowUp" ? -pixels : key === "ArrowDown" ? pixels : 0,
  }, anchorSize);
  return { x: position.x + delta.x, y: position.y + delta.y };
}

interface SnapPeer extends Point2D { id: string }

export function snapNormalizedPosition({
  position,
  anchorSize,
  layerSize,
  peers,
  thresholdPx = 6,
  bypass,
}: {
  position: Point2D;
  anchorSize: Size2D;
  layerSize: Size2D;
  peers: SnapPeer[];
  thresholdPx?: number;
  bypass: boolean;
}): { position: Point2D; guides: string[] } {
  if (bypass) return { position: { ...position }, guides: [] };
  if (anchorSize.width <= 0 || anchorSize.height <= 0) {
    return { position: { ...position }, guides: [] };
  }

  const xCandidates = [
    { value: layerSize.width / 2, guide: "anchor-left" },
    { value: 0.5, guide: "anchor-center-x" },
    { value: 1 - layerSize.width / 2, guide: "anchor-right" },
    ...peers.map((peer) => ({ value: peer.x, guide: `peer-x:${peer.id}` })),
  ];
  const yCandidates = [
    { value: layerSize.height / 2, guide: "anchor-top" },
    { value: 0.5, guide: "anchor-center-y" },
    { value: 1 - layerSize.height / 2, guide: "anchor-bottom" },
    ...peers.map((peer) => ({ value: peer.y, guide: `peer-y:${peer.id}` })),
  ];

  function nearest(
    value: number,
    candidates: Array<{ value: number; guide: string }>,
    pixelsPerUnit: number,
  ): { value: number; guide: string } | null {
    let match: { value: number; guide: string; distance: number } | null = null;
    for (const candidate of candidates) {
      const distance = Math.abs(value - candidate.value) * pixelsPerUnit;
      if (distance <= thresholdPx && (!match || distance < match.distance)) {
        match = { ...candidate, distance };
      }
    }
    return match ? { value: match.value, guide: match.guide } : null;
  }

  const x = nearest(position.x, xCandidates, anchorSize.width);
  const y = nearest(position.y, yCandidates, anchorSize.height);
  return {
    position: { x: x?.value ?? position.x, y: y?.value ?? position.y },
    guides: [x?.guide, y?.guide].filter((guide): guide is string => Boolean(guide)),
  };
}

export function classifyWatermarkOrientation(
  width: number,
  height: number,
): WatermarkOrientation {
  if (width <= 0 || height <= 0) throw new Error("照片尺寸必须大于 0");
  const ratio = width / height;
  if (ratio >= 0.95 && ratio <= 1.05) return "square";
  return width > height ? "landscape" : "portrait";
}

function defaultVariant(): LayoutVariant {
  return {
    canvasRatio: null,
    frame: { top: 0.04, right: 0.04, bottom: 0.14, left: 0.04 },
    background: { kind: "solid", color: "#ffffff", opacity: 1 },
    photo: {
      alignX: 0.5,
      alignY: 0.5,
      scale: 1,
      cornerRadiusRatio: 0,
      strokeWidthRatio: 0,
      strokeColor: "#ffffff",
      shadowBlurRatio: 0,
      shadowOpacity: 0,
      shadowOffsetXRatio: 0,
      shadowOffsetYRatio: 0,
    },
    layerLayouts: {},
  };
}

export function createDefaultWatermarkTemplate(id: string, name: string): WatermarkTemplate {
  return {
    schemaVersion: 1,
    id,
    name,
    shared: { layers: [], palette: ["#ffffff", "#111111"] },
    variants: {
      landscape: defaultVariant(),
      portrait: defaultVariant(),
      square: defaultVariant(),
    },
    resources: {},
  };
}

export function selectedLayoutVariant(width: number, height: number): WatermarkOrientation {
  return classifyWatermarkOrientation(width, height);
}

export function outputExtension(format: WatermarkOutputFormat): "jpg" | "png" {
  return format === "jpeg" ? "jpg" : "png";
}

export const DEFAULT_WATERMARK_OUTPUT: WatermarkOutputSettings = {
  format: "jpeg",
  jpegQuality: 90,
  sizing: { kind: "original", allowUpscale: false },
  colorSpace: "srgb",
  transparentBackground: false,
  jpegFlattenColor: "#ffffff",
  metadataPolicy: "privacy",
  outputDirectory: null,
  suffix: "_FramePair",
  collisionPolicy: "sequence",
};

interface WatermarkOutputSnapshotLike {
  rootPaths: string[];
  photos: Array<{
    fileName: string;
    sizeBytes?: number;
    pixelWidth?: number;
    pixelHeight?: number;
  }>;
}

export function defaultWatermarkOutputDirectory(
  snapshot: WatermarkOutputSnapshotLike,
): string | null {
  if (snapshot.rootPaths.length !== 1) return null;
  const root = snapshot.rootPaths[0].replace(/[\\/]+$/, "");
  const separator = root.includes("\\") ? "\\" : "/";
  const splitAt = Math.max(root.lastIndexOf("/"), root.lastIndexOf("\\"));
  const parent = splitAt > 0 ? root.slice(0, splitAt) : splitAt === 0 ? separator : ".";
  return `${parent}${parent.endsWith(separator) ? "" : separator}FramePair-Watermarked`;
}

function outputName(fileName: string, settings: WatermarkOutputSettings): string {
  const extensionAt = fileName.lastIndexOf(".");
  const stem = extensionAt > 0 ? fileName.slice(0, extensionAt) : fileName;
  return `${stem}${settings.suffix}.${outputExtension(settings.format)}`;
}

export function watermarkFilenameExamples(
  snapshot: WatermarkOutputSnapshotLike,
  settings: WatermarkOutputSettings,
): string[] {
  return snapshot.photos.slice(0, 2).map((photo) => outputName(photo.fileName, settings));
}

export function validateWatermarkOutputSettings(
  settings: WatermarkOutputSettings,
  snapshot: WatermarkOutputSnapshotLike,
): string | null {
  if (!Number.isInteger(settings.jpegQuality) || settings.jpegQuality < 1 || settings.jpegQuality > 100) {
    return "JPEG 质量必须在 1 到 100 之间";
  }
  if (settings.sizing.kind === "longEdge"
    && (!Number.isInteger(settings.sizing.pixels)
      || settings.sizing.pixels < 64
      || settings.sizing.pixels > 32768)) {
    return "输出长边必须在 64 到 32768 像素之间";
  }
  if (/[\x00-\x1f<>:"/\\|?*]/.test(settings.suffix)
    || /[ .]$/.test(settings.suffix)
    || settings.suffix.length > 120) {
    return "文件名后缀包含系统不允许的字符";
  }
  if (!/^#[0-9a-f]{6}$/i.test(settings.jpegFlattenColor)) {
    return "JPEG 铺底颜色必须使用六位十六进制颜色";
  }
  if (settings.format === "jpeg" && settings.transparentBackground) {
    return "JPEG 不支持透明背景，请关闭透明或改用 PNG";
  }
  if (snapshot.photos.length === 0) return "没有可导出的 JPG/JPEG 照片";
  if (!settings.outputDirectory && snapshot.rootPaths.length !== 1) {
    return "照片来自多个目录，请选择统一的输出目录";
  }
  return null;
}

export function estimateWatermarkOutputBytes(
  snapshot: WatermarkOutputSnapshotLike,
  settings: WatermarkOutputSettings,
): { minimum: number; maximum: number } {
  const sourceBytes = snapshot.photos.reduce((total, photo) => total + (photo.sizeBytes ?? 0), 0);
  const sizing = settings.sizing;
  const scale = sizing.kind === "longEdge"
    ? snapshot.photos.reduce((total, photo) => {
      const edge = Math.max(photo.pixelWidth ?? sizing.pixels, photo.pixelHeight ?? sizing.pixels);
      const ratio = sizing.allowUpscale
        ? sizing.pixels / Math.max(edge, 1)
        : Math.min(1, sizing.pixels / Math.max(edge, 1));
      return total + ratio * ratio;
    }, 0) / Math.max(snapshot.photos.length, 1)
    : 1;
  const formatRange = settings.format === "jpeg" ? [0.45, 1.8] : [1, 4] as const;
  return {
    minimum: Math.max(1, Math.round(sourceBytes * scale * formatRange[0])),
    maximum: Math.max(1, Math.round(sourceBytes * scale * formatRange[1])),
  };
}

export interface WatermarkExportProgress {
  phase: "idle" | "running" | "results";
  taskId: string | null;
  total: number;
  currentPhotoId: string | null;
  currentIndex: number | null;
  attemptResults: WatermarkOutputResult[];
  results: WatermarkOutputResult[];
  summary: WatermarkExportSummary | null;
  cancelRequested: boolean;
}

export function createWatermarkExportProgress(): WatermarkExportProgress {
  return {
    phase: "idle",
    taskId: null,
    total: 0,
    currentPhotoId: null,
    currentIndex: null,
    attemptResults: [],
    results: [],
    summary: null,
    cancelRequested: false,
  };
}

export function reduceWatermarkExportProgress(
  state: WatermarkExportProgress,
  event: WatermarkExportEvent,
): WatermarkExportProgress {
  switch (event.type) {
    case "started":
      return {
        ...state,
        phase: "running",
        taskId: event.taskId,
        total: event.total,
        currentPhotoId: null,
        currentIndex: null,
        attemptResults: [],
        summary: null,
        cancelRequested: false,
      };
    case "itemStarted":
      return { ...state, currentPhotoId: event.photoId, currentIndex: event.index };
    case "itemFinished": {
      const existing = state.results.findIndex((result) => result.photoId === event.result.photoId);
      const results = [...state.results];
      if (existing >= 0) results[existing] = event.result;
      else results.push(event.result);
      return { ...state, results, attemptResults: [...state.attemptResults, event.result] };
    }
    case "finished":
      return {
        ...state,
        phase: "results",
        currentPhotoId: null,
        currentIndex: null,
        summary: event.summary,
      };
  }
}

export function failedWatermarkPhotoIds(progress: WatermarkExportProgress): string[] {
  return progress.results
    .filter((result) => result.status === "failed")
    .map((result) => result.photoId);
}

function placementLayout(
  anchorSpace: "photo" | "frame" | "canvas",
  frameEdge: "top" | "right" | "bottom" | "left" | null,
  width: number,
  fontSizeRatio: number | null,
): VariantLayerLayout {
  return {
    placement: {
      anchorSpace,
      frameEdge,
      x: 0.5,
      y: 0.5,
      width,
      rotationDeg: 0,
      opacity: 1,
    },
    fontSizeRatio,
  };
}

export function defaultWatermarkLayerLayouts(
  kind: "text" | "exifText" | "image",
): Record<WatermarkOrientation, VariantLayerLayout> {
  const create = () => kind === "image"
    ? placementLayout("photo", null, 0.18, null)
    : placementLayout("frame", "bottom", 0.62, kind === "text" ? 0.055 : 0.042);
  return { landscape: create(), portrait: create(), square: create() };
}

function textStyle() {
  return {
    fontFamily: "Noto Sans CJK SC",
    fontWeight: 500,
    color: "#202321",
    align: "center" as const,
    letterSpacingRatio: 0,
    lineHeight: 1.2,
    strokeColor: "#ffffff",
    strokeWidthRatio: 0,
    shadowColor: "#00000066",
    shadowBlurRatio: 0,
    shadowOffsetXRatio: 0,
    shadowOffsetYRatio: 0,
  };
}

export function createWatermarkTextLayer(id: string): TextLayer {
  return {
    id,
    kind: "text",
    name: "文字",
    zIndex: 0,
    visible: true,
    locked: false,
    text: "YOUR NAME",
    ...textStyle(),
  };
}

export function createWatermarkExifLayer(id: string): ExifTextLayer {
  return {
    id,
    kind: "exifText",
    name: "拍摄参数",
    zIndex: 0,
    visible: true,
    locked: false,
    fields: ["cameraModel", "lensModel", "focalLength", "aperture", "shutterSpeed", "iso"],
    separator: " · ",
    prefix: "",
    suffix: "",
    missingValue: null,
    ...textStyle(),
  };
}

export function createWatermarkImageLayer(id: string, resourceId: string): ImageLayer {
  return {
    id,
    kind: "image",
    name: "图片水印",
    zIndex: 0,
    visible: true,
    locked: false,
    resourceId,
    fit: "contain",
  };
}

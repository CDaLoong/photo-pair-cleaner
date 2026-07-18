import type {
  LayoutVariant,
  ExifTextLayer,
  ImageLayer,
  TextLayer,
  VariantLayerLayout,
  WatermarkLayer,
  WatermarkOrientation,
  WatermarkOutputFormat,
  WatermarkTemplate,
} from "./types";

export interface Point2D { x: number; y: number }
export interface Size2D { width: number; height: number }

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

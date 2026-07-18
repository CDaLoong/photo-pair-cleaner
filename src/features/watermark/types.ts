export const WATERMARK_SCHEMA_VERSION = 1 as const;

export type WatermarkOrientation = "landscape" | "portrait" | "square";
export type WatermarkLayerKind = "text" | "exifText" | "image";
export type WatermarkAnchorSpace = "photo" | "frame" | "canvas";
export type WatermarkFrameEdge = "top" | "right" | "bottom" | "left";
export type WatermarkOutputFormat = "jpeg" | "png";
export type MetadataPolicy = "preserve" | "privacy" | "remove";
export type CollisionPolicy = "sequence" | "skip" | "overwriteOutput";

export interface NormalizedPlacement {
  anchorSpace: WatermarkAnchorSpace;
  frameEdge: WatermarkFrameEdge | null;
  x: number;
  y: number;
  width: number;
  rotationDeg: number;
  opacity: number;
}

export interface WatermarkLayerBase {
  id: string;
  name: string;
  zIndex: number;
  visible: boolean;
  locked: boolean;
}

export interface TextLayer extends WatermarkLayerBase {
  kind: "text";
  text: string;
  fontFamily: string;
  fontWeight: number;
  color: string;
  align: "left" | "center" | "right";
  letterSpacingRatio: number;
  lineHeight: number;
  strokeColor: string;
  strokeWidthRatio: number;
  shadowColor: string;
  shadowBlurRatio: number;
  shadowOffsetXRatio: number;
  shadowOffsetYRatio: number;
}

export interface ExifTextLayer extends Omit<TextLayer, "kind" | "text"> {
  kind: "exifText";
  fields: string[];
  separator: string;
  prefix: string;
  suffix: string;
  missingValue: string | null;
}

export interface ImageLayer extends WatermarkLayerBase {
  kind: "image";
  resourceId: string;
  fit: "contain" | "cover";
}

export type WatermarkLayer = TextLayer | ExifTextLayer | ImageLayer;

export interface FrameInsets {
  top: number;
  right: number;
  bottom: number;
  left: number;
}

export interface GradientStop {
  offset: number;
  color: string;
  opacity: number;
}

export type WatermarkBackground =
  | { kind: "transparent" }
  | { kind: "solid"; color: string; opacity: number }
  | { kind: "sampled"; x: number; y: number; color: string; sampleEachPhoto: boolean }
  | { kind: "linearGradient"; angleDeg: number; stops: GradientStop[] }
  | {
      kind: "radialGradient";
      centerX: number;
      centerY: number;
      radius: number;
      stops: GradientStop[];
    }
  | {
      kind: "blurredPhoto";
      blurRatio: number;
      scale: number;
      overlayColor: string;
      overlayOpacity: number;
    }
  | { kind: "image"; resourceId: string; fit: "contain" | "cover"; opacity: number };

export interface PhotoStyle {
  alignX: number;
  alignY: number;
  scale: number;
  cornerRadiusRatio: number;
  strokeWidthRatio: number;
  strokeColor: string;
  shadowBlurRatio: number;
  shadowOpacity: number;
  shadowOffsetXRatio: number;
  shadowOffsetYRatio: number;
}

export interface VariantLayerLayout {
  placement: NormalizedPlacement;
  fontSizeRatio: number | null;
}

export interface LayoutVariant {
  canvasRatio: number | null;
  frame: FrameInsets;
  background: WatermarkBackground;
  photo: PhotoStyle;
  layerLayouts: Record<string, VariantLayerLayout>;
}

export interface EmbeddedTemplateResource {
  id: string;
  name: string;
  mimeType: "image/png" | "image/jpeg";
  sha256: string;
  width: number;
  height: number;
  dataBase64: string;
}

export interface WatermarkTemplate {
  schemaVersion: typeof WATERMARK_SCHEMA_VERSION;
  id: string;
  name: string;
  shared: { layers: WatermarkLayer[]; palette: string[] };
  variants: Record<WatermarkOrientation, LayoutVariant>;
  resources: Record<string, EmbeddedTemplateResource>;
}

export type WatermarkSourceOrigin =
  | "directory"
  | "drop"
  | "preview-photo"
  | "preview-directory"
  | "preview-filter";

export interface WatermarkSourcePhoto {
  id: string;
  root: string;
  relativePath: string;
  fileName: string;
  sizeBytes: number;
  modifiedMs: number;
  pixelWidth: number;
  pixelHeight: number;
  orientation: WatermarkOrientation;
}

export interface WatermarkSourceSnapshot {
  id: string;
  createdAtMs: number;
  origin: WatermarkSourceOrigin;
  rootPaths: string[];
  photos: WatermarkSourcePhoto[];
  skippedRawOnly: number;
  skippedUnsupported: number;
}

export interface PhotoPlacementOverride {
  alignX: number;
  alignY: number;
  scale: number;
}

export type WatermarkSizing =
  | { kind: "original"; allowUpscale: false }
  | { kind: "longEdge"; pixels: number; allowUpscale: boolean };

export interface WatermarkOutputSettings {
  format: WatermarkOutputFormat;
  jpegQuality: number;
  sizing: WatermarkSizing;
  colorSpace: "srgb" | "preserve";
  transparentBackground: boolean;
  jpegFlattenColor: string;
  metadataPolicy: MetadataPolicy;
  outputDirectory: string | null;
  suffix: string;
  collisionPolicy: CollisionPolicy;
}

export interface WatermarkRenderRequest {
  schemaVersion: typeof WATERMARK_SCHEMA_VERSION;
  source: WatermarkSourcePhoto;
  template: WatermarkTemplate;
  photoOverride: PhotoPlacementOverride | null;
  colorSpace: "srgb" | "preserve";
  transparentBackground: boolean;
  jpegFlattenColor: string;
}

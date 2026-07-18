import type {
  LayoutVariant,
  WatermarkOrientation,
  WatermarkOutputFormat,
  WatermarkTemplate,
} from "./types";

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

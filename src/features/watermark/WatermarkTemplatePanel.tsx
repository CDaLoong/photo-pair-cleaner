import { Check, LayoutTemplate, Monitor, Smartphone } from "lucide-react";
import type { CSSProperties } from "react";
import type { WatermarkBackground, WatermarkOrientation, WatermarkTemplate } from "./types";

function backgroundStyle(background: WatermarkBackground): CSSProperties {
  if (background.kind === "solid") return { background: background.color };
  if (background.kind === "linearGradient") {
    return { background: `linear-gradient(${background.angleDeg}deg, ${background.stops.map((stop) => `${stop.color} ${stop.offset * 100}%`).join(", ")})` };
  }
  if (background.kind === "radialGradient") {
    return { background: `radial-gradient(circle, ${background.stops.map((stop) => `${stop.color} ${stop.offset * 100}%`).join(", ")})` };
  }
  return { background: "#d9dedb" };
}

const VARIANTS: Array<{ id: WatermarkOrientation; label: string }> = [
  { id: "landscape", label: "横版" },
  { id: "portrait", label: "竖版" },
  { id: "square", label: "方形" },
];

interface WatermarkTemplatePanelProps {
  template: WatermarkTemplate;
  orientation: WatermarkOrientation;
}

export function WatermarkTemplatePanel({ template, orientation }: WatermarkTemplatePanelProps) {
  return (
    <section className="watermark-template-panel" aria-label="本地水印模板">
      <header><LayoutTemplate aria-hidden="true" size={16} /><strong>本地模板</strong><span>1</span></header>
      <button className="watermark-template-item is-selected" type="button" aria-pressed="true">
        <span className="watermark-template-preview" aria-hidden="true">
          {VARIANTS.map(({ id }) => <i key={id} style={backgroundStyle(template.variants[id].background)} />)}
        </span>
        <span><strong>{template.name}</strong><small>内置 · 3 套方向版式</small></span>
        <Check aria-hidden="true" size={15} />
      </button>
      <div className="watermark-template-variants" aria-label="模板方向状态">
        {VARIANTS.map(({ id, label }) => (
          <span className={id === orientation ? "is-active" : undefined} key={id}>
            {id === "portrait" ? <Smartphone aria-hidden="true" size={14} /> : <Monitor aria-hidden="true" size={14} />}
            <strong>{label}</strong>
          </span>
        ))}
      </div>
    </section>
  );
}

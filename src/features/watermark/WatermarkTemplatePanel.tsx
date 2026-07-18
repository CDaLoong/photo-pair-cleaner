import {
  Check,
  Copy,
  Download,
  LayoutTemplate,
  Save,
  Trash2,
  Upload,
} from "lucide-react";
import { useEffect, useState } from "react";
import type { CSSProperties } from "react";
import type {
  WatermarkBackground,
  WatermarkOrientation,
  WatermarkTemplateEntry,
} from "./types";

function backgroundStyle(background: WatermarkBackground): CSSProperties {
  if (background.kind === "solid") return { background: background.color };
  if (background.kind === "linearGradient") return { background: `linear-gradient(${background.angleDeg}deg, ${background.stops.map((stop) => `${stop.color} ${stop.offset * 100}%`).join(", ")})` };
  if (background.kind === "radialGradient") return { background: `radial-gradient(circle, ${background.stops.map((stop) => `${stop.color} ${stop.offset * 100}%`).join(", ")})` };
  if (background.kind === "blurredPhoto") return { background: "linear-gradient(135deg, #6e9b91, #d9bd78)" };
  if (background.kind === "transparent") return { backgroundColor: "#eef0ed", backgroundImage: "linear-gradient(45deg, #ccd1cd 25%, transparent 25%), linear-gradient(-45deg, #ccd1cd 25%, transparent 25%), linear-gradient(45deg, transparent 75%, #ccd1cd 75%), linear-gradient(-45deg, transparent 75%, #ccd1cd 75%)", backgroundSize: "8px 8px", backgroundPosition: "0 0, 0 4px, 4px -4px, -4px 0" };
  return { background: "#d9dedb" };
}

interface WatermarkTemplatePanelProps {
  entries: WatermarkTemplateEntry[];
  activeTemplateId: string;
  orientation: WatermarkOrientation;
  busy: boolean;
  error: string | null;
  onSelect: (entry: WatermarkTemplateEntry) => void;
  onSave: (name: string, saveAs: boolean) => void;
  onDelete: () => void;
  onImport: () => void;
  onExport: () => void;
}

export function WatermarkTemplatePanel({
  entries,
  activeTemplateId,
  orientation,
  busy,
  error,
  onSelect,
  onSave,
  onDelete,
  onImport,
  onExport,
}: WatermarkTemplatePanelProps) {
  const active = entries.find((entry) => entry.template.id === activeTemplateId) ?? null;
  const [name, setName] = useState(active?.template.name ?? "");

  useEffect(() => setName(active?.template.name ?? ""), [active?.template.id, active?.template.name]);

  return (
    <section className="watermark-template-panel" aria-label="本地水印模板">
      <header><LayoutTemplate aria-hidden="true" size={16} /><strong>本地模板</strong><span>{entries.length}</span></header>
      {error ? <p className="watermark-template-error" role="alert">{error}</p> : null}
      <div className="watermark-template-list">
        {entries.map((entry) => {
          const selected = entry.template.id === activeTemplateId;
          return (
            <button className={selected ? "watermark-template-item is-selected" : "watermark-template-item"} type="button" aria-pressed={selected} onClick={() => onSelect(entry)} key={entry.template.id}>
              <span className="watermark-template-preview" aria-hidden="true">
                {(["landscape", "portrait", "square"] as const).map((id) => <i className={id === orientation ? "is-active" : undefined} key={id} style={backgroundStyle(entry.template.variants[id].background)} />)}
              </span>
              <span><strong>{entry.template.name}</strong><small>{entry.builtIn ? "内置模板" : "本地模板"} · 3 套方向</small></span>
              {selected ? <Check aria-hidden="true" size={15} /> : null}
            </button>
          );
        })}
      </div>
      <footer className="watermark-template-actions">
        <label><span>模板名称</span><input value={name} maxLength={100} onChange={(event) => setName(event.currentTarget.value)} disabled={busy} /></label>
        <div role="group" aria-label="模板管理">
          <button type="button" aria-label="保存本地模板" title="保存" onClick={() => onSave(name, false)} disabled={busy || !active || active.builtIn || !name.trim()}><Save aria-hidden="true" size={14} /></button>
          <button type="button" aria-label="另存为本地模板" title="另存为" onClick={() => onSave(name, true)} disabled={busy || !active || !name.trim()}><Copy aria-hidden="true" size={14} /></button>
          <button type="button" aria-label="导入模板" title="导入模板" onClick={onImport} disabled={busy}><Upload aria-hidden="true" size={14} /></button>
          <button type="button" aria-label="导出模板" title="导出模板" onClick={onExport} disabled={busy || !active}><Download aria-hidden="true" size={14} /></button>
          <button type="button" aria-label="删除本地模板" title="删除模板" onClick={onDelete} disabled={busy || !active || active.builtIn}><Trash2 aria-hidden="true" size={14} /></button>
        </div>
      </footer>
    </section>
  );
}

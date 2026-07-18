import { Eye, EyeOff, Image, Layers3, Lock, SlidersHorizontal, Type, Unlock } from "lucide-react";
import type { WatermarkLayer, WatermarkOrientation, WatermarkTemplate } from "./types";

const ORIENTATION_LABELS: Array<{ id: WatermarkOrientation; label: string }> = [
  { id: "landscape", label: "横版" },
  { id: "portrait", label: "竖版" },
  { id: "square", label: "方形" },
];

interface WatermarkInspectorProps {
  template: WatermarkTemplate;
  orientation: WatermarkOrientation;
  activeLayerId: string | null;
  onOrientationChange: (orientation: WatermarkOrientation) => void;
  onSelectLayer: (layerId: string) => void;
  onSetLayerVisible: (layerId: string, visible: boolean) => void;
  onSetLayerLocked: (layerId: string, locked: boolean) => void;
}

function layerIcon(layer: WatermarkLayer) {
  return layer.kind === "image"
    ? <Image aria-hidden="true" size={15} />
    : <Type aria-hidden="true" size={15} />;
}

export function WatermarkInspector({
  template,
  orientation,
  activeLayerId,
  onOrientationChange,
  onSelectLayer,
  onSetLayerVisible,
  onSetLayerLocked,
}: WatermarkInspectorProps) {
  const variant = template.variants[orientation];
  return (
    <section className="watermark-inspector" aria-label="水印属性">
      <header><SlidersHorizontal aria-hidden="true" size={16} /><strong>属性</strong></header>
      <div className="watermark-inspector-section">
        <strong>方向版式</strong>
        <div className="watermark-orientation-switch" role="group" aria-label="编辑方向版式">
          {ORIENTATION_LABELS.map((item) => (
            <button type="button" key={item.id} aria-pressed={orientation === item.id} onClick={() => onOrientationChange(item.id)}>{item.label}</button>
          ))}
        </div>
      </div>
      <div className="watermark-inspector-section watermark-layer-section">
        <div className="watermark-section-heading"><Layers3 aria-hidden="true" size={15} /><strong>图层</strong><span>{template.shared.layers.length}</span></div>
        {template.shared.layers.length > 0 ? (
          <div className="watermark-layer-list">
            {[...template.shared.layers].reverse().map((layer) => (
              <div className={layer.id === activeLayerId ? "is-active" : undefined} key={layer.id}>
                <button type="button" onClick={() => onSelectLayer(layer.id)}>{layerIcon(layer)}<span>{layer.name}</span></button>
                <button type="button" aria-label={layer.visible ? `隐藏${layer.name}` : `显示${layer.name}`} title={layer.visible ? "隐藏图层" : "显示图层"} onClick={() => onSetLayerVisible(layer.id, !layer.visible)}>
                  {layer.visible ? <Eye aria-hidden="true" size={14} /> : <EyeOff aria-hidden="true" size={14} />}
                </button>
                <button type="button" aria-label={layer.locked ? `解锁${layer.name}` : `锁定${layer.name}`} title={layer.locked ? "解锁图层" : "锁定图层"} onClick={() => onSetLayerLocked(layer.id, !layer.locked)}>
                  {layer.locked ? <Lock aria-hidden="true" size={14} /> : <Unlock aria-hidden="true" size={14} />}
                </button>
              </div>
            ))}
          </div>
        ) : <p className="watermark-inspector-empty">当前模板仅包含边框</p>}
      </div>
      <div className="watermark-inspector-section">
        <strong>画布与边框</strong>
        <dl className="watermark-layout-summary">
          <div><dt>上 / 下</dt><dd>{Math.round(variant.frame.top * 100)}% / {Math.round(variant.frame.bottom * 100)}%</dd></div>
          <div><dt>左 / 右</dt><dd>{Math.round(variant.frame.left * 100)}% / {Math.round(variant.frame.right * 100)}%</dd></div>
          <div><dt>照片缩放</dt><dd>{Math.round(variant.photo.scale * 100)}%</dd></div>
        </dl>
      </div>
    </section>
  );
}

import {
  ArrowDown,
  ArrowUp,
  Copy,
  Download,
  Eye,
  EyeOff,
  Image,
  Layers3,
  Link,
  Lock,
  Plus,
  SlidersHorizontal,
  Trash2,
  Type,
  Unlink,
  Unlock,
  X,
} from "lucide-react";
import { useMemo, useState } from "react";
import type { WatermarkEditorAction } from "./watermarkEditorState";
import type {
  ExifTextLayer,
  ImageLayer,
  PhotoPlacementOverride,
  TextLayer,
  WatermarkBackground,
  WatermarkFontSummary,
  WatermarkLayer,
  WatermarkOrientation,
  WatermarkOutputSettings,
  WatermarkTemplate,
} from "./types";
import { clampWatermarkNumber, normalizeWatermarkRotation } from "./watermarkUtils";

const ORIENTATION_LABELS: Array<{ id: WatermarkOrientation; label: string }> = [
  { id: "landscape", label: "横版" },
  { id: "portrait", label: "竖版" },
  { id: "square", label: "方形" },
];

const EXIF_FIELDS = [
  ["cameraMake", "相机品牌"],
  ["cameraModel", "相机型号"],
  ["lensModel", "镜头"],
  ["focalLength", "焦距"],
  ["aperture", "光圈"],
  ["shutterSpeed", "快门"],
  ["iso", "ISO"],
  ["captureDate", "拍摄时间"],
] as const;

interface NumberFieldProps {
  label: string;
  value: number;
  min: number;
  max: number;
  step: number;
  scale?: number;
  suffix?: string;
  onChange: (value: number) => void;
}

function NumberField({ label, value, min, max, step, scale = 1, suffix, onChange }: NumberFieldProps) {
  const shown = Number((value * scale).toFixed(4));
  return (
    <label className="watermark-number-field">
      <span>{label}</span>
      <span><input type="number" value={shown} min={min * scale} max={max * scale} step={step * scale} onChange={(event) => onChange(clampWatermarkNumber(event.currentTarget.valueAsNumber / scale, min, max, value))} />{suffix ? <small>{suffix}</small> : null}</span>
    </label>
  );
}

function ColorField({ label, value, onChange }: { label: string; value: string; onChange: (value: string) => void }) {
  const color = /^#[0-9a-f]{6}$/i.test(value) ? value : "#000000";
  return <label className="watermark-color-field"><span>{label}</span><input type="color" value={color} onChange={(event) => onChange(event.currentTarget.value)} title={`${label} ${value}`} /></label>;
}

function layerIcon(layer: WatermarkLayer) {
  return layer.kind === "image" ? <Image aria-hidden="true" size={15} /> : <Type aria-hidden="true" size={15} />;
}

function solidBackground(background: WatermarkBackground): Extract<WatermarkBackground, { kind: "solid" }> {
  return background.kind === "solid" ? background : { kind: "solid", color: "#ffffff", opacity: 1 };
}

interface WatermarkInspectorProps {
  template: WatermarkTemplate;
  orientation: WatermarkOrientation;
  activeLayerId: string | null;
  photoId: string | null;
  photoOverride: PhotoPlacementOverride | null;
  fonts: WatermarkFontSummary[];
  dispatch: (action: WatermarkEditorAction) => void;
  onOrientationChange: (orientation: WatermarkOrientation) => void;
  onAddText: () => void;
  onAddExif: () => void;
  onAddImage: () => void;
  outputSettings: WatermarkOutputSettings;
  exportDisabled: boolean;
  onOpenExport: () => void;
}

export function WatermarkInspector({
  template,
  orientation,
  activeLayerId,
  photoId,
  photoOverride,
  fonts,
  dispatch,
  onOrientationChange,
  onAddText,
  onAddExif,
  onAddImage,
  outputSettings,
  exportDisabled,
  onOpenExport,
}: WatermarkInspectorProps) {
  const [linkFrame, setLinkFrame] = useState(false);
  const [newExifField, setNewExifField] = useState("cameraModel");
  const variant = template.variants[orientation];
  const activeLayer = template.shared.layers.find((layer) => layer.id === activeLayerId) ?? null;
  const activeLayout = activeLayer ? variant.layerLayouts[activeLayer.id] : null;
  const font = activeLayer && activeLayer.kind !== "image"
    ? fonts.find((item) => item.family === activeLayer.fontFamily) ?? null
    : null;
  const background = variant.background;
  const resources = Object.values(template.resources);

  function updateLayer(patch: Partial<WatermarkLayer>) {
    if (!activeLayer) return;
    dispatch({ type: "updateLayer", layerId: activeLayer.id, patch, historyGroup: null });
  }

  function setPlacement(patch: Partial<NonNullable<typeof activeLayout>["placement"]>) {
    if (!activeLayer) return;
    dispatch({ type: "setLayerPlacement", orientation, layerId: activeLayer.id, patch, historyGroup: null });
  }

  function updateBackground(next: WatermarkBackground) {
    dispatch({ type: "setVariantBackground", orientation, background: next });
  }

  function updateFrame(edge: "top" | "right" | "bottom" | "left", value: number) {
    dispatch({
      type: "setVariantFrame",
      orientation,
      patch: linkFrame ? { top: value, right: value, bottom: value, left: value } : { [edge]: value },
    });
  }

  function moveExifField(index: number, direction: -1 | 1) {
    if (!activeLayer || activeLayer.kind !== "exifText") return;
    const fields = [...activeLayer.fields];
    const target = index + direction;
    if (target < 0 || target >= fields.length) return;
    [fields[index], fields[target]] = [fields[target], fields[index]];
    updateLayer({ fields } as Partial<ExifTextLayer>);
  }

  const missingExifFields = useMemo(() => EXIF_FIELDS.filter(([id]) => (
    !activeLayer || activeLayer.kind !== "exifText" || !activeLayer.fields.includes(id)
  )), [activeLayer]);

  return (
    <section className="watermark-inspector" aria-label="水印属性">
      <header><SlidersHorizontal aria-hidden="true" size={16} /><strong>属性</strong></header>

      <div className="watermark-inspector-section">
        <strong>方向版式</strong>
        <div className="watermark-orientation-switch" role="group" aria-label="编辑方向版式">
          {ORIENTATION_LABELS.map((item) => <button type="button" key={item.id} aria-pressed={orientation === item.id} onClick={() => onOrientationChange(item.id)}>{item.label}</button>)}
        </div>
      </div>

      <div className="watermark-inspector-section watermark-layer-section">
        <div className="watermark-section-heading"><Layers3 aria-hidden="true" size={15} /><strong>图层</strong><span>{template.shared.layers.length}</span></div>
        <div className="watermark-layer-add" role="group" aria-label="添加水印图层">
          <button type="button" onClick={onAddText}><Type aria-hidden="true" size={14} />文字</button>
          <button type="button" onClick={onAddExif}><Plus aria-hidden="true" size={14} />EXIF</button>
          <button type="button" onClick={onAddImage}><Image aria-hidden="true" size={14} />图片</button>
        </div>
        {template.shared.layers.length > 0 ? (
          <div className="watermark-layer-list">
            {[...template.shared.layers].reverse().map((layer) => {
              const index = template.shared.layers.findIndex((candidate) => candidate.id === layer.id);
              return (
                <div className={layer.id === activeLayerId ? "is-active" : undefined} key={layer.id}>
                  <button type="button" onClick={() => dispatch({ type: "setActiveLayer", layerId: layer.id })}>{layerIcon(layer)}<span>{layer.name}</span></button>
                  <button type="button" aria-label={layer.visible ? `隐藏${layer.name}` : `显示${layer.name}`} title={layer.visible ? "隐藏图层" : "显示图层"} onClick={() => dispatch({ type: "setLayerVisible", layerId: layer.id, visible: !layer.visible })}>{layer.visible ? <Eye aria-hidden="true" size={14} /> : <EyeOff aria-hidden="true" size={14} />}</button>
                  <button type="button" aria-label={layer.locked ? `解锁${layer.name}` : `锁定${layer.name}`} title={layer.locked ? "解锁图层" : "锁定图层"} onClick={() => dispatch({ type: "setLayerLocked", layerId: layer.id, locked: !layer.locked })}>{layer.locked ? <Lock aria-hidden="true" size={14} /> : <Unlock aria-hidden="true" size={14} />}</button>
                  {layer.id === activeLayerId ? (
                    <span className="watermark-layer-actions">
                      <button type="button" aria-label="上移图层" title="上移图层" disabled={index >= template.shared.layers.length - 1} onClick={() => dispatch({ type: "reorderLayer", layerId: layer.id, toIndex: index + 1 })}><ArrowUp aria-hidden="true" size={13} /></button>
                      <button type="button" aria-label="下移图层" title="下移图层" disabled={index <= 0} onClick={() => dispatch({ type: "reorderLayer", layerId: layer.id, toIndex: index - 1 })}><ArrowDown aria-hidden="true" size={13} /></button>
                      <button type="button" aria-label="复制图层" title="复制图层" onClick={() => dispatch({ type: "duplicateLayer", layerId: layer.id, newLayerId: crypto.randomUUID() })}><Copy aria-hidden="true" size={13} /></button>
                      <button type="button" aria-label="删除图层" title="删除图层" onClick={() => dispatch({ type: "deleteLayer", layerId: layer.id })}><Trash2 aria-hidden="true" size={13} /></button>
                    </span>
                  ) : null}
                </div>
              );
            })}
          </div>
        ) : <p className="watermark-inspector-empty">当前模板仅包含边框</p>}
      </div>

      {activeLayer && activeLayout ? (
        <>
          <div className="watermark-inspector-section">
            <strong>{activeLayer.kind === "image" ? "图片内容" : activeLayer.kind === "exifText" ? "EXIF 内容" : "文字内容"}</strong>
            <label className="watermark-text-field"><span>图层名称</span><input value={activeLayer.name} onChange={(event) => updateLayer({ name: event.currentTarget.value })} /></label>
            {activeLayer.kind === "text" ? <TextControls layer={activeLayer} font={font} fonts={fonts} updateLayer={updateLayer} /> : null}
            {activeLayer.kind === "exifText" ? (
              <>
                <div className="watermark-exif-fields">
                  {activeLayer.fields.map((field, index) => <div key={`${field}:${index}`}><span>{EXIF_FIELDS.find(([id]) => id === field)?.[1] ?? field}</span><button type="button" aria-label="上移字段" onClick={() => moveExifField(index, -1)} disabled={index === 0}><ArrowUp aria-hidden="true" size={12} /></button><button type="button" aria-label="下移字段" onClick={() => moveExifField(index, 1)} disabled={index === activeLayer.fields.length - 1}><ArrowDown aria-hidden="true" size={12} /></button><button type="button" aria-label="移除字段" onClick={() => updateLayer({ fields: activeLayer.fields.filter((_, itemIndex) => itemIndex !== index) } as Partial<ExifTextLayer>)}><X aria-hidden="true" size={12} /></button></div>)}
                </div>
                {missingExifFields.length > 0 ? <div className="watermark-exif-add"><select value={newExifField} onChange={(event) => setNewExifField(event.currentTarget.value)}>{missingExifFields.map(([id, label]) => <option value={id} key={id}>{label}</option>)}</select><button type="button" onClick={() => updateLayer({ fields: [...activeLayer.fields, newExifField] } as Partial<ExifTextLayer>)}><Plus aria-hidden="true" size={13} />添加</button></div> : null}
                <label className="watermark-text-field"><span>分隔符</span><input value={activeLayer.separator} onChange={(event) => updateLayer({ separator: event.currentTarget.value } as Partial<ExifTextLayer>)} /></label>
                <div className="watermark-two-fields"><label className="watermark-text-field"><span>前缀</span><input value={activeLayer.prefix} onChange={(event) => updateLayer({ prefix: event.currentTarget.value } as Partial<ExifTextLayer>)} /></label><label className="watermark-text-field"><span>后缀</span><input value={activeLayer.suffix} onChange={(event) => updateLayer({ suffix: event.currentTarget.value } as Partial<ExifTextLayer>)} /></label></div>
                <label className="watermark-text-field"><span>缺失值</span><input value={activeLayer.missingValue ?? ""} placeholder="留空则跳过" onChange={(event) => updateLayer({ missingValue: event.currentTarget.value || null } as Partial<ExifTextLayer>)} /></label>
                <TextStyleControls layer={activeLayer} font={font} fonts={fonts} updateLayer={updateLayer} />
              </>
            ) : null}
            {activeLayer.kind === "image" ? <ImageControls layer={activeLayer} resources={resources} updateLayer={updateLayer} onImport={onAddImage} /> : null}
          </div>

          <div className="watermark-inspector-section">
            <strong>位置</strong>
            <label className="watermark-select-field"><span>锚定区域</span><select value={activeLayout.placement.anchorSpace} onChange={(event) => setPlacement({ anchorSpace: event.currentTarget.value as "photo" | "frame" | "canvas", frameEdge: event.currentTarget.value === "frame" ? activeLayout.placement.frameEdge ?? "bottom" : null })}><option value="photo">照片</option><option value="frame">边框</option><option value="canvas">画布</option></select></label>
            {activeLayout.placement.anchorSpace === "frame" ? <label className="watermark-select-field"><span>边框位置</span><select value={activeLayout.placement.frameEdge ?? "bottom"} onChange={(event) => setPlacement({ frameEdge: event.currentTarget.value as "top" | "right" | "bottom" | "left" })}><option value="top">上边框</option><option value="right">右边框</option><option value="bottom">下边框</option><option value="left">左边框</option></select></label> : null}
            <div className="watermark-two-fields"><NumberField label="X" value={activeLayout.placement.x} min={-1} max={2} step={0.01} onChange={(x) => setPlacement({ x })} /><NumberField label="Y" value={activeLayout.placement.y} min={-1} max={2} step={0.01} onChange={(y) => setPlacement({ y })} /></div>
            <NumberField label="宽度" value={activeLayout.placement.width} min={0.01} max={1} step={0.01} scale={100} suffix="%" onChange={(width) => setPlacement({ width })} />
            {activeLayer.kind !== "image" ? <NumberField label="字号" value={activeLayout.fontSizeRatio ?? 0.04} min={0.005} max={0.5} step={0.005} scale={100} suffix="%" onChange={(fontSizeRatio) => dispatch({ type: "setLayerFontSize", orientation, layerId: activeLayer.id, fontSizeRatio, historyGroup: null })} /> : null}
            <NumberField label="旋转" value={activeLayout.placement.rotationDeg} min={-180} max={180} step={1} suffix="°" onChange={(rotationDeg) => setPlacement({ rotationDeg: normalizeWatermarkRotation(rotationDeg) })} />
            <NumberField label="透明度" value={activeLayout.placement.opacity} min={0} max={1} step={0.01} scale={100} suffix="%" onChange={(opacity) => setPlacement({ opacity })} />
          </div>
        </>
      ) : null}

      <div className="watermark-inspector-section">
        <div className="watermark-section-heading"><button className="watermark-link-button" type="button" aria-label={linkFrame ? "取消联动四边" : "联动四边"} title={linkFrame ? "取消联动四边" : "联动四边"} onClick={() => setLinkFrame((current) => !current)}>{linkFrame ? <Link aria-hidden="true" size={14} /> : <Unlink aria-hidden="true" size={14} />}</button><strong>画布与边框</strong></div>
        <div className="watermark-two-fields"><NumberField label="上" value={variant.frame.top} min={0} max={1} step={0.01} scale={100} suffix="%" onChange={(value) => updateFrame("top", value)} /><NumberField label="右" value={variant.frame.right} min={0} max={1} step={0.01} scale={100} suffix="%" onChange={(value) => updateFrame("right", value)} /><NumberField label="下" value={variant.frame.bottom} min={0} max={1} step={0.01} scale={100} suffix="%" onChange={(value) => updateFrame("bottom", value)} /><NumberField label="左" value={variant.frame.left} min={0} max={1} step={0.01} scale={100} suffix="%" onChange={(value) => updateFrame("left", value)} /></div>
        <label className="watermark-select-field"><span>画布比例</span><select value={variant.canvasRatio ?? "original"} onChange={(event) => dispatch({ type: "setCanvasRatio", orientation, canvasRatio: event.currentTarget.value === "original" ? null : Number(event.currentTarget.value) })}><option value="original">跟随照片</option><option value="1">1:1</option><option value="0.8">4:5</option><option value="1.5">3:2</option><option value="1.7777778">16:9</option></select></label>
        {variant.canvasRatio !== null ? <NumberField label="比例微调" value={variant.canvasRatio} min={0.05} max={20} step={0.01} onChange={(canvasRatio) => dispatch({ type: "setCanvasRatio", orientation, canvasRatio })} /> : null}
      </div>

      <div className="watermark-inspector-section">
        <strong>照片样式</strong>
        <div className="watermark-two-fields"><NumberField label="水平" value={variant.photo.alignX} min={0} max={1} step={0.01} scale={100} suffix="%" onChange={(alignX) => dispatch({ type: "setVariantPhoto", orientation, patch: { alignX } })} /><NumberField label="垂直" value={variant.photo.alignY} min={0} max={1} step={0.01} scale={100} suffix="%" onChange={(alignY) => dispatch({ type: "setVariantPhoto", orientation, patch: { alignY } })} /></div>
        <NumberField label="缩放" value={variant.photo.scale} min={0.01} max={8} step={0.01} scale={100} suffix="%" onChange={(scale) => dispatch({ type: "setVariantPhoto", orientation, patch: { scale } })} />
        <NumberField label="圆角" value={variant.photo.cornerRadiusRatio} min={0} max={0.5} step={0.005} scale={100} suffix="%" onChange={(cornerRadiusRatio) => dispatch({ type: "setVariantPhoto", orientation, patch: { cornerRadiusRatio } })} />
        <NumberField label="描边" value={variant.photo.strokeWidthRatio} min={0} max={0.2} step={0.002} scale={100} suffix="%" onChange={(strokeWidthRatio) => dispatch({ type: "setVariantPhoto", orientation, patch: { strokeWidthRatio } })} />
        <ColorField label="描边颜色" value={variant.photo.strokeColor} onChange={(strokeColor) => dispatch({ type: "setVariantPhoto", orientation, patch: { strokeColor } })} />
        <div className="watermark-two-fields"><NumberField label="阴影模糊" value={variant.photo.shadowBlurRatio} min={0} max={0.5} step={0.005} scale={100} suffix="%" onChange={(shadowBlurRatio) => dispatch({ type: "setVariantPhoto", orientation, patch: { shadowBlurRatio } })} /><NumberField label="阴影透明" value={variant.photo.shadowOpacity} min={0} max={1} step={0.01} scale={100} suffix="%" onChange={(shadowOpacity) => dispatch({ type: "setVariantPhoto", orientation, patch: { shadowOpacity } })} /></div>
        <div className="watermark-two-fields"><NumberField label="阴影 X" value={variant.photo.shadowOffsetXRatio} min={-0.5} max={0.5} step={0.005} scale={100} suffix="%" onChange={(shadowOffsetXRatio) => dispatch({ type: "setVariantPhoto", orientation, patch: { shadowOffsetXRatio } })} /><NumberField label="阴影 Y" value={variant.photo.shadowOffsetYRatio} min={-0.5} max={0.5} step={0.005} scale={100} suffix="%" onChange={(shadowOffsetYRatio) => dispatch({ type: "setVariantPhoto", orientation, patch: { shadowOffsetYRatio } })} /></div>
      </div>

      <div className="watermark-inspector-section">
        <strong>背景</strong>
        <label className="watermark-select-field"><span>类型</span><select value={background.kind} onChange={(event) => {
          const kind = event.currentTarget.value;
          if (kind === "transparent") updateBackground({ kind: "transparent" });
          else if (kind === "solid") updateBackground(solidBackground(background));
          else if (kind === "sampled") updateBackground({ kind: "sampled", x: 0.5, y: 0.5, color: "#ffffff", sampleEachPhoto: true });
          else if (kind === "linearGradient") updateBackground({ kind: "linearGradient", angleDeg: 0, stops: [{ offset: 0, color: "#ffffff", opacity: 1 }, { offset: 1, color: "#dfe5e1", opacity: 1 }] });
          else if (kind === "radialGradient") updateBackground({ kind: "radialGradient", centerX: 0.5, centerY: 0.5, radius: 0.75, stops: [{ offset: 0, color: "#ffffff", opacity: 1 }, { offset: 1, color: "#dfe5e1", opacity: 1 }] });
          else if (kind === "blurredPhoto") updateBackground({ kind: "blurredPhoto", blurRatio: 0.08, scale: 1.1, overlayColor: "#ffffff", overlayOpacity: 0.2 });
          else if (kind === "image" && resources[0]) updateBackground({ kind: "image", resourceId: resources[0].id, fit: "cover", opacity: 1 });
        }}><option value="transparent">透明</option><option value="solid">纯色</option><option value="sampled">照片取色</option><option value="linearGradient">线性渐变</option><option value="radialGradient">径向渐变</option><option value="blurredPhoto">模糊原图</option>{resources.length > 0 ? <option value="image">图片</option> : null}</select></label>
        {background.kind === "solid" ? <><ColorField label="背景颜色" value={background.color} onChange={(color) => updateBackground({ ...background, color })} /><NumberField label="背景透明度" value={background.opacity} min={0} max={1} step={0.01} scale={100} suffix="%" onChange={(opacity) => updateBackground({ ...background, opacity })} /></> : null}
        {background.kind === "sampled" ? <><div className="watermark-two-fields"><NumberField label="取色 X" value={background.x} min={0} max={1} step={0.01} scale={100} suffix="%" onChange={(x) => updateBackground({ ...background, x })} /><NumberField label="取色 Y" value={background.y} min={0} max={1} step={0.01} scale={100} suffix="%" onChange={(y) => updateBackground({ ...background, y })} /></div><ColorField label="回退颜色" value={background.color} onChange={(color) => updateBackground({ ...background, color })} /><label className="watermark-check-field"><input type="checkbox" checked={background.sampleEachPhoto} onChange={(event) => updateBackground({ ...background, sampleEachPhoto: event.currentTarget.checked })} />每张照片单独取色</label></> : null}
        {background.kind === "linearGradient" ? <><NumberField label="角度" value={background.angleDeg} min={-360} max={360} step={1} suffix="°" onChange={(angleDeg) => updateBackground({ ...background, angleDeg })} />{background.stops.map((stop, index) => <div className="watermark-gradient-stop" key={index}><ColorField label={`颜色 ${index + 1}`} value={stop.color} onChange={(color) => updateBackground({ ...background, stops: background.stops.map((item, itemIndex) => itemIndex === index ? { ...item, color } : item) })} /><NumberField label="位置" value={stop.offset} min={0} max={1} step={0.01} scale={100} suffix="%" onChange={(offset) => updateBackground({ ...background, stops: background.stops.map((item, itemIndex) => itemIndex === index ? { ...item, offset } : item) })} /><NumberField label="透明度" value={stop.opacity} min={0} max={1} step={0.01} scale={100} suffix="%" onChange={(opacity) => updateBackground({ ...background, stops: background.stops.map((item, itemIndex) => itemIndex === index ? { ...item, opacity } : item) })} /></div>)}</> : null}
        {background.kind === "radialGradient" ? <><div className="watermark-two-fields"><NumberField label="中心 X" value={background.centerX} min={0} max={1} step={0.01} scale={100} suffix="%" onChange={(centerX) => updateBackground({ ...background, centerX })} /><NumberField label="中心 Y" value={background.centerY} min={0} max={1} step={0.01} scale={100} suffix="%" onChange={(centerY) => updateBackground({ ...background, centerY })} /></div><NumberField label="半径" value={background.radius} min={0.01} max={2} step={0.01} scale={100} suffix="%" onChange={(radius) => updateBackground({ ...background, radius })} />{background.stops.map((stop, index) => <div className="watermark-gradient-stop" key={index}><ColorField label={`颜色 ${index + 1}`} value={stop.color} onChange={(color) => updateBackground({ ...background, stops: background.stops.map((item, itemIndex) => itemIndex === index ? { ...item, color } : item) })} /><NumberField label="位置" value={stop.offset} min={0} max={1} step={0.01} scale={100} suffix="%" onChange={(offset) => updateBackground({ ...background, stops: background.stops.map((item, itemIndex) => itemIndex === index ? { ...item, offset } : item) })} /><NumberField label="透明度" value={stop.opacity} min={0} max={1} step={0.01} scale={100} suffix="%" onChange={(opacity) => updateBackground({ ...background, stops: background.stops.map((item, itemIndex) => itemIndex === index ? { ...item, opacity } : item) })} /></div>)}</> : null}
        {background.kind === "blurredPhoto" ? <><NumberField label="模糊" value={background.blurRatio} min={0} max={0.5} step={0.01} scale={100} suffix="%" onChange={(blurRatio) => updateBackground({ ...background, blurRatio })} /><NumberField label="铺满缩放" value={background.scale} min={1} max={3} step={0.01} scale={100} suffix="%" onChange={(scale) => updateBackground({ ...background, scale })} /><ColorField label="叠加颜色" value={background.overlayColor} onChange={(overlayColor) => updateBackground({ ...background, overlayColor })} /><NumberField label="叠加透明度" value={background.overlayOpacity} min={0} max={1} step={0.01} scale={100} suffix="%" onChange={(overlayOpacity) => updateBackground({ ...background, overlayOpacity })} /></> : null}
        {background.kind === "image" ? <><label className="watermark-select-field"><span>图片资源</span><select value={background.resourceId} onChange={(event) => updateBackground({ ...background, resourceId: event.currentTarget.value })}>{resources.map((resource) => <option value={resource.id} key={resource.id}>{resource.name}</option>)}</select></label><div className="watermark-align-switch" role="group" aria-label="背景图片填充"><button type="button" aria-pressed={background.fit === "contain"} onClick={() => updateBackground({ ...background, fit: "contain" })}>完整显示</button><button type="button" aria-pressed={background.fit === "cover"} onClick={() => updateBackground({ ...background, fit: "cover" })}>铺满裁切</button></div><NumberField label="透明度" value={background.opacity} min={0} max={1} step={0.01} scale={100} suffix="%" onChange={(opacity) => updateBackground({ ...background, opacity })} /></> : null}
      </div>

      {photoId ? (
        <div className="watermark-inspector-section">
          <strong>单张照片调整</strong>
          <div className="watermark-two-fields"><NumberField label="水平" value={photoOverride?.alignX ?? variant.photo.alignX} min={0} max={1} step={0.01} scale={100} suffix="%" onChange={(alignX) => dispatch({ type: "setPhotoOverride", photoId, patch: { alignX }, historyGroup: null })} /><NumberField label="垂直" value={photoOverride?.alignY ?? variant.photo.alignY} min={0} max={1} step={0.01} scale={100} suffix="%" onChange={(alignY) => dispatch({ type: "setPhotoOverride", photoId, patch: { alignY }, historyGroup: null })} /></div>
          <NumberField label="缩放" value={photoOverride?.scale ?? variant.photo.scale} min={0.01} max={8} step={0.01} scale={100} suffix="%" onChange={(scale) => dispatch({ type: "setPhotoOverride", photoId, patch: { scale }, historyGroup: null })} />
          <button className="watermark-clear-override" type="button" onClick={() => dispatch({ type: "clearPhotoOverride", photoId })} disabled={!photoOverride}>清除单张调整</button>
        </div>
      ) : null}

      <div className="watermark-inspector-section watermark-export-shortcut">
        <div className="watermark-section-heading"><Download aria-hidden="true" size={15} /><strong>导出设置</strong></div>
        <dl>
          <div><dt>格式</dt><dd>{outputSettings.format === "jpeg" ? `JPEG ${outputSettings.jpegQuality}` : outputSettings.transparentBackground ? "PNG 透明" : "PNG"}</dd></div>
          <div><dt>尺寸</dt><dd>{outputSettings.sizing.kind === "original" ? "原始尺寸" : `长边 ${outputSettings.sizing.pixels}px`}</dd></div>
          <div><dt>元数据</dt><dd>{outputSettings.metadataPolicy === "privacy" ? "隐私模式" : outputSettings.metadataPolicy === "preserve" ? "完整保留" : "全部移除"}</dd></div>
        </dl>
        <button type="button" className="primary-command" onClick={onOpenExport} disabled={exportDisabled}><Download aria-hidden="true" size={15} />确认并导出</button>
      </div>
    </section>
  );
}

function TextControls({ layer, font, fonts, updateLayer }: { layer: TextLayer; font: WatermarkFontSummary | null; fonts: WatermarkFontSummary[]; updateLayer: (patch: Partial<WatermarkLayer>) => void }) {
  return <><label className="watermark-text-field"><span>文字</span><textarea rows={3} value={layer.text} onChange={(event) => updateLayer({ text: event.currentTarget.value } as Partial<TextLayer>)} /></label><TextStyleControls layer={layer} font={font} fonts={fonts} updateLayer={updateLayer} /></>;
}

function TextStyleControls({ layer, font, fonts, updateLayer }: { layer: TextLayer | ExifTextLayer; font: WatermarkFontSummary | null; fonts: WatermarkFontSummary[]; updateLayer: (patch: Partial<WatermarkLayer>) => void }) {
  return <><label className="watermark-select-field"><span>字体</span><select value={layer.fontFamily} onChange={(event) => updateLayer({ fontFamily: event.currentTarget.value } as Partial<TextLayer>)}>{fonts.length > 0 ? fonts.map((item) => <option value={item.family} key={item.family}>{item.family}{item.bundled ? "（内置）" : ""}</option>) : <option value={layer.fontFamily}>{layer.fontFamily}</option>}</select></label><label className="watermark-select-field"><span>字重</span><select value={layer.fontWeight} onChange={(event) => updateLayer({ fontWeight: Number(event.currentTarget.value) } as Partial<TextLayer>)}>{(font?.weights.length ? font.weights : [300, 400, 500, 600, 700]).map((weight) => <option value={weight} key={weight}>{weight}</option>)}</select></label><div className="watermark-align-switch" role="group" aria-label="文字对齐">{(["left", "center", "right"] as const).map((align) => <button type="button" key={align} aria-pressed={layer.align === align} onClick={() => updateLayer({ align } as Partial<TextLayer>)}>{align === "left" ? "左" : align === "center" ? "中" : "右"}</button>)}</div><ColorField label="文字颜色" value={layer.color} onChange={(color) => updateLayer({ color } as Partial<TextLayer>)} /><div className="watermark-two-fields"><NumberField label="字距" value={layer.letterSpacingRatio} min={-0.2} max={1} step={0.01} scale={100} suffix="%" onChange={(letterSpacingRatio) => updateLayer({ letterSpacingRatio } as Partial<TextLayer>)} /><NumberField label="行高" value={layer.lineHeight} min={0.5} max={3} step={0.05} onChange={(lineHeight) => updateLayer({ lineHeight } as Partial<TextLayer>)} /></div><ColorField label="描边颜色" value={layer.strokeColor} onChange={(strokeColor) => updateLayer({ strokeColor } as Partial<TextLayer>)} /><NumberField label="描边宽度" value={layer.strokeWidthRatio} min={0} max={0.1} step={0.002} scale={100} suffix="%" onChange={(strokeWidthRatio) => updateLayer({ strokeWidthRatio } as Partial<TextLayer>)} /><ColorField label="阴影颜色" value={layer.shadowColor} onChange={(shadowColor) => updateLayer({ shadowColor } as Partial<TextLayer>)} /><div className="watermark-two-fields"><NumberField label="阴影模糊" value={layer.shadowBlurRatio} min={0} max={0.2} step={0.002} scale={100} suffix="%" onChange={(shadowBlurRatio) => updateLayer({ shadowBlurRatio } as Partial<TextLayer>)} /><NumberField label="阴影 X" value={layer.shadowOffsetXRatio} min={-0.2} max={0.2} step={0.002} scale={100} suffix="%" onChange={(shadowOffsetXRatio) => updateLayer({ shadowOffsetXRatio } as Partial<TextLayer>)} /><NumberField label="阴影 Y" value={layer.shadowOffsetYRatio} min={-0.2} max={0.2} step={0.002} scale={100} suffix="%" onChange={(shadowOffsetYRatio) => updateLayer({ shadowOffsetYRatio } as Partial<TextLayer>)} /></div></>;
}

function ImageControls({ layer, resources, updateLayer, onImport }: { layer: ImageLayer; resources: Array<{ id: string; name: string }>; updateLayer: (patch: Partial<WatermarkLayer>) => void; onImport: () => void }) {
  return <><label className="watermark-select-field"><span>图片资源</span><select value={layer.resourceId} onChange={(event) => updateLayer({ resourceId: event.currentTarget.value } as Partial<ImageLayer>)}>{resources.map((resource) => <option value={resource.id} key={resource.id}>{resource.name}</option>)}</select></label><button className="watermark-import-resource" type="button" onClick={onImport}><Plus aria-hidden="true" size={13} />导入新图片</button><div className="watermark-align-switch" role="group" aria-label="图片填充方式"><button type="button" aria-pressed={layer.fit === "contain"} onClick={() => updateLayer({ fit: "contain" } as Partial<ImageLayer>)}>完整显示</button><button type="button" aria-pressed={layer.fit === "cover"} onClick={() => updateLayer({ fit: "cover" } as Partial<ImageLayer>)}>铺满裁切</button></div></>;
}

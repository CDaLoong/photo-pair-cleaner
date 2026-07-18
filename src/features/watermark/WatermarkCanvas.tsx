import {
  ChevronLeft,
  ChevronRight,
  ImageOff,
  LoaderCircle,
  RotateCw,
  ShieldCheck,
  TriangleAlert,
  ZoomIn,
  ZoomOut,
} from "lucide-react";
import { useEffect, useMemo, useRef, useState } from "react";
import type { KeyboardEvent, PointerEvent as ReactPointerEvent } from "react";
import type {
  NormalizedPlacement,
  WatermarkOrientation,
  WatermarkSourcePhoto,
  WatermarkTemplate,
} from "./types";
import {
  canManipulateWatermarkLayer,
  clampWatermarkNumber,
  keyboardNudgeNormalized,
  normalizeWatermarkRotation,
  snapNormalizedPosition,
  viewportDeltaToNormalized,
} from "./watermarkUtils";
import type {
  WatermarkPreviewLayerGeometry,
  WatermarkPreviewResult,
} from "./watermarkPreviewCache";

interface WatermarkCanvasProps {
  photo: WatermarkSourcePhoto | null;
  preview: WatermarkPreviewResult | null;
  template: WatermarkTemplate;
  orientation: WatermarkOrientation;
  activeLayerId: string | null;
  loading: boolean;
  error: string | null;
  originalUrl: string | null;
  compareOriginal: boolean;
  position: number;
  total: number;
  onPrevious: () => void;
  onNext: () => void;
  onSelectLayer: (layerId: string) => void;
  onSetLayerPlacement: (
    layerId: string,
    patch: Partial<NormalizedPlacement>,
    historyGroup: string | null,
  ) => void;
  onCloseHistoryGroup: () => void;
}

type DragMode = "move" | "scale" | "rotate";

interface DragSession {
  pointerId: number;
  mode: DragMode;
  layerId: string;
  group: string;
  startX: number;
  startY: number;
  initial: NormalizedPlacement;
  geometry: WatermarkPreviewLayerGeometry;
  cornerX: -1 | 1;
  cornerY: -1 | 1;
  startAngle: number;
}

interface ViewportSize { width: number; height: number }

const ZOOM_LEVELS = [0.5, 0.75, 1, 1.5, 2, 3, 4];

function sameAnchor(left: WatermarkPreviewLayerGeometry, right: WatermarkPreviewLayerGeometry): boolean {
  return left.anchorRect.x === right.anchorRect.x
    && left.anchorRect.y === right.anchorRect.y
    && left.anchorRect.width === right.anchorRect.width
    && left.anchorRect.height === right.anchorRect.height;
}

export function WatermarkCanvas({
  photo,
  preview,
  template,
  orientation,
  activeLayerId,
  loading,
  error,
  originalUrl,
  compareOriginal,
  position,
  total,
  onPrevious,
  onNext,
  onSelectLayer,
  onSetLayerPlacement,
  onCloseHistoryGroup,
}: WatermarkCanvasProps) {
  const viewportRef = useRef<HTMLDivElement>(null);
  const artboardRef = useRef<HTMLDivElement>(null);
  const dragRef = useRef<DragSession | null>(null);
  const [viewportSize, setViewportSize] = useState<ViewportSize>({ width: 0, height: 0 });
  const [zoom, setZoom] = useState(1);
  const [guides, setGuides] = useState<string[]>([]);
  const canGoPrevious = position > 1;
  const canGoNext = position > 0 && position < total;
  const showingOriginal = compareOriginal && originalUrl;
  const variant = template.variants[orientation];

  useEffect(() => {
    const viewport = viewportRef.current;
    if (!viewport) return;
    const update = () => setViewportSize({ width: viewport.clientWidth, height: viewport.clientHeight });
    update();
    const observer = new ResizeObserver(update);
    observer.observe(viewport);
    return () => observer.disconnect();
  }, []);

  const artboardSize = useMemo(() => {
    if (!preview || viewportSize.width <= 0 || viewportSize.height <= 0) return null;
    const fit = Math.min(
      Math.max(1, viewportSize.width - 36) / preview.width,
      Math.max(1, viewportSize.height - 36) / preview.height,
    );
    return {
      width: Math.max(1, preview.width * fit * zoom),
      height: Math.max(1, preview.height * fit * zoom),
    };
  }, [preview, viewportSize, zoom]);

  function zoomBy(direction: -1 | 1) {
    const currentIndex = ZOOM_LEVELS.findIndex((level) => level >= zoom - 0.001);
    const index = Math.max(0, Math.min(ZOOM_LEVELS.length - 1, currentIndex + direction));
    setZoom(ZOOM_LEVELS[index]);
  }

  function activePlacement(layerId: string): NormalizedPlacement | null {
    return variant.layerLayouts[layerId]?.placement ?? null;
  }

  function layerCanMove(layerId: string): boolean {
    const layer = template.shared.layers.find((candidate) => candidate.id === layerId);
    return Boolean(layer && canManipulateWatermarkLayer(layer));
  }

  function beginDrag(
    event: ReactPointerEvent<HTMLElement>,
    geometry: WatermarkPreviewLayerGeometry,
    mode: DragMode,
    cornerX: -1 | 1 = 1,
    cornerY: -1 | 1 = 1,
  ) {
    const initial = activePlacement(geometry.id);
    if (!initial || !layerCanMove(geometry.id) || !artboardRef.current) return;
    event.preventDefault();
    event.stopPropagation();
    onSelectLayer(geometry.id);
    event.currentTarget.setPointerCapture(event.pointerId);
    const artboard = artboardRef.current.getBoundingClientRect();
    const centerX = artboard.left + (geometry.centerX / (preview?.width ?? 1)) * artboard.width;
    const centerY = artboard.top + (geometry.centerY / (preview?.height ?? 1)) * artboard.height;
    dragRef.current = {
      pointerId: event.pointerId,
      mode,
      layerId: geometry.id,
      group: `pointer-${geometry.id}-${event.pointerId}-${Date.now()}`,
      startX: event.clientX,
      startY: event.clientY,
      initial: { ...initial },
      geometry,
      cornerX,
      cornerY,
      startAngle: Math.atan2(event.clientY - centerY, event.clientX - centerX) * 180 / Math.PI,
    };
  }

  function moveDrag(event: ReactPointerEvent<HTMLElement>) {
    const session = dragRef.current;
    const artboard = artboardRef.current?.getBoundingClientRect();
    if (!session || !artboard || session.pointerId !== event.pointerId || !preview) return;
    const anchorSize = {
      width: session.geometry.anchorRect.width / preview.width * artboard.width,
      height: session.geometry.anchorRect.height / preview.height * artboard.height,
    };
    if (session.mode === "move") {
      const delta = viewportDeltaToNormalized({
        dx: event.clientX - session.startX,
        dy: event.clientY - session.startY,
      }, anchorSize);
      const peers = preview.layers
        .filter((peer) => peer.id !== session.layerId && sameAnchor(peer, session.geometry))
        .map((peer) => ({
          id: peer.id,
          x: (peer.centerX - peer.anchorRect.x) / peer.anchorRect.width,
          y: (peer.centerY - peer.anchorRect.y) / peer.anchorRect.height,
        }));
      const snapped = snapNormalizedPosition({
        position: {
          x: clampWatermarkNumber(session.initial.x + delta.x, -1, 2, session.initial.x),
          y: clampWatermarkNumber(session.initial.y + delta.y, -1, 2, session.initial.y),
        },
        anchorSize,
        layerSize: {
          width: session.geometry.width / session.geometry.anchorRect.width,
          height: session.geometry.height / session.geometry.anchorRect.height,
        },
        peers,
        thresholdPx: 6,
        bypass: event.shiftKey,
      });
      setGuides(snapped.guides);
      onSetLayerPlacement(session.layerId, snapped.position, session.group);
      return;
    }
    if (session.mode === "scale") {
      const horizontal = session.cornerX * (event.clientX - session.startX) / anchorSize.width;
      const vertical = session.cornerY * (event.clientY - session.startY) / anchorSize.height;
      const width = clampWatermarkNumber(session.initial.width + (horizontal + vertical) / 2, 0.01, 1, session.initial.width);
      onSetLayerPlacement(session.layerId, { width }, session.group);
      return;
    }
    const centerX = artboard.left + session.geometry.centerX / preview.width * artboard.width;
    const centerY = artboard.top + session.geometry.centerY / preview.height * artboard.height;
    const angle = Math.atan2(event.clientY - centerY, event.clientX - centerX) * 180 / Math.PI;
    onSetLayerPlacement(session.layerId, {
      rotationDeg: normalizeWatermarkRotation(session.initial.rotationDeg + angle - session.startAngle),
    }, session.group);
  }

  function endDrag(event: ReactPointerEvent<HTMLElement>) {
    const session = dragRef.current;
    if (!session || session.pointerId !== event.pointerId) return;
    dragRef.current = null;
    setGuides([]);
    onCloseHistoryGroup();
  }

  function nudgeLayer(event: KeyboardEvent<HTMLDivElement>, geometry: WatermarkPreviewLayerGeometry) {
    if (!(["ArrowLeft", "ArrowRight", "ArrowUp", "ArrowDown"] as string[]).includes(event.key)) return;
    if (!layerCanMove(geometry.id) || !preview || !artboardSize) return;
    event.preventDefault();
    event.stopPropagation();
    const placement = activePlacement(geometry.id);
    if (!placement) return;
    const position = keyboardNudgeNormalized(
      placement,
      event.key as "ArrowLeft" | "ArrowRight" | "ArrowUp" | "ArrowDown",
      {
        width: geometry.anchorRect.width / preview.width * artboardSize.width,
        height: geometry.anchorRect.height / preview.height * artboardSize.height,
      },
      event.shiftKey,
    );
    onSetLayerPlacement(geometry.id, {
      x: clampWatermarkNumber(position.x, -1, 2, placement.x),
      y: clampWatermarkNumber(position.y, -1, 2, placement.y),
    }, null);
  }

  function guideStyle(guide: string): React.CSSProperties | null {
    if (!preview) return null;
    const active = preview.layers.find((layer) => layer.id === activeLayerId);
    if (!active) return null;
    const anchor = active.anchorRect;
    let axis: "x" | "y";
    let coordinate: number;
    if (guide === "anchor-left") { axis = "x"; coordinate = anchor.x; }
    else if (guide === "anchor-center-x") { axis = "x"; coordinate = anchor.x + anchor.width / 2; }
    else if (guide === "anchor-right") { axis = "x"; coordinate = anchor.x + anchor.width; }
    else if (guide === "anchor-top") { axis = "y"; coordinate = anchor.y; }
    else if (guide === "anchor-center-y") { axis = "y"; coordinate = anchor.y + anchor.height / 2; }
    else if (guide === "anchor-bottom") { axis = "y"; coordinate = anchor.y + anchor.height; }
    else {
      const [kind, peerId] = guide.split(":");
      const peer = preview.layers.find((layer) => layer.id === peerId);
      if (!peer) return null;
      axis = kind === "peer-x" ? "x" : "y";
      coordinate = axis === "x" ? peer.centerX : peer.centerY;
    }
    return axis === "x"
      ? { left: `${coordinate / preview.width * 100}%` }
      : { top: `${coordinate / preview.height * 100}%` };
  }

  return (
    <section className="watermark-canvas-panel" aria-label="水印效果预览">
      <header className="watermark-canvas-toolbar">
        <div className="watermark-canvas-navigation">
          <button type="button" onClick={onPrevious} disabled={!canGoPrevious} aria-label="上一张照片" title="上一张照片"><ChevronLeft aria-hidden="true" size={18} /></button>
          <button type="button" onClick={onNext} disabled={!canGoNext} aria-label="下一张照片" title="下一张照片"><ChevronRight aria-hidden="true" size={18} /></button>
        </div>
        <div className="watermark-canvas-title">
          <strong>{photo?.fileName ?? "尚未选择照片"}</strong>
          {photo ? <span>{photo.pixelWidth} x {photo.pixelHeight}</span> : null}
        </div>
        <div className="watermark-canvas-tools">
          <button type="button" aria-label="缩小画布" title="缩小画布" onClick={() => zoomBy(-1)} disabled={zoom <= ZOOM_LEVELS[0]}><ZoomOut aria-hidden="true" size={15} /></button>
          <button type="button" className="watermark-zoom-value" onClick={() => setZoom(1)} title="恢复 100%">{Math.round(zoom * 100)}%</button>
          <button type="button" aria-label="放大画布" title="放大画布" onClick={() => zoomBy(1)} disabled={zoom >= ZOOM_LEVELS.at(-1)!}><ZoomIn aria-hidden="true" size={15} /></button>
          <span>{position > 0 ? position : 0} / {total}</span>
        </div>
      </header>

      <div ref={viewportRef} className="watermark-canvas-viewport is-scrollable">
        {preview && artboardSize ? (
          <div className="watermark-canvas-scroll-content" style={{ minWidth: artboardSize.width + 36, minHeight: artboardSize.height + 36 }}>
            <div ref={artboardRef} className="watermark-canvas-artboard" style={{ width: artboardSize.width, height: artboardSize.height }}>
              <img
                src={showingOriginal ? originalUrl : preview.url}
                alt={photo ? `${photo.fileName} 的${showingOriginal ? "原图" : "水印"}预览` : "水印预览"}
                draggable={false}
              />
              {!showingOriginal ? preview.layers.map((geometry) => {
                const active = geometry.id === activeLayerId;
                const movable = layerCanMove(geometry.id);
                return (
                  <div
                    className={`watermark-layer-box${active ? " is-active" : ""}${movable ? "" : " is-locked"}`}
                    style={{
                      left: `${geometry.centerX / preview.width * 100}%`,
                      top: `${geometry.centerY / preview.height * 100}%`,
                      width: `${geometry.width / preview.width * 100}%`,
                      height: `${geometry.height / preview.height * 100}%`,
                      transform: `translate(-50%, -50%) rotate(${geometry.rotationDeg}deg)`,
                    }}
                    key={geometry.id}
                    role="button"
                    tabIndex={active ? 0 : -1}
                    aria-label={`编辑图层 ${template.shared.layers.find((layer) => layer.id === geometry.id)?.name ?? geometry.id}`}
                    onPointerDown={(event) => beginDrag(event, geometry, "move")}
                    onPointerMove={moveDrag}
                    onPointerUp={endDrag}
                    onPointerCancel={endDrag}
                    onKeyDown={(event) => nudgeLayer(event, geometry)}
                  >
                    {active && movable ? (
                      <>
                        {([[-1, -1], [1, -1], [-1, 1], [1, 1]] as Array<[-1 | 1, -1 | 1]>).map(([x, y]) => (
                          <span
                            className={`watermark-scale-handle is-x-${x} is-y-${y}`}
                            key={`${x}:${y}`}
                            onPointerDown={(event) => beginDrag(event, geometry, "scale", x, y)}
                          />
                        ))}
                        <span className="watermark-rotate-stem" aria-hidden="true" />
                        <span className="watermark-rotate-handle" onPointerDown={(event) => beginDrag(event, geometry, "rotate")} title="旋转图层"><RotateCw aria-hidden="true" size={11} /></span>
                      </>
                    ) : null}
                  </div>
                );
              }) : null}
              {!showingOriginal ? guides.map((guide) => {
                const style = guideStyle(guide);
                return style ? <span className={guide.includes("x") || guide.includes("left") || guide.includes("right") ? "watermark-snap-guide is-vertical" : "watermark-snap-guide is-horizontal"} style={style} key={guide} /> : null;
              }) : null}
            </div>
          </div>
        ) : error ? (
          <div className="watermark-canvas-message is-error"><TriangleAlert aria-hidden="true" size={28} /><strong>无法生成预览</strong><span>{error}</span></div>
        ) : (
          <div className="watermark-canvas-message">
            {loading ? <LoaderCircle className="spin" aria-hidden="true" size={28} /> : <ImageOff aria-hidden="true" size={28} />}
            <strong>{loading ? "正在生成预览" : "请选择一张照片"}</strong>
          </div>
        )}
        {loading && preview ? <div className="watermark-canvas-loading" role="status"><LoaderCircle className="spin" aria-hidden="true" size={14} />更新预览</div> : null}
      </div>

      {showingOriginal ? (
        <footer className="watermark-canvas-ready"><ShieldCheck aria-hidden="true" size={14} />正在对比只读原图</footer>
      ) : error && preview ? (
        <footer className="watermark-canvas-warning" role="alert"><TriangleAlert aria-hidden="true" size={14} /><span>{error}</span></footer>
      ) : preview?.warnings.length ? (
        <footer className="watermark-canvas-warning" role="status"><TriangleAlert aria-hidden="true" size={14} /><span>{preview.warnings.join("；")}</span></footer>
      ) : (
        <footer className="watermark-canvas-ready"><ShieldCheck aria-hidden="true" size={14} />预览不会修改原始照片</footer>
      )}
    </section>
  );
}

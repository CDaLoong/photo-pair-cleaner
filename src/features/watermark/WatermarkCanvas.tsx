import {
  ChevronLeft,
  ChevronRight,
  ImageOff,
  LoaderCircle,
  ShieldCheck,
  TriangleAlert,
} from "lucide-react";
import type { WatermarkSourcePhoto } from "./types";
import type { WatermarkPreviewResult } from "./watermarkPreviewCache";

interface WatermarkCanvasProps {
  photo: WatermarkSourcePhoto | null;
  preview: WatermarkPreviewResult | null;
  loading: boolean;
  error: string | null;
  position: number;
  total: number;
  onPrevious: () => void;
  onNext: () => void;
}

export function WatermarkCanvas({
  photo,
  preview,
  loading,
  error,
  position,
  total,
  onPrevious,
  onNext,
}: WatermarkCanvasProps) {
  const canGoPrevious = position > 1;
  const canGoNext = position > 0 && position < total;
  return (
    <section className="watermark-canvas-panel" aria-label="水印效果预览">
      <header className="watermark-canvas-toolbar">
        <div className="watermark-canvas-navigation">
          <button type="button" onClick={onPrevious} disabled={!canGoPrevious} title="上一张照片">
            <ChevronLeft aria-hidden="true" size={18} />
          </button>
          <button type="button" onClick={onNext} disabled={!canGoNext} title="下一张照片">
            <ChevronRight aria-hidden="true" size={18} />
          </button>
        </div>
        <div className="watermark-canvas-title">
          <strong>{photo?.fileName ?? "尚未选择照片"}</strong>
          {photo ? <span>{photo.pixelWidth} x {photo.pixelHeight}</span> : null}
        </div>
        <div className="watermark-canvas-position">
          <span>{position > 0 ? position : 0} / {total}</span>
          <small><ShieldCheck aria-hidden="true" size={13} />只读来源</small>
        </div>
      </header>

      <div className="watermark-canvas-viewport">
        {preview ? (
          <img
            src={preview.url}
            alt={photo ? `${photo.fileName} 的水印预览` : "水印预览"}
            width={preview.width}
            height={preview.height}
            draggable={false}
          />
        ) : error ? (
          <div className="watermark-canvas-message is-error">
            <TriangleAlert aria-hidden="true" size={28} />
            <strong>无法生成预览</strong>
            <span>{error}</span>
          </div>
        ) : (
          <div className="watermark-canvas-message">
            {loading
              ? <LoaderCircle className="spin" aria-hidden="true" size={28} />
              : <ImageOff aria-hidden="true" size={28} />}
            <strong>{loading ? "正在生成预览" : "请选择一张照片"}</strong>
          </div>
        )}
        {loading && preview ? (
          <div className="watermark-canvas-loading" role="status">
            <LoaderCircle className="spin" aria-hidden="true" size={14} />更新预览
          </div>
        ) : null}
      </div>

      {error && preview ? (
        <footer className="watermark-canvas-warning" role="alert">
          <TriangleAlert aria-hidden="true" size={14} />
          <span>{error}</span>
        </footer>
      ) : preview?.warnings.length ? (
        <footer className="watermark-canvas-warning" role="status">
          <TriangleAlert aria-hidden="true" size={14} />
          <span>{preview.warnings.join("；")}</span>
        </footer>
      ) : (
        <footer className="watermark-canvas-ready">
          <ShieldCheck aria-hidden="true" size={14} />预览不会修改原始照片
        </footer>
      )}
    </section>
  );
}

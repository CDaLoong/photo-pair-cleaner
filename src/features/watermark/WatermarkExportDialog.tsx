import {
  AlertTriangle,
  CheckCircle2,
  Download,
  FolderOpen,
  Image as ImageIcon,
  LoaderCircle,
  RefreshCw,
  X,
  XCircle,
} from "lucide-react";
import { useEffect, useMemo, useRef } from "react";
import { formatBytes } from "../../utils";
import type {
  WatermarkExportProgress,
} from "./watermarkUtils";
import {
  estimateWatermarkOutputBytes,
  failedWatermarkPhotoIds,
  validateWatermarkOutputSettings,
  watermarkFilenameExamples,
} from "./watermarkUtils";
import type {
  WatermarkOutputSettings,
  WatermarkSourceSnapshot,
} from "./types";

interface WatermarkExportDialogProps {
  open: boolean;
  snapshot: WatermarkSourceSnapshot;
  settings: WatermarkOutputSettings;
  progress: WatermarkExportProgress;
  error: string | null;
  blockingError: string | null;
  onSettingsChange: (settings: WatermarkOutputSettings) => void;
  onChooseDirectory: () => void;
  onStart: () => void;
  onCancel: () => void;
  onRetry: () => void;
  onReveal: () => void;
  onClose: () => void;
}

const METADATA_LABELS = {
  preserve: "完整保留",
  privacy: "隐私模式",
  remove: "全部移除",
} as const;

const COLLISION_LABELS = {
  sequence: "自动添加序号",
  skip: "跳过同名文件",
  overwriteOutput: "仅覆盖 FramePair 副本",
} as const;

function statusLabel(status: "succeeded" | "skipped" | "failed") {
  return status === "succeeded" ? "已完成" : status === "skipped" ? "已跳过" : "失败";
}

export function WatermarkExportDialog({
  open,
  snapshot,
  settings,
  progress,
  error,
  blockingError,
  onSettingsChange,
  onChooseDirectory,
  onStart,
  onCancel,
  onRetry,
  onReveal,
  onClose,
}: WatermarkExportDialogProps) {
  const dialogRef = useRef<HTMLDialogElement>(null);
  const validationError = blockingError ?? validateWatermarkOutputSettings(settings, snapshot);
  const examples = useMemo(
    () => watermarkFilenameExamples(snapshot, settings),
    [settings, snapshot],
  );
  const estimate = useMemo(
    () => estimateWatermarkOutputBytes(snapshot, settings),
    [settings, snapshot],
  );
  const failedIds = failedWatermarkPhotoIds(progress);
  const longEdgeSizing = settings.sizing.kind === "longEdge" ? settings.sizing : null;
  const runningResults = progress.phase === "running" ? progress.attemptResults : progress.results;
  const finishedCount = runningResults.length;
  const succeeded = runningResults.filter((item) => item.status === "succeeded").length;
  const skipped = runningResults.filter((item) => item.status === "skipped").length;
  const failed = runningResults.filter((item) => item.status === "failed").length;
  const percent = progress.total > 0 ? Math.min(100, Math.round(finishedCount / progress.total * 100)) : 0;

  useEffect(() => {
    const dialog = dialogRef.current;
    if (!dialog) return;
    if (open && !dialog.open) dialog.showModal();
    if (!open && dialog.open) dialog.close();
  }, [open]);

  function update(patch: Partial<WatermarkOutputSettings>) {
    onSettingsChange({ ...settings, ...patch });
  }

  return (
    <dialog
      ref={dialogRef}
      className="confirm-dialog watermark-export-dialog"
      aria-labelledby="watermark-export-title"
      onCancel={(event) => {
        event.preventDefault();
        onClose();
      }}
    >
      <div className="dialog-header">
        <div className="dialog-icon watermark-export-icon"><Download aria-hidden="true" size={20} /></div>
        <div>
          <h2 id="watermark-export-title">{progress.phase === "results" ? "水印导出结果" : progress.phase === "running" ? "正在导出水印副本" : "导出设置"}</h2>
          <p>{progress.phase === "idle" ? "确认格式、尺寸、目录和同名处理后生成新副本。" : progress.phase === "running" ? "正在本机处理，已完成的副本会保留。" : "原照片未修改，可以查看输出或重试失败项。"}</p>
        </div>
        <button className="icon-button" type="button" onClick={onClose} aria-label="关闭水印导出" title="关闭"><X aria-hidden="true" size={18} /></button>
      </div>

      {progress.phase === "idle" ? (
        <div className="watermark-export-confirm">
          <div className="watermark-export-settings">
            <section>
              <strong>文件格式</strong>
              <div className="watermark-export-segmented" role="group" aria-label="文件格式">
                <button type="button" aria-pressed={settings.format === "jpeg"} onClick={() => update({ format: "jpeg", transparentBackground: false })}>JPEG</button>
                <button type="button" aria-pressed={settings.format === "png"} onClick={() => update({ format: "png" })}>PNG</button>
              </div>
              {settings.format === "jpeg" ? (
                <label><span>JPEG 质量</span><span className="watermark-export-inline"><input type="range" min="1" max="100" value={settings.jpegQuality} onChange={(event) => update({ jpegQuality: Number(event.currentTarget.value) })} /><input aria-label="JPEG 质量数值" type="number" min="1" max="100" value={settings.jpegQuality} onChange={(event) => update({ jpegQuality: event.currentTarget.valueAsNumber })} /></span></label>
              ) : (
                <label className="watermark-export-check"><input type="checkbox" checked={settings.transparentBackground} onChange={(event) => update({ transparentBackground: event.currentTarget.checked })} /><span>保留透明背景</span></label>
              )}
            </section>

            <section>
              <strong>输出尺寸</strong>
              <div className="watermark-export-segmented" role="group" aria-label="输出尺寸">
                <button type="button" aria-pressed={settings.sizing.kind === "original"} onClick={() => update({ sizing: { kind: "original", allowUpscale: false } })}>原始尺寸</button>
                <button type="button" aria-pressed={settings.sizing.kind === "longEdge"} onClick={() => update({ sizing: { kind: "longEdge", pixels: 2400, allowUpscale: false } })}>限制长边</button>
              </div>
              {longEdgeSizing ? (
                <>
                  <label><span>长边像素</span><input type="number" min="64" max="32768" value={longEdgeSizing.pixels} onChange={(event) => update({ sizing: { kind: "longEdge", pixels: event.currentTarget.valueAsNumber, allowUpscale: longEdgeSizing.allowUpscale } })} /></label>
                  <label className="watermark-export-check"><input type="checkbox" checked={longEdgeSizing.allowUpscale} onChange={(event) => update({ sizing: { kind: "longEdge", pixels: longEdgeSizing.pixels, allowUpscale: event.currentTarget.checked } })} /><span>允许放大小尺寸照片</span></label>
                </>
              ) : null}
            </section>

            <section>
              <strong>颜色与元数据</strong>
              <label><span>颜色空间</span><select value={settings.colorSpace} onChange={(event) => update({ colorSpace: event.currentTarget.value as "srgb" | "preserve" })}><option value="srgb">转换为 sRGB</option><option value="preserve">兼容时保留来源 ICC</option></select></label>
              {settings.format === "jpeg" ? <label><span>透明区域铺底</span><input type="color" value={settings.jpegFlattenColor} onChange={(event) => update({ jpegFlattenColor: event.currentTarget.value })} /></label> : null}
              <label><span>元数据</span><select value={settings.metadataPolicy} onChange={(event) => update({ metadataPolicy: event.currentTarget.value as WatermarkOutputSettings["metadataPolicy"] })}><option value="privacy">隐私模式：移除 GPS 和序列号</option><option value="preserve">完整保留并修正尺寸</option><option value="remove">全部移除</option></select></label>
            </section>

            <section>
              <strong>文件名与同名文件</strong>
              <label><span>文件名后缀</span><input value={settings.suffix} maxLength={120} onChange={(event) => update({ suffix: event.currentTarget.value })} /></label>
              <label><span>同名文件</span><select value={settings.collisionPolicy} onChange={(event) => update({ collisionPolicy: event.currentTarget.value as WatermarkOutputSettings["collisionPolicy"] })}><option value="sequence">自动添加序号</option><option value="skip">跳过同名文件</option><option value="overwriteOutput">仅覆盖 FramePair 生成的副本</option></select></label>
            </section>
          </div>

          <div className="watermark-export-review" aria-label="导出确认摘要">
            <div><span>照片</span><strong>{snapshot.photos.length} 张 JPG/JPEG</strong></div>
            <div><span>格式</span><strong>{settings.format === "jpeg" ? `JPEG · 质量 ${settings.jpegQuality}` : settings.transparentBackground ? "PNG · 透明" : "PNG · 不透明"}</strong></div>
            <div><span>尺寸</span><strong>{settings.sizing.kind === "original" ? "原始尺寸" : `长边 ${settings.sizing.pixels}px${settings.sizing.allowUpscale ? " · 可放大" : " · 不放大"}`}</strong></div>
            <div><span>元数据</span><strong>{METADATA_LABELS[settings.metadataPolicy]}</strong></div>
            <div><span>同名文件</span><strong>{COLLISION_LABELS[settings.collisionPolicy]}</strong></div>
            <div><span>预计空间</span><strong>{formatBytes(estimate.minimum)} - {formatBytes(estimate.maximum)}</strong></div>
            <div className="watermark-export-directory"><span>输出目录</span><strong title={settings.outputDirectory ?? ""}>{settings.outputDirectory || "尚未选择"}</strong><button type="button" onClick={onChooseDirectory}><FolderOpen aria-hidden="true" size={14} />选择</button></div>
          </div>

          <div className="watermark-export-filenames"><strong>文件名示例</strong><ul>{examples.map((name) => <li key={name}>{name}</li>)}</ul></div>
          <div className="watermark-export-safety"><CheckCircle2 aria-hidden="true" size={16} /><span>只生成新副本，不会修改 JPG 原照片，也不会处理 RAW。</span></div>
          {validationError || error ? <div className="dialog-warning"><AlertTriangle aria-hidden="true" size={16} /><span>{validationError ?? error}</span></div> : null}
        </div>
      ) : null}

      {progress.phase === "running" ? (
        <div className="watermark-export-running">
          <LoaderCircle className="spinning" aria-hidden="true" size={28} />
          <strong>{progress.cancelRequested ? "正在停止后续任务" : "正在生成水印副本"}</strong>
          <span>{progress.currentPhotoId ? snapshot.photos.find((photo) => photo.id === progress.currentPhotoId)?.fileName ?? progress.currentPhotoId : "正在准备输出"}</span>
          <div className="watermark-export-progress" aria-label={`导出进度 ${percent}%`}><span style={{ width: `${percent}%` }} /></div>
          <div className="watermark-export-counts"><span>{finishedCount} / {progress.total}</span><span className="is-success">成功 {succeeded}</span><span>跳过 {skipped}</span><span className="is-failed">失败 {failed}</span></div>
          <p><AlertTriangle aria-hidden="true" size={15} />取消后不会再开始新的照片；已完成的副本会保留。</p>
          {error ? <div className="dialog-warning"><AlertTriangle aria-hidden="true" size={16} /><span>{error}</span></div> : null}
        </div>
      ) : null}

      {progress.phase === "results" ? (
        <div className="watermark-export-results">
          <div className="watermark-export-result-summary">
            <div className="is-success"><CheckCircle2 aria-hidden="true" size={18} /><span><strong>{progress.summary?.succeeded ?? succeeded}</strong><small>成功</small></span></div>
            <div><ImageIcon aria-hidden="true" size={18} /><span><strong>{progress.summary?.skipped ?? skipped}</strong><small>跳过</small></span></div>
            <div className="is-failed"><XCircle aria-hidden="true" size={18} /><span><strong>{progress.summary?.failed ?? failed}</strong><small>失败</small></span></div>
            <div><X aria-hidden="true" size={18} /><span><strong>{progress.summary?.cancelled ?? 0}</strong><small>未开始</small></span></div>
          </div>
          <div className="watermark-export-result-list">
            {progress.results.map((result) => (
              <div className={`is-${result.status}`} key={result.photoId}>
                <span>{statusLabel(result.status)}</span>
                <strong title={result.targetPath}>{snapshot.photos.find((photo) => photo.id === result.photoId)?.fileName ?? result.photoId}</strong>
                <small>{result.message}{result.sizeBytes ? ` · ${formatBytes(result.sizeBytes)}` : ""}</small>
              </div>
            ))}
          </div>
          {error ? <div className="dialog-warning"><AlertTriangle aria-hidden="true" size={16} /><span>{error}</span></div> : null}
        </div>
      ) : null}

      <div className="dialog-actions watermark-export-actions">
        {progress.phase === "idle" ? <><button type="button" className="secondary-command" onClick={onClose}>取消</button><button type="button" className="primary-command" onClick={onStart} disabled={Boolean(validationError)}><Download aria-hidden="true" size={17} />开始导出</button></> : null}
        {progress.phase === "running" ? <button type="button" className="secondary-command" onClick={onCancel} disabled={progress.cancelRequested}>{progress.cancelRequested ? "已请求停止" : "取消后续任务"}</button> : null}
        {progress.phase === "results" ? <><button type="button" className="secondary-command" onClick={onRetry} disabled={failedIds.length === 0}><RefreshCw aria-hidden="true" size={16} />重试失败项</button><button type="button" className="secondary-command" onClick={onReveal}><FolderOpen aria-hidden="true" size={16} />在文件管理器中显示</button><button type="button" className="primary-command" onClick={onClose}>完成</button></> : null}
      </div>
    </dialog>
  );
}

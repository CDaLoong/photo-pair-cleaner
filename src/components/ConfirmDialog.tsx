import { AlertTriangle, ArchiveRestore, CheckCircle2, Trash2, X } from "lucide-react";
import type { RefObject } from "react";
import type { CleanupDestination, ScanItem } from "../types";
import { cleanupActionLabel, formatBytes, selectionBreakdown } from "../utils";

interface ConfirmDialogProps {
  dialogRef: RefObject<HTMLDialogElement | null>;
  selectedItems: ScanItem[];
  selectedBytes: number;
  rawRoot: string;
  destination: CleanupDestination;
  acknowledged: boolean;
  busy: boolean;
  onDestinationChange: (destination: CleanupDestination) => void;
  onAcknowledgedChange: (checked: boolean) => void;
  onCancel: () => void;
  onConfirm: () => void;
}

export function ConfirmDialog({
  dialogRef,
  selectedItems,
  selectedBytes,
  rawRoot,
  destination,
  acknowledged,
  busy,
  onDestinationChange,
  onAcknowledgedChange,
  onCancel,
  onConfirm,
}: ConfirmDialogProps) {
  const breakdown = selectionBreakdown(selectedItems);
  const preview = selectedItems.slice(0, 4);
  const quarantining = destination === "quarantine";
  const DestinationIcon = quarantining ? ArchiveRestore : Trash2;

  return (
    <dialog ref={dialogRef} className="confirm-dialog" aria-labelledby="confirm-title" onClose={onCancel}>
      <div className="dialog-header">
        <div className="dialog-icon"><DestinationIcon aria-hidden="true" size={20} /></div>
        <div>
          <h2 id="confirm-title">确认执行安全清理</h2>
          <p>{quarantining ? "保留原目录结构，可从 FramePair 中恢复。" : "不会永久删除文件，但需要前往系统回收站恢复。"}</p>
        </div>
        <button className="icon-button" type="button" onClick={onCancel} aria-label="取消并关闭" title="取消并关闭">
          <X aria-hidden="true" size={18} />
        </button>
      </div>

      <div className="cleanup-destination" role="group" aria-label="清理文件去向">
        <button type="button" aria-pressed={!quarantining} onClick={() => onDestinationChange("trash")} disabled={busy}>
          <Trash2 aria-hidden="true" size={17} />
          <span><strong>系统回收站</strong><small>使用操作系统的恢复能力</small></span>
        </button>
        <button type="button" aria-pressed={quarantining} onClick={() => onDestinationChange("quarantine")} disabled={busy}>
          <ArchiveRestore aria-hidden="true" size={17} />
          <span><strong>FramePair 隔离区</strong><small>保留路径并支持应用内恢复</small></span>
        </button>
      </div>

      <div className="dialog-summary" aria-label="本次处理摘要">
        <div><span>选中文件</span><strong>{breakdown.total} 个</strong></div>
        <div><span>文件组成</span><strong>{breakdown.raw} RAW · {breakdown.sidecar} XMP</strong></div>
        <div><span>占用空间</span><strong>{formatBytes(selectedBytes)}</strong></div>
        <div><span>仅处理此目录</span><strong title={rawRoot}>{rawRoot}</strong></div>
      </div>

      <div className="dialog-file-preview">
        <strong>即将移动</strong>
        <ul>
          {preview.map((item) => <li key={item.id} title={item.relativePath}>{item.relativePath}</li>)}
        </ul>
        {selectedItems.length > preview.length && <span>另有 {selectedItems.length - preview.length} 个文件</span>}
      </div>

      <label className="confirm-check">
        <input
          type="checkbox"
          checked={acknowledged}
          onChange={(event) => onAcknowledgedChange(event.target.checked)}
          disabled={busy}
        />
        <span>
          <CheckCircle2 aria-hidden="true" size={17} />
          我已核对 RAW 目录、文件数量、清理列表和文件去向
        </span>
      </label>

      <div className="dialog-warning">
        <AlertTriangle aria-hidden="true" size={16} />
        JPG 参考目录不会被修改；恢复操作也不会覆盖已有文件。
      </div>

      <div className="dialog-actions">
        <button type="button" className="secondary-command" onClick={onCancel}>取消</button>
        <button type="button" className={quarantining ? "primary-command" : "danger-command"} onClick={onConfirm} disabled={!acknowledged || busy}>
          <DestinationIcon aria-hidden="true" size={17} />确认{cleanupActionLabel(destination)}
        </button>
      </div>
    </dialog>
  );
}

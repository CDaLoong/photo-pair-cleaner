import { AlertTriangle, CheckCircle2, Trash2, X } from "lucide-react";
import type { RefObject } from "react";
import type { ScanItem } from "../types";
import { formatBytes, selectionBreakdown } from "../utils";

interface ConfirmDialogProps {
  dialogRef: RefObject<HTMLDialogElement | null>;
  selectedItems: ScanItem[];
  selectedBytes: number;
  rawRoot: string;
  acknowledged: boolean;
  busy: boolean;
  onAcknowledgedChange: (checked: boolean) => void;
  onCancel: () => void;
  onConfirm: () => void;
}

export function ConfirmDialog({
  dialogRef,
  selectedItems,
  selectedBytes,
  rawRoot,
  acknowledged,
  busy,
  onAcknowledgedChange,
  onCancel,
  onConfirm,
}: ConfirmDialogProps) {
  const breakdown = selectionBreakdown(selectedItems);
  const preview = selectedItems.slice(0, 4);

  return (
    <dialog ref={dialogRef} className="confirm-dialog" aria-labelledby="confirm-title" onClose={onCancel}>
      <div className="dialog-header">
        <div className="dialog-icon"><Trash2 aria-hidden="true" size={20} /></div>
        <div>
          <h2 id="confirm-title">确认移入系统回收站/废纸篓</h2>
          <p>不会永久删除文件，但执行后需要前往系统回收站恢复。</p>
        </div>
        <button className="icon-button" type="button" onClick={onCancel} aria-label="取消并关闭" title="取消并关闭">
          <X aria-hidden="true" size={18} />
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
          我已核对 RAW 目录、文件数量和清理列表
        </span>
      </label>

      <div className="dialog-warning">
        <AlertTriangle aria-hidden="true" size={16} />
        JPG 参考目录不会被修改。
      </div>

      <div className="dialog-actions">
        <button type="button" className="secondary-command" onClick={onCancel}>取消</button>
        <button type="button" className="danger-command" onClick={onConfirm} disabled={!acknowledged || busy}>
          <Trash2 aria-hidden="true" size={17} />确认移入回收站
        </button>
      </div>
    </dialog>
  );
}

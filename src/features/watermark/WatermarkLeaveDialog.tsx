import { AlertTriangle, LogOut, X } from "lucide-react";
import { useEffect, useRef } from "react";

export interface WatermarkUnsavedWork {
  dirtyTemplate: boolean;
  unexportedChanges: boolean;
}

interface WatermarkLeaveDialogProps {
  open: boolean;
  reason: "navigate" | "close";
  unsaved: WatermarkUnsavedWork;
  onCancel: () => void;
  onConfirm: () => void;
}

export function WatermarkLeaveDialog({
  open,
  reason,
  unsaved,
  onCancel,
  onConfirm,
}: WatermarkLeaveDialogProps) {
  const dialogRef = useRef<HTMLDialogElement>(null);

  useEffect(() => {
    const dialog = dialogRef.current;
    if (!dialog) return;
    if (open && !dialog.open) dialog.showModal();
    if (!open && dialog.open) dialog.close();
  }, [open]);

  return (
    <dialog
      ref={dialogRef}
      className="confirm-dialog watermark-leave-dialog"
      aria-labelledby="watermark-leave-title"
      onCancel={(event) => {
        event.preventDefault();
        onCancel();
      }}
      onClose={() => { if (open) onCancel(); }}
    >
      <div className="dialog-header">
        <div className="dialog-icon"><AlertTriangle aria-hidden="true" size={20} /></div>
        <div>
          <h2 id="watermark-leave-title">当前水印任务尚未完成</h2>
          <p>{reason === "close" ? "关闭 FramePair 前，请确认是否放弃当前任务。" : "离开水印导出后，当前编辑现场将被清空。"}</p>
        </div>
        <button className="icon-button" type="button" onClick={onCancel} aria-label="继续编辑" title="继续编辑">
          <X aria-hidden="true" size={18} />
        </button>
      </div>

      <div className="watermark-leave-status">
        {unsaved.dirtyTemplate ? <span><strong>模板样式尚未保存</strong><small>图层或版式包含新的调整</small></span> : null}
        {unsaved.unexportedChanges ? <span><strong>照片尚未导出</strong><small>当前水印效果还没有生成副本</small></span> : null}
      </div>

      <p className="watermark-leave-note">
        原始 JPG 不会被修改；放弃更改只会清除本次水印编辑、单张微调和预览缓存。
      </p>

      <div className="dialog-actions">
        <button type="button" className="secondary-command" onClick={onCancel}>继续编辑</button>
        <button type="button" className="danger-command" onClick={onConfirm}>
          <LogOut aria-hidden="true" size={17} />
          {reason === "close" ? "放弃更改并退出" : "放弃更改并离开"}
        </button>
      </div>
    </dialog>
  );
}

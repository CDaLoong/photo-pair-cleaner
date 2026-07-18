import { AlertTriangle, CheckCircle2, FolderSync, X } from "lucide-react";
import { useEffect, useMemo, useRef, useState } from "react";
import { formatBytes } from "../../utils";
import { operationSelectionSummary } from "./ratingRuleUtils";
import type { OperationPlanItem, OperationPlanSummary } from "./types";

interface OperationExecuteDialogProps {
  open: boolean;
  plan: OperationPlanSummary | null;
  groupIds: string[];
  busy: boolean;
  onDismiss: () => void;
  onConfirm: () => void;
}

export function OperationExecuteDialog({
  open,
  plan,
  groupIds,
  busy,
  onDismiss,
  onConfirm,
}: OperationExecuteDialogProps) {
  const dialogRef = useRef<HTMLDialogElement>(null);
  const [acknowledged, setAcknowledged] = useState(false);
  const selected = useMemo(
    () => new Set(groupIds),
    [groupIds],
  );
  const items = (plan?.items ?? []) as OperationPlanItem[];
  const summary = operationSelectionSummary(items, selected);
  const preview = items
    .filter((item) => selected.has(item.groupId))
    .flatMap((item) => item.members.map((member) => member.targetPath))
    .filter((path): path is string => Boolean(path))
    .slice(0, 4);

  useEffect(() => {
    const dialog = dialogRef.current;
    if (!dialog) return;
    if (!open) {
      if (dialog.open) dialog.close();
      return;
    }
    setAcknowledged(false);
    if (!dialog.open) dialog.showModal();
  }, [open]);

  return (
    <dialog
      ref={dialogRef}
      className="confirm-dialog organizer-execute-dialog"
      aria-labelledby="organizer-execute-title"
      onCancel={(event) => {
        event.preventDefault();
        if (!busy) onDismiss();
      }}
      onClose={onDismiss}
    >
      <div className="dialog-header">
        <div className="dialog-icon organizer-dialog-icon"><FolderSync aria-hidden="true" size={20} /></div>
        <div>
          <h2 id="organizer-execute-title">确认执行评分整理</h2>
          <p>只执行选中的复制和移动照片组；待清理计划不会在本阶段执行。</p>
        </div>
        <button className="icon-button" type="button" onClick={onDismiss} disabled={busy} aria-label="取消并关闭" title="取消并关闭"><X aria-hidden="true" size={18} /></button>
      </div>

      <div className="dialog-summary" aria-label="评分整理执行摘要">
        <div><span>照片组</span><strong>{summary.groups} 组</strong></div>
        <div><span>操作组成</span><strong>{summary.moveGroups} 组移动 · {summary.copyGroups} 组复制</strong></div>
        <div><span>文件与大小</span><strong>{summary.files} 个 · {formatBytes(summary.bytes)}</strong></div>
        <div><span>照片根目录</span><strong title={plan?.root}>{plan?.root}</strong></div>
      </div>

      <div className="dialog-file-preview">
        <strong>目标路径预览</strong>
        <ul>{preview.map((path) => <li key={path} title={path}>{path}</li>)}</ul>
        {summary.files > preview.length ? <span>另有 {summary.files - preview.length} 个文件</span> : null}
      </div>

      <label className="confirm-check">
        <input type="checkbox" checked={acknowledged} onChange={(event) => setAcknowledged(event.target.checked)} disabled={busy} />
        <span><CheckCircle2 aria-hidden="true" size={17} />我已核对照片组、目标目录、文件数量和操作方式</span>
      </label>

      <div className="dialog-warning">
        <AlertTriangle aria-hidden="true" size={16} />
        不会覆盖已有文件。复制可撤销，移动可恢复；目标或原位置发生变化时会停止对应文件。
      </div>

      <div className="dialog-actions">
        <button type="button" className="secondary-command" onClick={onDismiss} disabled={busy}>取消</button>
        <button type="button" className="primary-command" onClick={onConfirm} disabled={!acknowledged || busy || summary.groups === 0}><FolderSync aria-hidden="true" size={17} />确认执行</button>
      </div>
    </dialog>
  );
}

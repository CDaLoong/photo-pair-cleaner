import { AlertTriangle, CheckCircle2, FolderSync, ShieldCheck, Trash2, X } from "lucide-react";
import { useEffect, useMemo, useRef, useState } from "react";
import { formatBytes } from "../../utils";
import { operationSelectionSummary } from "./ratingRuleUtils";
import type {
  CleanupExecutionDestination,
  OperationPlanItem,
  OperationPlanSummary,
} from "./types";

interface OperationExecuteDialogProps {
  open: boolean;
  plan: OperationPlanSummary | null;
  groupIds: string[];
  busy: boolean;
  onDismiss: () => void;
  onConfirm: (cleanupDestination: CleanupExecutionDestination | null) => void;
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
  const [cleanupDestination, setCleanupDestination] = useState<CleanupExecutionDestination>("quarantine");
  const selected = useMemo(
    () => new Set(groupIds),
    [groupIds],
  );
  const items = (plan?.items ?? []) as OperationPlanItem[];
  const summary = operationSelectionSummary(items, selected);
  const selectedItems = items.filter((item) => selected.has(item.groupId));
  const hasCleanup = summary.cleanupGroups > 0;
  const preview = selectedItems
    .flatMap((item) => item.members.map((member) => (
      item.terminalAction === "cleanup"
        ? member.sourceRelativePath
        : member.targetPath
    )))
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
    setCleanupDestination("quarantine");
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
          <p>执行选中的复制、移动和待清理照片组；待清理组统一使用下方选择的去向。</p>
        </div>
        <button className="icon-button" type="button" onClick={onDismiss} disabled={busy} aria-label="取消并关闭" title="取消并关闭"><X aria-hidden="true" size={18} /></button>
      </div>

      <div className="dialog-summary" aria-label="评分整理执行摘要">
        <div><span>照片组</span><strong>{summary.groups} 组</strong></div>
        <div><span>操作组成</span><strong>{summary.moveGroups} 组移动 · {summary.copyGroups} 组复制 · {summary.cleanupGroups} 组清理</strong></div>
        <div><span>文件与大小</span><strong>{summary.files} 个 · {formatBytes(summary.bytes)}</strong></div>
        <div><span>照片根目录</span><strong title={plan?.root}>{plan?.root}</strong></div>
      </div>

      {hasCleanup ? (
        <div className="organizer-cleanup-destination">
          <strong>待清理照片去向</strong>
          <div className="cleanup-destination" role="group" aria-label="待清理照片去向">
            <button type="button" className={cleanupDestination === "quarantine" ? "is-active" : ""} aria-pressed={cleanupDestination === "quarantine"} disabled={busy} onClick={() => setCleanupDestination("quarantine")}>
              <ShieldCheck aria-hidden="true" size={17} />
              <span><strong>FramePair 隔离区</strong><small>默认，可在操作历史中整组恢复</small></span>
            </button>
            <button type="button" className={cleanupDestination === "trash" ? "is-active is-danger" : ""} aria-pressed={cleanupDestination === "trash"} disabled={busy} onClick={() => setCleanupDestination("trash")}>
              <Trash2 aria-hidden="true" size={17} />
              <span><strong>系统回收站</strong><small>FramePair 内不可恢复</small></span>
            </button>
          </div>
        </div>
      ) : null}

      <div className="dialog-file-preview">
        <strong>{hasCleanup ? "执行路径预览" : "目标路径预览"}</strong>
        <ul>{preview.map((path) => <li key={path} title={path}>{path}</li>)}</ul>
        {summary.files > preview.length ? <span>另有 {summary.files - preview.length} 个文件</span> : null}
        {hasCleanup && cleanupDestination === "quarantine" ? <span title={plan?.root}>待清理文件将保留相对目录结构并移入 .framepair-quarantine/&lt;本次操作&gt;/</span> : null}
      </div>

      <label className="confirm-check">
        <input type="checkbox" checked={acknowledged} onChange={(event) => setAcknowledged(event.target.checked)} disabled={busy} />
        <span><CheckCircle2 aria-hidden="true" size={17} />我已核对照片组、文件数量、操作方式和待清理去向</span>
      </label>

      <div className="dialog-warning">
        <AlertTriangle aria-hidden="true" size={16} />
        {hasCleanup && cleanupDestination === "trash"
          ? "系统回收站中的文件不能从 FramePair 操作历史恢复；应用只保留操作回执。"
          : "不会覆盖已有文件。复制可撤销，移动和隔离可恢复；文件发生变化时会停止对应照片组。"}
      </div>

      <div className="dialog-actions">
        <button type="button" className="secondary-command" onClick={onDismiss} disabled={busy}>取消</button>
        <button type="button" className="primary-command" onClick={() => onConfirm(hasCleanup ? cleanupDestination : null)} disabled={!acknowledged || busy || summary.groups === 0}><FolderSync aria-hidden="true" size={17} />确认执行</button>
      </div>
    </dialog>
  );
}

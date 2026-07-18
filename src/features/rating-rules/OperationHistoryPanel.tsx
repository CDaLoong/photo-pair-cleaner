import { CheckCircle2, CopyX, ExternalLink, History, RotateCcw } from "lucide-react";
import { formatDate } from "../../utils";
import { organizerActionLabel, organizerGroupStatusLabel } from "./ratingRuleUtils";
import type {
  OperationHistoryEntry,
  OrganizerExecutionSummary,
  RecoveryKind,
} from "./types";

interface OperationHistoryPanelProps {
  history: OperationHistoryEntry[];
  latest: OrganizerExecutionSummary | null;
  busy: boolean;
  onRecover: (kind: RecoveryKind, operationId: string, groupIds: string[]) => void;
  onOpenTrash: () => void;
}

function recoverableGroups(entry: OperationHistoryEntry, kind: RecoveryKind): string[] {
  const action = kind === "restoreMove"
    ? "move"
    : kind === "restoreQuarantine"
      ? "quarantine"
      : "copy";
  const completed = new Set(
    entry.recoveries
      .filter((recovery) => recovery.status === "success")
      .map((recovery) => recovery.groupId),
  );
  return entry.manifest.groups
    .filter((group) => (
      group.action === action
      && (group.status === "success" || group.status === "partial")
      && !completed.has(group.groupId)
    ))
    .map((group) => group.groupId);
}

export function OperationHistoryPanel({
  history,
  latest,
  busy,
  onRecover,
  onOpenTrash,
}: OperationHistoryPanelProps) {
  return (
    <section id="rating-rules-history" className="organizer-history" aria-label="评分整理操作历史" data-tour="rating-rules-history">
      <header><History aria-hidden="true" size={18} /><div><h2>操作历史与恢复</h2><p>历史只追加记录；恢复和撤销都不会覆盖或删除已变化文件。</p></div></header>

      {latest ? (
        <div className="organizer-receipt" role="status">
          <CheckCircle2 aria-hidden="true" size={17} />
          <div><strong>最近执行：{latest.succeeded} 组完成，{latest.partial} 组部分完成，{latest.failed} 组失败</strong><span>操作编号 {latest.operationId}</span></div>
        </div>
      ) : null}

      {history.length === 0 ? <div className="organizer-history-empty">当前还没有评分整理或清理历史</div> : (
        <div className="organizer-history-list">
          {history.slice(0, 6).map((entry) => {
            const moveGroups = recoverableGroups(entry, "restoreMove");
            const copyGroups = recoverableGroups(entry, "undoCopy");
            const quarantineGroups = recoverableGroups(entry, "restoreQuarantine");
            const hasTrash = entry.manifest.groups.some((group) => (
              group.action === "trash"
              && (group.status === "success" || group.status === "partial")
            ));
            const success = entry.manifest.groups.filter((group) => group.status === "success").length;
            const partial = entry.manifest.groups.filter((group) => group.status === "partial").length;
            const failed = entry.manifest.groups.filter((group) => group.status === "failed").length;
            return (
              <article key={entry.manifest.operationId}>
                <div className="organizer-history-main">
                  <strong>{formatDate(entry.manifest.createdAtMs)}</strong>
                  <span title={entry.manifest.root}>{entry.manifest.root}</span>
                  <small>{success} 组完成 · {partial} 组部分完成 · {failed} 组失败</small>
                </div>
                <div className="organizer-history-statuses">
                  {entry.manifest.groups.slice(0, 4).map((group) => <span key={group.groupId} className={`is-${group.status}`}>{group.relativeStem} · {organizerActionLabel(group.action)} · {organizerGroupStatusLabel(group.status)}</span>)}
                  {entry.manifest.groups.length > 4 ? <small>另有 {entry.manifest.groups.length - 4} 组</small> : null}
                </div>
                <div className="organizer-history-actions">
                  {moveGroups.length > 0 ? <button className="secondary-command" type="button" disabled={busy} onClick={() => onRecover("restoreMove", entry.manifest.operationId, moveGroups)}><RotateCcw aria-hidden="true" size={15} />恢复移动 ({moveGroups.length})</button> : null}
                  {copyGroups.length > 0 ? <button className="secondary-command" type="button" disabled={busy} onClick={() => onRecover("undoCopy", entry.manifest.operationId, copyGroups)}><CopyX aria-hidden="true" size={15} />撤销复制 ({copyGroups.length})</button> : null}
                  {quarantineGroups.length > 0 ? <button className="secondary-command" type="button" disabled={busy} onClick={() => onRecover("restoreQuarantine", entry.manifest.operationId, quarantineGroups)}><RotateCcw aria-hidden="true" size={15} />恢复隔离 ({quarantineGroups.length})</button> : null}
                  {hasTrash ? <button className="secondary-command" type="button" disabled={busy} onClick={onOpenTrash}><ExternalLink aria-hidden="true" size={15} />打开系统回收站</button> : null}
                  {moveGroups.length === 0 && copyGroups.length === 0 && quarantineGroups.length === 0 && !hasTrash ? <span className="organizer-history-complete">无可恢复文件</span> : null}
                </div>
              </article>
            );
          })}
        </div>
      )}
    </section>
  );
}

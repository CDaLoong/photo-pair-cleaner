import { CheckCircle2, CopyX, History, RotateCcw } from "lucide-react";
import { formatDate } from "../../utils";
import { organizerGroupStatusLabel } from "./ratingRuleUtils";
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
}

function recoverableGroups(entry: OperationHistoryEntry, kind: RecoveryKind): string[] {
  const action = kind === "restoreMove" ? "move" : "copy";
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
}: OperationHistoryPanelProps) {
  return (
    <section className="organizer-history" aria-label="评分整理操作历史">
      <header><History aria-hidden="true" size={18} /><div><h2>操作历史与恢复</h2><p>历史只追加记录；恢复和撤销都不会覆盖或删除已变化文件。</p></div></header>

      {latest ? (
        <div className="organizer-receipt" role="status">
          <CheckCircle2 aria-hidden="true" size={17} />
          <div><strong>最近执行：{latest.succeeded} 组完成，{latest.partial} 组部分完成，{latest.failed} 组失败</strong><span>操作编号 {latest.operationId}</span></div>
        </div>
      ) : null}

      {history.length === 0 ? <div className="organizer-history-empty">当前还没有复制或移动历史</div> : (
        <div className="organizer-history-list">
          {history.slice(0, 6).map((entry) => {
            const moveGroups = recoverableGroups(entry, "restoreMove");
            const copyGroups = recoverableGroups(entry, "undoCopy");
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
                  {entry.manifest.groups.slice(0, 4).map((group) => <span key={group.groupId} className={`is-${group.status}`}>{group.relativeStem} · {organizerGroupStatusLabel(group.status)}</span>)}
                  {entry.manifest.groups.length > 4 ? <small>另有 {entry.manifest.groups.length - 4} 组</small> : null}
                </div>
                <div className="organizer-history-actions">
                  {moveGroups.length > 0 ? <button className="secondary-command" type="button" disabled={busy} onClick={() => onRecover("restoreMove", entry.manifest.operationId, moveGroups)}><RotateCcw aria-hidden="true" size={15} />恢复移动 ({moveGroups.length})</button> : null}
                  {copyGroups.length > 0 ? <button className="secondary-command" type="button" disabled={busy} onClick={() => onRecover("undoCopy", entry.manifest.operationId, copyGroups)}><CopyX aria-hidden="true" size={15} />撤销复制 ({copyGroups.length})</button> : null}
                  {moveGroups.length === 0 && copyGroups.length === 0 ? <span className="organizer-history-complete">无可恢复文件</span> : null}
                </div>
              </article>
            );
          })}
        </div>
      )}
    </section>
  );
}

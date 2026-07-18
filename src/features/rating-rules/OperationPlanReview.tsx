import { ChevronDown, ChevronRight, FileSearch, Play, ShieldCheck } from "lucide-react";
import { Fragment, useEffect, useMemo, useState } from "react";
import { formatBytes, formatDate } from "../../utils";
import {
  filterOperationPlanItems,
  defaultExecutableGroupIds,
  isExecutablePlanItem,
  memberKindLabel,
  operationSelectionSummary,
  operationStatusLabel,
  ruleActionLabel,
} from "./ratingRuleUtils";
import type {
  OperationPlanFilter,
  OperationPlanItem,
  OperationPlanSummary,
} from "./types";

interface OperationPlanReviewProps {
  plan: OperationPlanSummary;
  busy: boolean;
  onRequestExecute: (groupIds: string[]) => void;
}

const FILTERS: Array<{ value: OperationPlanFilter; label: string }> = [
  { value: "all", label: "全部" },
  { value: "sync", label: "同步" },
  { value: "move", label: "移动" },
  { value: "copy", label: "复制" },
  { value: "cleanup", label: "待清理" },
  { value: "keep", label: "保留" },
  { value: "conflict", label: "冲突" },
  { value: "skipped", label: "跳过" },
];

function ratingText(rating: number | null): string {
  return rating === null ? "-" : rating === 0 ? "未评分" : `${rating} 星`;
}

function PlanDetails({ item }: { item: OperationPlanItem }) {
  return (
    <div className="operation-plan-details">
      {item.issues.length > 0 ? <div className="operation-plan-issues">{item.issues.map((issue) => <span key={issue}>{issue}</span>)}</div> : null}
      {item.missingKinds.length > 0 ? <p>缺少格式：{item.missingKinds.map(memberKindLabel).join("、")}</p> : null}
      <table>
        <thead><tr><th>格式</th><th>源路径</th><th>模拟目标</th><th>大小</th><th>修改时间</th></tr></thead>
        <tbody>
          {item.members.map((member) => (
            <tr key={`${member.kind}:${member.sourceRelativePath}`}>
              <td>{memberKindLabel(member.kind)}</td>
              <td title={member.sourceRelativePath}>{member.sourceRelativePath}</td>
              <td title={member.targetPath ?? "保留原位置"}>{member.targetPath ?? "保留原位置"}</td>
              <td>{formatBytes(member.sizeBytes)}</td>
              <td>{formatDate(member.modifiedMs)}</td>
            </tr>
          ))}
        </tbody>
      </table>
      {item.syncActions.length > 0 ? (
        <div className="operation-plan-syncs">
          {item.syncActions.map((sync) => (
            <span key={`${sync.target}:${sync.targetPath}`}>{sync.target === "rawXmp" ? "RAW XMP" : "JPG 元数据"} → {sync.targetRating} 星 · {sync.targetPath}</span>
          ))}
        </div>
      ) : null}
    </div>
  );
}

export function OperationPlanReview({ plan, busy, onRequestExecute }: OperationPlanReviewProps) {
  const [filter, setFilter] = useState<OperationPlanFilter>("all");
  const [expanded, setExpanded] = useState<Set<string>>(new Set());
  const [selected, setSelected] = useState<Set<string>>(
    () => new Set(defaultExecutableGroupIds(plan.items)),
  );
  const visibleItems = useMemo(
    () => filterOperationPlanItems(plan.items, filter),
    [filter, plan.items],
  );
  const executableIds = useMemo(
    () => defaultExecutableGroupIds(plan.items),
    [plan.items],
  );
  const selectionSummary = useMemo(
    () => operationSelectionSummary(plan.items, selected),
    [plan.items, selected],
  );
  const allExecutableSelected = executableIds.length > 0
    && executableIds.every((groupId) => selected.has(groupId));

  useEffect(() => {
    setSelected(new Set(defaultExecutableGroupIds(plan.items)));
    setExpanded(new Set());
    setFilter("all");
  }, [plan.planId, plan.items]);

  function toggleExpanded(groupId: string) {
    setExpanded((current) => {
      const next = new Set(current);
      if (next.has(groupId)) next.delete(groupId);
      else next.add(groupId);
      return next;
    });
  }

  function toggleSelected(groupId: string) {
    setSelected((current) => {
      const next = new Set(current);
      if (next.has(groupId)) next.delete(groupId);
      else next.add(groupId);
      return next;
    });
  }

  function toggleAllExecutable() {
    setSelected(allExecutableSelected ? new Set() : new Set(executableIds));
  }

  return (
    <section className="operation-plan-review" data-tour="rating-rules-plan">
      <header>
        <div><FileSearch aria-hidden="true" size={18} /><span><h2>执行计划复核</h2><p>{plan.totalItems} 个照片组 · {plan.conflicts} 个冲突 · {plan.skipped} 个跳过</p></span></div>
        <dl className="operation-plan-summary">
          <div><dt>移动</dt><dd>{plan.moveGroups}</dd></div>
          <div><dt>复制</dt><dd>{plan.copyGroups}</dd></div>
          <div><dt>待清理</dt><dd>{plan.cleanupGroups}</dd></div>
          <div><dt>同步</dt><dd>{plan.syncGroups}</dd></div>
          <div><dt>预计新增</dt><dd>{formatBytes(plan.copyBytes)}</dd></div>
          <div><dt>预计释放</dt><dd>{formatBytes(plan.cleanupBytes)}</dd></div>
        </dl>
      </header>

      <div className="operation-plan-filters" role="tablist" aria-label="执行计划筛选">
        {FILTERS.map((option) => (
          <button key={option.value} type="button" role="tab" aria-selected={filter === option.value} onClick={() => setFilter(option.value)}>{option.label}</button>
        ))}
      </div>

      <div className="operation-plan-table-scroll">
        <table className="operation-plan-table">
          <thead><tr><th><input type="checkbox" checked={allExecutableSelected} disabled={busy || executableIds.length === 0} onChange={toggleAllExecutable} aria-label="选择全部可执行照片组" /></th><th aria-label="展开" /><th>照片组</th><th>工作评分</th><th>评分来源</th><th>命中规则</th><th>文件组成</th><th>最终操作</th><th>状态</th></tr></thead>
          <tbody>
            {visibleItems.map((item) => {
              const isExpanded = expanded.has(item.groupId);
              const executable = isExecutablePlanItem(item);
              return (
                <Fragment key={item.groupId}>
                  <tr className={`is-${item.status}`}>
                    <td><input type="checkbox" checked={selected.has(item.groupId)} disabled={busy || !executable} onChange={() => toggleSelected(item.groupId)} aria-label={`选择 ${item.relativeStem}`} title={item.terminalAction === "cleanup" ? "待清理将在第五阶段开放" : executable ? "选择执行" : "当前照片组不可执行"} /></td>
                    <td><button className="icon-button" type="button" onClick={() => toggleExpanded(item.groupId)} aria-label={isExpanded ? `收起 ${item.relativeStem}` : `展开 ${item.relativeStem}`} title={isExpanded ? "收起详情" : "展开详情"}>{isExpanded ? <ChevronDown aria-hidden="true" size={15} /> : <ChevronRight aria-hidden="true" size={15} />}</button></td>
                    <td><strong>{item.relativeStem}</strong></td>
                    <td>{ratingText(item.rating)}</td>
                    <td>FP {item.framePair} / JPG {ratingText(item.jpegMetadata)} / XMP {ratingText(item.rawXmp)}</td>
                    <td title={item.matchedRuleNames.join("、")}>{item.matchedRuleNames.join("、") || "-"}</td>
                    <td>{item.members.map((member) => memberKindLabel(member.kind)).join(" + ") || "-"}</td>
                    <td>{item.terminalAction ? ruleActionLabel(item.terminalAction) : "-"}</td>
                    <td><span>{item.terminalAction === "cleanup" && item.status === "ready" ? "第五阶段开放" : operationStatusLabel(item.status)}</span></td>
                  </tr>
                  {isExpanded ? <tr className="operation-plan-detail-row"><td colSpan={9}><PlanDetails item={item} /></td></tr> : null}
                </Fragment>
              );
            })}
          </tbody>
        </table>
        {visibleItems.length === 0 ? <div className="operation-plan-empty">当前筛选没有照片组</div> : null}
      </div>

      <footer>
        <div><ShieldCheck aria-hidden="true" size={16} /><span>已选 {selectionSummary.groups} 组 · {selectionSummary.files} 个文件 · {formatBytes(selectionSummary.bytes)}。不会覆盖已有文件，待清理不会执行。</span></div>
        <button className="primary-command" type="button" disabled={busy || selectionSummary.groups === 0} onClick={() => onRequestExecute(executableIds.filter((groupId) => selected.has(groupId)))}><Play aria-hidden="true" size={15} />执行所选</button>
      </footer>
    </section>
  );
}

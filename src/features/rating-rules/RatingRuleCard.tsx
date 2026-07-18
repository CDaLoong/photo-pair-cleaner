import {
  ArrowDown,
  ArrowUp,
  Copy,
  FolderOpen,
  MoveRight,
  ShieldCheck,
  Trash2,
} from "lucide-react";
import type {
  RatingCondition,
  RatingRule,
  RuleAction,
  RuleMemberKind,
} from "./types";

interface RatingRuleCardProps {
  rule: RatingRule;
  index: number;
  total: number;
  busy: boolean;
  onChange: (rule: RatingRule) => void;
  onChooseDestination: () => void;
  onMove: (direction: -1 | 1) => void;
  onRemove: () => void;
}

const MEMBER_OPTIONS: Array<{ value: RuleMemberKind; label: string }> = [
  { value: "jpeg", label: "JPG" },
  { value: "raw", label: "RAW" },
  { value: "xmp", label: "XMP" },
];

const ACTION_OPTIONS: Array<{
  value: RuleAction;
  label: string;
  icon: typeof ShieldCheck;
}> = [
  { value: "keep", label: "保留", icon: ShieldCheck },
  { value: "copy", label: "复制", icon: Copy },
  { value: "move", label: "移动", icon: MoveRight },
  { value: "cleanup", label: "待清理", icon: Trash2 },
];

function conditionFor(type: RatingCondition["type"]): RatingCondition {
  if (type === "unrated") return { type };
  if (type === "between") return { type, minimum: 1, maximum: 5 };
  return { type, rating: type === "atLeast" ? 4 : type === "atMost" ? 2 : 3 };
}

export function RatingRuleCard({
  rule,
  index,
  total,
  busy,
  onChange,
  onChooseDestination,
  onMove,
  onRemove,
}: RatingRuleCardProps) {
  const needsDestination = rule.action === "copy" || rule.action === "move";

  function toggleMember(kind: RuleMemberKind, checked: boolean) {
    const memberScope = checked
      ? [...rule.memberScope, kind]
      : rule.memberScope.filter((item) => item !== kind);
    onChange({ ...rule, memberScope });
  }

  function changeAction(action: RuleAction) {
    onChange({
      ...rule,
      action,
      destination: action === "copy" || action === "move" ? rule.destination : null,
    });
  }

  function updateConditionValue(key: "rating" | "minimum" | "maximum", value: number) {
    if (rule.condition.type === "unrated") return;
    onChange({
      ...rule,
      condition: { ...rule.condition, [key]: value } as RatingCondition,
    });
  }

  return (
    <article className={rule.enabled ? "rating-rule-card" : "rating-rule-card is-disabled"}>
      <header>
        <label className="rating-rule-enabled">
          <input
            type="checkbox"
            checked={rule.enabled}
            disabled={busy}
            onChange={(event) => onChange({ ...rule, enabled: event.target.checked })}
          />
          <span>启用</span>
        </label>
        <input
          className="rating-rule-name"
          value={rule.name}
          maxLength={80}
          disabled={busy}
          aria-label={`规则 ${index + 1} 名称`}
          onChange={(event) => onChange({ ...rule, name: event.target.value })}
        />
        <span className="rating-rule-order">{index + 1}/{total}</span>
        <button className="icon-button" type="button" disabled={busy || index === 0} onClick={() => onMove(-1)} aria-label="上移规则" title="上移规则"><ArrowUp aria-hidden="true" size={15} /></button>
        <button className="icon-button" type="button" disabled={busy || index === total - 1} onClick={() => onMove(1)} aria-label="下移规则" title="下移规则"><ArrowDown aria-hidden="true" size={15} /></button>
        <button className="icon-button danger-icon" type="button" disabled={busy} onClick={onRemove} aria-label="删除规则" title="删除规则"><Trash2 aria-hidden="true" size={15} /></button>
      </header>

      <div className="rating-rule-fields">
        <label className="rating-rule-condition">
          <span>评分条件</span>
          <div>
            <select
              value={rule.condition.type}
              disabled={busy}
              aria-label={`规则 ${index + 1} 评分条件`}
              onChange={(event) => onChange({
                ...rule,
                condition: conditionFor(event.target.value as RatingCondition["type"]),
              })}
            >
              <option value="unrated">未评分</option>
              <option value="equal">等于</option>
              <option value="atLeast">高于或等于</option>
              <option value="atMost">低于或等于</option>
              <option value="between">闭区间</option>
            </select>
            {rule.condition.type !== "unrated" && rule.condition.type !== "between" ? (
              <input type="number" min="0" max="5" step="1" value={rule.condition.rating} disabled={busy} aria-label="星级" onChange={(event) => updateConditionValue("rating", Number(event.target.value))} />
            ) : null}
            {rule.condition.type === "between" ? (
              <>
                <input type="number" min="0" max="5" step="1" value={rule.condition.minimum} disabled={busy} aria-label="最低星级" onChange={(event) => updateConditionValue("minimum", Number(event.target.value))} />
                <span aria-hidden="true">至</span>
                <input type="number" min="0" max="5" step="1" value={rule.condition.maximum} disabled={busy} aria-label="最高星级" onChange={(event) => updateConditionValue("maximum", Number(event.target.value))} />
              </>
            ) : null}
          </div>
        </label>

        <fieldset className="rating-rule-members">
          <legend>处理格式</legend>
          <div>
            {MEMBER_OPTIONS.map((option) => (
              <label key={option.value}>
                <input type="checkbox" checked={rule.memberScope.includes(option.value)} disabled={busy} onChange={(event) => toggleMember(option.value, event.target.checked)} />
                <span>{option.label}</span>
              </label>
            ))}
          </div>
        </fieldset>

        <fieldset className="rating-rule-actions">
          <legend>最终操作</legend>
          <div>
            {ACTION_OPTIONS.map((option) => {
              const Icon = option.icon;
              return (
                <button key={option.value} type="button" aria-pressed={rule.action === option.value} disabled={busy} onClick={() => changeAction(option.value)}>
                  <Icon aria-hidden="true" size={14} />{option.label}
                </button>
              );
            })}
          </div>
        </fieldset>
      </div>

      {needsDestination ? (
        <div className="rating-rule-target">
          <button type="button" className="rating-rule-destination" disabled={busy} onClick={onChooseDestination}>
            <FolderOpen aria-hidden="true" size={16} />
            <span><strong>{rule.destination || "选择目标目录"}</strong><small>可在系统选择器中创建文件夹</small></span>
          </button>
          <div className="rating-rule-path-mode" role="group" aria-label="目录结构">
            <button type="button" aria-pressed={rule.preserveRelativePath} disabled={busy} onClick={() => onChange({ ...rule, preserveRelativePath: true })}>保留结构</button>
            <button type="button" aria-pressed={!rule.preserveRelativePath} disabled={busy} onClick={() => onChange({ ...rule, preserveRelativePath: false })}>平铺</button>
          </div>
        </div>
      ) : null}
    </article>
  );
}

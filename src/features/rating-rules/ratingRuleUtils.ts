import type {
  OperationPlanFilter,
  OperationPlanItem,
  OperationPlanStatus,
  OrganizerGroupStatus,
  RatingCondition,
  RatingRule,
  RatingRuleTemplate,
  RatingRuleTemplateId,
  RuleAction,
  RuleMemberKind,
} from "./types";

const ALL_MEMBER_KINDS: RuleMemberKind[] = ["jpeg", "raw", "xmp"];

export const RATING_RULE_TEMPLATES: RatingRuleTemplate[] = [
  { id: "curatedArchive", name: "精选归档", detail: "4 星以上移动到指定目录" },
  { id: "lowRatingCleanup", name: "低分清理", detail: "2 星以下进入待清理计划" },
  { id: "backupAll", name: "保留全部备份", detail: "按评分复制完整照片组" },
  { id: "custom", name: "完全自定义", detail: "从空规则列表开始" },
];

export function createRatingRule(id: string): RatingRule {
  return {
    id,
    name: "自定义规则",
    enabled: true,
    condition: { type: "equal", rating: 3 },
    memberScope: [...ALL_MEMBER_KINDS],
    action: "move",
    destination: null,
    preserveRelativePath: true,
  };
}

export function rulesForTemplate(
  template: RatingRuleTemplateId,
  createId: () => string,
): RatingRule[] {
  if (template === "custom") return [];
  const base = createRatingRule(createId());
  if (template === "curatedArchive") {
    return [{ ...base, name: "4 星以上精选归档", condition: { type: "atLeast", rating: 4 } }];
  }
  if (template === "lowRatingCleanup") {
    return [{
      ...base,
      name: "2 星以下待清理",
      condition: { type: "atMost", rating: 2 },
      action: "cleanup",
    }];
  }
  return [{
    ...base,
    name: "保留全部备份",
    condition: { type: "between", minimum: 0, maximum: 5 },
    action: "copy",
  }];
}

function conditionError(condition: RatingCondition): string | null {
  const validRating = (rating: number) => Number.isInteger(rating) && rating >= 0 && rating <= 5;
  if (condition.type === "unrated") return null;
  if (condition.type === "between") {
    if (!validRating(condition.minimum) || !validRating(condition.maximum)) {
      return "评分必须在 0 到 5 星之间";
    }
    if (condition.minimum > condition.maximum) return "最低评分不能高于最高评分";
    return null;
  }
  return validRating(condition.rating) ? null : "评分必须在 0 到 5 星之间";
}

export function validateRatingRuleDrafts(
  rules: RatingRule[],
): { valid: true } | { valid: false; message: string } {
  if (rules.length === 0) return { valid: false, message: "请至少创建一条评分规则" };
  if (rules.length > 100) return { valid: false, message: "最多只能创建 100 条规则" };
  const ids = new Set<string>();
  for (const rule of rules) {
    if (ids.has(rule.id)) return { valid: false, message: `规则 ID 重复：${rule.id}` };
    ids.add(rule.id);
  }
  for (const rule of rules) {
    const name = rule.name.trim();
    if (!name) return { valid: false, message: "规则名称不能为空" };
    if ([...name].length > 80) return { valid: false, message: `规则“${name}”名称不能超过 80 个字符` };
    const condition = conditionError(rule.condition);
    if (condition) return { valid: false, message: `规则“${name}”：${condition}` };
    if (rule.memberScope.length === 0) {
      return { valid: false, message: `规则“${name}”至少选择一种处理格式` };
    }
    if (new Set(rule.memberScope).size !== rule.memberScope.length) {
      return { valid: false, message: `规则“${name}”处理格式不能重复` };
    }
    const needsDestination = rule.action === "copy" || rule.action === "move";
    if (needsDestination && !rule.destination?.trim()) {
      return { valid: false, message: `规则“${name}”必须选择目标目录` };
    }
    if (!needsDestination && rule.destination?.trim()) {
      return { valid: false, message: `规则“${name}”不能设置目标目录` };
    }
  }
  return { valid: true };
}

export function ratingConditionLabel(condition: RatingCondition): string {
  if (condition.type === "unrated") return "未评分";
  if (condition.type === "equal") return `等于 ${condition.rating} 星`;
  if (condition.type === "atLeast") return `${condition.rating} 星及以上`;
  if (condition.type === "atMost") return `${condition.rating} 星及以下`;
  return `${condition.minimum}-${condition.maximum} 星`;
}

export function ruleActionLabel(action: RuleAction): string {
  if (action === "keep") return "保留";
  if (action === "copy") return "复制";
  if (action === "move") return "移动";
  return "待清理";
}

export function memberKindLabel(kind: RuleMemberKind): string {
  if (kind === "jpeg") return "JPG";
  if (kind === "raw") return "RAW";
  return "XMP";
}

export function operationStatusLabel(status: OperationPlanStatus): string {
  if (status === "ready") return "计划就绪";
  if (status === "keep") return "保留";
  if (status === "skipped") return "已跳过";
  return "存在冲突";
}

export function filterOperationPlanItems<T extends Pick<
  OperationPlanItem,
  "terminalAction" | "status" | "syncActions"
>>(items: T[], filter: OperationPlanFilter): T[] {
  if (filter === "all") return items;
  if (filter === "sync") return items.filter((item) => item.syncActions.length > 0);
  if (filter === "conflict") return items.filter((item) => item.status === "conflict");
  if (filter === "skipped") return items.filter((item) => item.status === "skipped");
  if (filter === "keep") return items.filter((item) => item.status === "keep");
  return items.filter((item) => item.terminalAction === filter);
}

export function isExecutablePlanItem<T extends Pick<
  OperationPlanItem,
  "status" | "terminalAction"
>>(item: T): boolean {
  return item.status === "ready"
    && (item.terminalAction === "copy" || item.terminalAction === "move");
}

export function defaultExecutableGroupIds<T extends Pick<
  OperationPlanItem,
  "groupId" | "status" | "terminalAction"
>>(items: T[]): string[] {
  return items.filter(isExecutablePlanItem).map((item) => item.groupId);
}

export function operationSelectionSummary<T extends Pick<
  OperationPlanItem,
  "groupId" | "status" | "terminalAction" | "members"
>>(items: T[], selected: Set<string>) {
  const executable = items.filter(
    (item) => selected.has(item.groupId) && isExecutablePlanItem(item),
  );
  return {
    groups: executable.length,
    copyGroups: executable.filter((item) => item.terminalAction === "copy").length,
    moveGroups: executable.filter((item) => item.terminalAction === "move").length,
    files: executable.reduce((total, item) => total + item.members.length, 0),
    bytes: executable.reduce(
      (total, item) => total + item.members.reduce(
        (memberTotal, member) => memberTotal + member.sizeBytes,
        0,
      ),
      0,
    ),
  };
}

export function organizerGroupStatusLabel(status: OrganizerGroupStatus): string {
  if (status === "success") return "已完成";
  if (status === "failed") return "失败";
  if (status === "partial") return "部分完成";
  return "已跳过";
}

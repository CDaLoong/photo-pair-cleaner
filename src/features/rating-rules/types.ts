import type {
  RatingConflictPolicy,
  RatingSyncTarget,
  RatingSyncTargets,
} from "../rating-sync/types";

export type RatingCondition =
  | { type: "unrated" }
  | { type: "equal"; rating: number }
  | { type: "atLeast"; rating: number }
  | { type: "atMost"; rating: number }
  | { type: "between"; minimum: number; maximum: number };

export type RuleMemberKind = "jpeg" | "raw" | "xmp";
export type RuleAction = "keep" | "copy" | "move" | "cleanup";
export type RatingRuleTemplateId =
  | "curatedArchive"
  | "lowRatingCleanup"
  | "backupAll"
  | "custom";

export interface RatingRule {
  id: string;
  name: string;
  enabled: boolean;
  condition: RatingCondition;
  memberScope: RuleMemberKind[];
  action: RuleAction;
  destination: string | null;
  preserveRelativePath: boolean;
}

export interface RatingRuleState {
  rules: RatingRule[];
}

export interface OperationSyncPreference {
  enabled: boolean;
  targets: RatingSyncTargets;
  jpegWriteConfirmed: boolean;
  syncCleanupBefore: boolean;
}

export interface OperationPlanRequest {
  root: string;
  rules: RatingRule[];
  conflictPolicy: RatingConflictPolicy;
  sync: OperationSyncPreference;
}

export type SyncTiming = "source" | "destination" | "beforeCleanup";
export type OperationPlanStatus = "ready" | "keep" | "skipped" | "conflict";
export type OperationPlanFilter =
  | "all"
  | "sync"
  | "move"
  | "copy"
  | "cleanup"
  | "keep"
  | "conflict"
  | "skipped";

export interface PlannedSyncAction {
  target: RatingSyncTarget;
  targetPath: string;
  targetRating: number;
  timing: SyncTiming;
}

export interface PlannedMember {
  kind: RuleMemberKind;
  sourceRelativePath: string;
  targetPath: string | null;
  sizeBytes: number;
  modifiedMs: number | null;
}

export interface OperationPlanItem {
  groupId: string;
  relativeStem: string;
  rating: number | null;
  framePair: number;
  jpegMetadata: number | null;
  rawXmp: number | null;
  matchedRuleIds: string[];
  matchedRuleNames: string[];
  terminalAction: RuleAction | null;
  status: OperationPlanStatus;
  members: PlannedMember[];
  missingKinds: RuleMemberKind[];
  syncActions: PlannedSyncAction[];
  issues: string[];
}

export interface OperationPlanSummary {
  planId: string;
  root: string;
  totalItems: number;
  ready: number;
  kept: number;
  skipped: number;
  conflicts: number;
  moveGroups: number;
  copyGroups: number;
  cleanupGroups: number;
  syncGroups: number;
  jpegFiles: number;
  rawFiles: number;
  xmpFiles: number;
  copyBytes: number;
  cleanupBytes: number;
  items: OperationPlanItem[];
}

export interface RatingRuleTemplate {
  id: RatingRuleTemplateId;
  name: string;
  detail: string;
}

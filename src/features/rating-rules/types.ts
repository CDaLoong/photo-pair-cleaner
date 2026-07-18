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

export type CleanupExecutionDestination = "quarantine" | "trash";
export type OrganizerAction = "copy" | "move" | "quarantine" | "trash";
export type OrganizerGroupStatus = "success" | "failed" | "partial" | "skipped";
export type RecoveryKind = "restoreMove" | "undoCopy";

export interface FileFingerprint {
  sizeBytes: number;
  modifiedMs: number | null;
  sha256: string;
}

export interface OperationMemberRecord {
  kind: RuleMemberKind;
  sourcePath: string;
  targetPath: string;
  expectedSizeBytes: number;
  expectedModifiedMs: number | null;
  targetSnapshot: FileFingerprint | null;
  message: string;
}

export interface OperationGroupRecord {
  groupId: string;
  relativeStem: string;
  action: OrganizerAction;
  status: OrganizerGroupStatus;
  message: string;
  members: OperationMemberRecord[];
}

export interface OrganizerExecutionSummary {
  operationId: string;
  planId: string;
  succeeded: number;
  failed: number;
  partial: number;
  skipped: number;
  groups: OperationGroupRecord[];
}

export interface RecoveryMemberResult {
  sourcePath: string;
  targetPath: string;
  success: boolean;
  message: string;
}

export interface RecoveryRecord {
  operationId: string;
  groupId: string;
  kind: RecoveryKind;
  createdAtMs: number;
  status: OrganizerGroupStatus;
  message: string;
  members: RecoveryMemberResult[];
}

export interface OperationManifest {
  operationId: string;
  planId: string;
  root: string;
  createdAtMs: number;
  rules: RatingRule[];
  sync: OperationSyncPreference;
  groups: OperationGroupRecord[];
}

export interface OperationHistoryEntry {
  manifest: OperationManifest;
  recoveries: RecoveryRecord[];
  recoverableGroups: number;
}

export interface OrganizerRecoverySummary {
  operationId: string;
  succeeded: number;
  failed: number;
  partial: number;
  results: RecoveryRecord[];
}

export interface RatingRuleTemplate {
  id: RatingRuleTemplateId;
  name: string;
  detail: string;
}

export type RatingSyncMode = "manual" | "automatic";
export type RatingConflictPolicy = "skip" | "framePair" | "external" | "highest";
export type RatingSyncTarget = "rawXmp" | "jpegMetadata";
export type RatingSyncStatus = "ready" | "unchanged" | "conflict";
export type AutoSyncStatus = "disabled" | "unchanged" | "synced" | "pending";
export type CleanupTaskType = "pairCleanup" | "ratingSync";

export interface RatingSyncTargets {
  rawXmp: boolean;
  jpegMetadata: boolean;
}

export interface RatingSyncSettings {
  mode: RatingSyncMode;
  targets: RatingSyncTargets;
  conflictPolicy: RatingConflictPolicy;
  jpegWriteConfirmed: boolean;
}

export interface PendingRatingSync {
  root: string;
  assetId: string;
  rating: number;
  targets: RatingSyncTargets;
  error: string;
  failedAtMs: number;
}

export interface RatingSyncState {
  settings: RatingSyncSettings;
  pending: PendingRatingSync[];
}

export interface RatingSyncWrite {
  target: RatingSyncTarget;
  relativePath: string;
  currentRating: number | null;
  targetRating: number;
}

export interface RatingSyncPlanItem {
  assetId: string;
  relativeStem: string;
  framePair: number;
  jpegMetadata: number | null;
  rawXmp: number | null;
  resolved: number | null;
  status: RatingSyncStatus;
  writes: RatingSyncWrite[];
  issues: string[];
}

export interface RatingSyncPlanSummary {
  planId: string;
  root: string;
  totalItems: number;
  ready: number;
  unchanged: number;
  conflicts: number;
  items: RatingSyncPlanItem[];
}

export interface RatingSyncExecutionResult {
  assetId: string;
  target: RatingSyncTarget;
  relativePath: string;
  success: boolean;
  message: string;
}

export interface RatingSyncExecutionSummary {
  succeeded: number;
  failed: number;
  results: RatingSyncExecutionResult[];
}

export interface AutoSyncOutcome {
  status: AutoSyncStatus;
  message: string | null;
}

export interface RatingSyncPlanRequest {
  root: string;
  minimumRating: number;
  maximumRating: number;
  assetIds: string[];
  targets: RatingSyncTargets;
  conflictPolicy: RatingConflictPolicy;
  jpegWriteConfirmed: boolean;
}

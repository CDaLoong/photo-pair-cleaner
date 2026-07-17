export type ScanMode = "cleanupRaw" | "auditReference";
export type MatchStatus = "matched" | "unmatched";
export type FileKind = "raw" | "reference" | "sidecar";
export type DirectoryKind = "reference" | "raw";

export interface ScanItem {
  id: string;
  relativePath: string;
  fileName: string;
  extension: string;
  sizeBytes: number;
  modifiedMs: number | null;
  matchStatus: MatchStatus;
  kind: FileKind;
  matchedPath: string | null;
}

export interface ScanSummary {
  planId: string;
  mode: ScanMode;
  referenceFiles: number;
  rawFiles: number;
  matched: number;
  unmatched: number;
  sidecars: number;
  reclaimableBytes: number;
  duplicateReferenceKeys: number;
  scannedAtMs: number;
  warnings: string[];
  items: ScanItem[];
}

export type CleanupDestination = "trash" | "quarantine";

export interface CleanupResult {
  relativePath: string;
  success: boolean;
  message: string;
}

export interface CleanupSummary {
  succeeded: number;
  failed: number;
  destination: CleanupDestination;
  operationId: string | null;
  quarantinePath: string | null;
  results: CleanupResult[];
  logPath: string | null;
  logWarning: string | null;
}

export interface RestoreSummary {
  succeeded: number;
  failed: number;
  results: CleanupResult[];
}

export interface QuarantineOperation {
  operationId: string;
  createdAtMs: number;
  moved: number;
  recoverable: number;
  restored: number;
  manifestPath: string;
}

export type FilterMode = "unmatched" | "matched" | "all";
export type WorkPhase = "idle" | "scanning" | "executing";

export interface Notice {
  tone: "success" | "warning" | "error" | "info";
  title: string;
  detail?: string;
}

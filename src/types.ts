export type ScanStatus = "keep" | "delete";
export type FileKind = "raw" | "sidecar";

export interface ScanItem {
  id: string;
  relativePath: string;
  fileName: string;
  extension: string;
  sizeBytes: number;
  modifiedMs: number | null;
  status: ScanStatus;
  kind: FileKind;
  matchedReference: string | null;
}

export interface ScanSummary {
  planId: string;
  referenceFiles: number;
  rawFiles: number;
  matchedRaws: number;
  missingRaws: number;
  sidecars: number;
  reclaimableBytes: number;
  duplicateReferenceKeys: number;
  scannedAtMs: number;
  warnings: string[];
  items: ScanItem[];
}

export interface DeleteSummary {
  succeeded: number;
  failed: number;
  results: Array<{
    relativePath: string;
    success: boolean;
    message: string;
  }>;
  logPath: string | null;
  logWarning: string | null;
}

export type FilterMode = "delete" | "keep" | "all";
export type WorkPhase = "idle" | "scanning" | "deleting";

export interface Notice {
  tone: "success" | "warning" | "error" | "info";
  title: string;
  detail?: string;
}

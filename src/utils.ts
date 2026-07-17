import type {
  CleanupDestination,
  DirectoryKind,
  FileKind,
  MatchStatus,
  Notice,
  ReferenceSourceType,
  ScanItem,
  ScanMode,
  ScanSummary,
} from "./types";

export function canAuditReferenceSource(source: ReferenceSourceType): boolean {
  return source === "directory";
}

export function cleanupActionLabel(destination: CleanupDestination): string {
  return destination === "trash"
    ? "移入系统回收站"
    : "移入 FramePair 隔离区";
}

interface ReclaimableItem {
  matchStatus: MatchStatus;
  kind: FileKind;
  sizeBytes: number;
}

export function isActionableItem(
  item: Pick<ScanItem, "kind" | "matchStatus">,
  mode: ScanMode,
): boolean {
  return mode === "cleanupRaw"
    && item.matchStatus === "unmatched"
    && (item.kind === "raw" || item.kind === "sidecar");
}

export interface DirectoryDropBounds {
  kind: DirectoryKind;
  left: number;
  top: number;
  right: number;
  bottom: number;
}

export function directoryDropTargetAtPoint(
  x: number,
  y: number,
  bounds: DirectoryDropBounds[],
): DirectoryKind | null {
  return bounds.find(
    (item) =>
      x >= item.left && x <= item.right && y >= item.top && y <= item.bottom,
  )?.kind ?? null;
}

export function rawFormatCounts(
  items: Array<Pick<ScanItem, "kind" | "extension">>,
): Record<string, number> {
  return items.reduce<Record<string, number>>((counts, item) => {
    if (item.kind !== "raw") return counts;
    const key = item.extension.replace(/^\./, "").toUpperCase();
    counts[key] = (counts[key] ?? 0) + 1;
    return counts;
  }, {});
}

export function reclaimableBytes(
  items: ReclaimableItem[],
  includeSidecars: boolean,
): number {
  return items
    .filter(
      (item) =>
        item.matchStatus === "unmatched" && (item.kind === "raw" || includeSidecars),
    )
    .reduce((sum, item) => sum + item.sizeBytes, 0);
}

export function cleanableItems(
  items: ScanItem[],
  includeSidecars: boolean,
  mode: ScanMode = "cleanupRaw",
): ScanItem[] {
  return items.filter(
    (item) =>
      isActionableItem(item, mode) && (item.kind === "raw" || includeSidecars),
  );
}

export function selectionBreakdown(items: ScanItem[]): {
  raw: number;
  sidecar: number;
  total: number;
} {
  let raw = 0;
  let sidecar = 0;
  for (const item of items) {
    if (item.kind === "raw") raw += 1;
    else sidecar += 1;
  }
  return { raw, sidecar, total: raw + sidecar };
}

export function decisionReason(item: ScanItem): string {
  if (item.matchStatus === "matched") {
    if (item.kind === "reference") {
      return item.matchedPath ? `匹配 RAW：${item.matchedPath}` : "已找到同路径同名 RAW";
    }
    return item.matchedPath ? `匹配 JPG：${item.matchedPath}` : "已找到同路径同名 JPG";
  }
  return item.kind === "sidecar"
    ? "跟随未配对 RAW 处理"
    : item.kind === "reference"
      ? "未找到同路径同名 RAW"
      : "未找到同路径同名 JPG";
}

export function scanHasBlockingIssues(scan: ScanSummary | null): boolean {
  return scan?.mode === "cleanupRaw" && scan.duplicateReferenceKeys > 0;
}

export function noticeAfterRescanFailure(notice: Notice, error: string): Notice {
  return {
    ...notice,
    detail: [notice.detail, `清理已执行，但自动重新扫描失败：${error}`]
      .filter(Boolean)
      .join("；"),
  };
}

export function formatBytes(bytes: number): string {
  if (bytes <= 0) return "0 B";
  const units = ["B", "KB", "MB", "GB", "TB"];
  const index = Math.min(Math.floor(Math.log(bytes) / Math.log(1024)), units.length - 1);
  const value = bytes / 1024 ** index;
  const digits = index === 0 || value >= 10 ? 0 : 1;
  return `${new Intl.NumberFormat("zh-CN", {
    maximumFractionDigits: digits,
  }).format(value)} ${units[index]}`;
}

export function formatDate(timestamp: number | null): string {
  if (!timestamp) return "未知";
  return new Intl.DateTimeFormat("zh-CN", {
    year: "numeric",
    month: "2-digit",
    day: "2-digit",
    hour: "2-digit",
    minute: "2-digit",
  }).format(new Date(timestamp));
}

export function errorMessage(error: unknown): string {
  if (typeof error === "string") return error;
  if (error instanceof Error) return error.message;
  return "发生未知错误";
}

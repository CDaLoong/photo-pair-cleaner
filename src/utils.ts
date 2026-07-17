import type { FileKind, Notice, ScanStatus } from "./types";

interface ReclaimableItem {
  status: ScanStatus;
  kind: FileKind;
  sizeBytes: number;
}

export function reclaimableBytes(
  items: ReclaimableItem[],
  includeSidecars: boolean,
): number {
  return items
    .filter(
      (item) =>
        item.status === "delete" && (item.kind === "raw" || includeSidecars),
    )
    .reduce((sum, item) => sum + item.sizeBytes, 0);
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

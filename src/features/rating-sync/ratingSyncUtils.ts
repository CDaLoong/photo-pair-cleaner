import type {
  AutoSyncOutcome,
  RatingSyncMode,
  RatingSyncStatus,
  RatingSyncTargets,
} from "./types";

export function syncModeNotice(mode: RatingSyncMode): string {
  return mode === "automatic"
    ? "自动同步只更新评分元数据，不会复制、移动或清理照片。"
    : "手动同步会先生成只读计划，确认后才更新评分元数据。";
}

export function validateSyncTargets(
  targets: RatingSyncTargets,
  jpegWriteConfirmed: boolean,
): { valid: true } | { valid: false; message: string } {
  if (!targets.rawXmp && !targets.jpegMetadata) {
    return { valid: false, message: "请至少选择一个评分同步目标" };
  }
  if (targets.jpegMetadata && !jpegWriteConfirmed) {
    return {
      valid: false,
      message: "请先确认允许 FramePair 修改 JPG 内嵌评分元数据",
    };
  }
  return { valid: true };
}

export function autoSyncOutcomeNotice(outcome: AutoSyncOutcome): {
  tone: "success" | "info" | "warning";
  title: string;
  detail?: string;
} | null {
  if (outcome.status === "disabled") return null;
  if (outcome.status === "synced") {
    return { tone: "success", title: "评分已自动同步" };
  }
  if (outcome.status === "unchanged") {
    return { tone: "info", title: "外部评分已经一致" };
  }
  return {
    tone: "warning",
    title: "FramePair 评分已保存，外部同步待处理",
    ...(outcome.message ? { detail: outcome.message } : {}),
  };
}

export function syncStatusLabel(status: RatingSyncStatus): string {
  if (status === "ready") return "待同步";
  if (status === "unchanged") return "已一致";
  return "存在冲突";
}

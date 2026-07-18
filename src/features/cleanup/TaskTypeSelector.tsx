import { RefreshCw, ScanSearch } from "lucide-react";
import type { CleanupTaskType } from "../rating-sync/types";

interface TaskTypeSelectorProps {
  value: CleanupTaskType;
  busy: boolean;
  onChange: (value: CleanupTaskType) => void;
}

export function TaskTypeSelector({ value, busy, onChange }: TaskTypeSelectorProps) {
  return (
    <div className="cleanup-task-selector" role="group" aria-label="照片处理任务" data-tour="task-type">
      <button type="button" aria-pressed={value === "pairCleanup"} disabled={busy} onClick={() => onChange("pairCleanup")}>
        <ScanSearch aria-hidden="true" size={16} />
        <span><strong>配对清理</strong><small>检查 JPG 与 RAW 配对关系</small></span>
      </button>
      <button type="button" aria-pressed={value === "ratingSync"} disabled={busy} onClick={() => onChange("ratingSync")}>
        <RefreshCw aria-hidden="true" size={16} />
        <span><strong>评分同步</strong><small>同步 FramePair、XMP 与 JPG 评分</small></span>
      </button>
    </div>
  );
}

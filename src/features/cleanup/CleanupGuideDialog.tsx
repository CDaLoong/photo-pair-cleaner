import {
  ArchiveRestore,
  CircleHelp,
  FileCheck2,
  FolderInput,
  RefreshCw,
  ScanSearch,
  Settings2,
} from "lucide-react";
import { GuidedTourDialog } from "../../components/GuidedTourDialog";
import type { GuidedTourStep } from "../../components/GuidedTourDialog";
import type { CleanupTaskType } from "../rating-sync/types";

const GUIDE_STEPS: GuidedTourStep[] = [
  {
    title: "先选择本次要处理的任务",
    description: "配对清理和评分同步共享三阶段流程，但不会互相执行文件操作。",
    icon: CircleHelp,
    selector: "[data-tour='task-type']",
    placement: "bottom",
    points: [
      { label: "配对清理", detail: "检查 JPG/RAW 配对，复核后可移入回收站或隔离区。" },
      { label: "评分同步", detail: "只更新启用的评分元数据，不会移动、复制或清理照片。" },
    ],
    tip: "任务可以随时切换，两边已经填写的内容会分别保留。",
  },
  {
    title: "先选择你要完成的任务",
    description: "这里决定本次扫描的方向。",
    icon: CircleHelp,
    selector: "[data-tour='scan-mode']",
    placement: "left",
    points: [
      { label: "清理无 JPG 的 RAW", detail: "找出没有保留依据的 RAW，复核后才能处理。" },
      { label: "检查无 RAW 的 JPG", detail: "反向检查缺少 RAW 的 JPG，只读并可导出清单。" },
    ],
    tip: "第一次使用请选择“清理无 JPG 的 RAW”。",
  },
  {
    title: "选择照片的保留依据",
    description: "FramePair 根据这里的内容判断哪些 RAW 应该保留。",
    icon: FileCheck2,
    selector: "[data-tour='reference-source']",
    placement: "left",
    points: [
      { label: "JPG 目录（推荐）", detail: "适合先筛选 JPG，再清理对应 RAW 的日常流程。" },
      { label: "清单或 XMP 星级", detail: "适合已有导出清单，或已将 Lightroom/Bridge 评分写入 XMP。" },
    ],
    tip: "不确定时直接选择“JPG 目录”。",
  },
  {
    title: "添加保留目录和 RAW 目录",
    description: "先添加左侧保留依据，再添加右侧 RAW 源目录。",
    icon: FolderInput,
    selector: "[data-tour='reference-picker']",
    placement: "right",
    points: [
      { label: "点击或拖拽", detail: "点击区域使用系统选择器，也可以把文件夹直接拖进来。" },
      { label: "保持子目录结构", detail: "系统按相对路径和文件名匹配，避免同名照片串组。" },
    ],
    tip: "两个根目录可以不同，内部日期或项目子目录应尽量对应。",
  },
  {
    title: "开始只读扫描",
    description: "目录就绪后从这里生成复核结果，扫描本身不会移动文件。",
    icon: ScanSearch,
    selector: "[data-tour='scan-command']",
    placement: "top",
    points: [
      { label: "先看结果", detail: "已配对和未配对文件会分开显示，并说明判断原因。" },
      { label: "再做选择", detail: "未配对不等于已删除，只有你勾选的项目会进入确认窗口。" },
    ],
    tip: "不确定的文件先不要勾选，重新扫描也不会修改照片。",
  },
  {
    title: "复核并安全执行",
    description: "顶部始终显示当前处于选择、复核还是执行阶段。",
    icon: ArchiveRestore,
    selector: "[data-tour='workflow-progress']",
    placement: "bottom",
    points: [
      { label: "复核结果", detail: "核对文件路径、数量、空间和处理去向后再确认。" },
      { label: "保留恢复能力", detail: "可选择系统回收站；第一次使用更建议选 FramePair 隔离区。" },
    ],
    tip: "FramePair 不提供永久删除入口，反向审计也不会处理 JPG。",
  },
];

const RATING_SYNC_GUIDE_STEPS: GuidedTourStep[] = [
  GUIDE_STEPS[0],
  {
    title: "选择要同步的照片根目录",
    description: "FramePair 会递归读取照片组和三种评分来源。",
    icon: FolderInput,
    selector: "[data-tour='rating-sync-root']",
    placement: "bottom",
    points: [
      { label: "点击或拖入", detail: "可以使用系统目录选择器，也可以直接拖入一个文件夹。" },
      { label: "只读索引", detail: "选择目录和生成计划不会写入照片或 XMP。" },
    ],
    tip: "请选择同时包含 JPG、RAW 或对应 XMP 的共同照片根目录。",
  },
  {
    title: "设置范围、目标和冲突策略",
    description: "先决定哪些评分进入计划，以及允许写入哪些元数据。",
    icon: Settings2,
    selector: "[data-tour='rating-sync-settings']",
    placement: "left",
    points: [
      { label: "RAW XMP（推荐）", detail: "创建或更新同名 XMP，永远不修改 RAW 原文件。" },
      { label: "JPG 元数据", detail: "默认关闭，启用后还需要单独确认。" },
      { label: "冲突策略", detail: "默认不覆盖并提示，也可以明确选择 FramePair、外部或较高评分。" },
    ],
    tip: "自动同步也只更新评分元数据，不包含移动、复制或清理。",
  },
  {
    title: "复核只读同步计划",
    description: "每个照片组都会列出三种评分、工作评分、目标文件和状态。",
    icon: ScanSearch,
    selector: "[data-tour='rating-sync-plan']",
    placement: "top",
    points: [
      { label: "待同步", detail: "可以勾选并执行。" },
      { label: "已一致", detail: "不需要重复写入，也不会进入执行选择。" },
      { label: "存在冲突", detail: "必须调整策略或修复元数据后重新生成计划。" },
    ],
    tip: "只有“待同步”照片组能够勾选，冲突不会被静默跳过后继续覆盖。",
  },
  {
    title: "确认后才写入评分元数据",
    description: "执行前会再次校验目标文件与计划快照。",
    icon: RefreshCw,
    selector: "[data-tour='workflow-progress']",
    placement: "bottom",
    points: [
      { label: "扫描后变化会拒绝", detail: "目标新增、被修改或变成符号链接时不会继续写入。" },
      { label: "独立记录结果", detail: "一个照片组失败不会阻断其他组，失败项可以重新生成计划。" },
    ],
    tip: "RAW 文件本身不会被修改；JPG 写入必须由你明确启用。",
  },
];

interface CleanupGuideDialogProps {
  taskType: CleanupTaskType;
  open: boolean;
  onDismiss: () => void;
}

export function CleanupGuideDialog({ taskType, open, onDismiss }: CleanupGuideDialogProps) {
  return (
    <GuidedTourDialog
      open={open}
      onDismiss={onDismiss}
      steps={taskType === "ratingSync" ? RATING_SYNC_GUIDE_STEPS : GUIDE_STEPS}
      label={taskType === "ratingSync" ? "评分同步引导" : "配对清理引导"}
    />
  );
}

import {
  ArchiveRestore,
  CircleHelp,
  FileCheck2,
  FolderInput,
  ListFilter,
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
    description: "配对清理、评分同步和评分整理共享三阶段流程，但各自状态互不混用。",
    icon: CircleHelp,
    selector: "[data-tour='task-type']",
    placement: "bottom",
    points: [
      { label: "配对清理", detail: "检查 JPG/RAW 配对，复核后可移入回收站或隔离区。" },
      { label: "评分同步", detail: "只更新启用的评分元数据，不会移动、复制或清理照片。" },
      { label: "评分整理", detail: "按评分生成移动、复制、保留或待清理计划，复核后安全执行。" },
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

const RATING_RULES_GUIDE_STEPS: GuidedTourStep[] = [
  GUIDE_STEPS[0],
  {
    title: "选择照片根目录和规则模板",
    description: "模板只填充可编辑草稿，不会创建目录或自动扫描。",
    icon: FolderInput,
    selector: "[data-tour='rating-rules-root']",
    placement: "bottom",
    points: [
      { label: "照片根目录", detail: "点击选择或直接拖入包含 JPG、RAW 与 XMP 的共同目录。" },
      { label: "四种模板", detail: "精选归档、低分清理、保留全部备份和完全自定义都可以继续修改。" },
    ],
    tip: "复制和移动模板不会替你创建目标文件夹，目标位置始终由你选择。",
  },
  {
    title: "逐条编辑评分处理规则",
    description: "每条规则独立设置评分、格式、最终操作和目录结构。",
    icon: ListFilter,
    selector: "[data-tour='rating-rules-editor']",
    placement: "left",
    points: [
      { label: "评分与格式", detail: "支持未评分、单星级、上下限和闭区间，以及 JPG/RAW/XMP 任意组合。" },
      { label: "最终操作", detail: "保留、复制、移动或待清理；移动是新增规则的默认值。" },
      { label: "冲突不会按顺序覆盖", detail: "同一照片命中多条规则时会标为冲突，而不是第一条胜出。" },
    ],
    tip: "默认处理整个照片组并保留相对目录结构，避免拆散 JPG、RAW 和 XMP。",
  },
  {
    title: "按需叠加评分同步预览",
    description: "评分同步与文件规则分开计算，只在你明确启用时进入执行计划。",
    icon: RefreshCw,
    selector: "[data-tour='rating-rules-sync']",
    placement: "left",
    points: [
      { label: "RAW XMP", detail: "永远不会修改 RAW 原文件。" },
      { label: "JPG 元数据", detail: "默认关闭，启用后仍需明确确认。" },
      { label: "待清理照片", detail: "只有勾选清理前同步后才会显示同步动作。" },
    ],
    tip: "这里仍然只是预览；自动模式也不会移动、复制或清理照片。",
  },
  {
    title: "复核执行计划",
    description: "查看数量、空间、规则命中、每个源路径、目标和冲突原因；待清理还需选择隔离区或系统回收站。",
    icon: ScanSearch,
    selector: "[data-tour='rating-rules-plan']",
    placement: "top",
    points: [
      { label: "展开照片组", detail: "核对每个 JPG、RAW、XMP 的源路径、目标、大小和修改时间。" },
      { label: "处理冲突", detail: "目标已存在、平铺重名、目录嵌套和多规则命中都会阻止该照片组。" },
    ],
    tip: "阶段三没有文件操作按钮；修改配置后必须重新生成计划。",
  },
];

interface CleanupGuideDialogProps {
  taskType: CleanupTaskType;
  open: boolean;
  onDismiss: () => void;
}

export function CleanupGuideDialog({ taskType, open, onDismiss }: CleanupGuideDialogProps) {
  const steps = taskType === "ratingSync"
    ? RATING_SYNC_GUIDE_STEPS
    : taskType === "ratingRules"
      ? RATING_RULES_GUIDE_STEPS
      : GUIDE_STEPS;
  const label = taskType === "ratingSync"
    ? "评分同步引导"
    : taskType === "ratingRules"
      ? "评分整理引导"
      : "配对清理引导";
  return (
    <GuidedTourDialog
      open={open}
      onDismiss={onDismiss}
      steps={steps}
      label={label}
    />
  );
}

import {
  ArchiveRestore,
  CircleHelp,
  FileCheck2,
  FolderInput,
  ScanSearch,
} from "lucide-react";
import { GuidedTourDialog } from "../../components/GuidedTourDialog";
import type { GuidedTourStep } from "../../components/GuidedTourDialog";

const GUIDE_STEPS: GuidedTourStep[] = [
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

interface CleanupGuideDialogProps {
  open: boolean;
  onDismiss: () => void;
}

export function CleanupGuideDialog({ open, onDismiss }: CleanupGuideDialogProps) {
  return <GuidedTourDialog open={open} onDismiss={onDismiss} steps={GUIDE_STEPS} />;
}

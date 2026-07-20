import {
  FolderTree,
  Images,
  MousePointer2,
  SlidersHorizontal,
  Star,
} from "lucide-react";
import { GuidedTourDialog } from "../../components/GuidedTourDialog";
import type { GuidedTourStep } from "../../components/GuidedTourDialog";

const PREVIEW_GUIDE_STEPS: GuidedTourStep[] = [
  {
    title: "先选择要浏览的子目录",
    description: "目录树让一个照片根目录保持完整，同时只查看当前项目或日期。",
    icon: FolderTree,
    selector: "[data-preview-tour='directories']",
    placement: "right",
    points: [
      { label: "全部照片", detail: "查看所选根目录及所有子目录中的照片。" },
      { label: "点击子目录", detail: "只展示该目录及其下级目录，数量会同步更新。" },
    ],
    tip: "目录树只用于浏览范围，不会移动或修改照片。",
  },
  {
    title: "按照片组合筛选",
    description: "这里区分配对照片与只有单一格式的照片。",
    icon: SlidersHorizontal,
    selector: "[data-preview-tour='type-filters']",
    placement: "bottom",
    points: [
      { label: "已配对", detail: "同一路径、同文件名，同时存在 JPG 和 RAW。" },
      { label: "仅 JPG / 仅 RAW", detail: "只存在一种格式；按钮后的数字是当前目录数量。" },
    ],
    tip: "数量为 0 的分类不能点击，避免进入含义不明的空白页。",
  },
  {
    title: "用单张预览快速选片",
    description: "当前引导已打开单张预览，照片始终按完整比例展示。",
    icon: Images,
    selector: "[data-preview-tour='loupe']",
    placement: "top",
    points: [
      { label: "左右切换", detail: "使用方向键、顶部按钮或底部胶片栏切换照片。" },
      { label: "回到网格", detail: "按 Esc，或点击右上角网格按钮。" },
    ],
    tip: "macOS 使用系统 Quick Look 原生预览，相邻照片只准备高清占位，避免后台负载影响切换。",
  },
  {
    title: "点击星星或按数字键评分",
    description: "评分保存在 FramePair 本地，不会改动原始照片。",
    icon: Star,
    selector: "[data-preview-tour='rating']",
    placement: "top",
    points: [
      { label: "设置 1-5 星", detail: "点击星星，或直接按键盘数字 1 到 5。" },
      { label: "清除评分", detail: "再次点击当前星级，或按数字 0。" },
    ],
    tip: "评分会在网格和胶片栏显示，并在下次打开目录时恢复。",
  },
  {
    title: "筛选评分，也可以右键操作",
    description: "完成初筛后，只查看达到指定星级的照片。",
    icon: MousePointer2,
    selector: "[data-preview-tour='rating-filter']",
    placement: "bottom",
    points: [
      { label: "最低评分", detail: "例如选择“4 星以上”，只保留 4 星和 5 星照片。" },
      { label: "中文右键菜单", detail: "在照片或胶片栏右键，可评分、预览、定位或打开编辑。" },
    ],
    tip: "类型、目录、搜索和评分条件可以组合使用。",
  },
];

interface PreviewGuideDialogProps {
  open: boolean;
  onDismiss: () => void;
}

export function PreviewGuideDialog({ open, onDismiss }: PreviewGuideDialogProps) {
  return <GuidedTourDialog open={open} onDismiss={onDismiss} steps={PREVIEW_GUIDE_STEPS} label="照片浏览引导" />;
}

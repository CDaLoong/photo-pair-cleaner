import {
  Download,
  ImagePlus,
  Layers3,
  LayoutTemplate,
  PanelsTopLeft,
} from "lucide-react";
import { GuidedTourDialog } from "../../components/GuidedTourDialog";
import type { GuidedTourStep } from "../../components/GuidedTourDialog";

const WATERMARK_GUIDE_STEPS: GuidedTourStep[] = [
  {
    title: "导入要发布的 JPG",
    description: "可以选择目录、拖入 JPG，或接收照片浏览中的当前筛选结果。",
    icon: ImagePlus,
    selector: "[data-watermark-tour='sources-templates']",
    placement: "right",
    points: [
      { label: "只处理 JPG", detail: "JPG+RAW 组合只读取 JPG，RAW-only 会明确跳过。" },
      { label: "任务列表固定", detail: "导入后形成快照，原筛选变化不会偷偷改变列表。" },
    ],
    tip: "导入和预览不会修改任何原照片。",
  },
  {
    title: "从模板开始",
    description: "选择内置模板，或打开保存在本机的自定义模板。",
    icon: LayoutTemplate,
    selector: "[data-watermark-tour='templates']",
    placement: "right",
    points: [
      { label: "内置模板", detail: "内置模板只读，调整后可以另存为我的模板。" },
      { label: "三种方向", detail: "一个模板同时保存横版、竖版和方形布局。" },
    ],
    tip: "模板只保存排版和素材，不包含待处理照片。",
  },
  {
    title: "在画布调整图层",
    description: "选择文字、EXIF 或 Logo 图层，拖动后再用右侧属性精确微调。",
    icon: Layers3,
    selector: "[data-watermark-tour='canvas']",
    placement: "top",
    points: [
      { label: "选择作用区域", detail: "图层可以锚定照片、指定边框或整个画布。" },
      { label: "精确控制", detail: "位置、尺寸、角度、透明度和样式都可以输入数值。" },
    ],
    tip: "撤销和重做会把一次拖动视为一个操作。",
  },
  {
    title: "检查三种照片方向",
    description: "通过胶片栏切换照片，检查横版、竖版和方形是否都排版正确。",
    icon: PanelsTopLeft,
    selector: "[data-watermark-tour='filmstrip']",
    placement: "top",
    points: [
      { label: "自动匹配版式", detail: "照片会按校正方向后的宽高自动选择布局。" },
      { label: "单张微调", detail: "只调整当前照片的缩放和位置，不破坏模板结构。" },
    ],
    tip: "选中照片会自动滚动到胶片栏可见范围。",
  },
  {
    title: "确认设置并导出副本",
    description: "核对格式、尺寸、元数据、目录和同名处理后再开始批量导出。",
    icon: Download,
    selector: "[data-watermark-tour='export']",
    placement: "left",
    points: [
      { label: "JPEG 或 PNG", detail: "JPEG 适合发布，PNG 支持无损与透明背景。" },
      { label: "默认隐私模式", detail: "保留拍摄信息，同时移除 GPS 和设备序列号。" },
    ],
    tip: "FramePair 只生成新副本，绝不覆盖源照片。",
  },
];

interface WatermarkGuideDialogProps {
  open: boolean;
  onDismiss: () => void;
}

export function WatermarkGuideDialog({ open, onDismiss }: WatermarkGuideDialogProps) {
  return (
    <GuidedTourDialog
      open={open}
      onDismiss={onDismiss}
      steps={WATERMARK_GUIDE_STEPS}
      label="水印导出引导"
    />
  );
}

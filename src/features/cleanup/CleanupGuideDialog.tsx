import {
  ArchiveRestore,
  ArrowLeft,
  ArrowRight,
  Check,
  CheckCircle2,
  CircleHelp,
  FileCheck2,
  FolderInput,
  ScanSearch,
  X,
} from "lucide-react";
import { useEffect, useLayoutEffect, useRef, useState } from "react";
import type { LucideIcon } from "lucide-react";
import type { CSSProperties } from "react";

type Placement = "top" | "right" | "bottom" | "left";

interface GuidePoint {
  label: string;
  detail: string;
}

interface GuideStep {
  title: string;
  description: string;
  icon: LucideIcon;
  selector: string;
  placement: Placement;
  points: GuidePoint[];
  tip: string;
}

interface SpotlightRect {
  top: number;
  left: number;
  width: number;
  height: number;
}

interface PopoverPosition {
  top: number;
  left: number;
  placement: Placement;
}

const GUIDE_STEPS: GuideStep[] = [
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

const SCREEN_MARGIN = 12;
const TARGET_PADDING = 8;
const POPOVER_GAP = 14;

function clamp(value: number, minimum: number, maximum: number) {
  return Math.min(Math.max(value, minimum), maximum);
}

function positionPopover(
  target: SpotlightRect,
  popover: { width: number; height: number },
  preferred: Placement,
): PopoverPosition {
  const viewportWidth = window.innerWidth;
  const viewportHeight = window.innerHeight;
  const placementOrder: Placement[] = [preferred, "right", "left", "bottom", "top"];
  const placements = placementOrder.filter((value, index, values) => values.indexOf(value) === index);

  for (const placement of placements) {
    const centeredLeft = target.left + target.width / 2 - popover.width / 2;
    const centeredTop = target.top + target.height / 2 - popover.height / 2;
    const candidate = placement === "right"
      ? { top: centeredTop, left: target.left + target.width + POPOVER_GAP }
      : placement === "left"
        ? { top: centeredTop, left: target.left - popover.width - POPOVER_GAP }
        : placement === "bottom"
          ? { top: target.top + target.height + POPOVER_GAP, left: centeredLeft }
          : { top: target.top - popover.height - POPOVER_GAP, left: centeredLeft };
    const fits = candidate.left >= SCREEN_MARGIN
      && candidate.top >= SCREEN_MARGIN
      && candidate.left + popover.width <= viewportWidth - SCREEN_MARGIN
      && candidate.top + popover.height <= viewportHeight - SCREEN_MARGIN;
    if (fits) return { ...candidate, placement };
  }

  return {
    top: clamp(target.top + target.height + POPOVER_GAP, SCREEN_MARGIN, viewportHeight - popover.height - SCREEN_MARGIN),
    left: clamp(target.left + target.width / 2 - popover.width / 2, SCREEN_MARGIN, viewportWidth - popover.width - SCREEN_MARGIN),
    placement: "bottom",
  };
}

interface CleanupGuideDialogProps {
  open: boolean;
  onDismiss: () => void;
}

export function CleanupGuideDialog({ open, onDismiss }: CleanupGuideDialogProps) {
  const dialogRef = useRef<HTMLDialogElement>(null);
  const popoverRef = useRef<HTMLDivElement>(null);
  const [stepIndex, setStepIndex] = useState(0);
  const [spotlight, setSpotlight] = useState<SpotlightRect | null>(null);
  const [popoverPosition, setPopoverPosition] = useState<PopoverPosition | null>(null);
  const step = GUIDE_STEPS[stepIndex];
  const isLastStep = stepIndex === GUIDE_STEPS.length - 1;

  useEffect(() => {
    const dialog = dialogRef.current;
    if (!dialog) return;
    if (open) {
      setStepIndex(0);
      if (!dialog.open) dialog.showModal();
    } else if (dialog.open) {
      dialog.close();
    }
  }, [open]);

  useLayoutEffect(() => {
    if (!open) return;
    const requestedTarget = document.querySelector<HTMLElement>(step.selector);
    const requestedBounds = requestedTarget?.getBoundingClientRect();
    const target = requestedTarget && requestedBounds && requestedBounds.width > 0 && requestedBounds.height > 0
      ? requestedTarget
      : document.querySelector<HTMLElement>(".app-header");
    if (!target) return;

    const updateSpotlight = () => {
      const bounds = target.getBoundingClientRect();
      const left = clamp(bounds.left - TARGET_PADDING, SCREEN_MARGIN, window.innerWidth - SCREEN_MARGIN);
      const top = clamp(bounds.top - TARGET_PADDING, SCREEN_MARGIN, window.innerHeight - SCREEN_MARGIN);
      const right = clamp(bounds.right + TARGET_PADDING, SCREEN_MARGIN, window.innerWidth - SCREEN_MARGIN);
      const bottom = clamp(bounds.bottom + TARGET_PADDING, SCREEN_MARGIN, window.innerHeight - SCREEN_MARGIN);
      setSpotlight({ top, left, width: Math.max(0, right - left), height: Math.max(0, bottom - top) });
    };

    const centerTarget = () => {
      const scrollContainer = target.closest<HTMLElement>(".setup-view");
      if (!scrollContainer) {
        target.scrollIntoView({ block: "center", inline: "nearest", behavior: "auto" });
        return;
      }
      const targetBounds = target.getBoundingClientRect();
      const containerBounds = scrollContainer.getBoundingClientRect();
      scrollContainer.scrollTop += targetBounds.top
        - containerBounds.top
        - (scrollContainer.clientHeight - targetBounds.height) / 2;
    };

    centerTarget();
    let secondFrame = 0;
    let resizeFrame = 0;
    const firstFrame = requestAnimationFrame(() => {
      secondFrame = requestAnimationFrame(updateSpotlight);
    });
    const handleResize = () => {
      centerTarget();
      cancelAnimationFrame(resizeFrame);
      resizeFrame = requestAnimationFrame(updateSpotlight);
    };
    window.addEventListener("resize", handleResize);
    window.addEventListener("scroll", updateSpotlight, { capture: true, passive: true });

    return () => {
      cancelAnimationFrame(firstFrame);
      cancelAnimationFrame(secondFrame);
      cancelAnimationFrame(resizeFrame);
      window.removeEventListener("resize", handleResize);
      window.removeEventListener("scroll", updateSpotlight, true);
    };
  }, [open, step.selector]);

  useLayoutEffect(() => {
    if (!open || !spotlight || !popoverRef.current) return;
    const bounds = popoverRef.current.getBoundingClientRect();
    setPopoverPosition(positionPopover(spotlight, bounds, step.placement));
  }, [open, spotlight, step.placement]);

  const StepIcon = step.icon;
  const maskSegments: CSSProperties[] = spotlight
    ? [
        { inset: `0 0 auto 0`, height: spotlight.top },
        { top: spotlight.top, left: 0, width: spotlight.left, height: spotlight.height },
        { top: spotlight.top, left: spotlight.left + spotlight.width, right: 0, height: spotlight.height },
        { top: spotlight.top + spotlight.height, right: 0, bottom: 0, left: 0 },
      ]
    : [{ inset: 0 }];

  return (
    <dialog
      ref={dialogRef}
      className="tour-layer"
      aria-labelledby="tour-title"
      aria-describedby="tour-description"
      onCancel={(event) => {
        event.preventDefault();
        onDismiss();
      }}
    >
      {maskSegments.map((style, index) => <div className="tour-mask" style={style} key={index} />)}
      {spotlight ? <div className="tour-spotlight" style={spotlight} aria-hidden="true" /> : null}

      <div
        ref={popoverRef}
        className="tour-popover"
        data-placement={popoverPosition?.placement ?? step.placement}
        style={popoverPosition ? { top: popoverPosition.top, left: popoverPosition.left } : undefined}
      >
        <div className="tour-meta">
          <span>使用引导 · {stepIndex + 1}/{GUIDE_STEPS.length}</span>
          <button type="button" onClick={onDismiss} aria-label="关闭使用引导" title="关闭使用引导"><X aria-hidden="true" size={16} /></button>
        </div>
        <div className="tour-heading">
          <span><StepIcon aria-hidden="true" size={19} /></span>
          <div>
            <h2 id="tour-title">{step.title}</h2>
            <p id="tour-description">{step.description}</p>
          </div>
        </div>
        <ul className="tour-points">
          {step.points.map((point) => (
            <li key={point.label}>
              <CheckCircle2 aria-hidden="true" size={16} />
              <span><strong>{point.label}</strong><small>{point.detail}</small></span>
            </li>
          ))}
        </ul>
        <div className="tour-tip"><Check aria-hidden="true" size={15} />{step.tip}</div>
        <div className="tour-progress" aria-label={`引导进度：第 ${stepIndex + 1} 步，共 ${GUIDE_STEPS.length} 步`}>
          {GUIDE_STEPS.map((item, index) => <span key={item.title} className={index <= stepIndex ? "is-complete" : ""} />)}
        </div>
        <footer className="tour-actions">
          <button className="tour-skip" type="button" onClick={onDismiss}>跳过</button>
          <div>
            <button className="secondary-command" type="button" onClick={() => setStepIndex((index) => index - 1)} disabled={stepIndex === 0} aria-label="上一步" title="上一步">
              <ArrowLeft aria-hidden="true" size={16} />
            </button>
            <button className="primary-command" type="button" onClick={() => isLastStep ? onDismiss() : setStepIndex((index) => index + 1)}>
              {isLastStep ? <><Check aria-hidden="true" size={16} />完成</> : <>下一步<ArrowRight aria-hidden="true" size={16} /></>}
            </button>
          </div>
        </footer>
      </div>
    </dialog>
  );
}

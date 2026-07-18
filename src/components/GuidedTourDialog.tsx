import {
  ArrowLeft,
  ArrowRight,
  Check,
  CheckCircle2,
  X,
} from "lucide-react";
import { useEffect, useId, useLayoutEffect, useRef, useState } from "react";
import type { LucideIcon } from "lucide-react";
import type { CSSProperties } from "react";

export type TourPlacement = "top" | "right" | "bottom" | "left";

export interface GuidedTourPoint {
  label: string;
  detail: string;
}

export interface GuidedTourStep {
  title: string;
  description: string;
  icon: LucideIcon;
  selector: string;
  placement: TourPlacement;
  points: GuidedTourPoint[];
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
  placement: TourPlacement;
}

const SCREEN_MARGIN = 12;
const TARGET_PADDING = 8;
const POPOVER_GAP = 14;

function clamp(value: number, minimum: number, maximum: number) {
  return Math.min(Math.max(value, minimum), maximum);
}

function positionPopover(
  target: SpotlightRect,
  popover: { width: number; height: number },
  preferred: TourPlacement,
): PopoverPosition {
  const viewportWidth = window.innerWidth;
  const viewportHeight = window.innerHeight;
  const placementOrder: TourPlacement[] = [preferred, "right", "left", "bottom", "top"];
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

interface GuidedTourDialogProps {
  open: boolean;
  onDismiss: () => void;
  steps: GuidedTourStep[];
  label?: string;
}

export function GuidedTourDialog({
  open,
  onDismiss,
  steps,
  label = "使用引导",
}: GuidedTourDialogProps) {
  const dialogRef = useRef<HTMLDialogElement>(null);
  const popoverRef = useRef<HTMLDivElement>(null);
  const titleId = useId();
  const descriptionId = useId();
  const [stepIndex, setStepIndex] = useState(0);
  const [spotlight, setSpotlight] = useState<SpotlightRect | null>(null);
  const [popoverPosition, setPopoverPosition] = useState<PopoverPosition | null>(null);
  const step = steps[stepIndex] ?? steps[0];
  const isLastStep = stepIndex === steps.length - 1;

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
    if (!open || !step) return;
    const requestedTarget = document.querySelector<HTMLElement>(step.selector);
    const requestedBounds = requestedTarget?.getBoundingClientRect();
    const fallbackTarget = Array.from(document.querySelectorAll<HTMLElement>(".preview-header, .app-header"))
      .find((element) => {
        const bounds = element.getBoundingClientRect();
        return bounds.width > 0 && bounds.height > 0;
      });
    const target = requestedTarget && requestedBounds && requestedBounds.width > 0 && requestedBounds.height > 0
      ? requestedTarget
      : fallbackTarget;
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
      const scrollContainer = target.closest<HTMLElement>(".setup-view, .photo-grid-scroll, .folder-tree-scroll, .rating-rules-workspace, .watermark-studio");
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
  }, [open, step]);

  useLayoutEffect(() => {
    if (!open || !spotlight || !popoverRef.current || !step) return;
    const bounds = popoverRef.current.getBoundingClientRect();
    setPopoverPosition(positionPopover(spotlight, bounds, step.placement));
  }, [open, spotlight, step]);

  if (!step) return null;
  const StepIcon = step.icon;
  const maskSegments: CSSProperties[] = spotlight
    ? [
        { inset: "0 0 auto 0", height: spotlight.top },
        { top: spotlight.top, left: 0, width: spotlight.left, height: spotlight.height },
        { top: spotlight.top, left: spotlight.left + spotlight.width, right: 0, height: spotlight.height },
        { top: spotlight.top + spotlight.height, right: 0, bottom: 0, left: 0 },
      ]
    : [{ inset: 0 }];

  return (
    <dialog
      ref={dialogRef}
      className="tour-layer"
      aria-labelledby={titleId}
      aria-describedby={descriptionId}
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
          <span>{label} · {stepIndex + 1}/{steps.length}</span>
          <button type="button" onClick={onDismiss} aria-label={`关闭${label}`} title={`关闭${label}`}><X aria-hidden="true" size={16} /></button>
        </div>
        <div className="tour-heading">
          <span><StepIcon aria-hidden="true" size={19} /></span>
          <div>
            <h2 id={titleId}>{step.title}</h2>
            <p id={descriptionId}>{step.description}</p>
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
        <div className="tour-progress" style={{ gridTemplateColumns: `repeat(${steps.length}, minmax(0, 1fr))` }} aria-label={`引导进度：第 ${stepIndex + 1} 步，共 ${steps.length} 步`}>
          {steps.map((item, index) => <span key={item.title} className={index <= stepIndex ? "is-complete" : ""} />)}
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

import { ChevronDown, FolderTree, Image, ListFilter, Stamp } from "lucide-react";
import { useEffect, useRef, useState } from "react";
import type { WatermarkTransferScope } from "./types";

interface SendToWatermarkMenuProps {
  counts: Record<WatermarkTransferScope, number>;
  disabled?: boolean;
  onSelect: (scope: WatermarkTransferScope) => void;
}

const OPTIONS: Array<{
  scope: WatermarkTransferScope;
  label: string;
  icon: typeof Image;
}> = [
  { scope: "currentPhoto", label: "当前照片", icon: Image },
  { scope: "currentDirectory", label: "当前目录", icon: FolderTree },
  { scope: "currentFilter", label: "当前筛选结果", icon: ListFilter },
];

export function SendToWatermarkMenu({
  counts,
  disabled = false,
  onSelect,
}: SendToWatermarkMenuProps) {
  const [open, setOpen] = useState(false);
  const containerRef = useRef<HTMLDivElement>(null);

  useEffect(() => {
    if (!open) return;
    const dismiss = (event: PointerEvent) => {
      if (!containerRef.current?.contains(event.target as Node)) setOpen(false);
    };
    const dismissWithEscape = (event: KeyboardEvent) => {
      if (event.key === "Escape") setOpen(false);
    };
    window.addEventListener("pointerdown", dismiss);
    window.addEventListener("keydown", dismissWithEscape);
    return () => {
      window.removeEventListener("pointerdown", dismiss);
      window.removeEventListener("keydown", dismissWithEscape);
    };
  }, [open]);

  return (
    <div ref={containerRef} className="send-watermark-menu">
      <button
        className="secondary-command"
        type="button"
        disabled={disabled}
        aria-haspopup="menu"
        aria-expanded={open}
        onClick={() => setOpen((current) => !current)}
      >
        <Stamp aria-hidden="true" size={16} />发送水印<ChevronDown aria-hidden="true" size={14} />
      </button>
      {open ? (
        <div className="send-watermark-popover" role="menu" aria-label="发送到水印导出">
          {OPTIONS.map(({ scope, label, icon: Icon }) => (
            <button
              key={scope}
              type="button"
              role="menuitem"
              disabled={counts[scope] === 0}
              onClick={() => {
                onSelect(scope);
                setOpen(false);
              }}
            >
              <Icon aria-hidden="true" size={16} />
              <span>{label}</span>
              <small>{counts[scope]} 张</small>
            </button>
          ))}
        </div>
      ) : null}
    </div>
  );
}

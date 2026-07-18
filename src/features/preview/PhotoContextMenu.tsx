import {
  ExternalLink,
  FolderOpen,
  Grid3X3,
  Maximize2,
  Star,
  X,
} from "lucide-react";
import { useEffect, useRef } from "react";
import type { CSSProperties } from "react";
import type { PhotoAsset, PreviewView } from "./types";

interface PhotoContextMenuProps {
  asset: PhotoAsset;
  position: { left: number; top: number };
  view: PreviewView;
  editorLabel: string;
  ratingBusy: boolean;
  editorBusy: boolean;
  onRate: (rating: number) => void;
  onOpenLoupe: () => void;
  onShowGrid: () => void;
  onReveal: () => void;
  onEdit: () => void;
  onDismiss: () => void;
}

export function PhotoContextMenu({
  asset,
  position,
  view,
  editorLabel,
  ratingBusy,
  editorBusy,
  onRate,
  onOpenLoupe,
  onShowGrid,
  onReveal,
  onEdit,
  onDismiss,
}: PhotoContextMenuProps) {
  const menuRef = useRef<HTMLDivElement>(null);

  useEffect(() => {
    menuRef.current?.focus();
    const dismiss = () => onDismiss();
    window.addEventListener("pointerdown", dismiss);
    window.addEventListener("blur", dismiss);
    window.addEventListener("resize", dismiss);
    window.addEventListener("scroll", dismiss, true);
    return () => {
      window.removeEventListener("pointerdown", dismiss);
      window.removeEventListener("blur", dismiss);
      window.removeEventListener("resize", dismiss);
      window.removeEventListener("scroll", dismiss, true);
    };
  }, [onDismiss]);

  function run(command: () => void) {
    command();
    onDismiss();
  }

  return (
    <div
      ref={menuRef}
      className="photo-context-menu"
      style={position as CSSProperties}
      role="menu"
      tabIndex={-1}
      aria-label={`${asset.name} 操作菜单`}
      onPointerDown={(event) => event.stopPropagation()}
      onKeyDown={(event) => {
        if (event.key === "Escape") {
          event.stopPropagation();
          onDismiss();
        }
      }}
    >
      <div className="photo-context-heading"><strong>{asset.name}</strong><small>{asset.extensions.join(" + ")}</small></div>
      <div className="photo-context-rating" role="group" aria-label="设置评分">
        <span>评分</span>
        <div>
          {[1, 2, 3, 4, 5].map((value) => (
            <button key={value} type="button" disabled={ratingBusy} className={asset.rating >= value ? "is-active" : ""} onClick={() => run(() => onRate(asset.rating === value ? 0 : value))} aria-label={asset.rating === value ? `清除 ${value} 星评分` : `设为 ${value} 星`} title={asset.rating === value ? "再次点击清除评分" : `设为 ${value} 星`}>
              <Star aria-hidden="true" size={16} fill={asset.rating >= value ? "currentColor" : "none"} />
            </button>
          ))}
        </div>
        {asset.rating > 0 ? <button className="photo-context-clear" type="button" onClick={() => run(() => onRate(0))} aria-label="清除评分" title="清除评分"><X aria-hidden="true" size={14} /></button> : null}
      </div>
      <div className="photo-context-separator" />
      {view === "grid" ? (
        <button type="button" role="menuitem" onClick={() => run(onOpenLoupe)}><Maximize2 aria-hidden="true" size={16} /><span>单张预览</span></button>
      ) : <button type="button" role="menuitem" onClick={() => run(onShowGrid)}><Grid3X3 aria-hidden="true" size={16} /><span>返回网格</span></button>}
      <button type="button" role="menuitem" onClick={() => run(onReveal)}><FolderOpen aria-hidden="true" size={16} /><span>在文件管理器中显示</span></button>
      <button type="button" role="menuitem" disabled={editorBusy} onClick={() => run(onEdit)}><ExternalLink aria-hidden="true" size={16} /><span>使用 {editorLabel} 打开</span></button>
    </div>
  );
}

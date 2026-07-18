import {
  Eye,
  Download,
  FolderOpen,
  Maximize2,
  Minimize2,
  PanelLeftClose,
  PanelLeftOpen,
  PanelRightClose,
  PanelRightOpen,
  Redo2,
  Stamp,
  Undo2,
} from "lucide-react";
import type { WatermarkOrientation } from "./types";

const ORIENTATION_LABELS: Record<WatermarkOrientation, string> = {
  landscape: "横版",
  portrait: "竖版",
  square: "方形",
};

interface WatermarkHeaderProps {
  photoCount: number;
  templateName: string;
  orientation: WatermarkOrientation;
  busy: boolean;
  canUndo: boolean;
  canRedo: boolean;
  compareOriginal: boolean;
  compareAvailable: boolean;
  leftCollapsed: boolean;
  rightCollapsed: boolean;
  immersive: boolean;
  workspaceReady: boolean;
  exportDisabled: boolean;
  onChooseDirectory: () => void;
  onUndo: () => void;
  onRedo: () => void;
  onCompare: () => void;
  onToggleLeft: () => void;
  onToggleRight: () => void;
  onToggleImmersive: () => void;
  onExport: () => void;
}

export function WatermarkHeader({
  photoCount,
  templateName,
  orientation,
  busy,
  canUndo,
  canRedo,
  compareOriginal,
  compareAvailable,
  leftCollapsed,
  rightCollapsed,
  immersive,
  workspaceReady,
  exportDisabled,
  onChooseDirectory,
  onUndo,
  onRedo,
  onCompare,
  onToggleLeft,
  onToggleRight,
  onToggleImmersive,
  onExport,
}: WatermarkHeaderProps) {
  return (
    <header className="watermark-studio-header">
      <div className="module-heading watermark-studio-heading">
        <Stamp aria-hidden="true" size={20} />
        <div><strong>水印导出</strong><span>{photoCount > 0 ? `${templateName} · ${ORIENTATION_LABELS[orientation]}` : "边框、署名与发布副本"}</span></div>
      </div>

      <div className="watermark-history-tools" role="group" aria-label="编辑历史">
        <button type="button" aria-label="撤销" title="撤销" onClick={onUndo} disabled={!canUndo}><Undo2 aria-hidden="true" size={17} /></button>
        <button type="button" aria-label="重做" title="重做" onClick={onRedo} disabled={!canRedo}><Redo2 aria-hidden="true" size={17} /></button>
      </div>

      <div className="watermark-header-context">
        {photoCount > 0 ? <span>{photoCount} 张 JPG/JPEG</span> : <span>尚未添加照片</span>}
        <button
          className={compareOriginal ? "watermark-icon-command is-active" : "watermark-icon-command"}
          type="button"
          aria-label="对比原图"
          title="对比原图"
          aria-pressed={compareOriginal}
          onClick={onCompare}
          disabled={!compareAvailable}
        ><Eye aria-hidden="true" size={17} /></button>
        <button className="watermark-icon-command" type="button" aria-label={leftCollapsed ? "展开照片与模板" : "收起照片与模板"} title={leftCollapsed ? "展开照片与模板" : "收起照片与模板"} onClick={onToggleLeft} disabled={!workspaceReady}>
          {leftCollapsed ? <PanelLeftOpen aria-hidden="true" size={17} /> : <PanelLeftClose aria-hidden="true" size={17} />}
        </button>
        <button className="watermark-icon-command" type="button" aria-label={rightCollapsed ? "展开属性面板" : "收起属性面板"} title={rightCollapsed ? "展开属性面板" : "收起属性面板"} onClick={onToggleRight} disabled={!workspaceReady}>
          {rightCollapsed ? <PanelRightOpen aria-hidden="true" size={17} /> : <PanelRightClose aria-hidden="true" size={17} />}
        </button>
        <button className="watermark-icon-command" type="button" aria-label={immersive ? "退出沉浸模式" : "进入沉浸模式"} title={immersive ? "退出沉浸模式" : "进入沉浸模式"} onClick={onToggleImmersive} disabled={!workspaceReady}>
          {immersive ? <Minimize2 aria-hidden="true" size={17} /> : <Maximize2 aria-hidden="true" size={17} />}
        </button>
        <button className="secondary-command watermark-choose-command" type="button" aria-label="选择照片目录" onClick={onChooseDirectory} disabled={busy}>
          <FolderOpen aria-hidden="true" size={17} /><span>{busy ? "正在载入" : "选择目录"}</span>
        </button>
        <button className="primary-command watermark-export-command" data-watermark-tour="export" type="button" onClick={onExport} disabled={exportDisabled}>
          <Download aria-hidden="true" size={17} /><span>导出副本</span>
        </button>
      </div>
    </header>
  );
}

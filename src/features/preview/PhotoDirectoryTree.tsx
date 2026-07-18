import {
  ChevronDown,
  ChevronRight,
  Folder,
  FolderOpen,
  Images,
  PanelLeftClose,
  PanelLeftOpen,
} from "lucide-react";
import { useState } from "react";
import type { CSSProperties } from "react";
import type { PhotoDirectoryNode } from "./types";

interface DirectoryBranchProps {
  nodes: PhotoDirectoryNode[];
  depth: number;
  selectedPath: string;
  expandedPaths: Set<string>;
  onSelect: (path: string) => void;
  onToggle: (path: string) => void;
}

function DirectoryBranch({
  nodes,
  depth,
  selectedPath,
  expandedPaths,
  onSelect,
  onToggle,
}: DirectoryBranchProps) {
  return nodes.map((node) => {
    const expanded = expandedPaths.has(node.path);
    const hasChildren = node.children.length > 0;
    const selected = selectedPath === node.path;
    return (
      <li key={node.path}>
        <div className={selected ? "folder-tree-row is-selected" : "folder-tree-row"} style={{ "--folder-depth": depth } as CSSProperties}>
          {hasChildren ? (
            <button className="folder-tree-toggle" type="button" onClick={() => onToggle(node.path)} aria-label={expanded ? `收起 ${node.name}` : `展开 ${node.name}`} title={expanded ? "收起子目录" : "展开子目录"}>
              {expanded ? <ChevronDown aria-hidden="true" size={14} /> : <ChevronRight aria-hidden="true" size={14} />}
            </button>
          ) : <span className="folder-tree-toggle" aria-hidden="true" />}
          <button className="folder-tree-select" type="button" onClick={() => onSelect(node.path)} aria-current={selected ? "page" : undefined} title={node.path}>
            {selected ? <FolderOpen aria-hidden="true" size={15} /> : <Folder aria-hidden="true" size={15} />}
            <span>{node.name}</span>
            <small>{node.totalCount}</small>
          </button>
        </div>
        {hasChildren && expanded ? (
          <ul>
            <DirectoryBranch nodes={node.children} depth={depth + 1} selectedPath={selectedPath} expandedPaths={expandedPaths} onSelect={onSelect} onToggle={onToggle} />
          </ul>
        ) : null}
      </li>
    );
  });
}

interface PhotoDirectoryTreeProps {
  nodes: PhotoDirectoryNode[];
  totalCount: number;
  selectedPath: string;
  collapsed: boolean;
  onSelect: (path: string) => void;
  onCollapsedChange: (collapsed: boolean) => void;
}

export function PhotoDirectoryTree({
  nodes,
  totalCount,
  selectedPath,
  collapsed,
  onSelect,
  onCollapsedChange,
}: PhotoDirectoryTreeProps) {
  const [expandedPaths, setExpandedPaths] = useState(() => new Set(nodes.map((node) => node.path)));

  function toggle(path: string) {
    setExpandedPaths((current) => {
      const next = new Set(current);
      if (next.has(path)) next.delete(path);
      else next.add(path);
      return next;
    });
  }

  return (
    <aside className={collapsed ? "photo-folder-sidebar is-collapsed" : "photo-folder-sidebar"} aria-label="照片目录" data-preview-tour="directories">
      <div className="folder-tree-heading">
        <Folder aria-hidden="true" size={15} />
        <strong>照片目录</strong>
        <button
          className="folder-sidebar-toggle"
          type="button"
          onClick={() => onCollapsedChange(!collapsed)}
          aria-label={collapsed ? "展开照片目录侧边栏" : "收起照片目录侧边栏"}
          aria-expanded={!collapsed}
          aria-controls="photo-directory-tree"
          title={collapsed ? "展开照片目录" : "收起照片目录"}
        >
          {collapsed
            ? <PanelLeftOpen aria-hidden="true" size={16} />
            : <PanelLeftClose aria-hidden="true" size={16} />}
        </button>
      </div>
      <div id="photo-directory-tree" className="folder-tree-scroll">
        <button className={!selectedPath ? "folder-tree-root is-selected" : "folder-tree-root"} type="button" onClick={() => onSelect("")} aria-current={!selectedPath ? "page" : undefined}>
          <Images aria-hidden="true" size={16} />
          <span>全部照片</span>
          <small>{totalCount}</small>
        </button>
        {nodes.length > 0 ? (
          <ul className="folder-tree-list">
            <DirectoryBranch nodes={nodes} depth={0} selectedPath={selectedPath} expandedPaths={expandedPaths} onSelect={onSelect} onToggle={toggle} />
          </ul>
        ) : <p className="folder-tree-empty">当前目录没有子目录</p>}
      </div>
    </aside>
  );
}

import {
  FileImage,
  Folder,
  FolderInput,
  FolderOpen,
  ImageOff,
  LoaderCircle,
  ShieldCheck,
  TriangleAlert,
} from "lucide-react";
import { useMemo } from "react";
import type { WatermarkSourcePhoto, WatermarkSourceSnapshot } from "./types";

interface SourceDirectoryNode {
  id: string;
  name: string;
  count: number;
  children: SourceDirectoryNode[];
}

interface WatermarkSourcePanelProps {
  snapshot: WatermarkSourceSnapshot | null;
  busy: boolean;
  error: string | null;
  onChooseDirectory: () => void;
  onDismissError: () => void;
}

function sourceDirectoryTree(photos: WatermarkSourcePhoto[]): SourceDirectoryNode[] {
  const roots = new Map<string, SourceDirectoryNode>();
  const nodes = new Map<string, SourceDirectoryNode>();
  for (const photo of photos) {
    let root = roots.get(photo.root);
    if (!root) {
      root = {
        id: photo.root,
        name: photo.root.split(/[\\/]/).filter(Boolean).at(-1) ?? photo.root,
        count: 0,
        children: [],
      };
      roots.set(photo.root, root);
    }
    root.count += 1;
    const parts = photo.relativePath.split("/").filter(Boolean);
    parts.pop();
    let parent = root;
    let path = photo.root;
    for (const part of parts) {
      path = `${path}/${part}`;
      let node = nodes.get(path);
      if (!node) {
        node = { id: path, name: part, count: 0, children: [] };
        nodes.set(path, node);
        parent.children.push(node);
      }
      node.count += 1;
      parent = node;
    }
  }

  const sort = (items: SourceDirectoryNode[]) => {
    items.sort((left, right) => left.name.localeCompare(right.name, "zh-CN", { numeric: true }));
    for (const item of items) sort(item.children);
  };
  const result = [...roots.values()];
  sort(result);
  return result;
}

function SourceTree({ nodes, depth = 0 }: { nodes: SourceDirectoryNode[]; depth?: number }) {
  return (
    <ul className="watermark-source-tree" style={{ "--source-depth": depth } as React.CSSProperties}>
      {nodes.map((node) => (
        <li key={node.id}>
          <span title={node.id}>
            <Folder aria-hidden="true" size={15} />
            <strong>{node.name}</strong>
            <small>{node.count}</small>
          </span>
          {node.children.length > 0 ? <SourceTree nodes={node.children} depth={depth + 1} /> : null}
        </li>
      ))}
    </ul>
  );
}

export function WatermarkSourcePanel({
  snapshot,
  busy,
  error,
  onChooseDirectory,
  onDismissError,
}: WatermarkSourcePanelProps) {
  const directoryTree = useMemo(
    () => sourceDirectoryTree(snapshot?.photos ?? []),
    [snapshot],
  );

  return (
    <main className="watermark-source-workspace">
      {error ? (
        <div className="watermark-source-notice" role="alert">
          <TriangleAlert aria-hidden="true" size={18} />
          <div><strong>来源未载入</strong><span>{error}</span></div>
          <button type="button" onClick={onDismissError}>关闭</button>
        </div>
      ) : null}

      {!snapshot ? (
        <section className="watermark-source-empty">
          <span><FolderInput aria-hidden="true" size={30} /></span>
          <h1>{busy ? "正在检查照片" : "添加需要加水印的照片"}</h1>
          <p>仅支持 JPG/JPEG，RAW 和 XMP 不会进入水印任务。</p>
          <button className="primary-command" type="button" onClick={onChooseDirectory} disabled={busy}>
            {busy ? <LoaderCircle className="spin" aria-hidden="true" size={18} /> : <FolderOpen aria-hidden="true" size={18} />}
            {busy ? "正在载入" : "选择照片目录"}
          </button>
        </section>
      ) : (
        <div className="watermark-source-layout">
          <aside className="watermark-source-sidebar" aria-label="水印照片目录">
            <header><FolderOpen aria-hidden="true" size={16} /><strong>照片来源</strong></header>
            <div className="watermark-source-summary">
              <span><strong>{snapshot.photos.length}</strong><small>JPG/JPEG</small></span>
              <span><strong>{snapshot.rootPaths.length}</strong><small>来源目录</small></span>
            </div>
            <div className="watermark-source-tree-scroll">
              {directoryTree.length > 0
                ? <SourceTree nodes={directoryTree} />
                : <p>没有可用的 JPG/JPEG</p>}
            </div>
          </aside>

          <section className="watermark-source-list" aria-label="水印照片列表">
            <header>
              <div><FileImage aria-hidden="true" size={17} /><strong>待处理照片</strong></div>
              <span><ShieldCheck aria-hidden="true" size={15} />只生成副本</span>
            </header>
            {snapshot.photos.length > 0 ? (
              <div className="watermark-source-rows">
                {snapshot.photos.map((photo, index) => (
                  <article key={photo.id}>
                    <span className="watermark-source-index">{index + 1}</span>
                    <FileImage aria-hidden="true" size={18} />
                    <div><strong>{photo.fileName}</strong><small>{photo.relativePath}</small></div>
                    <span>{photo.pixelWidth} x {photo.pixelHeight}</span>
                    <span>{photo.orientation === "landscape" ? "横版" : photo.orientation === "portrait" ? "竖版" : "方形"}</span>
                  </article>
                ))}
              </div>
            ) : (
              <div className="watermark-source-no-jpeg">
                <ImageOff aria-hidden="true" size={28} />
                <strong>没有找到可用的 JPG/JPEG</strong>
                <button className="secondary-command" type="button" onClick={onChooseDirectory}>重新选择</button>
              </div>
            )}
          </section>
        </div>
      )}

      {snapshot && (snapshot.skippedRawOnly > 0 || snapshot.skippedUnsupported > 0) ? (
        <footer className="watermark-source-skipped" role="status">
          <TriangleAlert aria-hidden="true" size={15} />
          {snapshot.skippedRawOnly > 0 ? <span>已跳过 {snapshot.skippedRawOnly} 组仅 RAW 照片</span> : null}
          {snapshot.skippedUnsupported > 0 ? <span>已跳过 {snapshot.skippedUnsupported} 个不支持的文件</span> : null}
        </footer>
      ) : null}
    </main>
  );
}

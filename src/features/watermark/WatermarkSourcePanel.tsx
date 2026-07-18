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
import { useEffect, useMemo, useRef, useState } from "react";
import {
  loadPhotoPreviewUrl,
  peekPhotoPreviewUrl,
  type PreviewRequest,
} from "../preview/previewCache";
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
  selectedPhotoId: string | null;
  onChooseDirectory: () => void;
  onDismissError: () => void;
  onSelectPhoto: (photoId: string) => void;
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

function SourceThumbnail({ photo, snapshotId }: { photo: WatermarkSourcePhoto; snapshotId: string }) {
  const request = useMemo<PreviewRequest>(() => ({
    root: photo.root,
    relativePath: photo.relativePath,
    maxEdge: 220,
    version: `${snapshotId}:${photo.sizeBytes}:${photo.modifiedMs}`,
  }), [photo, snapshotId]);
  const [url, setUrl] = useState(() => peekPhotoPreviewUrl(request));

  useEffect(() => {
    let disposed = false;
    const cached = peekPhotoPreviewUrl(request);
    if (cached) {
      setUrl(cached);
      return () => { disposed = true; };
    }
    void loadPhotoPreviewUrl(request)
      .then((loadedUrl) => { if (!disposed) setUrl(loadedUrl); })
      .catch(() => { if (!disposed) setUrl(null); });
    return () => { disposed = true; };
  }, [request]);

  return url
    ? <img src={url} alt="" draggable={false} />
    : <span><FileImage aria-hidden="true" size={17} /></span>;
}

export function WatermarkSourcePanel({
  snapshot,
  busy,
  error,
  selectedPhotoId,
  onChooseDirectory,
  onDismissError,
  onSelectPhoto,
}: WatermarkSourcePanelProps) {
  const selectedRowRef = useRef<HTMLButtonElement>(null);
  const directoryTree = useMemo(
    () => sourceDirectoryTree(snapshot?.photos ?? []),
    [snapshot],
  );

  useEffect(() => {
    selectedRowRef.current?.scrollIntoView({ block: "nearest", inline: "nearest" });
  }, [selectedPhotoId]);

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
        <aside className="watermark-source-browser" aria-label="水印照片来源">
          <header>
            <div><FolderOpen aria-hidden="true" size={16} /><strong>照片来源</strong></div>
            <span><ShieldCheck aria-hidden="true" size={14} />{snapshot.photos.length} 张</span>
          </header>
          <div className="watermark-source-tree-compact">
            {directoryTree.length > 0
              ? <SourceTree nodes={directoryTree} />
              : <p>没有可用的 JPG/JPEG</p>}
          </div>
          {snapshot.photos.length > 0 ? (
            <div className="watermark-source-rows is-compact">
              {snapshot.photos.map((photo, index) => (
                <button
                  className={selectedPhotoId === photo.id ? "is-selected" : undefined}
                  type="button"
                  key={photo.id}
                  ref={selectedPhotoId === photo.id ? selectedRowRef : undefined}
                  onClick={() => onSelectPhoto(photo.id)}
                  aria-pressed={selectedPhotoId === photo.id}
                >
                  <SourceThumbnail photo={photo} snapshotId={snapshot.id} />
                  <div>
                    <strong>{photo.fileName}</strong>
                    <small>{index + 1} · {photo.pixelWidth} x {photo.pixelHeight}</small>
                  </div>
                  <span>{photo.orientation === "landscape" ? "横" : photo.orientation === "portrait" ? "竖" : "方"}</span>
                </button>
              ))}
            </div>
          ) : (
            <div className="watermark-source-no-jpeg">
              <ImageOff aria-hidden="true" size={28} />
              <strong>没有找到可用的 JPG/JPEG</strong>
              <button className="secondary-command" type="button" onClick={onChooseDirectory}>重新选择</button>
            </div>
          )}
        </aside>
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

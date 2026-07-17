import { invoke, isTauri } from "@tauri-apps/api/core";
import { open } from "@tauri-apps/plugin-dialog";
import {
  ArrowLeft,
  ArrowRight,
  FolderInput,
  FolderOpen,
  Grid3X3,
  Image,
  Images,
  LoaderCircle,
  Maximize2,
  RefreshCw,
  Search,
  ShieldCheck,
  X,
} from "lucide-react";
import {
  useDeferredValue,
  useEffect,
  useMemo,
  useRef,
  useState,
} from "react";
import type { CSSProperties } from "react";
import { errorMessage, formatBytes, formatDate } from "../../utils";
import { PhotoThumbnail } from "./PhotoThumbnail";
import {
  adjacentPreviewAssetId,
  filterPreviewAssets,
  sortPreviewAssets,
} from "./previewUtils";
import type {
  PhotoAsset,
  PhotoIndex,
  PreviewFilter,
  PreviewSort,
  PreviewView,
} from "./types";

const PREVIEW_ROOT_STORAGE_KEY = "framepair.preview.root.v1";

interface PreviewModuleProps {
  active: boolean;
}

const FILTERS: Array<{ value: PreviewFilter; label: string }> = [
  { value: "all", label: "全部" },
  { value: "paired", label: "JPG + RAW" },
  { value: "jpeg", label: "仅 JPG" },
  { value: "raw", label: "仅 RAW" },
];

export function PreviewModule({ active }: PreviewModuleProps) {
  const [index, setIndex] = useState<PhotoIndex | null>(null);
  const [root, setRoot] = useState(() => {
    try {
      return localStorage.getItem(PREVIEW_ROOT_STORAGE_KEY) ?? "";
    } catch {
      return "";
    }
  });
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [search, setSearch] = useState("");
  const [filter, setFilter] = useState<PreviewFilter>("all");
  const [sort, setSort] = useState<PreviewSort>("name");
  const [view, setView] = useState<PreviewView>("grid");
  const [selectedId, setSelectedId] = useState<string | null>(null);
  const [tileSize, setTileSize] = useState(180);
  const [dropActive, setDropActive] = useState(false);
  const attemptedStoredRoot = useRef(false);
  const busyRef = useRef(busy);
  const loadDirectoryRef = useRef<(path: string) => Promise<void>>(async () => undefined);
  const deferredSearch = useDeferredValue(search);

  const visibleAssets = useMemo(
    () => sortPreviewAssets(
      filterPreviewAssets(index?.assets ?? [], filter, deferredSearch),
      sort,
    ),
    [deferredSearch, filter, index, sort],
  );
  const effectiveSelectedId = visibleAssets.some((asset) => asset.id === selectedId)
    ? selectedId
    : visibleAssets[0]?.id ?? null;
  const selectedAsset = visibleAssets.find((asset) => asset.id === effectiveSelectedId) ?? null;

  async function loadDirectory(path: string) {
    if (!path || busyRef.current) return;
    busyRef.current = true;
    setBusy(true);
    setError(null);
    try {
      const result = await invoke<PhotoIndex>("index_photo_directory", { root: path });
      setIndex(result);
      setRoot(result.root);
      setSelectedId(result.assets[0]?.id ?? null);
      setSearch("");
      setFilter("all");
      setView("grid");
      try {
        localStorage.setItem(PREVIEW_ROOT_STORAGE_KEY, result.root);
      } catch {
        // The current session remains usable if preferences cannot be stored.
      }
    } catch (loadError) {
      setError(errorMessage(loadError));
      setIndex(null);
    } finally {
      busyRef.current = false;
      setBusy(false);
    }
  }

  busyRef.current = busy;
  loadDirectoryRef.current = loadDirectory;

  useEffect(() => {
    if (!active || !root || attemptedStoredRoot.current || !isTauri()) return;
    attemptedStoredRoot.current = true;
    void loadDirectoryRef.current(root);
  }, [active, root]);

  useEffect(() => {
    if (!active || !isTauri()) return;
    let disposed = false;
    let unlisten: (() => void) | undefined;

    async function listenForDrops() {
      const { getCurrentWebview } = await import("@tauri-apps/api/webview");
      const stopListening = await getCurrentWebview().onDragDropEvent((event) => {
        if (disposed) return;
        if (event.payload.type === "leave") {
          setDropActive(false);
          return;
        }
        if (busyRef.current) return;
        if (event.payload.type === "over") {
          setDropActive(true);
          return;
        }
        setDropActive(false);
        if (event.payload.paths.length !== 1) {
          setError("一次只能打开一个照片目录");
          return;
        }
        void loadDirectoryRef.current(event.payload.paths[0]);
      });
      if (disposed) stopListening();
      else unlisten = stopListening;
    }

    void listenForDrops().catch(() => setError("无法启用目录拖放，请使用目录选择器"));
    return () => {
      disposed = true;
      setDropActive(false);
      unlisten?.();
    };
  }, [active]);

  useEffect(() => {
    if (!active || view !== "loupe") return;
    function handleKeyDown(event: KeyboardEvent) {
      if (event.target instanceof HTMLInputElement || event.target instanceof HTMLSelectElement) return;
      if (event.key === "Escape") {
        setView("grid");
      } else if (event.key === "ArrowLeft" || event.key === "ArrowRight") {
        event.preventDefault();
        const nextId = adjacentPreviewAssetId(
          visibleAssets,
          effectiveSelectedId,
          event.key === "ArrowLeft" ? -1 : 1,
        );
        setSelectedId(nextId);
      }
    }
    window.addEventListener("keydown", handleKeyDown);
    return () => window.removeEventListener("keydown", handleKeyDown);
  }, [active, effectiveSelectedId, view, visibleAssets]);

  async function chooseDirectory() {
    try {
      const selected = await open({
        directory: true,
        multiple: false,
        title: "选择照片目录",
      });
      if (typeof selected === "string") await loadDirectory(selected);
    } catch (chooseError) {
      setError(errorMessage(chooseError));
    }
  }

  function openLoupe(asset: PhotoAsset) {
    setSelectedId(asset.id);
    setView("loupe");
  }

  async function revealAsset(asset: PhotoAsset) {
    const relativePath = asset.previewPath ?? asset.rawPaths[0] ?? asset.jpegPaths[0];
    if (!relativePath || !index) return;
    try {
      await invoke("reveal_scan_item", { root: index.root, relativePath });
    } catch (revealError) {
      setError(errorMessage(revealError));
    }
  }

  return (
    <section className="preview-module" aria-label="照片浏览">
      <header className="preview-header">
        <div className="module-heading">
          <Images aria-hidden="true" size={20} />
          <div><strong>照片浏览</strong><span>本地照片索引与预览</span></div>
        </div>
        <div className="preview-root" title={index?.root || root || "尚未选择照片目录"}>
          <FolderOpen aria-hidden="true" size={15} />
          <span>{index?.root || root || "尚未选择照片目录"}</span>
        </div>
        <div className="preview-header-actions">
          {index ? (
            <button className="icon-button" type="button" onClick={() => void loadDirectory(index.root)} disabled={busy} aria-label="刷新照片目录" title="刷新照片目录">
              <RefreshCw className={busy ? "spin" : undefined} aria-hidden="true" size={17} />
            </button>
          ) : null}
          <button className="secondary-command" type="button" onClick={() => void chooseDirectory()} disabled={busy}>
            <FolderOpen aria-hidden="true" size={16} />选择目录
          </button>
        </div>
      </header>

      {busy ? <div className="activity-line" aria-hidden="true"><span /></div> : null}
      {error ? (
        <div className="notice notice-warning" role="alert">
          <Image aria-hidden="true" size={18} />
          <div><strong>无法打开照片目录</strong><span>{error}</span></div>
          <button className="notice-close" type="button" onClick={() => setError(null)} aria-label="关闭消息" title="关闭消息"><X aria-hidden="true" size={16} /></button>
        </div>
      ) : null}

      {index ? (
        <>
          <div className="preview-toolbar">
            <label className="preview-search">
              <Search aria-hidden="true" size={16} />
              <input value={search} onChange={(event) => setSearch(event.target.value)} placeholder="搜索文件名或目录" aria-label="搜索照片" />
            </label>
            <div className="preview-filters" role="group" aria-label="照片类型">
              {FILTERS.map((item) => (
                <button key={item.value} type="button" aria-pressed={filter === item.value} onClick={() => setFilter(item.value)}>{item.label}</button>
              ))}
            </div>
            <label className="preview-sort">
              <span className="sr-only">排序方式</span>
              <select value={sort} onChange={(event) => setSort(event.target.value as PreviewSort)} aria-label="排序方式">
                <option value="name">按名称</option>
                <option value="modified">按修改时间</option>
                <option value="size">按文件大小</option>
              </select>
            </label>
            {view === "grid" ? (
              <label className="tile-size-control" title="缩略图大小">
                <Grid3X3 aria-hidden="true" size={15} />
                <input type="range" min="148" max="260" step="16" value={tileSize} onChange={(event) => setTileSize(Number(event.target.value))} aria-label="缩略图大小" />
              </label>
            ) : null}
            <div className="preview-view-switch" role="group" aria-label="浏览视图">
              <button type="button" aria-pressed={view === "grid"} onClick={() => setView("grid")} aria-label="网格视图" title="网格视图"><Grid3X3 aria-hidden="true" size={17} /></button>
              <button type="button" aria-pressed={view === "loupe"} onClick={() => setView("loupe")} disabled={!selectedAsset} aria-label="单张预览" title="单张预览"><Maximize2 aria-hidden="true" size={17} /></button>
            </div>
          </div>

          {view === "grid" ? (
            <main className="photo-grid-scroll">
              <div
                className="photo-grid"
                style={{ "--preview-tile-size": `${tileSize}px` } as CSSProperties}
              >
                {visibleAssets.map((asset) => (
                  <button
                    key={asset.id}
                    className={asset.id === effectiveSelectedId ? "photo-tile is-selected" : "photo-tile"}
                    type="button"
                    onClick={() => setSelectedId(asset.id)}
                    onDoubleClick={() => openLoupe(asset)}
                    aria-pressed={asset.id === effectiveSelectedId}
                    title={`${asset.relativeStem} · ${asset.extensions.join(" + ")}`}
                  >
                    <PhotoThumbnail root={index.root} relativePath={asset.previewPath} maxEdge={480} alt="" />
                    <span className="photo-tile-meta">
                      <strong>{asset.name}</strong>
                      <small>{asset.extensions.join(" + ")}</small>
                    </span>
                  </button>
                ))}
              </div>
              {visibleAssets.length === 0 ? (
                <div className="preview-empty compact"><Search aria-hidden="true" size={28} /><strong>没有符合条件的照片</strong></div>
              ) : null}
            </main>
          ) : selectedAsset ? (
            <main className="loupe-workspace">
              <div className="loupe-commandbar">
                <div>
                  <button className="icon-button" type="button" onClick={() => setSelectedId(adjacentPreviewAssetId(visibleAssets, effectiveSelectedId, -1))} disabled={effectiveSelectedId === visibleAssets[0]?.id} aria-label="上一张" title="上一张"><ArrowLeft aria-hidden="true" size={18} /></button>
                  <button className="icon-button" type="button" onClick={() => setSelectedId(adjacentPreviewAssetId(visibleAssets, effectiveSelectedId, 1))} disabled={effectiveSelectedId === visibleAssets.at(-1)?.id} aria-label="下一张" title="下一张"><ArrowRight aria-hidden="true" size={18} /></button>
                </div>
                <div className="loupe-title"><strong>{selectedAsset.name}</strong><span>{selectedAsset.relativeStem}</span></div>
                <button className="secondary-command" type="button" onClick={() => void revealAsset(selectedAsset)}><FolderOpen aria-hidden="true" size={16} />在文件管理器中显示</button>
              </div>
              <div className="loupe-stage">
                <PhotoThumbnail root={index.root} relativePath={selectedAsset.previewPath} maxEdge={1800} alt={selectedAsset.name} eager />
              </div>
              <div className="loupe-metadata">
                <span><strong>{selectedAsset.extensions.join(" + ")}</strong><small>文件组合</small></span>
                <span><strong>{formatBytes(selectedAsset.sizeBytes)}</strong><small>组合大小</small></span>
                <span><strong>{formatDate(selectedAsset.modifiedMs)}</strong><small>修改时间</small></span>
                <span className="loupe-safety"><ShieldCheck aria-hidden="true" size={15} /><strong>只读预览</strong><small>不会修改原始照片</small></span>
              </div>
              <div className="loupe-filmstrip" aria-label="照片胶片栏">
                {visibleAssets.map((asset) => (
                  <button key={asset.id} type="button" className={asset.id === effectiveSelectedId ? "is-selected" : ""} onClick={() => setSelectedId(asset.id)} aria-label={asset.name} aria-pressed={asset.id === effectiveSelectedId} title={asset.name}>
                    <PhotoThumbnail root={index.root} relativePath={asset.previewPath} maxEdge={160} alt="" />
                  </button>
                ))}
              </div>
            </main>
          ) : null}

          <footer className="preview-statusbar">
            <span>{visibleAssets.length} / {index.totalAssets} 张照片</span>
            <span>{index.pairedAssets} 组 JPG + RAW</span>
            {index.rawOnlyAssets > 0 ? <span>{index.rawOnlyAssets} 张仅 RAW</span> : null}
            <span>索引于 {formatDate(index.indexedAtMs)}</span>
          </footer>
        </>
      ) : (
        <main className={dropActive ? "preview-empty is-drop-target" : "preview-empty"}>
          <span className="preview-empty-icon"><FolderInput aria-hidden="true" size={30} /></span>
          <h1>{dropActive ? "松开以打开照片目录" : "选择照片目录"}</h1>
          <p>JPG/JPEG 用于生成本地缩略图，RAW 文件保持只读。</p>
          <button className="primary-command primary-command-large" type="button" onClick={() => void chooseDirectory()} disabled={busy}>
            {busy ? <LoaderCircle className="spin" aria-hidden="true" size={19} /> : <FolderOpen aria-hidden="true" size={19} />}
            {busy ? "正在建立索引" : "打开照片目录"}
          </button>
        </main>
      )}

      {dropActive && index ? (
        <div className="preview-drop-overlay"><FolderInput aria-hidden="true" size={28} /><strong>松开以切换照片目录</strong></div>
      ) : null}
    </section>
  );
}

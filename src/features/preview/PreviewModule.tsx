import { invoke, isTauri } from "@tauri-apps/api/core";
import { open } from "@tauri-apps/plugin-dialog";
import {
  ArrowLeft,
  ArrowRight,
  CircleHelp,
  ExternalLink,
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
  Star,
  X,
} from "lucide-react";
import {
  useCallback,
  useDeferredValue,
  useEffect,
  useLayoutEffect,
  useMemo,
  useRef,
  useState,
} from "react";
import type { CSSProperties, MouseEvent as ReactMouseEvent } from "react";
import {
  errorMessage,
  formatBytes,
  formatDate,
  storedBooleanPreference,
} from "../../utils";
import type { Notice } from "../../types";
import { RatingSyncDialog } from "../rating-sync/RatingSyncDialog";
import { autoSyncOutcomeNotice } from "../rating-sync/ratingSyncUtils";
import { PhotoContextMenu } from "./PhotoContextMenu";
import { PhotoDirectoryTree } from "./PhotoDirectoryTree";
import { PhotoThumbnail } from "./PhotoThumbnail";
import { PreviewGuideDialog } from "./PreviewGuideDialog";
import { RatingControl } from "./RatingControl";
import {
  clearPhotoPreviewCache,
  loadPhotoPreviewUrl,
  photoPreviewRequest,
  photoPreviewVersion,
  preloadPreviewRequests,
  type PreloadProgress,
  type PreviewRequest,
} from "./previewCache";
import {
  adjacentPreviewAssetId,
  availablePreviewFilter,
  buildPhotoDirectoryTree,
  contextMenuPosition,
  filmstripScrollTarget,
  filterAssetsByDirectory,
  filterPreviewAssets,
  previewFilterCounts,
  previewKeyboardShortcutsEnabled,
  previewAssetPosition,
  shouldOpenPreviewGuide,
  sortPreviewAssets,
  withFramePairRating,
} from "./previewUtils";
import type {
  ExternalEditor,
  PhotoAsset,
  PhotoIndex,
  PreviewFilter,
  PreviewSort,
  PreviewView,
  RatingUpdate,
} from "./types";

const PREVIEW_ROOT_STORAGE_KEY = "framepair.preview.root.v1";
const PREVIEW_GUIDE_STORAGE_KEY = "framepair.preview.guide.v1";
const FOLDER_SIDEBAR_STORAGE_KEY = "framepair.preview.folder-sidebar-collapsed.v1";
const LOUPE_PREVIEW_EDGE = 1800;
const PRELOAD_CONCURRENCY = 3;
const EMPTY_PRELOAD_PROGRESS: PreloadProgress = { total: 0, completed: 0, failed: 0 };
const SYSTEM_EDITOR: ExternalEditor = {
  id: "system",
  label: "系统默认应用",
  kind: "system",
};

interface PreviewModuleProps {
  active: boolean;
}

const FILTERS: Array<{ value: PreviewFilter; label: string }> = [
  { value: "all", label: "全部" },
  { value: "paired", label: "已配对" },
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
  const [selectedDirectory, setSelectedDirectory] = useState("");
  const [minimumRating, setMinimumRating] = useState(0);
  const [sort, setSort] = useState<PreviewSort>("name");
  const [view, setView] = useState<PreviewView>("grid");
  const [selectedId, setSelectedId] = useState<string | null>(null);
  const [tileSize, setTileSize] = useState(180);
  const [dropActive, setDropActive] = useState(false);
  const [ratings, setRatings] = useState<Record<string, number>>({});
  const [ratingBusyId, setRatingBusyId] = useState<string | null>(null);
  const [actionError, setActionError] = useState<string | null>(null);
  const [actionNotice, setActionNotice] = useState<Notice | null>(null);
  const [externalEditors, setExternalEditors] = useState<ExternalEditor[]>([SYSTEM_EDITOR]);
  const [editorId, setEditorId] = useState("system");
  const [editorBusy, setEditorBusy] = useState(false);
  const [contextMenu, setContextMenu] = useState<{ assetId: string; left: number; top: number } | null>(null);
  const [guideOpen, setGuideOpen] = useState(false);
  const [syncDialogOpen, setSyncDialogOpen] = useState(false);
  const [syncAssetId, setSyncAssetId] = useState<string | null>(null);
  const [folderSidebarCollapsed, setFolderSidebarCollapsed] = useState(() => {
    try {
      return storedBooleanPreference(localStorage.getItem(FOLDER_SIDEBAR_STORAGE_KEY));
    } catch {
      return false;
    }
  });
  const [preloadProgress, setPreloadProgress] = useState<PreloadProgress>(EMPTY_PRELOAD_PROGRESS);
  const attemptedStoredRoot = useRef(false);
  const busyRef = useRef(busy);
  const indexedRootRef = useRef<string | null>(null);
  const filmstripRef = useRef<HTMLDivElement>(null);
  const selectedFilmstripItemRef = useRef<HTMLButtonElement>(null);
  const attemptedEditorDiscovery = useRef(false);
  const attemptedPreviewGuide = useRef(false);
  const loadDirectoryRef = useRef<(path: string) => Promise<void>>(async () => undefined);
  const deferredSearch = useDeferredValue(search);

  const ratedAssets = useMemo(
    () => (index?.assets ?? []).map((asset) => (
      withFramePairRating(asset, ratings[asset.id] ?? asset.rating)
    )),
    [index, ratings],
  );
  const directoryTree = useMemo(
    () => buildPhotoDirectoryTree(index?.assets ?? []),
    [index],
  );
  const directoryAssets = useMemo(
    () => filterAssetsByDirectory(ratedAssets, selectedDirectory),
    [ratedAssets, selectedDirectory],
  );
  const filterCounts = useMemo(
    () => previewFilterCounts(directoryAssets),
    [directoryAssets],
  );
  const visibleAssets = useMemo(
    () => sortPreviewAssets(
      filterPreviewAssets(directoryAssets, filter, deferredSearch, minimumRating),
      sort,
    ),
    [deferredSearch, directoryAssets, filter, minimumRating, sort],
  );
  const selectedPosition = previewAssetPosition(visibleAssets, selectedId);
  const effectiveSelectedPosition = selectedPosition || (visibleAssets.length > 0 ? 1 : 0);
  const selectedAsset = visibleAssets[effectiveSelectedPosition - 1] ?? null;
  const effectiveSelectedId = selectedAsset?.id ?? null;
  const ratedCount = useMemo(
    () => directoryAssets.filter((asset) => asset.rating > 0).length,
    [directoryAssets],
  );
  const contextAsset = contextMenu
    ? ratedAssets.find((asset) => asset.id === contextMenu.assetId) ?? null
    : null;
  const syncAsset = syncAssetId
    ? ratedAssets.find((asset) => asset.id === syncAssetId) ?? null
    : null;
  const selectedEditor = externalEditors.find((editor) => editor.id === editorId) ?? SYSTEM_EDITOR;
  const dismissContextMenu = useCallback(() => setContextMenu(null), []);

  async function loadDirectory(
    path: string,
    options: { preserveContext?: boolean } = {},
  ) {
    if (!path || busyRef.current) return;
    busyRef.current = true;
    setBusy(true);
    setError(null);
    try {
      const result = await invoke<PhotoIndex>("index_photo_directory", { root: path });
      const preserveContext = options.preserveContext
        && indexedRootRef.current === result.root;
      if (indexedRootRef.current) clearPhotoPreviewCache(indexedRootRef.current);
      indexedRootRef.current = result.root;
      setIndex(result);
      setRatings(Object.fromEntries(result.assets.map((asset) => [asset.id, asset.rating])));
      setRoot(result.root);
      if (preserveContext) {
        setSelectedId((current) => result.assets.some((asset) => asset.id === current)
          ? current
          : result.assets[0]?.id ?? null);
      } else {
        setSelectedId(result.assets[0]?.id ?? null);
        setSearch("");
        setFilter("all");
        setSelectedDirectory("");
        setMinimumRating(0);
      }
      setContextMenu(null);
      let autoOpenGuide = false;
      if (!preserveContext && result.assets.length > 0 && !attemptedPreviewGuide.current) {
        attemptedPreviewGuide.current = true;
        try {
          autoOpenGuide = shouldOpenPreviewGuide(localStorage.getItem(PREVIEW_GUIDE_STORAGE_KEY));
        } catch {
          autoOpenGuide = true;
        }
      }
      if (!preserveContext) {
        setView(autoOpenGuide ? "loupe" : "grid");
        setGuideOpen(autoOpenGuide);
      }
      try {
        localStorage.setItem(PREVIEW_ROOT_STORAGE_KEY, result.root);
      } catch {
        // The current session remains usable if preferences cannot be stored.
      }
    } catch (loadError) {
      setError(errorMessage(loadError));
      if (indexedRootRef.current) clearPhotoPreviewCache(indexedRootRef.current);
      indexedRootRef.current = null;
      setIndex(null);
      setRatings({});
      setSelectedDirectory("");
      setContextMenu(null);
      setGuideOpen(false);
      setPreloadProgress(EMPTY_PRELOAD_PROGRESS);
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
    if (!active || !isTauri() || attemptedEditorDiscovery.current) return;
    attemptedEditorDiscovery.current = true;
    void invoke<ExternalEditor[]>("list_external_editors")
      .then((editors) => {
        const nextEditors = editors.length > 0 ? editors : [SYSTEM_EDITOR];
        setExternalEditors(nextEditors);
        setEditorId((current) => nextEditors.some((editor) => editor.id === current)
          ? current
          : nextEditors[0].id);
      })
      .catch((discoveryError) => setActionError(errorMessage(discoveryError)));
  }, [active]);

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
    if (!active || !previewKeyboardShortcutsEnabled(view, guideOpen, contextMenu !== null)) return;
    function handleKeyDown(event: KeyboardEvent) {
      if (event.target instanceof HTMLInputElement || event.target instanceof HTMLSelectElement) return;
      if (event.key === "Escape") {
        setView("grid");
      } else if (/^[0-5]$/.test(event.key) && selectedAsset && !ratingBusyId) {
        event.preventDefault();
        void rateAsset(selectedAsset, Number(event.key));
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
  }, [active, contextMenu, effectiveSelectedId, guideOpen, ratingBusyId, selectedAsset, view, visibleAssets]);

  useLayoutEffect(() => {
    if (!active || view !== "loupe" || !effectiveSelectedId) return;
    const filmstrip = filmstripRef.current;
    const selectedItem = selectedFilmstripItemRef.current;
    if (!filmstrip || !selectedItem) return;

    const filmstripRect = filmstrip.getBoundingClientRect();
    const itemRect = selectedItem.getBoundingClientRect();
    filmstrip.scrollLeft = filmstripScrollTarget({
      scrollLeft: filmstrip.scrollLeft,
      clientWidth: filmstrip.clientWidth,
      scrollWidth: filmstrip.scrollWidth,
      itemOffsetLeft: itemRect.left - filmstripRect.left + filmstrip.scrollLeft,
      itemWidth: itemRect.width,
    });
  }, [active, effectiveSelectedId, view, visibleAssets]);

  useEffect(() => {
    if (!index) {
      setPreloadProgress(EMPTY_PRELOAD_PROGRESS);
      return;
    }
    const controller = new AbortController();
    const requests = index.assets
      .map((asset) => photoPreviewRequest(
        index.root,
        asset,
        LOUPE_PREVIEW_EDGE,
        index.indexedAtMs,
      ))
      .filter((request): request is PreviewRequest => request !== null);

    void preloadPreviewRequests(requests, loadPhotoPreviewUrl, {
      concurrency: PRELOAD_CONCURRENCY,
      signal: controller.signal,
      onProgress: setPreloadProgress,
    });
    return () => controller.abort();
  }, [index]);

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

  function selectDirectory(path: string) {
    const nextAssets = filterAssetsByDirectory(ratedAssets, path);
    const nextCounts = previewFilterCounts(nextAssets);
    const nextFilter = availablePreviewFilter(filter, nextCounts);
    setSelectedDirectory(path);
    setFilter(nextFilter);
    setSelectedId((current) => nextAssets.some((asset) => asset.id === current)
      ? current
      : nextAssets[0]?.id ?? null);
    setContextMenu(null);
  }

  function clearPreviewFilters() {
    setSearch("");
    setFilter("all");
    setMinimumRating(0);
  }

  function showContextMenu(event: ReactMouseEvent, asset: PhotoAsset) {
    event.preventDefault();
    event.stopPropagation();
    setSelectedId(asset.id);
    setContextMenu({
      assetId: asset.id,
      ...contextMenuPosition(event.clientX, event.clientY, window.innerWidth, window.innerHeight, 260, 220),
    });
  }

  function openPreviewGuide() {
    const firstAsset = selectedAsset ?? visibleAssets[0] ?? directoryAssets[0];
    if (!firstAsset) return;
    setSelectedId(firstAsset.id);
    setView("loupe");
    changeFolderSidebarCollapsed(false);
    setContextMenu(null);
    setGuideOpen(true);
  }

  function changeFolderSidebarCollapsed(collapsed: boolean) {
    setFolderSidebarCollapsed(collapsed);
    try {
      localStorage.setItem(FOLDER_SIDEBAR_STORAGE_KEY, String(collapsed));
    } catch {
      // The current session remains usable if layout preferences cannot be stored.
    }
  }

  function dismissPreviewGuide() {
    try {
      localStorage.setItem(PREVIEW_GUIDE_STORAGE_KEY, "true");
    } catch {
      // The guide remains available from the header when storage is unavailable.
    }
    setGuideOpen(false);
  }

  function openRatingSync(asset: PhotoAsset | null) {
    setSyncAssetId(asset?.id ?? null);
    setContextMenu(null);
    setSyncDialogOpen(true);
  }

  async function rateAsset(asset: PhotoAsset, rating: number) {
    if (!index || ratingBusyId) return;
    const relativePath = asset.rawPaths[0] ?? asset.previewPath ?? asset.jpegPaths[0];
    if (!relativePath) return;
    const previousRating = ratings[asset.id] ?? asset.rating;
    setRatingBusyId(asset.id);
    setActionError(null);
    setActionNotice(null);
    setRatings((current) => ({ ...current, [asset.id]: rating }));
    try {
      const update = await invoke<RatingUpdate>("set_photo_rating", {
        root: index.root,
        relativePath,
        rating,
      });
      if (update.assetId !== asset.id) throw new Error("评分结果与当前照片不匹配");
      setRatings((current) => ({ ...current, [update.assetId]: update.rating }));
      setActionNotice(autoSyncOutcomeNotice(update.autoSync));
      if (update.autoSync.status === "synced" || update.autoSync.status === "unchanged") {
        await loadDirectory(index.root, { preserveContext: true });
      }
    } catch (ratingError) {
      setRatings((current) => ({ ...current, [asset.id]: previousRating }));
      setActionError(errorMessage(ratingError));
    } finally {
      setRatingBusyId(null);
    }
  }

  async function openInEditor(asset: PhotoAsset) {
    if (!index || editorBusy) return;
    const relativePath = asset.rawPaths[0] ?? asset.previewPath ?? asset.jpegPaths[0];
    if (!relativePath) return;
    setEditorBusy(true);
    setActionError(null);
    try {
      await invoke("open_photo_in_editor", {
        root: index.root,
        relativePath,
        editorId,
      });
    } catch (editorError) {
      setActionError(errorMessage(editorError));
    } finally {
      setEditorBusy(false);
    }
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
          {index ? <button className="secondary-command" type="button" onClick={() => openRatingSync(selectedAsset)} disabled={busy || !selectedAsset} title="设置自动同步或同步当前照片"><RefreshCw aria-hidden="true" size={16} />评分同步</button> : null}
          <button className="guide-trigger" type="button" onClick={openPreviewGuide} disabled={busy || !index?.assets.length} title={index?.assets.length ? "查看照片浏览与评分引导" : "选择目录后可查看引导"}>
            <CircleHelp aria-hidden="true" size={16} />使用引导
          </button>
          {index ? (
            <button className="icon-button" type="button" onClick={() => void loadDirectory(index.root)} disabled={busy} aria-label="刷新照片目录" title="刷新照片目录">
              <RefreshCw className={busy ? "spin" : undefined} aria-hidden="true" size={17} />
            </button>
          ) : null}
          <button className="secondary-command" type="button" onClick={() => void chooseDirectory()} disabled={busy} data-preview-tour="choose-directory">
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
      {actionError ? (
        <div className="notice notice-warning" role="alert">
          <Image aria-hidden="true" size={18} />
          <div><strong>操作未完成</strong><span>{actionError}</span></div>
          <button className="notice-close" type="button" onClick={() => setActionError(null)} aria-label="关闭消息" title="关闭消息"><X aria-hidden="true" size={16} /></button>
        </div>
      ) : null}
      {actionNotice ? (
        <div className={`notice notice-${actionNotice.tone}`} role="status">
          <RefreshCw aria-hidden="true" size={18} />
          <div><strong>{actionNotice.title}</strong>{actionNotice.detail ? <span>{actionNotice.detail}</span> : null}</div>
          <button className="notice-close" type="button" onClick={() => setActionNotice(null)} aria-label="关闭消息" title="关闭消息"><X aria-hidden="true" size={16} /></button>
        </div>
      ) : null}

      {index ? (
        <>
          <div className="preview-toolbar">
            <label className="preview-search">
              <Search aria-hidden="true" size={16} />
              <input value={search} onChange={(event) => setSearch(event.target.value)} placeholder="搜索文件名或目录" aria-label="搜索照片" />
            </label>
            <div className="preview-filters" role="group" aria-label="照片类型" data-preview-tour="type-filters">
              {FILTERS.map((item) => (
                <button
                  key={item.value}
                  type="button"
                  aria-pressed={filter === item.value}
                  aria-label={`${item.label}，${filterCounts[item.value]} 张`}
                  disabled={item.value !== "all" && filterCounts[item.value] === 0}
                  title={item.value === "paired" ? "同路径、同文件名，同时存在 JPG 和 RAW" : `${item.label}照片`}
                  onClick={() => setFilter(item.value)}
                >
                  <span>{item.label}</span><small>{filterCounts[item.value]}</small>
                </button>
              ))}
            </div>
            <label className="preview-rating-filter" title="最低评分" data-preview-tour="rating-filter">
              <Star aria-hidden="true" size={15} />
              <select value={minimumRating} onChange={(event) => setMinimumRating(Number(event.target.value))} aria-label="最低评分">
                <option value={0}>全部评分</option>
                <option value={1}>1 星以上</option>
                <option value={2}>2 星以上</option>
                <option value={3}>3 星以上</option>
                <option value={4}>4 星以上</option>
                <option value={5}>仅 5 星</option>
              </select>
            </label>
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

          <div className={folderSidebarCollapsed ? "preview-browser is-folder-sidebar-collapsed" : "preview-browser"}>
            <PhotoDirectoryTree
              key={`${index.root}:${index.indexedAtMs}`}
              nodes={directoryTree}
              totalCount={ratedAssets.length}
              selectedPath={selectedDirectory}
              collapsed={folderSidebarCollapsed}
              onSelect={selectDirectory}
              onCollapsedChange={changeFolderSidebarCollapsed}
            />
            {view === "grid" ? (
            <main className="photo-grid-scroll" data-preview-tour="grid">
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
                    onContextMenu={(event) => showContextMenu(event, asset)}
                    aria-pressed={asset.id === effectiveSelectedId}
                    title={`${asset.relativeStem} · ${asset.extensions.join(" + ")}`}
                  >
                    <PhotoThumbnail root={index.root} relativePath={asset.previewPath} maxEdge={480} version={photoPreviewVersion(asset, index.indexedAtMs)} alt="" />
                    {asset.rating > 0 ? <span className="photo-rating-badge"><Star aria-hidden="true" size={11} fill="currentColor" />{asset.rating}</span> : null}
                    <span className="photo-tile-meta">
                      <strong>{asset.name}</strong>
                      <small>{asset.extensions.join(" + ")}</small>
                    </span>
                  </button>
                ))}
              </div>
              {visibleAssets.length === 0 ? (
                <div className="preview-empty compact">
                  <Search aria-hidden="true" size={28} />
                  <strong>当前目录没有符合筛选条件的照片</strong>
                  <button className="secondary-command" type="button" onClick={clearPreviewFilters}>清除筛选</button>
                </div>
              ) : null}
            </main>
          ) : selectedAsset ? (
            <main className="loupe-workspace" data-preview-tour="loupe">
              <div className="loupe-commandbar">
                <div>
                  <button className="icon-button" type="button" onClick={() => setSelectedId(adjacentPreviewAssetId(visibleAssets, effectiveSelectedId, -1))} disabled={effectiveSelectedId === visibleAssets[0]?.id} aria-label="上一张" title="上一张"><ArrowLeft aria-hidden="true" size={18} /></button>
                  <button className="icon-button" type="button" onClick={() => setSelectedId(adjacentPreviewAssetId(visibleAssets, effectiveSelectedId, 1))} disabled={effectiveSelectedId === visibleAssets.at(-1)?.id} aria-label="下一张" title="下一张"><ArrowRight aria-hidden="true" size={18} /></button>
                </div>
                <div className="loupe-title"><strong>{selectedAsset.name}</strong><span>{selectedAsset.relativeStem}</span></div>
                <div className="loupe-actions">
                  <button className="secondary-command" type="button" onClick={() => openRatingSync(selectedAsset)}><RefreshCw aria-hidden="true" size={16} />同步评分</button>
                  <label className="external-editor-select">
                    <span className="sr-only">外部编辑器</span>
                    <select value={editorId} onChange={(event) => setEditorId(event.target.value)} aria-label="外部编辑器">
                      {externalEditors.map((editor) => <option key={editor.id} value={editor.id}>{editor.label}</option>)}
                    </select>
                  </label>
                  <button className="secondary-command" type="button" onClick={() => void openInEditor(selectedAsset)} disabled={editorBusy}>
                    {editorBusy ? <LoaderCircle className="spin" aria-hidden="true" size={16} /> : <ExternalLink aria-hidden="true" size={16} />}
                    打开编辑
                  </button>
                  <button className="icon-button" type="button" onClick={() => void revealAsset(selectedAsset)} aria-label="在文件管理器中显示" title="在文件管理器中显示"><FolderOpen aria-hidden="true" size={16} /></button>
                </div>
              </div>
              <div className="loupe-stage" onContextMenu={(event) => showContextMenu(event, selectedAsset)}>
                <PhotoThumbnail root={index.root} relativePath={selectedAsset.previewPath} maxEdge={LOUPE_PREVIEW_EDGE} version={photoPreviewVersion(selectedAsset, index.indexedAtMs)} alt={selectedAsset.name} eager />
              </div>
              <div className="loupe-metadata">
                <span className="loupe-rating" data-preview-tour="rating">
                  <RatingControl rating={selectedAsset.rating} onChange={(rating) => void rateAsset(selectedAsset, rating)} disabled={ratingBusyId === selectedAsset.id} />
                  <small>{ratingBusyId === selectedAsset.id ? "正在保存" : selectedAsset.rating > 0 ? `${selectedAsset.rating} 星` : "未评分"}</small>
                </span>
                <span><strong>{selectedAsset.extensions.join(" + ")}</strong><small>文件组合</small></span>
                <span><strong>{formatBytes(selectedAsset.sizeBytes)}</strong><small>组合大小</small></span>
                <span><strong>{formatDate(selectedAsset.modifiedMs)}</strong><small>修改时间</small></span>
                <span className="loupe-safety"><ShieldCheck aria-hidden="true" size={15} /><strong>只读预览</strong><small>不会修改原始照片</small></span>
              </div>
              <div ref={filmstripRef} className="loupe-filmstrip" aria-label="照片胶片栏">
                {visibleAssets.map((asset) => (
                  <button ref={asset.id === effectiveSelectedId ? selectedFilmstripItemRef : undefined} key={asset.id} type="button" className={asset.id === effectiveSelectedId ? "is-selected" : ""} onClick={() => setSelectedId(asset.id)} onContextMenu={(event) => showContextMenu(event, asset)} aria-label={asset.name} aria-pressed={asset.id === effectiveSelectedId} title={asset.name}>
                    <PhotoThumbnail root={index.root} relativePath={asset.previewPath} maxEdge={160} version={photoPreviewVersion(asset, index.indexedAtMs)} alt="" />
                    {asset.rating > 0 ? <span className="filmstrip-rating"><Star aria-hidden="true" size={10} fill="currentColor" />{asset.rating}</span> : null}
                  </button>
                ))}
              </div>
            </main>
          ) : null}
            {contextAsset && contextMenu ? (
              <PhotoContextMenu
                asset={contextAsset}
                position={{ left: contextMenu.left, top: contextMenu.top }}
                view={view}
                editorLabel={selectedEditor.label}
                ratingBusy={ratingBusyId === contextAsset.id}
                editorBusy={editorBusy}
                onRate={(rating) => void rateAsset(contextAsset, rating)}
                onOpenLoupe={() => openLoupe(contextAsset)}
                onShowGrid={() => setView("grid")}
                onReveal={() => void revealAsset(contextAsset)}
                onEdit={() => void openInEditor(contextAsset)}
                onSync={() => openRatingSync(contextAsset)}
                onDismiss={dismissContextMenu}
              />
            ) : null}
          </div>

          <footer className="preview-statusbar">
            <span>
              {view === "loupe" ? effectiveSelectedPosition : visibleAssets.length}
              {" / "}
              {view === "loupe" ? visibleAssets.length : directoryAssets.length} 张照片
            </span>
            <span>{filterCounts.paired} 组已配对</span>
            <span>{ratedCount} 张已评分</span>
            {filterCounts.raw > 0 ? <span>{filterCounts.raw} 张仅 RAW</span> : null}
            {preloadProgress.total > 0 ? (
              <span className={preloadProgress.completed < preloadProgress.total ? "preload-status is-loading" : "preload-status"} aria-live="polite">
                {preloadProgress.completed < preloadProgress.total
                  ? `预加载 ${preloadProgress.completed} / ${preloadProgress.total}`
                  : preloadProgress.failed > 0
                    ? `${preloadProgress.total - preloadProgress.failed} 张预览就绪 · ${preloadProgress.failed} 张失败`
                    : `${preloadProgress.total} 张预览已就绪`}
              </span>
            ) : null}
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
      <PreviewGuideDialog open={active && guideOpen} onDismiss={dismissPreviewGuide} />
      <RatingSyncDialog
        open={active && syncDialogOpen}
        root={index?.root ?? ""}
        asset={syncAsset}
        onDismiss={() => setSyncDialogOpen(false)}
        onSynced={async () => {
          if (index) await loadDirectory(index.root, { preserveContext: true });
        }}
      />
    </section>
  );
}

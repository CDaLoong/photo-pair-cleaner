import type {
  PhotoAsset,
  PhotoDirectoryNode,
  PreviewFilter,
  PreviewSort,
  PreviewView,
} from "./types";

const DISPLAY_PREVIEW_EDGES = [1600, 2560, 4096] as const;

export function displayPreviewEdge(
  widthCssPixels: number,
  heightCssPixels: number,
  devicePixelRatio: number,
): number {
  const cssEdge = Math.max(
    1,
    Number.isFinite(widthCssPixels) ? widthCssPixels : 1,
    Number.isFinite(heightCssPixels) ? heightCssPixels : 1,
  );
  const pixelRatio = Number.isFinite(devicePixelRatio)
    ? Math.max(1, devicePixelRatio)
    : 1;
  const requiredEdge = Math.ceil(cssEdge * pixelRatio);
  return DISPLAY_PREVIEW_EDGES.find((edge) => edge >= requiredEdge)
    ?? DISPLAY_PREVIEW_EDGES.at(-1)!;
}

export interface PhotoViewportTransform {
  scale: number;
  x: number;
  y: number;
}

export interface PhotoViewportGeometry {
  viewportWidth: number;
  viewportHeight: number;
  imageWidth: number;
  imageHeight: number;
}

function finiteNumber(value: number, fallback: number): number {
  return Number.isFinite(value) ? value : fallback;
}

export function clampPhotoViewport(
  transform: PhotoViewportTransform,
  geometry: PhotoViewportGeometry,
  minScale = 1,
  maxScale = 8,
): PhotoViewportTransform {
  const lowerScale = Math.max(0.01, finiteNumber(minScale, 1));
  const upperScale = Math.max(lowerScale, finiteNumber(maxScale, 8));
  const scale = Math.min(
    upperScale,
    Math.max(lowerScale, finiteNumber(transform.scale, lowerScale)),
  );
  if (scale <= lowerScale) return { scale: lowerScale, x: 0, y: 0 };

  const viewportWidth = Math.max(0, finiteNumber(geometry.viewportWidth, 0));
  const viewportHeight = Math.max(0, finiteNumber(geometry.viewportHeight, 0));
  const imageWidth = Math.max(0, finiteNumber(geometry.imageWidth, 0));
  const imageHeight = Math.max(0, finiteNumber(geometry.imageHeight, 0));
  const maxX = Math.max(0, (imageWidth * scale - viewportWidth) / 2);
  const maxY = Math.max(0, (imageHeight * scale - viewportHeight) / 2);

  return {
    scale,
    x: Math.min(maxX, Math.max(-maxX, finiteNumber(transform.x, 0))),
    y: Math.min(maxY, Math.max(-maxY, finiteNumber(transform.y, 0))),
  };
}

export function zoomPhotoViewportAtPoint(
  transform: PhotoViewportTransform,
  targetScale: number,
  pointX: number,
  pointY: number,
  geometry: PhotoViewportGeometry,
  minScale = 1,
  maxScale = 8,
): PhotoViewportTransform {
  const current = clampPhotoViewport(transform, geometry, minScale, maxScale);
  const upperScale = Math.max(minScale, maxScale);
  const scale = Math.min(
    upperScale,
    Math.max(minScale, finiteNumber(targetScale, current.scale)),
  );
  if (scale <= minScale) return { scale: minScale, x: 0, y: 0 };

  const ratio = scale / current.scale;
  const anchorX = finiteNumber(pointX, 0);
  const anchorY = finiteNumber(pointY, 0);
  return clampPhotoViewport({
    scale,
    x: anchorX - (anchorX - current.x) * ratio,
    y: anchorY - (anchorY - current.y) * ratio,
  }, geometry, minScale, upperScale);
}

function matchesFilter(asset: PhotoAsset, filter: PreviewFilter): boolean {
  const hasJpeg = asset.jpegPaths.length > 0;
  const hasRaw = asset.rawPaths.length > 0;
  if (filter === "paired") return hasJpeg && hasRaw;
  if (filter === "jpeg") return hasJpeg && !hasRaw;
  if (filter === "raw") return !hasJpeg && hasRaw;
  return true;
}

export function withFramePairRating(asset: PhotoAsset, rating: number): PhotoAsset {
  const sourceRatings = [
    rating > 0 ? rating : null,
    asset.ratingState.jpegMetadata,
    asset.ratingState.rawXmp,
  ].filter((value): value is number => value !== null && value > 0);
  const firstRating = sourceRatings[0];
  const sourceConflict = firstRating !== undefined
    && sourceRatings.some((value) => value !== firstRating);

  return {
    ...asset,
    rating,
    ratingState: {
      ...asset.ratingState,
      framePair: rating,
      resolved: rating,
      conflict: asset.ratingIssues.length > 0 || sourceConflict,
    },
  };
}

export function buildPhotoDirectoryTree(assets: PhotoAsset[]): PhotoDirectoryNode[] {
  const roots: PhotoDirectoryNode[] = [];
  const nodes = new Map<string, PhotoDirectoryNode>();

  for (const asset of assets) {
    const parts = asset.relativeStem.split("/").filter(Boolean);
    parts.pop();
    let siblings = roots;
    let path = "";
    let lastNode: PhotoDirectoryNode | null = null;

    for (const part of parts) {
      path = path ? `${path}/${part}` : part;
      let node = nodes.get(path);
      if (!node) {
        node = { name: part, path, directCount: 0, totalCount: 0, children: [] };
        nodes.set(path, node);
        siblings.push(node);
      }
      node.totalCount += 1;
      lastNode = node;
      siblings = node.children;
    }
    if (lastNode) lastNode.directCount += 1;
  }

  const sortNodes = (items: PhotoDirectoryNode[]) => {
    items.sort((left, right) => left.name.localeCompare(right.name, "zh-CN", {
      numeric: true,
      sensitivity: "base",
    }));
    for (const item of items) sortNodes(item.children);
  };
  sortNodes(roots);
  return roots;
}

export function filterAssetsByDirectory(
  assets: PhotoAsset[],
  directoryPath: string,
): PhotoAsset[] {
  if (!directoryPath) return assets;
  const prefix = `${directoryPath.replace(/\/+$/, "")}/`.toLocaleLowerCase();
  return assets.filter((asset) => asset.relativeStem.toLocaleLowerCase().startsWith(prefix));
}

export function previewFilterCounts(assets: PhotoAsset[]): Record<PreviewFilter, number> {
  return {
    all: assets.length,
    paired: assets.filter((asset) => matchesFilter(asset, "paired")).length,
    jpeg: assets.filter((asset) => matchesFilter(asset, "jpeg")).length,
    raw: assets.filter((asset) => matchesFilter(asset, "raw")).length,
  };
}

export function availablePreviewFilter(
  filter: PreviewFilter,
  counts: Record<PreviewFilter, number>,
): PreviewFilter {
  return filter === "all" || counts[filter] > 0 ? filter : "all";
}

export function shouldOpenPreviewGuide(storedValue: string | null): boolean {
  return storedValue !== "true";
}

export function previewKeyboardShortcutsEnabled(
  view: PreviewView,
  guideOpen: boolean,
  contextMenuOpen: boolean,
): boolean {
  return view === "loupe" && !guideOpen && !contextMenuOpen;
}

export function contextMenuPosition(
  x: number,
  y: number,
  viewportWidth: number,
  viewportHeight: number,
  menuWidth: number,
  menuHeight: number,
): { left: number; top: number } {
  const margin = 8;
  return {
    left: Math.max(margin, Math.min(x, viewportWidth - menuWidth - margin)),
    top: Math.max(margin, Math.min(y, viewportHeight - menuHeight - margin)),
  };
}

export function filterPreviewAssets(
  assets: PhotoAsset[],
  filter: PreviewFilter,
  search: string,
  minimumRating = 0,
): PhotoAsset[] {
  const term = search.trim().toLocaleLowerCase();
  return assets.filter((asset) => {
    if (!matchesFilter(asset, filter)) return false;
    if (asset.rating < minimumRating) return false;
    if (!term) return true;
    return `${asset.name}\n${asset.relativeStem}\n${asset.extensions.join(" ")}`
      .toLocaleLowerCase()
      .includes(term);
  });
}

export function sortPreviewAssets(
  assets: PhotoAsset[],
  sort: PreviewSort,
): PhotoAsset[] {
  return [...assets].sort((left, right) => {
    if (sort === "modified") {
      return (right.modifiedMs ?? 0) - (left.modifiedMs ?? 0)
        || left.relativeStem.localeCompare(right.relativeStem, "zh-CN", { numeric: true, sensitivity: "base" });
    }
    if (sort === "size") {
      return right.sizeBytes - left.sizeBytes
        || left.relativeStem.localeCompare(right.relativeStem, "zh-CN", { numeric: true, sensitivity: "base" });
    }
    return left.relativeStem.localeCompare(right.relativeStem, "zh-CN", {
      numeric: true,
      sensitivity: "base",
    });
  });
}

export function adjacentPreviewAssetId(
  assets: PhotoAsset[],
  currentId: string | null,
  direction: -1 | 1,
): string | null {
  if (assets.length === 0) return null;
  const currentIndex = assets.findIndex((asset) => asset.id === currentId);
  if (currentIndex < 0) return assets[0].id;
  const nextIndex = Math.max(0, Math.min(assets.length - 1, currentIndex + direction));
  return assets[nextIndex].id;
}

export function previewAssetPosition(
  assets: PhotoAsset[],
  currentId: string | null,
): number {
  if (!currentId) return 0;
  const index = assets.findIndex((asset) => asset.id === currentId);
  return index < 0 ? 0 : index + 1;
}

export function filmstripScrollTarget({
  scrollLeft,
  clientWidth,
  scrollWidth,
  itemOffsetLeft,
  itemWidth,
  padding = 10,
}: {
  scrollLeft: number;
  clientWidth: number;
  scrollWidth: number;
  itemOffsetLeft: number;
  itemWidth: number;
  padding?: number;
}): number {
  const visibleLeft = scrollLeft + padding;
  const visibleRight = scrollLeft + clientWidth - padding;
  const itemRight = itemOffsetLeft + itemWidth;
  let target = scrollLeft;

  if (itemOffsetLeft < visibleLeft) {
    target = itemOffsetLeft - padding;
  } else if (itemRight > visibleRight) {
    target = itemRight - clientWidth + padding;
  }

  return Math.max(0, Math.min(scrollWidth - clientWidth, target));
}

export interface VirtualFilmstripWindow {
  totalWidth: number;
  startIndex: number;
  endIndex: number;
}

export function virtualFilmstripWindow(input: {
  itemCount: number;
  itemPitch: number;
  viewportWidth: number;
  scrollLeft: number;
  overscan?: number;
}): VirtualFilmstripWindow {
  const itemCount = Math.max(0, Math.floor(input.itemCount));
  const itemPitch = Math.max(1, input.itemPitch);
  const viewportWidth = Math.max(0, input.viewportWidth);
  const scrollLeft = Math.max(0, input.scrollLeft);
  const overscan = Math.max(0, Math.floor(input.overscan ?? 4));
  const firstVisible = Math.floor(scrollLeft / itemPitch);
  const lastVisible = Math.ceil((scrollLeft + viewportWidth) / itemPitch);

  return {
    totalWidth: itemCount * itemPitch,
    startIndex: Math.min(itemCount, Math.max(0, firstVisible - overscan)),
    endIndex: Math.min(itemCount, lastVisible + overscan),
  };
}

export interface VirtualPhotoGridWindow {
  columns: number;
  tileHeight: number;
  rowPitch: number;
  totalHeight: number;
  startIndex: number;
  endIndex: number;
}

export function virtualPhotoGridWindow(input: {
  itemCount: number;
  tileSize: number;
  viewportWidth: number;
  viewportHeight: number;
  scrollTop: number;
  gap?: number;
  overscanRows?: number;
}): VirtualPhotoGridWindow {
  const gap = Math.max(0, input.gap ?? 12);
  const overscanRows = Math.max(0, Math.floor(input.overscanRows ?? 2));
  const tileSize = Math.max(1, input.tileSize);
  const viewportWidth = Math.max(tileSize, input.viewportWidth);
  const columns = Math.max(1, Math.floor((viewportWidth + gap) / (tileSize + gap)));
  const tileHeight = Math.ceil(Math.max(0, tileSize - 2) * 0.75 + 42);
  const rowPitch = tileHeight + gap;
  const rowCount = Math.ceil(Math.max(0, input.itemCount) / columns);
  const totalHeight = rowCount === 0 ? 0 : rowCount * tileHeight + (rowCount - 1) * gap;
  const firstVisibleRow = Math.floor(Math.max(0, input.scrollTop) / rowPitch);
  const lastVisibleRow = Math.ceil(
    (Math.max(0, input.scrollTop) + Math.max(0, input.viewportHeight)) / rowPitch,
  );
  const startRow = Math.max(0, firstVisibleRow - overscanRows);
  const endRow = Math.min(rowCount, lastVisibleRow + overscanRows);

  return {
    columns,
    tileHeight,
    rowPitch,
    totalHeight,
    startIndex: Math.min(input.itemCount, startRow * columns),
    endIndex: Math.min(input.itemCount, endRow * columns),
  };
}

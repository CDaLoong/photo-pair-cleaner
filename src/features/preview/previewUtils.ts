import type { PhotoAsset, PreviewFilter, PreviewSort } from "./types";

function matchesFilter(asset: PhotoAsset, filter: PreviewFilter): boolean {
  const hasJpeg = asset.jpegPaths.length > 0;
  const hasRaw = asset.rawPaths.length > 0;
  if (filter === "paired") return hasJpeg && hasRaw;
  if (filter === "jpeg") return hasJpeg && !hasRaw;
  if (filter === "raw") return !hasJpeg && hasRaw;
  return true;
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

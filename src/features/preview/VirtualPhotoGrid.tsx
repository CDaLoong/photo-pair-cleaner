import { Star } from "lucide-react";
import {
  useEffect,
  useLayoutEffect,
  useMemo,
  useRef,
  useState,
} from "react";
import type { CSSProperties, MouseEvent as ReactMouseEvent } from "react";
import { PhotoThumbnail } from "./PhotoThumbnail";
import { photoPreviewVersion } from "./previewCache";
import { virtualPhotoGridWindow } from "./previewUtils";
import type { PhotoAsset } from "./types";

interface VirtualPhotoGridProps {
  root: string;
  assets: PhotoAsset[];
  selectedId: string | null;
  tileSize: number;
  onSelect: (asset: PhotoAsset) => void;
  onOpen: (asset: PhotoAsset) => void;
  onContextMenu: (event: ReactMouseEvent, asset: PhotoAsset) => void;
}

interface GridViewport {
  width: number;
  height: number;
  scrollTop: number;
}

export function VirtualPhotoGrid({
  root,
  assets,
  selectedId,
  tileSize,
  onSelect,
  onOpen,
  onContextMenu,
}: VirtualPhotoGridProps) {
  const scrollRef = useRef<HTMLElement>(null);
  const ensuredSelectionRef = useRef("");
  const [viewport, setViewport] = useState<GridViewport>({
    width: tileSize,
    height: 600,
    scrollTop: 0,
  });

  useLayoutEffect(() => {
    const element = scrollRef.current;
    if (!element) return;
    let frame = 0;
    const measure = () => {
      frame = 0;
      const next = {
        width: Math.max(tileSize, element.clientWidth - 32),
        height: element.clientHeight,
        scrollTop: Math.max(0, element.scrollTop - 16),
      };
      setViewport((current) => (
        current.width === next.width
          && current.height === next.height
          && current.scrollTop === next.scrollTop
          ? current
          : next
      ));
    };
    const scheduleMeasure = () => {
      if (!frame) frame = requestAnimationFrame(measure);
    };
    measure();
    element.addEventListener("scroll", scheduleMeasure, { passive: true });
    const observer = typeof ResizeObserver === "undefined"
      ? null
      : new ResizeObserver(scheduleMeasure);
    observer?.observe(element);
    window.addEventListener("resize", scheduleMeasure);
    return () => {
      if (frame) cancelAnimationFrame(frame);
      observer?.disconnect();
      element.removeEventListener("scroll", scheduleMeasure);
      window.removeEventListener("resize", scheduleMeasure);
    };
  }, [tileSize]);

  const windowState = useMemo(() => virtualPhotoGridWindow({
    itemCount: assets.length,
    tileSize,
    viewportWidth: viewport.width,
    viewportHeight: viewport.height,
    scrollTop: viewport.scrollTop,
  }), [assets.length, tileSize, viewport]);

  useEffect(() => {
    const element = scrollRef.current;
    if (!element || !selectedId) return;
    const selectedIndex = assets.findIndex((asset) => asset.id === selectedId);
    if (selectedIndex < 0) return;
    const selectionKey = `${selectedId}:${selectedIndex}:${windowState.columns}:${tileSize}`;
    if (ensuredSelectionRef.current === selectionKey) return;
    ensuredSelectionRef.current = selectionKey;
    const row = Math.floor(selectedIndex / windowState.columns);
    const top = row * windowState.rowPitch;
    const bottom = top + windowState.tileHeight;
    const visibleTop = Math.max(0, element.scrollTop - 16);
    const visibleBottom = visibleTop + element.clientHeight;
    if (top >= visibleTop && bottom <= visibleBottom) return;
    element.scrollTop = Math.max(
      0,
      top - Math.max(0, (element.clientHeight - windowState.tileHeight) / 2) + 16,
    );
  }, [assets, selectedId, tileSize, windowState]);

  const visibleAssets = assets.slice(windowState.startIndex, windowState.endIndex);

  return (
    <main ref={scrollRef} className="photo-grid-scroll" data-preview-tour="grid">
      <div
        className="photo-grid photo-grid-virtual"
        style={{
          "--preview-tile-size": `${tileSize}px`,
          height: `${windowState.totalHeight}px`,
        } as CSSProperties}
      >
        {visibleAssets.map((asset, visibleIndex) => {
          const assetIndex = windowState.startIndex + visibleIndex;
          const row = Math.floor(assetIndex / windowState.columns);
          const column = assetIndex % windowState.columns;
          return (
            <button
              key={asset.id}
              className={asset.id === selectedId ? "photo-tile is-selected" : "photo-tile"}
              type="button"
              onClick={() => onSelect(asset)}
              onDoubleClick={() => onOpen(asset)}
              onContextMenu={(event) => onContextMenu(event, asset)}
              aria-pressed={asset.id === selectedId}
              title={`${asset.relativeStem} · ${asset.extensions.join(" + ")}`}
              style={{
                width: `${tileSize}px`,
                height: `${windowState.tileHeight}px`,
                transform: `translate(${column * (tileSize + 12)}px, ${row * windowState.rowPitch}px)`,
              }}
            >
              <PhotoThumbnail
                root={root}
                relativePath={asset.previewPath}
                maxEdge={480}
                version={photoPreviewVersion(asset)}
                alt=""
              />
              {asset.rating > 0 ? (
                <span className="photo-rating-badge">
                  <Star aria-hidden="true" size={11} fill="currentColor" />
                  {asset.rating}
                </span>
              ) : null}
              <span className="photo-tile-meta">
                <strong>{asset.name}</strong>
                <small>{asset.extensions.join(" + ")}</small>
              </span>
            </button>
          );
        })}
      </div>
    </main>
  );
}

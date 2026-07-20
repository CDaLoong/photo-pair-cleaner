import { Star } from "lucide-react";
import {
  useLayoutEffect,
  useMemo,
  useRef,
  useState,
} from "react";
import type { CSSProperties, MouseEvent as ReactMouseEvent } from "react";
import { PhotoThumbnail } from "./PhotoThumbnail";
import { photoPreviewVersion } from "./previewCache";
import {
  filmstripScrollTarget,
  virtualFilmstripWindow,
} from "./previewUtils";
import type { PhotoAsset } from "./types";

const ITEM_WIDTH = 88;
const ITEM_GAP = 7;
const ITEM_PITCH = ITEM_WIDTH + ITEM_GAP;
const HORIZONTAL_PADDING = 10;

interface VirtualPhotoFilmstripProps {
  root: string;
  assets: PhotoAsset[];
  selectedId: string | null;
  onSelect: (asset: PhotoAsset) => void;
  onContextMenu: (event: ReactMouseEvent, asset: PhotoAsset) => void;
}

interface FilmstripViewport {
  width: number;
  scrollLeft: number;
}

export function VirtualPhotoFilmstrip({
  root,
  assets,
  selectedId,
  onSelect,
  onContextMenu,
}: VirtualPhotoFilmstripProps) {
  const scrollRef = useRef<HTMLDivElement>(null);
  const [viewport, setViewport] = useState<FilmstripViewport>({
    width: 950,
    scrollLeft: 0,
  });

  useLayoutEffect(() => {
    const element = scrollRef.current;
    if (!element) return;
    let frame = 0;
    const measure = () => {
      frame = 0;
      const next = {
        width: element.clientWidth,
        scrollLeft: element.scrollLeft,
      };
      setViewport((current) => (
        current.width === next.width && current.scrollLeft === next.scrollLeft
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
  }, []);

  useLayoutEffect(() => {
    const element = scrollRef.current;
    if (!element || !selectedId) return;
    const selectedIndex = assets.findIndex((asset) => asset.id === selectedId);
    if (selectedIndex < 0) return;
    const target = filmstripScrollTarget({
      scrollLeft: element.scrollLeft,
      clientWidth: element.clientWidth,
      scrollWidth: assets.length * ITEM_PITCH + HORIZONTAL_PADDING * 2,
      itemOffsetLeft: HORIZONTAL_PADDING + selectedIndex * ITEM_PITCH,
      itemWidth: ITEM_WIDTH,
    });
    if (target === element.scrollLeft) return;
    element.scrollLeft = target;
    setViewport((current) => ({ ...current, scrollLeft: target }));
  }, [assets, selectedId]);

  const windowState = useMemo(() => virtualFilmstripWindow({
    itemCount: assets.length,
    itemPitch: ITEM_PITCH,
    viewportWidth: viewport.width,
    scrollLeft: viewport.scrollLeft,
    overscan: 5,
  }), [assets.length, viewport]);
  const visibleAssets = assets.slice(windowState.startIndex, windowState.endIndex);

  return (
    <div
      ref={scrollRef}
      className="loupe-filmstrip"
      role="listbox"
      aria-label="照片胶片栏"
      aria-orientation="horizontal"
    >
      <div
        className="loupe-filmstrip-window"
        style={{
          width: `${windowState.totalWidth + HORIZONTAL_PADDING * 2}px`,
        } as CSSProperties}
      >
        {visibleAssets.map((asset, visibleIndex) => {
          const assetIndex = windowState.startIndex + visibleIndex;
          const selected = asset.id === selectedId;
          return (
            <button
              key={asset.id}
              type="button"
              role="option"
              className={selected ? "filmstrip-item is-selected" : "filmstrip-item"}
              onClick={() => onSelect(asset)}
              onContextMenu={(event) => onContextMenu(event, asset)}
              aria-selected={selected}
              aria-label={asset.name}
              title={asset.name}
              style={{ left: `${HORIZONTAL_PADDING + assetIndex * ITEM_PITCH}px` }}
            >
              <PhotoThumbnail
                root={root}
                relativePath={asset.previewPath}
                maxEdge={512}
                version={photoPreviewVersion(asset)}
                alt=""
              />
              {asset.rating > 0 ? (
                <span className="filmstrip-rating">
                  <Star aria-hidden="true" size={10} fill="currentColor" />
                  {asset.rating}
                </span>
              ) : null}
            </button>
          );
        })}
      </div>
    </div>
  );
}

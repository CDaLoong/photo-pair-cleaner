import { CircleAlert, LoaderCircle } from "lucide-react";
import { useLayoutEffect, useRef } from "react";
import type { KeyboardEvent } from "react";
import type { PreloadProgress } from "../preview/previewCache";
import { filmstripScrollTarget } from "../preview/previewUtils";
import { WatermarkPhotoThumbnail } from "./WatermarkSourcePanel";
import type { WatermarkSourcePhoto } from "./types";

const ORIENTATION_LABELS = { landscape: "横", portrait: "竖", square: "方" } as const;

interface WatermarkFilmstripProps {
  "data-watermark-tour"?: string;
  photos: WatermarkSourcePhoto[];
  snapshotId: string;
  selectedPhotoId: string | null;
  preloadProgress: PreloadProgress;
  warningPhotoIds: ReadonlySet<string>;
  onSelectPhoto: (photoId: string) => void;
}

export function WatermarkFilmstrip({
  "data-watermark-tour": tourTarget,
  photos,
  snapshotId,
  selectedPhotoId,
  preloadProgress,
  warningPhotoIds,
  onSelectPhoto,
}: WatermarkFilmstripProps) {
  const scrollerRef = useRef<HTMLDivElement>(null);
  const selectedItemRef = useRef<HTMLButtonElement>(null);
  const selectedIndex = photos.findIndex((photo) => photo.id === selectedPhotoId);

  useLayoutEffect(() => {
    const scroller = scrollerRef.current;
    const selectedItem = selectedItemRef.current;
    if (!scroller || !selectedItem) return;
    const scrollerRect = scroller.getBoundingClientRect();
    const itemRect = selectedItem.getBoundingClientRect();
    scroller.scrollLeft = filmstripScrollTarget({
      scrollLeft: scroller.scrollLeft,
      clientWidth: scroller.clientWidth,
      scrollWidth: scroller.scrollWidth,
      itemOffsetLeft: itemRect.left - scrollerRect.left + scroller.scrollLeft,
      itemWidth: itemRect.width,
    });
  }, [selectedPhotoId, photos]);

  function handleKeyDown(event: KeyboardEvent<HTMLDivElement>) {
    let nextIndex = selectedIndex;
    if (event.key === "ArrowLeft") nextIndex = Math.max(0, selectedIndex - 1);
    else if (event.key === "ArrowRight") nextIndex = Math.min(photos.length - 1, selectedIndex + 1);
    else if (event.key === "Home") nextIndex = 0;
    else if (event.key === "End") nextIndex = photos.length - 1;
    else return;
    event.preventDefault();
    event.stopPropagation();
    const photo = photos[nextIndex];
    if (photo) onSelectPhoto(photo.id);
  }

  const preloadComplete = preloadProgress.total > 0 && preloadProgress.completed >= preloadProgress.total;
  return (
    <section className="watermark-filmstrip" aria-label="照片胶片栏" data-watermark-tour={tourTarget}>
      <header>
        <strong>{selectedIndex >= 0 ? selectedIndex + 1 : 0} / {photos.length}</strong>
        <span className={preloadComplete ? "is-ready" : undefined}>
          {!preloadComplete ? <LoaderCircle className="spin" aria-hidden="true" size={12} /> : null}
          {preloadProgress.completed} / {preloadProgress.total} 缩略图
        </span>
      </header>
      <div ref={scrollerRef} className="watermark-filmstrip-scroll" role="listbox" aria-orientation="horizontal" tabIndex={0} aria-label="选择要预览的照片" onKeyDown={handleKeyDown}>
        {photos.map((photo, index) => {
          const selected = photo.id === selectedPhotoId;
          return (
            <button
              ref={selected ? selectedItemRef : undefined}
              type="button"
              role="option"
              aria-selected={selected}
              className={selected ? "is-selected" : undefined}
              key={photo.id}
              onClick={() => onSelectPhoto(photo.id)}
              title={`${index + 1}. ${photo.fileName}`}
            >
              <WatermarkPhotoThumbnail photo={photo} snapshotId={snapshotId} />
              <span className="watermark-filmstrip-orientation">{ORIENTATION_LABELS[photo.orientation]}</span>
              {warningPhotoIds.has(photo.id) ? <CircleAlert className="watermark-filmstrip-warning" aria-label="此照片存在预览提示" size={13} /> : null}
              <small>{photo.fileName}</small>
            </button>
          );
        })}
      </div>
    </section>
  );
}

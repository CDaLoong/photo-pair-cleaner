import {
  FileImage,
  ImageOff,
  LoaderCircle,
  Minimize2,
  ZoomIn,
  ZoomOut,
} from "lucide-react";
import { useCallback, useEffect, useRef, useState } from "react";
import type {
  KeyboardEvent as ReactKeyboardEvent,
  PointerEvent as ReactPointerEvent,
  WheelEvent as ReactWheelEvent,
} from "react";
import {
  acquirePhotoOriginalUrl,
  acquirePhotoPreviewUrl,
  originalPhotoRequestKey,
  peekPhotoOriginalUrl,
  peekPhotoPreviewUrl,
  previewRequestKey,
  type OriginalPhotoRequest,
  type PreviewUrlLease,
  type PreviewRequest,
} from "./previewCache";
import {
  clampPhotoViewport,
  zoomPhotoViewportAtPoint,
  type PhotoViewportGeometry,
  type PhotoViewportTransform,
} from "./previewUtils";

interface PhotoThumbnailProps {
  root: string;
  relativePath: string | null;
  maxEdge: number;
  version: string;
  alt: string;
  eager?: boolean;
  qualityFirst?: boolean;
  onFullReady?: () => void;
  originalFirst?: boolean;
  zoomable?: boolean;
}

type LoadState = "idle" | "loading" | "preview" | "ready" | "error";

const QUICK_PREVIEW_EDGE = 512;
const MIN_ZOOM = 1;
const MAX_ZOOM = 8;
const CLICK_ZOOM = 2;
const DEFAULT_VIEWPORT: PhotoViewportTransform = { scale: MIN_ZOOM, x: 0, y: 0 };

interface PhotoDragState {
  pointerId: number;
  startClientX: number;
  startClientY: number;
  originX: number;
  originY: number;
  scale: number;
  moved: boolean;
}

export function PhotoThumbnail({
  root,
  relativePath,
  maxEdge,
  version,
  alt,
  eager = false,
  qualityFirst = false,
  onFullReady,
  originalFirst = false,
  zoomable = false,
}: PhotoThumbnailProps) {
  const containerRef = useRef<HTMLDivElement>(null);
  const imageRef = useRef<HTMLImageElement>(null);
  const onFullReadyRef = useRef(onFullReady);
  const dragRef = useRef<PhotoDragState | null>(null);
  const [visible, setVisible] = useState(eager);
  const [loaded, setLoaded] = useState<{ key: string; url: string } | null>(null);
  const [displayLoaded, setDisplayLoaded] = useState<{ key: string; url: string } | null>(null);
  const [status, setStatus] = useState<{ key: string; state: LoadState } | null>(null);
  const [paintedSource, setPaintedSource] = useState<string | null>(null);
  const [viewport, setViewport] = useState<PhotoViewportTransform>(DEFAULT_VIEWPORT);
  const [dragging, setDragging] = useState(false);
  const request: PreviewRequest | null = relativePath ? {
    root,
    relativePath,
    maxEdge,
    version,
  } : null;
  const originalRequest: OriginalPhotoRequest | null = relativePath ? {
    root,
    relativePath,
    version,
  } : null;
  const requestKey = originalFirst && originalRequest
    ? originalPhotoRequestKey(originalRequest)
    : request
      ? previewRequestKey(request)
      : "";
  const previewEdge = Math.min(maxEdge, QUICK_PREVIEW_EDGE);
  const cachedFull = originalFirst && originalRequest
    ? peekPhotoOriginalUrl(originalRequest)
    : request
      ? peekPhotoPreviewUrl(request)
      : null;
  const cachedPreview = relativePath ? peekPhotoPreviewUrl({
    root,
    relativePath,
    maxEdge: previewEdge,
    version,
  }) : null;
  const cachedDisplay = originalFirst && request ? peekPhotoPreviewUrl(request) : null;
  const currentStatus = status?.key === requestKey ? status.state : null;
  const currentSource = loaded?.key === requestKey
    ? loaded.url
    : cachedFull ?? (originalFirst ? null : cachedPreview);
  const fallbackSource = originalFirst
    ? displayLoaded?.key === requestKey
      ? displayLoaded.url
      : cachedDisplay
    : null;
  const source = currentSource ?? fallbackSource;
  const imageLayerSources = Array.from(new Set(
    [paintedSource, fallbackSource, source].filter((value): value is string => Boolean(value)),
  ));
  const loadState: LoadState = currentStatus
    ? currentStatus
    : currentSource
      ? "ready"
      : relativePath && visible
        ? "loading"
        : "idle";

  const photoGeometry = useCallback((): PhotoViewportGeometry | null => {
    const container = containerRef.current;
    const image = imageRef.current;
    if (!container || !image) return null;
    return {
      viewportWidth: container.clientWidth,
      viewportHeight: container.clientHeight,
      imageWidth: image.offsetWidth,
      imageHeight: image.offsetHeight,
    };
  }, []);

  const clampToPhoto = useCallback((candidate: PhotoViewportTransform) => {
    const geometry = photoGeometry();
    return geometry
      ? clampPhotoViewport(candidate, geometry, MIN_ZOOM, MAX_ZOOM)
      : candidate;
  }, [photoGeometry]);

  const zoomAt = useCallback((
    targetScale: number,
    clientX?: number,
    clientY?: number,
  ) => {
    const container = containerRef.current;
    const geometry = photoGeometry();
    if (!container || !geometry) return;
    const bounds = container.getBoundingClientRect();
    const pointX = clientX === undefined
      ? 0
      : clientX - (bounds.left + bounds.width / 2);
    const pointY = clientY === undefined
      ? 0
      : clientY - (bounds.top + bounds.height / 2);
    setViewport((current) => zoomPhotoViewportAtPoint(
      current,
      targetScale,
      pointX,
      pointY,
      geometry,
      MIN_ZOOM,
      MAX_ZOOM,
    ));
  }, [photoGeometry]);

  const zoomBy = useCallback((factor: number, clientX?: number, clientY?: number) => {
    const container = containerRef.current;
    const geometry = photoGeometry();
    if (!container || !geometry) return;
    const bounds = container.getBoundingClientRect();
    const pointX = clientX === undefined
      ? 0
      : clientX - (bounds.left + bounds.width / 2);
    const pointY = clientY === undefined
      ? 0
      : clientY - (bounds.top + bounds.height / 2);
    setViewport((current) => zoomPhotoViewportAtPoint(
      current,
      current.scale * factor,
      pointX,
      pointY,
      geometry,
      MIN_ZOOM,
      MAX_ZOOM,
    ));
  }, [photoGeometry]);

  const resetViewport = useCallback(() => {
    dragRef.current = null;
    setDragging(false);
    setViewport({ ...DEFAULT_VIEWPORT });
  }, []);

  const handlePointerDown = useCallback((event: ReactPointerEvent<HTMLDivElement>) => {
    if (!zoomable || !source || event.button !== 0) return;
    event.preventDefault();
    event.currentTarget.focus({ preventScroll: true });
    event.currentTarget.setPointerCapture(event.pointerId);
    dragRef.current = {
      pointerId: event.pointerId,
      startClientX: event.clientX,
      startClientY: event.clientY,
      originX: viewport.x,
      originY: viewport.y,
      scale: viewport.scale,
      moved: false,
    };
    setDragging(viewport.scale > MIN_ZOOM);
  }, [source, viewport, zoomable]);

  const handlePointerMove = useCallback((event: ReactPointerEvent<HTMLDivElement>) => {
    const drag = dragRef.current;
    if (!drag || drag.pointerId !== event.pointerId) return;
    const deltaX = event.clientX - drag.startClientX;
    const deltaY = event.clientY - drag.startClientY;
    if (Math.hypot(deltaX, deltaY) >= 3) drag.moved = true;
    if (drag.scale <= MIN_ZOOM) return;
    event.preventDefault();
    setViewport(clampToPhoto({
      scale: drag.scale,
      x: drag.originX + deltaX,
      y: drag.originY + deltaY,
    }));
  }, [clampToPhoto]);

  const finishPointer = useCallback((event: ReactPointerEvent<HTMLDivElement>, cancelled: boolean) => {
    const drag = dragRef.current;
    if (!drag || drag.pointerId !== event.pointerId) return;
    dragRef.current = null;
    if (event.currentTarget.hasPointerCapture(event.pointerId)) {
      event.currentTarget.releasePointerCapture(event.pointerId);
    }
    setDragging(false);
    if (cancelled || drag.moved) return;
    if (drag.scale > MIN_ZOOM) {
      resetViewport();
    } else {
      zoomAt(CLICK_ZOOM, event.clientX, event.clientY);
    }
  }, [resetViewport, zoomAt]);

  const handleWheel = useCallback((event: ReactWheelEvent<HTMLDivElement>) => {
    if (!zoomable || !source) return;
    event.preventDefault();
    const deltaPixels = event.deltaMode === 1
      ? event.deltaY * 16
      : event.deltaMode === 2
        ? event.deltaY * event.currentTarget.clientHeight
        : event.deltaY;
    const factor = Math.min(1.35, Math.max(0.74, Math.exp(-deltaPixels * 0.0015)));
    zoomBy(factor, event.clientX, event.clientY);
  }, [source, zoomBy, zoomable]);

  const handleZoomKey = useCallback((event: ReactKeyboardEvent<HTMLDivElement>) => {
    if (!zoomable || !source) return;
    if (event.key === "+" || event.key === "=") {
      event.preventDefault();
      event.stopPropagation();
      zoomBy(1.5);
    } else if (event.key === "-") {
      event.preventDefault();
      event.stopPropagation();
      zoomBy(1 / 1.5);
    } else if (event.key === "0") {
      event.preventDefault();
      event.stopPropagation();
      resetViewport();
    }
  }, [resetViewport, source, zoomBy, zoomable]);

  useEffect(() => {
    onFullReadyRef.current = onFullReady;
  }, [onFullReady]);

  useEffect(() => {
    if (zoomable) resetViewport();
  }, [requestKey, resetViewport, zoomable]);

  useEffect(() => {
    if (!zoomable || typeof ResizeObserver === "undefined") return;
    const container = containerRef.current;
    if (!container) return;
    const observer = new ResizeObserver(() => {
      setViewport((current) => clampToPhoto(current));
    });
    observer.observe(container);
    return () => observer.disconnect();
  }, [clampToPhoto, zoomable]);

  useEffect(() => {
    if (eager) {
      setVisible(true);
      return;
    }
    const element = containerRef.current;
    if (!element || typeof IntersectionObserver === "undefined") {
      setVisible(true);
      return;
    }
    const observer = new IntersectionObserver(
      (entries) => {
        setVisible(entries.some((entry) => entry.isIntersecting));
      },
      { rootMargin: "600px" },
    );
    observer.observe(element);
    return () => observer.disconnect();
  }, [eager]);

  useEffect(() => {
    if (!relativePath) {
      setLoaded(null);
      setDisplayLoaded(null);
      setStatus(null);
      return;
    }
    if (!visible) {
      setLoaded(null);
      setStatus({ key: requestKey, state: "idle" });
      return;
    }
    let disposed = false;
    let previewLease: PreviewUrlLease | null = null;
    let displayLease: PreviewUrlLease | null = null;
    let fullLease: PreviewUrlLease | null = null;
    const previewRequest: PreviewRequest = {
      root,
      relativePath,
      maxEdge: previewEdge,
      version,
    };
    const fullRequest: PreviewRequest = { root, relativePath, maxEdge, version };
    const fullOriginalRequest: OriginalPhotoRequest = { root, relativePath, version };
    const nextKey = originalFirst
      ? originalPhotoRequestKey(fullOriginalRequest)
      : previewRequestKey(fullRequest);
    const initialFullUrl = originalFirst
      ? peekPhotoOriginalUrl(fullOriginalRequest)
      : peekPhotoPreviewUrl(fullRequest);
    const initialDisplayUrl = originalFirst ? peekPhotoPreviewUrl(fullRequest) : null;
    const initialPreviewUrl = peekPhotoPreviewUrl(previewRequest);
    const acquireFullPhoto = (priority: "foreground" | "background") => originalFirst
      ? acquirePhotoOriginalUrl(fullOriginalRequest, priority)
      : acquirePhotoPreviewUrl(fullRequest, priority);
    setDisplayLoaded(initialDisplayUrl ? { key: nextKey, url: initialDisplayUrl } : null);
    if (initialFullUrl) {
      setLoaded({ key: nextKey, url: initialFullUrl });
      setStatus({ key: nextKey, state: "ready" });
      onFullReadyRef.current?.();
    } else if (initialDisplayUrl || (!originalFirst && initialPreviewUrl)) {
      setLoaded(originalFirst ? null : {
        key: nextKey,
        url: initialDisplayUrl ?? initialPreviewUrl!,
      });
      setStatus({
        key: nextKey,
        state: originalFirst || previewEdge !== maxEdge ? "preview" : "ready",
      });
    } else {
      setLoaded(null);
      setStatus({ key: nextKey, state: "loading" });
    }

    if (originalFirst) {
      displayLease = acquirePhotoPreviewUrl(fullRequest, "foreground");
      void displayLease.promise.then((displayUrl) => {
        if (!disposed) setDisplayLoaded({ key: nextKey, url: displayUrl });
      }).catch(() => undefined);
    }

    void (async () => {
      if (qualityFirst) {
        if (originalFirst) {
          fullLease = acquireFullPhoto("foreground");
          try {
            const fullUrl = await fullLease.promise;
            if (disposed) return;
            setLoaded({ key: nextKey, url: fullUrl });
            setStatus({ key: nextKey, state: "ready" });
            onFullReadyRef.current?.();
          } catch {
            if (!disposed) {
              setStatus({
                key: nextKey,
                state: initialDisplayUrl ? "preview" : "error",
              });
            }
          }
          return;
        }
        if (initialFullUrl) {
          fullLease = acquireFullPhoto("foreground");
          try {
            const fullUrl = await fullLease.promise;
            if (disposed) return;
            setLoaded({ key: nextKey, url: fullUrl });
            setStatus({ key: nextKey, state: "ready" });
            onFullReadyRef.current?.();
          } catch {
            if (!disposed) setStatus({ key: nextKey, state: "error" });
          }
          return;
        }
        let previewReady = false;
        fullLease = acquireFullPhoto("foreground");
        previewLease = acquirePhotoPreviewUrl(previewRequest, "foreground");
        const previewPromise = previewLease.promise
          .then((previewUrl) => {
            if (disposed) return;
            previewReady = true;
            setLoaded((current) => current?.key === nextKey ? current : {
              key: nextKey,
              url: previewUrl,
            });
            setStatus((current) => current?.key === nextKey && current.state === "ready"
              ? current
              : { key: nextKey, state: "preview" });
          })
          .catch(() => undefined);
        try {
          const fullUrl = await fullLease.promise;
          if (disposed) return;
          setLoaded({ key: nextKey, url: fullUrl });
          setStatus({ key: nextKey, state: "ready" });
          onFullReadyRef.current?.();
          previewLease.release();
          previewLease = null;
          return;
        } catch {
          if (disposed) return;
          await previewPromise;
          if (!disposed) {
            setStatus({ key: nextKey, state: previewReady ? "ready" : "error" });
          }
          return;
        }
      }

      setLoaded(null);
      try {
        previewLease = acquirePhotoPreviewUrl(previewRequest, "foreground");
        const previewUrl = await previewLease.promise;
        if (disposed) return;
        setLoaded({ key: nextKey, url: previewUrl });

        if (previewEdge === maxEdge) {
          setStatus({ key: nextKey, state: "ready" });
          onFullReadyRef.current?.();
          return;
        }

        setStatus({ key: nextKey, state: "preview" });
        try {
          fullLease = acquirePhotoPreviewUrl(
            fullRequest,
            eager ? "foreground" : "background",
          );
          const fullUrl = await fullLease.promise;
          if (disposed) return;
          setLoaded({ key: nextKey, url: fullUrl });
          setStatus({ key: nextKey, state: "ready" });
          onFullReadyRef.current?.();
          previewLease.release();
          previewLease = null;
        } catch {
          if (!disposed) setStatus({ key: nextKey, state: "ready" });
        }
      } catch {
        if (!disposed) setStatus({ key: nextKey, state: "error" });
      }
    })();

    return () => {
      disposed = true;
      previewLease?.release();
      displayLease?.release();
      fullLease?.release();
    };
  }, [eager, maxEdge, originalFirst, qualityFirst, relativePath, requestKey, root, version, visible]);

  const className = [
    "photo-thumbnail",
    `is-${loadState}`,
    zoomable ? "is-zoomable" : "",
    zoomable && viewport.scale > MIN_ZOOM ? "is-zoomed" : "",
    dragging ? "is-dragging" : "",
  ].filter(Boolean).join(" ");

  return (
    <div
      ref={containerRef}
      className={className}
      role={zoomable ? "group" : undefined}
      aria-label={zoomable ? `${alt} 预览` : undefined}
      tabIndex={zoomable ? 0 : undefined}
      onKeyDown={zoomable ? handleZoomKey : undefined}
      onPointerDown={zoomable ? handlePointerDown : undefined}
      onPointerMove={zoomable ? handlePointerMove : undefined}
      onPointerUp={zoomable ? (event) => finishPointer(event, false) : undefined}
      onPointerCancel={zoomable ? (event) => finishPointer(event, true) : undefined}
      onWheel={zoomable ? handleWheel : undefined}
    >
      {imageLayerSources.length > 0 ? imageLayerSources.map((imageSource) => {
        const candidate = imageSource === source;
        const fallbackCandidate = imageSource === fallbackSource;
        const current = imageSource === paintedSource || paintedSource === null;
        return <img
          key={imageSource}
          ref={candidate ? imageRef : undefined}
          className={`photo-image-layer ${current ? "is-current" : "is-pending"}`}
          src={imageSource}
          alt={candidate ? alt : ""}
          aria-hidden={candidate ? undefined : true}
          draggable={false}
          decoding="async"
          style={zoomable ? {
            transform: `translate3d(${viewport.x}px, ${viewport.y}px, 0) scale(${viewport.scale})`,
          } : undefined}
          onLoad={candidate || fallbackCandidate ? (event) => {
            const image = event.currentTarget;
            const promote = () => {
              if (!image.isConnected || image.getAttribute("src") !== imageSource) return;
              setPaintedSource((painted) => {
                if (fallbackCandidate && painted === source && source !== fallbackSource) {
                  return painted;
                }
                return imageSource;
              });
              if (zoomable) {
                setViewport((viewportState) => clampToPhoto(viewportState));
              }
            };
            if (typeof image.decode === "function") {
              void image.decode().then(promote).catch(() => undefined);
            } else {
              promote();
            }
          } : undefined}
        />;
      }) : relativePath ? (
        loadState === "error"
          ? <span className="thumbnail-fallback"><ImageOff aria-hidden="true" size={24} /><small>无法预览</small></span>
          : loadState === "idle"
            ? null
            : <span className="thumbnail-fallback"><LoaderCircle className="spin" aria-hidden="true" size={22} /><small>载入中</small></span>
      ) : <span className="thumbnail-fallback raw-fallback"><FileImage aria-hidden="true" size={27} /><small>RAW</small></span>}
      {zoomable && source ? (
        <div
          className="photo-zoom-controls"
          role="toolbar"
          aria-label="缩放控制"
          onPointerDown={(event) => event.stopPropagation()}
        >
          <button
            className="icon-button zoom-control-button"
            type="button"
            onClick={(event) => { event.stopPropagation(); zoomBy(1 / 1.5); }}
            disabled={viewport.scale <= MIN_ZOOM}
            aria-label="缩小"
            title="缩小"
          ><ZoomOut aria-hidden="true" size={17} /></button>
          <button
            className="icon-button zoom-control-button"
            type="button"
            onClick={(event) => { event.stopPropagation(); resetViewport(); }}
            disabled={viewport.scale <= MIN_ZOOM}
            aria-label="适合窗口"
            title="适合窗口"
          ><Minimize2 aria-hidden="true" size={17} /></button>
          <button
            className="icon-button zoom-control-button"
            type="button"
            onClick={(event) => { event.stopPropagation(); zoomBy(1.5); }}
            disabled={viewport.scale >= MAX_ZOOM}
            aria-label="放大"
            title="放大"
          ><ZoomIn aria-hidden="true" size={17} /></button>
        </div>
      ) : null}
    </div>
  );
}

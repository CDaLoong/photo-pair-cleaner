import { FileImage, ImageOff, LoaderCircle } from "lucide-react";
import { useEffect, useRef, useState } from "react";
import {
  acquirePhotoPreviewUrl,
  peekPhotoPreviewUrl,
  previewRequestKey,
  type PreviewUrlLease,
  type PreviewRequest,
} from "./previewCache";

interface PhotoThumbnailProps {
  root: string;
  relativePath: string | null;
  maxEdge: number;
  version: string;
  alt: string;
  eager?: boolean;
}

type LoadState = "idle" | "loading" | "preview" | "ready" | "error";

const QUICK_PREVIEW_EDGE = 256;

export function PhotoThumbnail({
  root,
  relativePath,
  maxEdge,
  version,
  alt,
  eager = false,
}: PhotoThumbnailProps) {
  const containerRef = useRef<HTMLDivElement>(null);
  const [visible, setVisible] = useState(eager);
  const [loaded, setLoaded] = useState<{ key: string; url: string } | null>(null);
  const [status, setStatus] = useState<{ key: string; state: LoadState } | null>(null);
  const request: PreviewRequest | null = relativePath ? {
    root,
    relativePath,
    maxEdge,
    version,
  } : null;
  const requestKey = request ? previewRequestKey(request) : "";
  const previewEdge = Math.min(maxEdge, QUICK_PREVIEW_EDGE);
  const cachedPreview = relativePath ? peekPhotoPreviewUrl({
    root,
    relativePath,
    maxEdge: previewEdge,
    version,
  }) : null;
  const source = loaded?.key === requestKey ? loaded.url : cachedPreview;
  const loadState: LoadState = status?.key === requestKey
    ? status.state
    : source
      ? "ready"
      : relativePath && visible
        ? "loading"
        : "idle";

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
    let fullLease: PreviewUrlLease | null = null;
    const previewRequest: PreviewRequest = {
      root,
      relativePath,
      maxEdge: previewEdge,
      version,
    };
    const fullRequest: PreviewRequest = { root, relativePath, maxEdge, version };
    const nextKey = previewRequestKey(fullRequest);
    setLoaded(null);
    setStatus({ key: nextKey, state: "loading" });

    void (async () => {
      try {
        previewLease = acquirePhotoPreviewUrl(previewRequest, "foreground");
        const previewUrl = await previewLease.promise;
        if (disposed) return;
        setLoaded({ key: nextKey, url: previewUrl });

        if (previewEdge === maxEdge) {
          setStatus({ key: nextKey, state: "ready" });
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
      fullLease?.release();
    };
  }, [eager, maxEdge, relativePath, requestKey, root, version, visible]);

  return (
    <div ref={containerRef} className={`photo-thumbnail is-${loadState}`}>
      {source ? <img src={source} alt={alt} draggable={false} decoding="async" /> : relativePath ? (
        loadState === "error"
          ? <span className="thumbnail-fallback"><ImageOff aria-hidden="true" size={24} /><small>无法预览</small></span>
          : loadState === "idle"
            ? null
            : <span className="thumbnail-fallback"><LoaderCircle className="spin" aria-hidden="true" size={22} /><small>载入中</small></span>
      ) : <span className="thumbnail-fallback raw-fallback"><FileImage aria-hidden="true" size={27} /><small>RAW</small></span>}
    </div>
  );
}

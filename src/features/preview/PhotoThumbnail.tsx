import { FileImage, ImageOff, LoaderCircle } from "lucide-react";
import { useEffect, useRef, useState } from "react";
import {
  loadPhotoPreviewUrl,
  peekPhotoPreviewUrl,
  previewRequestKey,
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

type LoadState = "idle" | "loading" | "ready" | "error";

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
  const cachedSource = request ? peekPhotoPreviewUrl(request) : null;
  const source = cachedSource ?? (loaded?.key === requestKey ? loaded.url : null);
  const loadState: LoadState = source
    ? "ready"
    : status?.key === requestKey
      ? status.state
      : relativePath
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
        if (entries.some((entry) => entry.isIntersecting)) {
          setVisible(true);
          observer.disconnect();
        }
      },
      { rootMargin: "240px" },
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
    if (!visible) return;
    let disposed = false;
    const nextRequest: PreviewRequest = { root, relativePath, maxEdge, version };
    const nextKey = previewRequestKey(nextRequest);
    const cached = peekPhotoPreviewUrl(nextRequest);
    if (cached) {
      setLoaded({ key: nextKey, url: cached });
      setStatus({ key: nextKey, state: "ready" });
      return;
    }
    setStatus({ key: nextKey, state: "loading" });

    void loadPhotoPreviewUrl(nextRequest).then((url) => {
      if (disposed) return;
      setLoaded({ key: nextKey, url });
      setStatus({ key: nextKey, state: "ready" });
    }).catch(() => {
      if (!disposed) setStatus({ key: nextKey, state: "error" });
    });

    return () => {
      disposed = true;
    };
  }, [maxEdge, relativePath, root, version, visible]);

  return (
    <div ref={containerRef} className={`photo-thumbnail is-${loadState}`}>
      {source ? <img src={source} alt={alt} draggable={false} /> : relativePath ? (
        loadState === "error"
          ? <span className="thumbnail-fallback"><ImageOff aria-hidden="true" size={24} /><small>无法预览</small></span>
          : <span className="thumbnail-fallback"><LoaderCircle className="spin" aria-hidden="true" size={22} /><small>载入中</small></span>
      ) : <span className="thumbnail-fallback raw-fallback"><FileImage aria-hidden="true" size={27} /><small>RAW</small></span>}
    </div>
  );
}

import { invoke } from "@tauri-apps/api/core";
import { FileImage, ImageOff, LoaderCircle } from "lucide-react";
import { useEffect, useRef, useState } from "react";

interface PhotoThumbnailProps {
  root: string;
  relativePath: string | null;
  maxEdge: number;
  alt: string;
  eager?: boolean;
}

type LoadState = "idle" | "loading" | "ready" | "error";

export function PhotoThumbnail({
  root,
  relativePath,
  maxEdge,
  alt,
  eager = false,
}: PhotoThumbnailProps) {
  const containerRef = useRef<HTMLDivElement>(null);
  const [visible, setVisible] = useState(eager);
  const [source, setSource] = useState<string | null>(null);
  const [loadState, setLoadState] = useState<LoadState>("idle");

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
      setSource(null);
      setLoadState("idle");
      return;
    }
    if (!visible) return;
    let disposed = false;
    let objectUrl: string | null = null;
    setLoadState("loading");
    setSource(null);

    void invoke<ArrayBuffer | number[]>("load_photo_thumbnail", {
      root,
      relativePath,
      maxEdge,
    }).then((response) => {
      if (disposed) return;
      const bytes = response instanceof ArrayBuffer
        ? new Uint8Array(response)
        : Uint8Array.from(response);
      objectUrl = URL.createObjectURL(new Blob([bytes.buffer as ArrayBuffer], { type: "image/jpeg" }));
      setSource(objectUrl);
      setLoadState("ready");
    }).catch(() => {
      if (!disposed) setLoadState("error");
    });

    return () => {
      disposed = true;
      if (objectUrl) URL.revokeObjectURL(objectUrl);
    };
  }, [maxEdge, relativePath, root, visible]);

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

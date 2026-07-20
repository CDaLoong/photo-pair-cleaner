import { invoke, isTauri } from "@tauri-apps/api/core";
import { useEffect, useMemo, useRef } from "react";
import { displayPreviewEdge } from "./previewUtils";

interface NativePhotoPreviewProps {
  root: string;
  relativePath: string;
  version: string;
  enabled: boolean;
  onAvailabilityChange: (available: boolean | null) => void;
  onDisplayEdgeChange: (maxEdge: number) => void;
}

const EMBEDDED_QUICK_LOOK_ENABLED = import.meta.env.VITE_ENABLE_EMBEDDED_QUICK_LOOK === "true";

export function NativePhotoPreview({
  root,
  relativePath,
  version,
  enabled,
  onAvailabilityChange,
  onDisplayEdgeChange,
}: NativePhotoPreviewProps) {
  const anchorRef = useRef<HTMLDivElement>(null);
  const lastDisplayEdgeRef = useRef(0);
  const shownPreviewIdRef = useRef<string | null>(null);
  const updateSequenceRef = useRef(0);
  const previewId = useMemo(
    () => JSON.stringify([root, relativePath, version]),
    [relativePath, root, version],
  );

  const quickLookSupported = isTauri()
    && /Macintosh|Mac OS X/i.test(window.navigator.userAgent);
  const nativeEnabled = enabled && quickLookSupported && EMBEDDED_QUICK_LOOK_ENABLED;

  useEffect(() => {
    if (nativeEnabled) return;
    updateSequenceRef.current += 1;
    const shownPreviewId = shownPreviewIdRef.current;
    shownPreviewIdRef.current = null;
    if (shownPreviewId) {
      void invoke("hide_native_photo_preview", { previewId: shownPreviewId })
        .catch(() => undefined);
    }
    onAvailabilityChange(false);
  }, [nativeEnabled, onAvailabilityChange, quickLookSupported]);

  useEffect(() => () => {
    updateSequenceRef.current += 1;
    const shownPreviewId = shownPreviewIdRef.current;
    if (shownPreviewId) {
      void invoke("hide_native_photo_preview", { previewId: shownPreviewId })
        .catch(() => undefined);
    }
  }, []);

  useEffect(() => {
    if (nativeEnabled) onAvailabilityChange(true);

    const anchor = anchorRef.current;
    if (!anchor) return;
    let disposed = false;
    let animationFrame = 0;
    let updatePending = false;

    const update = async () => {
      updatePending = false;
      const bounds = anchor.getBoundingClientRect();
      if (bounds.width <= 0 || bounds.height <= 0) return;
      const maxEdge = displayPreviewEdge(
        bounds.width,
        bounds.height,
        window.devicePixelRatio,
      );
      if (lastDisplayEdgeRef.current !== maxEdge) {
        lastDisplayEdgeRef.current = maxEdge;
        onDisplayEdgeChange(maxEdge);
      }
      if (!nativeEnabled) return;
      const sequence = updateSequenceRef.current + 1;
      updateSequenceRef.current = sequence;
      shownPreviewIdRef.current = previewId;
      try {
        const available = await invoke<boolean>("show_native_photo_preview", {
          root,
          relativePath,
          previewId,
          rect: {
            x: bounds.left,
            y: bounds.top,
            width: bounds.width,
            height: bounds.height,
            viewportWidth: window.innerWidth,
            viewportHeight: window.innerHeight,
          },
        });
        if (!disposed && updateSequenceRef.current === sequence) {
          onAvailabilityChange(available);
        }
      } catch {
        if (!disposed && updateSequenceRef.current === sequence) {
          onAvailabilityChange(false);
        }
      }
    };

    const scheduleUpdate = () => {
      if (updatePending) return;
      updatePending = true;
      animationFrame = window.requestAnimationFrame(() => void update());
    };
    const observer = new ResizeObserver(scheduleUpdate);
    observer.observe(anchor);
    window.addEventListener("resize", scheduleUpdate);
    scheduleUpdate();

    return () => {
      disposed = true;
      observer.disconnect();
      window.removeEventListener("resize", scheduleUpdate);
      window.cancelAnimationFrame(animationFrame);
    };
  }, [nativeEnabled, onAvailabilityChange, onDisplayEdgeChange, previewId, relativePath, root]);

  return <div ref={anchorRef} className="native-preview-anchor" aria-hidden="true" />;
}

import { invoke } from "@tauri-apps/api/core";
import type { PhotoAsset } from "./types";

export interface PreviewRequest {
  root: string;
  relativePath: string;
  maxEdge: number;
  version: string;
}

export interface PreloadProgress {
  total: number;
  completed: number;
  failed: number;
}

interface CacheEntry {
  root: string;
  promise: Promise<string>;
  url: string | null;
  disposed: boolean;
}

export function previewRequestKey(request: PreviewRequest): string {
  return JSON.stringify([
    request.root,
    request.relativePath,
    request.maxEdge,
    request.version,
  ]);
}

export class PreviewUrlCache {
  private readonly entries = new Map<string, CacheEntry>();
  private readonly release: (url: string) => void;

  constructor(release: (url: string) => void = () => undefined) {
    this.release = release;
  }

  getOrLoad(
    request: PreviewRequest,
    loader: () => Promise<string>,
  ): Promise<string> {
    const key = previewRequestKey(request);
    const existing = this.entries.get(key);
    if (existing) return existing.promise;

    const entry: CacheEntry = {
      root: request.root,
      promise: Promise.resolve(""),
      url: null,
      disposed: false,
    };
    entry.promise = loader()
      .then((url) => {
        if (entry.disposed) {
          this.release(url);
          throw new Error("预览缓存已失效");
        }
        entry.url = url;
        return url;
      })
      .catch((error) => {
        if (this.entries.get(key) === entry) this.entries.delete(key);
        throw error;
      });
    this.entries.set(key, entry);
    return entry.promise;
  }

  peek(request: PreviewRequest): string | null {
    return this.entries.get(previewRequestKey(request))?.url ?? null;
  }

  clearRoot(root: string): void {
    for (const [key, entry] of this.entries) {
      if (entry.root !== root) continue;
      entry.disposed = true;
      if (entry.url) this.release(entry.url);
      this.entries.delete(key);
    }
  }
}

export function photoPreviewRequest(
  root: string,
  asset: PhotoAsset,
  maxEdge: number,
  generation = 0,
): PreviewRequest | null {
  if (!asset.previewPath) return null;
  return {
    root,
    relativePath: asset.previewPath,
    maxEdge,
    version: photoPreviewVersion(asset, generation),
  };
}

export function photoPreviewVersion(asset: PhotoAsset, generation = 0): string {
  return `${asset.sizeBytes}:${asset.modifiedMs ?? 0}:${generation}`;
}

export async function preloadPreviewRequests<T>(
  requests: T[],
  load: (request: T) => Promise<unknown>,
  options: {
    concurrency?: number;
    onProgress?: (progress: PreloadProgress) => void;
    signal?: AbortSignal;
  } = {},
): Promise<PreloadProgress> {
  const progress: PreloadProgress = {
    total: requests.length,
    completed: 0,
    failed: 0,
  };
  options.onProgress?.({ ...progress });
  if (requests.length === 0) return progress;

  const workerCount = Math.min(
    requests.length,
    Math.max(1, Math.floor(options.concurrency ?? 3)),
  );
  let cursor = 0;

  async function runWorker() {
    while (!options.signal?.aborted) {
      const requestIndex = cursor;
      cursor += 1;
      if (requestIndex >= requests.length) return;
      try {
        await load(requests[requestIndex]);
      } catch {
        if (!options.signal?.aborted) progress.failed += 1;
      }
      if (options.signal?.aborted) return;
      progress.completed += 1;
      options.onProgress?.({ ...progress });
    }
  }

  await Promise.all(Array.from({ length: workerCount }, () => runWorker()));
  return progress;
}

const previewCache = new PreviewUrlCache((url) => URL.revokeObjectURL(url));

async function decodeImage(url: string): Promise<void> {
  if (typeof Image === "undefined") return;
  const image = new Image();
  image.decoding = "async";
  if (typeof image.decode === "function") {
    image.src = url;
    await image.decode();
    return;
  }
  await new Promise<void>((resolve, reject) => {
    image.onload = () => resolve();
    image.onerror = () => reject(new Error("照片解码失败"));
    image.src = url;
  });
}

export function peekPhotoPreviewUrl(request: PreviewRequest): string | null {
  return previewCache.peek(request);
}

export function loadPhotoPreviewUrl(request: PreviewRequest): Promise<string> {
  return previewCache.getOrLoad(request, async () => {
    const response = await invoke<ArrayBuffer | number[]>("load_photo_thumbnail", {
      root: request.root,
      relativePath: request.relativePath,
      maxEdge: request.maxEdge,
    });
    const bytes = response instanceof ArrayBuffer
      ? new Uint8Array(response)
      : Uint8Array.from(response);
    const objectUrl = URL.createObjectURL(
      new Blob([bytes.buffer as ArrayBuffer], { type: "image/jpeg" }),
    );
    try {
      await decodeImage(objectUrl);
      return objectUrl;
    } catch (error) {
      URL.revokeObjectURL(objectUrl);
      throw error;
    }
  });
}

export function clearPhotoPreviewCache(root: string): void {
  previewCache.clearRoot(root);
}

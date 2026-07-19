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
  consumers: number;
  leased: boolean;
  controller: AbortController;
}

export interface PreviewUrlLease {
  promise: Promise<string>;
  release: () => void;
}

export type PreviewLoadPriority = "foreground" | "background";

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
  private readonly maxEntries: number;

  constructor(
    release: (url: string) => void = () => undefined,
    maxEntries = Number.POSITIVE_INFINITY,
  ) {
    this.release = release;
    this.maxEntries = Math.max(1, Math.floor(maxEntries));
  }

  getOrLoad(
    request: PreviewRequest,
    loader: (signal: AbortSignal) => Promise<string>,
  ): Promise<string> {
    const key = previewRequestKey(request);
    const existing = this.entries.get(key);
    if (existing) {
      this.touch(key, existing);
      return existing.promise;
    }

    const entry: CacheEntry = {
      root: request.root,
      promise: Promise.resolve(""),
      url: null,
      disposed: false,
      consumers: 0,
      leased: false,
      controller: new AbortController(),
    };
    entry.promise = loader(entry.controller.signal)
      .then((url) => {
        if (entry.disposed) {
          this.release(url);
          throw new Error("预览缓存已失效");
        }
        entry.url = url;
        this.touch(key, entry);
        this.trim();
        return url;
      })
      .catch((error) => {
        if (this.entries.get(key) === entry) this.entries.delete(key);
        throw error;
      });
    this.entries.set(key, entry);
    return entry.promise;
  }

  acquire(
    request: PreviewRequest,
    loader: (signal: AbortSignal) => Promise<string>,
  ): PreviewUrlLease {
    const key = previewRequestKey(request);
    const promise = this.getOrLoad(request, loader);
    const entry = this.entries.get(key);
    if (!entry) throw new Error("无法建立预览缓存租约");
    entry.leased = true;
    entry.consumers += 1;
    let released = false;
    return {
      promise,
      release: () => {
        if (released) return;
        released = true;
        const current = this.entries.get(key);
        if (current !== entry) return;
        entry.consumers = Math.max(0, entry.consumers - 1);
        if (entry.consumers === 0 && entry.leased && !entry.url) {
          entry.disposed = true;
          entry.controller.abort();
          this.entries.delete(key);
          return;
        }
        this.trim();
      },
    };
  }

  peek(request: PreviewRequest): string | null {
    return this.entries.get(previewRequestKey(request))?.url ?? null;
  }

  clearRoot(root: string): void {
    for (const [key, entry] of this.entries) {
      if (entry.root !== root) continue;
      entry.disposed = true;
      entry.controller.abort();
      if (entry.url) this.release(entry.url);
      this.entries.delete(key);
    }
  }

  private touch(key: string, entry: CacheEntry): void {
    if (this.entries.get(key) !== entry) return;
    this.entries.delete(key);
    this.entries.set(key, entry);
  }

  private trim(): void {
    if (this.entries.size <= this.maxEntries) return;
    for (const [key, entry] of this.entries) {
      if (this.entries.size <= this.maxEntries) return;
      if (entry.consumers > 0 || !entry.url) continue;
      entry.disposed = true;
      entry.controller.abort();
      this.release(entry.url);
      this.entries.delete(key);
    }
  }
}

export class PreviewLoadScheduler {
  private active = 0;
  private readonly foreground: Array<() => Promise<void>> = [];
  private readonly background: Array<() => Promise<void>> = [];
  private readonly concurrency: number;

  constructor(concurrency = 2) {
    this.concurrency = concurrency;
  }

  schedule<T>(
    task: () => Promise<T>,
    priority: PreviewLoadPriority = "background",
    signal?: AbortSignal,
  ): Promise<T> {
    return new Promise<T>((resolve, reject) => {
      const run = async () => {
        if (signal?.aborted) {
          reject(new Error("预览请求已取消"));
          return;
        }
        try {
          resolve(await task());
        } catch (error) {
          reject(error);
        }
      };
      (priority === "foreground" ? this.foreground : this.background).push(run);
      this.pump();
    });
  }

  private pump(): void {
    const maximum = Math.max(1, Math.floor(this.concurrency));
    while (this.active < maximum) {
      const next = this.foreground.shift() ?? this.background.shift();
      if (!next) return;
      this.active += 1;
      void next().finally(() => {
        this.active -= 1;
        this.pump();
      });
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

const previewCache = new PreviewUrlCache((url) => URL.revokeObjectURL(url), 96);
const previewScheduler = new PreviewLoadScheduler(2);

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

async function fetchPhotoPreviewUrl(request: PreviewRequest): Promise<string> {
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
}

export function loadPhotoPreviewUrl(request: PreviewRequest): Promise<string> {
  return previewCache.getOrLoad(
    request,
    (signal) => previewScheduler.schedule(
      () => fetchPhotoPreviewUrl(request),
      request.maxEdge > 512 ? "foreground" : "background",
      signal,
    ),
  );
}

export function preloadPhotoPreviewUrl(request: PreviewRequest): Promise<string> {
  return previewCache.getOrLoad(
    request,
    (signal) => previewScheduler.schedule(
      () => fetchPhotoPreviewUrl(request),
      "background",
      signal,
    ),
  );
}

export function warmPhotoPreviewCache(
  request: PreviewRequest,
  signal?: AbortSignal,
): Promise<void> {
  return previewScheduler.schedule(
    () => invoke<void>("warm_photo_thumbnail", {
      root: request.root,
      relativePath: request.relativePath,
      maxEdge: request.maxEdge,
    }),
    "background",
    signal,
  );
}

export function acquirePhotoPreviewUrl(
  request: PreviewRequest,
  priority: PreviewLoadPriority = "background",
): PreviewUrlLease {
  return previewCache.acquire(
    request,
    (signal) => previewScheduler.schedule(
      () => fetchPhotoPreviewUrl(request),
      priority,
      signal,
    ),
  );
}

export function clearPhotoPreviewCache(root: string): void {
  previewCache.clearRoot(root);
}

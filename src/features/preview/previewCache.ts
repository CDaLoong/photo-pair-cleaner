import { convertFileSrc, invoke } from "@tauri-apps/api/core";
import type { PhotoAsset } from "./types";

export interface PreviewRequest {
  root: string;
  relativePath: string;
  maxEdge: number;
  version: string;
}

export interface OriginalPhotoRequest {
  root: string;
  relativePath: string;
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
  costBytes: number;
  controller: AbortController;
}

interface PreviewCacheLimits {
  maxEntries: number;
  maxCostBytes: number;
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

export function originalPhotoRequestKey(request: OriginalPhotoRequest): string {
  return JSON.stringify([request.root, request.relativePath, request.version, "original"]);
}

export class PreviewUrlCache {
  private readonly entries = new Map<string, CacheEntry>();
  private readonly release: (url: string) => void;
  private readonly maxEntries: number;
  private readonly maxCostBytes: number;
  private retainedCostBytes = 0;

  constructor(
    release: (url: string) => void = () => undefined,
    limits: number | Partial<PreviewCacheLimits> = Number.POSITIVE_INFINITY,
  ) {
    this.release = release;
    const normalized = typeof limits === "number" ? { maxEntries: limits } : limits;
    this.maxEntries = Math.max(
      1,
      Math.floor(normalized.maxEntries ?? Number.POSITIVE_INFINITY),
    );
    this.maxCostBytes = Math.max(
      1,
      Math.floor(normalized.maxCostBytes ?? Number.POSITIVE_INFINITY),
    );
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
      // The cache retains compressed JPEG blobs, not RGBA pixel buffers.
      costBytes: Math.max(1, request.maxEdge * request.maxEdge),
      controller: new AbortController(),
    };
    entry.promise = loader(entry.controller.signal)
      .then((url) => {
        if (entry.disposed) {
          this.release(url);
          throw new Error("预览缓存已失效");
        }
        entry.url = url;
        this.retainedCostBytes += entry.costBytes;
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
      if (entry.url) this.retainedCostBytes = Math.max(0, this.retainedCostBytes - entry.costBytes);
      this.entries.delete(key);
    }
  }

  private touch(key: string, entry: CacheEntry): void {
    if (this.entries.get(key) !== entry) return;
    this.entries.delete(key);
    this.entries.set(key, entry);
  }

  private trim(): void {
    if (this.entries.size <= this.maxEntries && this.retainedCostBytes <= this.maxCostBytes) {
      return;
    }
    for (const [key, entry] of this.entries) {
      if (this.entries.size <= this.maxEntries && this.retainedCostBytes <= this.maxCostBytes) {
        return;
      }
      if (entry.consumers > 0 || !entry.url) continue;
      entry.disposed = true;
      entry.controller.abort();
      this.release(entry.url);
      this.retainedCostBytes = Math.max(0, this.retainedCostBytes - entry.costBytes);
      this.entries.delete(key);
    }
  }

  estimatedCostBytes(): number {
    return this.retainedCostBytes;
  }
}

export class PreviewLoadScheduler {
  private active = 0;
  private activeBackground = 0;
  private readonly foreground: ScheduledPreviewTask[] = [];
  private readonly background: ScheduledPreviewTask[] = [];
  private readonly concurrency: number;
  private readonly backgroundConcurrency: number;

  constructor(concurrency = 2, backgroundConcurrency = 1) {
    this.concurrency = Math.max(1, Math.floor(concurrency));
    this.backgroundConcurrency = Math.max(
      1,
      Math.min(this.concurrency, Math.floor(backgroundConcurrency)),
    );
  }

  schedule<T>(
    taskInput: () => Promise<T>,
    priority: PreviewLoadPriority = "background",
    signal?: AbortSignal,
  ): Promise<T> {
    return new Promise<T>((resolve, reject) => {
      const scheduled: ScheduledPreviewTask = {
        priority,
        signal,
        cancelled: false,
        started: false,
        abort: () => undefined,
        run: async () => {
          scheduled.started = true;
          signal?.removeEventListener("abort", scheduled.abort);
          if (scheduled.cancelled || signal?.aborted) {
            reject(new Error("预览请求已取消"));
            return;
          }
          try {
            resolve(await taskInput());
          } catch (error) {
            reject(error);
          }
        },
      };
      scheduled.abort = () => {
        if (scheduled.started || scheduled.cancelled) return;
        scheduled.cancelled = true;
        reject(new Error("预览请求已取消"));
        this.pump();
      };
      if (signal?.aborted) {
        scheduled.abort();
        return;
      }
      signal?.addEventListener("abort", scheduled.abort, { once: true });
      (priority === "foreground" ? this.foreground : this.background).push(scheduled);
      this.pump();
    });
  }

  private next(queue: ScheduledPreviewTask[]): ScheduledPreviewTask | null {
    while (queue.length > 0) {
      const task = queue.shift()!;
      if (!task.cancelled) return task;
    }
    return null;
  }

  private start(task: ScheduledPreviewTask) {
    this.active += 1;
    if (task.priority === "background") this.activeBackground += 1;
    void task.run().finally(() => {
      this.active -= 1;
      if (task.priority === "background") this.activeBackground -= 1;
      this.pump();
    });
  }

  private pump(): void {
    while (this.active < this.concurrency) {
      const foreground = this.next(this.foreground);
      if (foreground) {
        this.start(foreground);
        continue;
      }
      if (this.activeBackground >= this.backgroundConcurrency) return;
      const background = this.next(this.background);
      if (!background) return;
      this.start(background);
    }
  }
}

interface OriginalCacheEntry {
  root: string;
  promise: Promise<string>;
  url: string | null;
  disposed: boolean;
  consumers: number;
  costBytes: number;
  controller: AbortController;
}

interface LoadedOriginalPhoto {
  url: string;
}

class OriginalPhotoUrlCache {
  private readonly entries = new Map<string, OriginalCacheEntry>();
  private retainedCostBytes = 0;

  constructor(
    private readonly maxEntries: number,
    private readonly maxCostBytes: number,
  ) {}

  acquire(
    request: OriginalPhotoRequest,
    loader: (signal: AbortSignal) => Promise<LoadedOriginalPhoto>,
  ): PreviewUrlLease {
    const key = originalPhotoRequestKey(request);
    let entry = this.entries.get(key);
    if (!entry) {
      entry = {
        root: request.root,
        promise: Promise.resolve(""),
        url: null,
        disposed: false,
        consumers: 0,
        costBytes: 0,
        controller: new AbortController(),
      };
      const nextEntry = entry;
      nextEntry.promise = loader(nextEntry.controller.signal)
        .then(({ url }) => {
          if (nextEntry.disposed) {
            throw new Error("原图缓存已失效");
          }
          nextEntry.url = url;
          nextEntry.costBytes = 1;
          this.retainedCostBytes += nextEntry.costBytes;
          this.touch(key, nextEntry);
          this.trim();
          return url;
        })
        .catch((error) => {
          if (this.entries.get(key) === nextEntry) this.entries.delete(key);
          throw error;
        });
      this.entries.set(key, nextEntry);
    } else {
      this.touch(key, entry);
    }

    entry.consumers += 1;
    let released = false;
    return {
      promise: entry.promise,
      release: () => {
        if (released) return;
        released = true;
        const current = this.entries.get(key);
        if (current !== entry) return;
        entry.consumers = Math.max(0, entry.consumers - 1);
        if (entry.consumers === 0 && !entry.url) {
          entry.disposed = true;
          entry.controller.abort();
          this.entries.delete(key);
          return;
        }
        this.trim();
      },
    };
  }

  peek(request: OriginalPhotoRequest): string | null {
    const key = originalPhotoRequestKey(request);
    const entry = this.entries.get(key);
    if (!entry?.url) return null;
    this.touch(key, entry);
    return entry.url;
  }

  clearRoot(root: string): void {
    for (const [key, entry] of this.entries) {
      if (entry.root !== root) continue;
      entry.disposed = true;
      entry.controller.abort();
      if (entry.url) this.retainedCostBytes = Math.max(0, this.retainedCostBytes - entry.costBytes);
      this.entries.delete(key);
    }
  }

  private touch(key: string, entry: OriginalCacheEntry): void {
    if (this.entries.get(key) !== entry) return;
    this.entries.delete(key);
    this.entries.set(key, entry);
  }

  private trim(): void {
    if (this.entries.size <= this.maxEntries && this.retainedCostBytes <= this.maxCostBytes) {
      return;
    }
    for (const [key, entry] of this.entries) {
      if (this.entries.size <= this.maxEntries && this.retainedCostBytes <= this.maxCostBytes) {
        return;
      }
      if (entry.consumers > 0 || !entry.url) continue;
      entry.disposed = true;
      entry.controller.abort();
      this.retainedCostBytes = Math.max(0, this.retainedCostBytes - entry.costBytes);
      this.entries.delete(key);
    }
  }
}

interface ScheduledPreviewTask {
  priority: PreviewLoadPriority;
  signal?: AbortSignal;
  cancelled: boolean;
  started: boolean;
  abort: () => void;
  run: () => Promise<void>;
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

export function photoOriginalRequest(
  root: string,
  asset: PhotoAsset,
  generation = 0,
): OriginalPhotoRequest | null {
  if (!asset.previewPath) return null;
  return {
    root,
    relativePath: asset.previewPath,
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

const previewCache = new PreviewUrlCache((url) => URL.revokeObjectURL(url), {
  maxEntries: 128,
  maxCostBytes: 512 * 1024 * 1024,
});
const originalPhotoCache = new OriginalPhotoUrlCache(100_000, Number.POSITIVE_INFINITY);
const previewScheduler = new PreviewLoadScheduler(3, 2);
const PREVIEW_LOAD_TIMEOUT_MS = 12_000;
const ORIGINAL_LOAD_TIMEOUT_MS = 60_000;

function withTimeout<T>(promise: Promise<T>, timeoutMs: number): Promise<T> {
  let timeoutId: ReturnType<typeof setTimeout> | undefined;
  const timeout = new Promise<never>((_, reject) => {
    timeoutId = setTimeout(
      () => reject(new Error("照片预览加载超时")),
      timeoutMs,
    );
  });
  return Promise.race([promise, timeout]).finally(() => {
    if (timeoutId !== undefined) clearTimeout(timeoutId);
  });
}

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

async function fetchPhotoPreviewUrl(
  request: PreviewRequest,
  signal?: AbortSignal,
): Promise<string> {
  if (signal?.aborted) throw new Error("预览请求已取消");
  const response = await withTimeout(
    invoke<ArrayBuffer | number[]>("load_photo_thumbnail", {
      root: request.root,
      relativePath: request.relativePath,
      maxEdge: request.maxEdge,
    }),
    PREVIEW_LOAD_TIMEOUT_MS,
  );
  if (signal?.aborted) throw new Error("预览请求已取消");
  const bytes = response instanceof ArrayBuffer
    ? new Uint8Array(response)
    : Uint8Array.from(response);
  const objectUrl = URL.createObjectURL(
    new Blob([bytes.buffer as ArrayBuffer], { type: "image/jpeg" }),
  );
  try {
    await decodeImage(objectUrl);
    if (signal?.aborted) throw new Error("预览请求已取消");
    return objectUrl;
  } catch (error) {
    URL.revokeObjectURL(objectUrl);
    throw error;
  }
}

async function fetchPhotoOriginalUrl(
  request: OriginalPhotoRequest,
  signal?: AbortSignal,
): Promise<LoadedOriginalPhoto> {
  if (signal?.aborted) throw new Error("原图请求已取消");
  const sourcePath = await withTimeout(
    invoke<string>("prepare_photo_original", {
      root: request.root,
      relativePath: request.relativePath,
    }),
    ORIGINAL_LOAD_TIMEOUT_MS,
  );
  if (signal?.aborted) throw new Error("原图请求已取消");
  const assetUrl = convertFileSrc(sourcePath);
  const separator = assetUrl.includes("?") ? "&" : "?";
  const sourceUrl = (
    `${assetUrl}${separator}framepairVersion=${encodeURIComponent(request.version)}`
  );
  await withTimeout(decodeImage(sourceUrl), ORIGINAL_LOAD_TIMEOUT_MS);
  if (signal?.aborted) throw new Error("原图请求已取消");
  return { url: sourceUrl };
}

export function peekPhotoOriginalUrl(request: OriginalPhotoRequest): string | null {
  return originalPhotoCache.peek(request);
}

export function acquirePhotoOriginalUrl(
  request: OriginalPhotoRequest,
  priority: PreviewLoadPriority = "background",
): PreviewUrlLease {
  return originalPhotoCache.acquire(
    request,
    (signal) => previewScheduler.schedule(
      () => fetchPhotoOriginalUrl(request, signal),
      priority,
      signal,
    ),
  );
}

export async function preloadPhotoOriginalUrl(
  request: OriginalPhotoRequest,
  externalSignal?: AbortSignal,
): Promise<string> {
  if (externalSignal?.aborted) throw new Error("原图请求已取消");
  const lease = acquirePhotoOriginalUrl(request, "background");
  const abort = () => lease.release();
  externalSignal?.addEventListener("abort", abort, { once: true });
  try {
    const url = await lease.promise;
    if (externalSignal?.aborted) throw new Error("原图请求已取消");
    return url;
  } finally {
    externalSignal?.removeEventListener("abort", abort);
    lease.release();
  }
}

export function loadPhotoPreviewUrl(request: PreviewRequest): Promise<string> {
  return previewCache.getOrLoad(
    request,
    (signal) => previewScheduler.schedule(
      () => fetchPhotoPreviewUrl(request, signal),
      request.maxEdge > 512 ? "foreground" : "background",
      signal,
    ),
  );
}

export async function preloadPhotoPreviewUrl(
  request: PreviewRequest,
  externalSignal?: AbortSignal,
): Promise<string> {
  if (externalSignal?.aborted) throw new Error("预览请求已取消");
  const lease = previewCache.acquire(
    request,
    (signal) => previewScheduler.schedule(
      () => fetchPhotoPreviewUrl(request, signal),
      "background",
      signal,
    ),
  );
  const abort = () => lease.release();
  externalSignal?.addEventListener("abort", abort, { once: true });
  try {
    const url = await lease.promise;
    if (externalSignal?.aborted) throw new Error("预览请求已取消");
    return url;
  } finally {
    externalSignal?.removeEventListener("abort", abort);
    lease.release();
  }
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
      () => fetchPhotoPreviewUrl(request, signal),
      priority,
      signal,
    ),
  );
}

export function clearPhotoPreviewCache(root: string): void {
  previewCache.clearRoot(root);
  originalPhotoCache.clearRoot(root);
}

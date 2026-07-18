import { invoke } from "@tauri-apps/api/core";
import type { WatermarkRenderRequest } from "./types";

export interface WatermarkPreviewHeader {
  width: number;
  height: number;
  warnings: string[];
}

export interface WatermarkPreviewResult extends WatermarkPreviewHeader {
  url: string;
}

export interface WatermarkPreviewDescriptor {
  key: string;
  photoId: string;
  root: string;
  templateId: string;
}

export interface WatermarkPreviewToken {
  photoId: string;
  requestHash: string;
  generation: number;
}

interface CacheEntry {
  descriptor: WatermarkPreviewDescriptor;
  promise: Promise<WatermarkPreviewResult>;
  result: WatermarkPreviewResult | null;
  disposed: boolean;
}

function canonicalValue(value: unknown, ancestors: Set<object>): unknown {
  if (value === null || typeof value === "string" || typeof value === "boolean") return value;
  if (typeof value === "number") {
    if (!Number.isFinite(value)) throw new Error("水印预览请求包含无效数字");
    return Object.is(value, -0) ? 0 : value;
  }
  if (Array.isArray(value)) {
    if (ancestors.has(value)) throw new Error("水印预览请求不能循环引用");
    ancestors.add(value);
    const result = value.map((item) => (
      item === undefined || typeof item === "function" || typeof item === "symbol"
        ? null
        : canonicalValue(item, ancestors)
    ));
    ancestors.delete(value);
    return result;
  }
  if (typeof value === "object") {
    const object = value as Record<string, unknown>;
    if (ancestors.has(object)) throw new Error("水印预览请求不能循环引用");
    ancestors.add(object);
    const result: Record<string, unknown> = {};
    for (const key of Object.keys(object).sort()) {
      const item = object[key];
      if (item === undefined || typeof item === "function" || typeof item === "symbol") continue;
      result[key] = canonicalValue(item, ancestors);
    }
    ancestors.delete(object);
    return result;
  }
  throw new Error("水印预览请求包含不支持的值");
}

export function stableWatermarkStringify(value: unknown): string {
  return JSON.stringify(canonicalValue(value, new Set()));
}

function fnv1a64(value: string): string {
  let hash = 0xcbf29ce484222325n;
  for (let index = 0; index < value.length; index += 1) {
    hash ^= BigInt(value.charCodeAt(index));
    hash = BigInt.asUintN(64, hash * 0x100000001b3n);
  }
  return hash.toString(16).padStart(16, "0");
}

export function watermarkPreviewRequestKey(request: unknown, maxEdge: number): string {
  if (!Number.isInteger(maxEdge) || maxEdge < 256 || maxEdge > 2400) {
    throw new Error("水印预览长边必须在 256 到 2400 像素之间");
  }
  return `${maxEdge}:${fnv1a64(stableWatermarkStringify(request))}`;
}

export class WatermarkPreviewCache {
  private readonly entries = new Map<string, CacheEntry>();
  private readonly generations = new Map<string, WatermarkPreviewToken>();
  private readonly ownedUrls = new Set<string>();
  private readonly release: (url: string) => void;

  constructor(release: (url: string) => void = () => undefined) {
    this.release = release;
  }

  begin(photoId: string, requestHash: string): WatermarkPreviewToken {
    const token = {
      photoId,
      requestHash,
      generation: (this.generations.get(photoId)?.generation ?? 0) + 1,
    };
    this.generations.set(photoId, token);
    return token;
  }

  isCurrent(token: WatermarkPreviewToken): boolean {
    const current = this.generations.get(token.photoId);
    return current?.generation === token.generation && current.requestHash === token.requestHash;
  }

  accept(token: WatermarkPreviewToken, url: string): boolean {
    if (this.isCurrent(token)) return true;
    if (!this.ownedUrls.has(url)) this.release(url);
    return false;
  }

  getOrLoad(
    descriptor: WatermarkPreviewDescriptor,
    loader: () => Promise<WatermarkPreviewResult>,
  ): Promise<WatermarkPreviewResult> {
    const existing = this.entries.get(descriptor.key);
    if (existing) return existing.promise;
    this.removeWhere((entry) => (
      entry.descriptor.photoId === descriptor.photoId
      && entry.descriptor.key !== descriptor.key
    ));

    const entry: CacheEntry = {
      descriptor,
      promise: Promise.resolve({ url: "", width: 0, height: 0, warnings: [] }),
      result: null,
      disposed: false,
    };
    entry.promise = loader()
      .then((result) => {
        if (entry.disposed) {
          this.release(result.url);
          throw new Error("水印预览缓存已失效");
        }
        entry.result = result;
        this.ownedUrls.add(result.url);
        return result;
      })
      .catch((error) => {
        if (this.entries.get(descriptor.key) === entry) this.entries.delete(descriptor.key);
        throw error;
      });
    this.entries.set(descriptor.key, entry);
    return entry.promise;
  }

  peek(key: string): WatermarkPreviewResult | null {
    return this.entries.get(key)?.result ?? null;
  }

  invalidateRoot(root: string): void {
    this.removeWhere((entry) => entry.descriptor.root === root);
  }

  invalidateTemplate(templateId: string): void {
    this.removeWhere((entry) => entry.descriptor.templateId === templateId);
  }

  retainPhotos(photoIds: ReadonlySet<string>): void {
    this.removeWhere((entry) => !photoIds.has(entry.descriptor.photoId));
  }

  clear(): void {
    this.removeWhere(() => true);
    this.generations.clear();
  }

  private removeWhere(predicate: (entry: CacheEntry) => boolean): void {
    for (const [key, entry] of this.entries) {
      if (!predicate(entry)) continue;
      entry.disposed = true;
      if (entry.result) {
        this.ownedUrls.delete(entry.result.url);
        this.release(entry.result.url);
      }
      this.entries.delete(key);
    }
  }
}

function responseBytes(response: ArrayBuffer | Uint8Array | number[]): Uint8Array {
  if (response instanceof Uint8Array) return response;
  if (response instanceof ArrayBuffer) return new Uint8Array(response);
  return Uint8Array.from(response);
}

export function decodeWatermarkPreviewEnvelope(
  response: ArrayBuffer | Uint8Array | number[],
): { header: WatermarkPreviewHeader; png: Uint8Array } {
  const bytes = responseBytes(response);
  if (bytes.byteLength < 4) throw new Error("水印预览响应不完整");
  const headerLength = new DataView(bytes.buffer, bytes.byteOffset, 4).getUint32(0, false);
  const headerEnd = 4 + headerLength;
  if (headerLength === 0 || headerEnd > bytes.byteLength) {
    throw new Error("水印预览信息长度无效");
  }
  let parsed: unknown;
  try {
    parsed = JSON.parse(new TextDecoder().decode(bytes.subarray(4, headerEnd)));
  } catch {
    throw new Error("水印预览信息无法解析");
  }
  const header = parsed as Partial<WatermarkPreviewHeader>;
  if (!Number.isInteger(header.width) || (header.width ?? 0) <= 0
    || !Number.isInteger(header.height) || (header.height ?? 0) <= 0
    || !Array.isArray(header.warnings)
    || !header.warnings.every((warning) => typeof warning === "string")) {
    throw new Error("水印预览信息格式无效");
  }
  const png = bytes.slice(headerEnd);
  if (png.byteLength === 0) throw new Error("水印预览图片为空");
  return {
    header: {
      width: header.width as number,
      height: header.height as number,
      warnings: header.warnings,
    },
    png,
  };
}

async function decodeImage(url: string): Promise<void> {
  if (typeof Image === "undefined") return;
  const image = new Image();
  image.decoding = "async";
  image.src = url;
  if (typeof image.decode === "function") {
    await image.decode();
    return;
  }
  await new Promise<void>((resolve, reject) => {
    image.onload = () => resolve();
    image.onerror = () => reject(new Error("水印预览图片解码失败"));
  });
}

export async function loadWatermarkPreview(
  request: WatermarkRenderRequest,
  maxEdge: number,
): Promise<WatermarkPreviewResult> {
  const response = await invoke<ArrayBuffer | number[]>("render_watermark_preview", {
    photo: request.source,
    request,
    maxEdge,
  });
  const { header, png } = decodeWatermarkPreviewEnvelope(response);
  const pngBuffer = png.slice().buffer as ArrayBuffer;
  const url = URL.createObjectURL(new Blob([pngBuffer], { type: "image/png" }));
  try {
    await decodeImage(url);
    return { ...header, url };
  } catch (error) {
    URL.revokeObjectURL(url);
    throw error;
  }
}

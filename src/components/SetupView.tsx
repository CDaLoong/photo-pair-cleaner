import { isTauri } from "@tauri-apps/api/core";
import {
  ArrowRight,
  FileImage,
  FileText,
  FolderInput,
  FolderOpen,
  HardDrive,
  ScanSearch,
  Settings2,
  ShieldCheck,
  Star,
} from "lucide-react";
import { useEffect, useRef, useState } from "react";
import type { DirectoryKind, ReferenceSourceType, ScanMode } from "../types";
import { directoryDropTargetAtPoint } from "../utils";

interface SetupViewProps {
  referenceRoot: string;
  rawRoot: string;
  referenceSourceType: ReferenceSourceType;
  manifestPath: string;
  ratingRoot: string;
  minimumRating: number;
  includeSidecars: boolean;
  caseSensitive: boolean;
  scanMode: ScanMode;
  busy: boolean;
  onChooseDirectory: (kind: DirectoryKind) => void;
  onChooseManifest: () => void;
  onDropDirectories: (kind: DirectoryKind, paths: string[]) => void;
  onDropError: (message: string) => void;
  onIncludeSidecarsChange: (checked: boolean) => void;
  onCaseSensitiveChange: (checked: boolean) => void;
  onScanModeChange: (mode: ScanMode) => void;
  onReferenceSourceTypeChange: (source: ReferenceSourceType) => void;
  onMinimumRatingChange: (rating: number) => void;
  onUseRawRootForRatings: () => void;
  onScan: () => void;
}

interface DirectoryPickerProps {
  kind: DirectoryKind;
  path: string;
  busy: boolean;
  isDropTarget: boolean;
  buttonRef: React.RefObject<HTMLButtonElement | null>;
  onChoose: (kind: DirectoryKind) => void;
  label?: string;
  description?: string;
  emptyText?: string;
  filePicker?: boolean;
}

function DirectoryPicker({
  kind,
  path,
  busy,
  isDropTarget,
  buttonRef,
  onChoose,
  label: labelOverride,
  description: descriptionOverride,
  emptyText,
  filePicker = false,
}: DirectoryPickerProps) {
  const isReference = kind === "reference";
  const Icon = filePicker ? FileText : isReference ? FileImage : HardDrive;
  const TrailingIcon = filePicker ? FileText : FolderOpen;
  const label = labelOverride ?? (isReference ? "JPG 参考目录" : "RAW 源目录");
  const description = descriptionOverride ?? (isReference
    ? "只读，用这些 JPG 决定哪些 RAW 需要保留"
    : "仅此目录中的未匹配 RAW 会进入清理列表");

  return (
    <button
      ref={buttonRef}
      className={isDropTarget ? "directory-picker is-drop-target" : "directory-picker"}
      type="button"
      onClick={() => onChoose(kind)}
      disabled={busy}
      title={path || `选择${label}`}
    >
      <span className="directory-step" aria-hidden="true">{isReference ? "1" : "2"}</span>
      <span className="directory-icon"><Icon aria-hidden="true" size={22} /></span>
      <span className="directory-copy">
        <strong>{label}</strong>
        <span>{description}</span>
        <span className={path ? "directory-path" : "directory-path is-empty"}>
          {path || emptyText || (filePicker ? "点击选择 UTF-8 文本清单" : "拖拽目录到这里，或点击选择")}
        </span>
      </span>
      <TrailingIcon aria-hidden="true" size={20} />
      {isDropTarget && (
        <span className="directory-drop-overlay" aria-live="polite">
          <FolderInput aria-hidden="true" size={24} />
          <strong>松开以添加{label}</strong>
          <small>{path ? "将替换当前目录" : "仅接受单个文件夹"}</small>
        </span>
      )}
    </button>
  );
}

export function SetupView({
  referenceRoot,
  rawRoot,
  referenceSourceType,
  manifestPath,
  ratingRoot,
  minimumRating,
  includeSidecars,
  caseSensitive,
  scanMode,
  busy,
  onChooseDirectory,
  onChooseManifest,
  onDropDirectories,
  onDropError,
  onIncludeSidecarsChange,
  onCaseSensitiveChange,
  onScanModeChange,
  onReferenceSourceTypeChange,
  onMinimumRatingChange,
  onUseRawRootForRatings,
  onScan,
}: SetupViewProps) {
  const activeReferencePath = referenceSourceType === "directory"
    ? referenceRoot
    : referenceSourceType === "manifest"
      ? manifestPath
      : ratingRoot;
  const ready = Boolean(activeReferencePath && rawRoot);
  const referenceButton = useRef<HTMLButtonElement>(null);
  const rawButton = useRef<HTMLButtonElement>(null);
  const busyRef = useRef(busy);
  const dropDirectoriesRef = useRef(onDropDirectories);
  const dropErrorRef = useRef(onDropError);
  const referenceSourceTypeRef = useRef(referenceSourceType);
  const [dropTarget, setDropTarget] = useState<DirectoryKind | null>(null);

  useEffect(() => {
    busyRef.current = busy;
    dropDirectoriesRef.current = onDropDirectories;
    dropErrorRef.current = onDropError;
    referenceSourceTypeRef.current = referenceSourceType;
    if (busy || referenceSourceType === "manifest") setDropTarget(null);
  }, [busy, onDropDirectories, onDropError, referenceSourceType]);

  useEffect(() => {
    if (!isTauri()) return;
    let disposed = false;
    let unlisten: (() => void) | undefined;

    async function listenForDrops() {
      const [{ getCurrentWebview }, { getCurrentWindow }] = await Promise.all([
        import("@tauri-apps/api/webview"),
        import("@tauri-apps/api/window"),
      ]);
      const scaleFactor = await getCurrentWindow().scaleFactor();
      const stopListening = await getCurrentWebview().onDragDropEvent((event) => {
        if (disposed) return;
        if (event.payload.type === "leave") {
          setDropTarget(null);
          return;
        }
        if (busyRef.current) return;

        const position = event.payload.position.toLogical(scaleFactor);
        const elements = [
          { kind: "reference" as const, element: referenceButton.current },
          { kind: "raw" as const, element: rawButton.current },
        ].filter(({ kind }) => kind !== "reference" || referenceSourceTypeRef.current !== "manifest");
        const target = directoryDropTargetAtPoint(
          position.x,
          position.y,
          elements.flatMap(({ kind, element }) => {
            if (!element) return [];
            const rect = element.getBoundingClientRect();
            return [{ kind, left: rect.left, top: rect.top, right: rect.right, bottom: rect.bottom }];
          }),
        );
        setDropTarget(target);

        if (event.payload.type === "drop") {
          setDropTarget(null);
          if (target) dropDirectoriesRef.current(target, event.payload.paths);
        }
      });
      if (disposed) stopListening();
      else unlisten = stopListening;
    }

    void listenForDrops().catch(() => {
      if (!disposed) dropErrorRef.current("无法启用目录拖放，请使用点击选择");
    });

    return () => {
      disposed = true;
      unlisten?.();
    };
  }, []);

  return (
    <main className="setup-view">
      <section className="setup-heading" aria-labelledby="setup-title">
        <div>
          <h1 id="setup-title">选择目录并进行只读扫描</h1>
          <p>{scanMode === "cleanupRaw" ? "以保留的 JPG 为准，找出未配对 RAW。" : "反向检查保留的 JPG 是否仍有对应 RAW。"}扫描阶段不会修改任何文件。</p>
        </div>
        <div className="safety-assurance">
          <ShieldCheck aria-hidden="true" size={18} />
          <span><strong>扫描仅比较文件</strong><small>只有最终确认后才会移动 RAW</small></span>
        </div>
      </section>

      <section className="directory-flow" aria-label="目录选择">
        <DirectoryPicker
          kind="reference"
          path={activeReferencePath}
          busy={busy}
          isDropTarget={dropTarget === "reference"}
          buttonRef={referenceButton}
          onChoose={(kind) => referenceSourceType === "manifest" ? onChooseManifest() : onChooseDirectory(kind)}
          label={referenceSourceType === "directory" ? "JPG 参考目录" : referenceSourceType === "manifest" ? "保留文件清单" : "XMP 评分目录"}
          description={referenceSourceType === "directory" ? "只读，用这些 JPG 决定哪些 RAW 需要保留" : referenceSourceType === "manifest" ? "每行一个 JPG/JPEG 相对路径，支持 # 注释" : `只保留达到 ${minimumRating} 星的 XMP 对应 RAW`}
          filePicker={referenceSourceType === "manifest"}
        />
        <ArrowRight className="directory-arrow" aria-hidden="true" size={22} />
        <DirectoryPicker
          kind="raw"
          path={rawRoot}
          busy={busy}
          isDropTarget={dropTarget === "raw"}
          buttonRef={rawButton}
          onChoose={onChooseDirectory}
        />
      </section>

      <section className="scan-settings" aria-labelledby="settings-title">
        <div className="settings-heading">
          <Settings2 aria-hidden="true" size={18} />
          <div>
            <h2 id="settings-title">扫描设置</h2>
            <p>支持 Nikon、Canon、Sony、Fujifilm、DNG 等主流 RAW，匹配键为相对路径和文件名。</p>
          </div>
        </div>
        <div className="settings-controls">
          <div className="reference-source-control" role="group" aria-label="保留依据">
            {([
              ["directory", "JPG 目录", "以保留下来的 JPG 为准"],
              ["manifest", "文件清单", "读取 UTF-8 相对路径列表"],
              ["xmpRating", "XMP 星级", "读取 Lightroom/Bridge 写入的评分"],
            ] as const).map(([value, label, detail]) => (
              <button key={value} type="button" aria-pressed={referenceSourceType === value} onClick={() => onReferenceSourceTypeChange(value)} disabled={busy}>
                <strong>{label}</strong><small>{detail}</small>
              </button>
            ))}
          </div>
          <div className="scan-mode-control" role="group" aria-label="扫描方向">
            <button type="button" aria-pressed={scanMode === "cleanupRaw"} onClick={() => onScanModeChange("cleanupRaw")} disabled={busy}>
              <strong>清理无 JPG 的 RAW</strong><small>生成可执行清理计划</small>
            </button>
            <button type="button" aria-pressed={scanMode === "auditReference"} onClick={() => onScanModeChange("auditReference")} disabled={busy || referenceSourceType !== "directory"} title={referenceSourceType === "directory" ? undefined : "反向审计只支持 JPG 目录参考源"}>
              <strong>检查无 RAW 的 JPG</strong><small>只读审计并可导出清单</small>
            </button>
          </div>
          {referenceSourceType === "xmpRating" && <div className="rating-threshold-row">
            <span><Star aria-hidden="true" size={17} /><span><strong>最低保留星级</strong><small>读取磁盘 XMP 中的 Rating，不修改评分</small></span></span>
            <div>
              <input type="number" min="1" max="5" step="1" value={minimumRating} onChange={(event) => onMinimumRatingChange(Number(event.target.value))} disabled={busy} aria-label="最低保留星级" />
              <button className="secondary-command" type="button" onClick={onUseRawRootForRatings} disabled={busy || ratingRoot === rawRoot}>使用 RAW 目录</button>
            </div>
          </div>}
          {scanMode === "cleanupRaw" && <label className="toggle-row">
            <span><strong>包含 XMP</strong><small>跟随对应的未配对 RAW 一起处理</small></span>
            <input
              type="checkbox"
              checked={includeSidecars}
              onChange={(event) => onIncludeSidecarsChange(event.target.checked)}
              disabled={busy}
            />
          </label>}
          <label className="toggle-row">
            <span><strong>区分大小写</strong><small>关闭时 DSC_001 与 dsc_001 视为同名</small></span>
            <input
              type="checkbox"
              checked={caseSensitive}
              onChange={(event) => onCaseSensitiveChange(event.target.checked)}
              disabled={busy}
            />
          </label>
        </div>
      </section>

      <div className="setup-command-row">
        <p>{ready ? (scanMode === "cleanupRaw" ? "参考源与 RAW 目录已就绪，可以生成清理预览。" : "目录已就绪，可以生成只读审计结果。") : "请先选择保留依据和 RAW 源目录。"}</p>
        <button
          className="primary-command primary-command-large"
          type="button"
          onClick={onScan}
          disabled={busy || !ready}
        >
          <ScanSearch aria-hidden="true" size={19} />
          {busy ? "正在扫描" : scanMode === "cleanupRaw" ? "开始只读扫描" : "开始反向检查"}
        </button>
      </div>
    </main>
  );
}

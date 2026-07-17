import { isTauri } from "@tauri-apps/api/core";
import {
  ArrowRight,
  FileImage,
  FolderInput,
  FolderOpen,
  HardDrive,
  ScanSearch,
  Settings2,
  ShieldCheck,
} from "lucide-react";
import { useEffect, useRef, useState } from "react";
import type { DirectoryKind } from "../types";
import { directoryDropTargetAtPoint } from "../utils";

interface SetupViewProps {
  referenceRoot: string;
  rawRoot: string;
  includeSidecars: boolean;
  caseSensitive: boolean;
  busy: boolean;
  onChooseDirectory: (kind: DirectoryKind) => void;
  onDropDirectories: (kind: DirectoryKind, paths: string[]) => void;
  onDropError: (message: string) => void;
  onIncludeSidecarsChange: (checked: boolean) => void;
  onCaseSensitiveChange: (checked: boolean) => void;
  onScan: () => void;
}

interface DirectoryPickerProps {
  kind: DirectoryKind;
  path: string;
  busy: boolean;
  isDropTarget: boolean;
  buttonRef: React.RefObject<HTMLButtonElement | null>;
  onChoose: (kind: DirectoryKind) => void;
}

function DirectoryPicker({
  kind,
  path,
  busy,
  isDropTarget,
  buttonRef,
  onChoose,
}: DirectoryPickerProps) {
  const isReference = kind === "reference";
  const Icon = isReference ? FileImage : HardDrive;
  const label = isReference ? "JPG 参考目录" : "RAW 源目录";
  const description = isReference
    ? "只读，用这些 JPG 决定哪些 RAW 需要保留"
    : "仅此目录中的未匹配 RAW 会进入清理列表";

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
          {path || "拖拽目录到这里，或点击选择"}
        </span>
      </span>
      <FolderOpen aria-hidden="true" size={20} />
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
  includeSidecars,
  caseSensitive,
  busy,
  onChooseDirectory,
  onDropDirectories,
  onDropError,
  onIncludeSidecarsChange,
  onCaseSensitiveChange,
  onScan,
}: SetupViewProps) {
  const ready = Boolean(referenceRoot && rawRoot);
  const referenceButton = useRef<HTMLButtonElement>(null);
  const rawButton = useRef<HTMLButtonElement>(null);
  const busyRef = useRef(busy);
  const dropDirectoriesRef = useRef(onDropDirectories);
  const dropErrorRef = useRef(onDropError);
  const [dropTarget, setDropTarget] = useState<DirectoryKind | null>(null);

  useEffect(() => {
    busyRef.current = busy;
    dropDirectoriesRef.current = onDropDirectories;
    dropErrorRef.current = onDropError;
    if (busy) setDropTarget(null);
  }, [busy, onDropDirectories, onDropError]);

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
        ];
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
          <p>按相对路径和文件名比较 JPG 与主流 RAW，扫描阶段不会修改任何文件。</p>
        </div>
        <div className="safety-assurance">
          <ShieldCheck aria-hidden="true" size={18} />
          <span><strong>扫描仅比较文件</strong><small>只有最终确认后才会移动 RAW</small></span>
        </div>
      </section>

      <section className="directory-flow" aria-label="目录选择">
        <DirectoryPicker
          kind="reference"
          path={referenceRoot}
          busy={busy}
          isDropTarget={dropTarget === "reference"}
          buttonRef={referenceButton}
          onChoose={onChooseDirectory}
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
          <label className="toggle-row">
            <span><strong>包含 XMP</strong><small>跟随对应的未配对 RAW 一起处理</small></span>
            <input
              type="checkbox"
              checked={includeSidecars}
              onChange={(event) => onIncludeSidecarsChange(event.target.checked)}
              disabled={busy}
            />
          </label>
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
        <p>{ready ? "目录已就绪，可以安全生成清理预览。" : "请先选择 JPG 参考目录和 RAW 源目录。"}</p>
        <button
          className="primary-command primary-command-large"
          type="button"
          onClick={onScan}
          disabled={busy || !ready}
        >
          <ScanSearch aria-hidden="true" size={19} />
          {busy ? "正在扫描" : "开始只读扫描"}
        </button>
      </div>
    </main>
  );
}

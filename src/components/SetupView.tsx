import {
  ArrowRight,
  FileImage,
  FolderOpen,
  HardDrive,
  ScanSearch,
  Settings2,
  ShieldCheck,
} from "lucide-react";

interface SetupViewProps {
  referenceRoot: string;
  rawRoot: string;
  includeSidecars: boolean;
  caseSensitive: boolean;
  busy: boolean;
  onChooseDirectory: (kind: "reference" | "raw") => void;
  onIncludeSidecarsChange: (checked: boolean) => void;
  onCaseSensitiveChange: (checked: boolean) => void;
  onScan: () => void;
}

interface DirectoryPickerProps {
  kind: "reference" | "raw";
  path: string;
  busy: boolean;
  onChoose: (kind: "reference" | "raw") => void;
}

function DirectoryPicker({ kind, path, busy, onChoose }: DirectoryPickerProps) {
  const isReference = kind === "reference";
  const Icon = isReference ? FileImage : HardDrive;
  const label = isReference ? "JPG 参考目录" : "RAW 源目录";
  const description = isReference
    ? "只读，用这些 JPG 决定哪些 RAW 需要保留"
    : "仅此目录中的未匹配 RAW 会进入清理列表";

  return (
    <button
      className="directory-picker"
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
          {path || "点击选择目录"}
        </span>
      </span>
      <FolderOpen aria-hidden="true" size={20} />
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
  onIncludeSidecarsChange,
  onCaseSensitiveChange,
  onScan,
}: SetupViewProps) {
  const ready = Boolean(referenceRoot && rawRoot);

  return (
    <main className="setup-view">
      <section className="setup-heading" aria-labelledby="setup-title">
        <div>
          <h1 id="setup-title">选择目录并进行只读扫描</h1>
          <p>按相对路径和文件名比较 JPG 与 NEF，扫描阶段不会修改任何文件。</p>
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
          onChoose={onChooseDirectory}
        />
        <ArrowRight className="directory-arrow" aria-hidden="true" size={22} />
        <DirectoryPicker
          kind="raw"
          path={rawRoot}
          busy={busy}
          onChoose={onChooseDirectory}
        />
      </section>

      <section className="scan-settings" aria-labelledby="settings-title">
        <div className="settings-heading">
          <Settings2 aria-hidden="true" size={18} />
          <div>
            <h2 id="settings-title">扫描设置</h2>
            <p>参考格式 JPG/JPEG，RAW 格式 NEF，匹配键为相对路径和文件名。</p>
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

import { invoke } from "@tauri-apps/api/core";
import { open } from "@tauri-apps/plugin-dialog";
import {
  AlertTriangle,
  Aperture,
  Check,
  CheckCircle2,
  FileCode2,
  FileImage,
  FolderOpen,
  HardDrive,
  LoaderCircle,
  RefreshCw,
  ScanSearch,
  Search,
  Trash2,
  X,
} from "lucide-react";
import { useMemo, useRef, useState } from "react";
import type {
  DeleteSummary,
  FilterMode,
  Notice,
  ScanItem,
  ScanSummary,
  WorkPhase,
} from "./types";
import { errorMessage, formatBytes, formatDate } from "./utils";

const STORAGE_KEY = "framepair.settings.v1";

interface StoredSettings {
  referenceRoot: string;
  rawRoot: string;
  includeSidecars: boolean;
  caseSensitive: boolean;
}

function loadSettings(): StoredSettings {
  try {
    const stored = localStorage.getItem(STORAGE_KEY);
    if (stored) {
      return {
        referenceRoot: "",
        rawRoot: "",
        includeSidecars: true,
        caseSensitive: false,
        ...(JSON.parse(stored) as Partial<StoredSettings>),
      };
    }
  } catch {
    // A damaged preference should not prevent the app from opening.
  }
  return {
    referenceRoot: "",
    rawRoot: "",
    includeSidecars: true,
    caseSensitive: false,
  };
}

function statusIcon(item: ScanItem) {
  if (item.status === "keep") {
    return <CheckCircle2 aria-hidden="true" size={16} />;
  }
  return <AlertTriangle aria-hidden="true" size={16} />;
}

function App() {
  const initial = useMemo(loadSettings, []);
  const [referenceRoot, setReferenceRoot] = useState(initial.referenceRoot);
  const [rawRoot, setRawRoot] = useState(initial.rawRoot);
  const [includeSidecars, setIncludeSidecars] = useState(initial.includeSidecars);
  const [caseSensitive, setCaseSensitive] = useState(initial.caseSensitive);
  const [phase, setPhase] = useState<WorkPhase>("idle");
  const [scan, setScan] = useState<ScanSummary | null>(null);
  const [selectedIds, setSelectedIds] = useState<Set<string>>(new Set());
  const [selectedRowId, setSelectedRowId] = useState<string | null>(null);
  const [filter, setFilter] = useState<FilterMode>("delete");
  const [search, setSearch] = useState("");
  const [notice, setNotice] = useState<Notice | null>(null);
  const confirmDialog = useRef<HTMLDialogElement>(null);

  const busy = phase !== "idle";
  const deleteItems = useMemo(
    () => scan?.items.filter((item) => item.status === "delete") ?? [],
    [scan],
  );
  const selectedItems = useMemo(
    () => deleteItems.filter((item) => selectedIds.has(item.id)),
    [deleteItems, selectedIds],
  );
  const selectedBytes = selectedItems.reduce((sum, item) => sum + item.sizeBytes, 0);
  const selectedRow = scan?.items.find((item) => item.id === selectedRowId) ?? null;

  const visibleItems = useMemo(() => {
    const term = search.trim().toLocaleLowerCase();
    return (scan?.items ?? []).filter((item) => {
      if (filter !== "all" && item.status !== filter) return false;
      if (!term) return true;
      return (
        item.relativePath.toLocaleLowerCase().includes(term) ||
        item.matchedReference?.toLocaleLowerCase().includes(term)
      );
    });
  }, [filter, scan, search]);

  const visibleDeleteItems = visibleItems.filter((item) => item.status === "delete");
  const allVisibleSelected =
    visibleDeleteItems.length > 0 && visibleDeleteItems.every((item) => selectedIds.has(item.id));

  function persistSettings(next?: Partial<StoredSettings>) {
    localStorage.setItem(
      STORAGE_KEY,
      JSON.stringify({
        referenceRoot,
        rawRoot,
        includeSidecars,
        caseSensitive,
        ...next,
      }),
    );
  }

  async function chooseDirectory(kind: "reference" | "raw") {
    try {
      const selected = await open({
        directory: true,
        multiple: false,
        title: kind === "reference" ? "选择 JPG 参考目录" : "选择 RAW 源目录",
      });
      if (typeof selected !== "string") return;
      if (kind === "reference") {
        setReferenceRoot(selected);
        persistSettings({ referenceRoot: selected });
      } else {
        setRawRoot(selected);
        persistSettings({ rawRoot: selected });
      }
      setScan(null);
      setSelectedIds(new Set());
      setSelectedRowId(null);
      setNotice(null);
    } catch (error) {
      setNotice({ tone: "error", title: "无法打开目录选择器", detail: errorMessage(error) });
    }
  }

  async function runScan(options: { silent?: boolean } = {}) {
    if (!referenceRoot || !rawRoot) {
      setNotice({ tone: "warning", title: "目录尚未选择" });
      return false;
    }
    setPhase("scanning");
    if (!options.silent) setNotice(null);
    try {
      const result = await invoke<ScanSummary>("scan_pairs", {
        request: {
          referenceRoot,
          rawRoot,
          referenceExtensions: ["jpg", "jpeg"],
          rawExtensions: ["nef"],
          sidecarExtensions: ["xmp"],
          caseSensitive,
        },
      });
      setScan(result);
      setSelectedIds(
        new Set(
          result.items
            .filter(
              (item) =>
                item.status === "delete" && (item.kind === "raw" || includeSidecars),
            )
            .map((item) => item.id),
        ),
      );
      setSelectedRowId(result.items.find((item) => item.status === "delete")?.id ?? null);
      setFilter(result.missingRaws > 0 ? "delete" : "all");
      persistSettings();
      if (!options.silent) {
        setNotice(
          result.missingRaws > 0
            ? {
                tone: "warning",
                title: `发现 ${result.missingRaws} 个未配对 RAW`,
                detail: `预计可释放 ${formatBytes(result.reclaimableBytes)}`,
              }
            : { tone: "success", title: "所有 RAW 均有对应参考文件" },
        );
      }
      return true;
    } catch (error) {
      setScan(null);
      setSelectedIds(new Set());
      setNotice({ tone: "error", title: "扫描失败", detail: errorMessage(error) });
      return false;
    } finally {
      setPhase("idle");
    }
  }

  function toggleItem(item: ScanItem) {
    if (item.status !== "delete") return;
    setSelectedIds((current) => {
      const next = new Set(current);
      if (next.has(item.id)) next.delete(item.id);
      else next.add(item.id);
      return next;
    });
  }

  function toggleVisibleItems() {
    setSelectedIds((current) => {
      const next = new Set(current);
      for (const item of visibleDeleteItems) {
        if (allVisibleSelected) next.delete(item.id);
        else next.add(item.id);
      }
      return next;
    });
  }

  function updateSidecars(checked: boolean) {
    setIncludeSidecars(checked);
    persistSettings({ includeSidecars: checked });
    setSelectedIds((current) => {
      const next = new Set(current);
      for (const item of deleteItems.filter((candidate) => candidate.kind === "sidecar")) {
        if (checked) next.add(item.id);
        else next.delete(item.id);
      }
      return next;
    });
  }

  function updateCaseSensitivity(checked: boolean) {
    setCaseSensitive(checked);
    persistSettings({ caseSensitive: checked });
    setScan(null);
    setSelectedIds(new Set());
    setNotice(null);
  }

  function requestDelete() {
    if (selectedItems.length === 0) {
      setNotice({ tone: "warning", title: "没有选中待处理文件" });
      return;
    }
    confirmDialog.current?.showModal();
  }

  async function executeDelete() {
    confirmDialog.current?.close();
    setPhase("deleting");
    setNotice(null);
    try {
      const result = await invoke<DeleteSummary>("move_to_trash", {
        request: {
          rawRoot,
          items: selectedItems.map((item) => ({
            relativePath: item.relativePath,
            expectedSizeBytes: item.sizeBytes,
            expectedModifiedMs: item.modifiedMs,
          })),
        },
      });
      const firstFailure = result.results.find((item) => !item.success);
      setNotice(
        result.failed > 0
          ? {
              tone: "error",
              title: `${result.succeeded} 个文件已处理，${result.failed} 个失败`,
              detail: firstFailure
                ? `${firstFailure.relativePath}：${firstFailure.message}`
                : result.logWarning ?? undefined,
            }
          : {
              tone: "success",
              title: `${result.succeeded} 个文件已移入回收站/废纸篓`,
              detail: result.logWarning ?? undefined,
            },
      );
      await runScan({ silent: true });
    } catch (error) {
      setNotice({ tone: "error", title: "清理失败", detail: errorMessage(error) });
    } finally {
      setPhase("idle");
    }
  }

  return (
    <div className="app-shell">
      <header className="app-header">
        <div className="brand-lockup">
          <Aperture aria-hidden="true" size={22} strokeWidth={2.25} />
          <div>
            <strong>FramePair</strong>
            <span>影像配对</span>
          </div>
        </div>
        <div className="header-state" aria-live="polite">
          {phase === "scanning" && <><LoaderCircle className="spin" aria-hidden="true" size={16} />正在扫描</>}
          {phase === "deleting" && <><LoaderCircle className="spin" aria-hidden="true" size={16} />正在移入回收站</>}
          {phase === "idle" && scan && <>上次扫描 {formatDate(scan.scannedAtMs)}</>}
          {phase === "idle" && !scan && <>等待扫描</>}
        </div>
      </header>

      <section className="source-strip" aria-label="扫描目录">
        <div className="path-field">
          <div className="path-label"><FileImage aria-hidden="true" size={17} />JPG 参考目录</div>
          <div className={`path-value ${referenceRoot ? "" : "is-empty"}`} title={referenceRoot}>
            {referenceRoot || "未选择"}
          </div>
          <button
            className="icon-button"
            type="button"
            onClick={() => chooseDirectory("reference")}
            disabled={busy}
            aria-label="选择 JPG 参考目录"
            title="选择 JPG 参考目录"
          >
            <FolderOpen aria-hidden="true" size={18} />
          </button>
        </div>
        <div className="path-field">
          <div className="path-label"><HardDrive aria-hidden="true" size={17} />RAW 源目录</div>
          <div className={`path-value ${rawRoot ? "" : "is-empty"}`} title={rawRoot}>
            {rawRoot || "未选择"}
          </div>
          <button
            className="icon-button"
            type="button"
            onClick={() => chooseDirectory("raw")}
            disabled={busy}
            aria-label="选择 RAW 源目录"
            title="选择 RAW 源目录"
          >
            <FolderOpen aria-hidden="true" size={18} />
          </button>
        </div>
        <button
          className="primary-command"
          type="button"
          onClick={() => runScan()}
          disabled={busy || !referenceRoot || !rawRoot}
        >
          {scan ? <RefreshCw aria-hidden="true" size={18} /> : <ScanSearch aria-hidden="true" size={18} />}
          {scan ? "重新扫描" : "开始扫描"}
        </button>
      </section>

      {busy && <div className="activity-line" aria-hidden="true"><span /></div>}

      {scan && (
        <section className="summary-band" aria-label="扫描汇总">
          <div><span>参考文件</span><strong>{scan.referenceFiles}</strong></div>
          <div><span>RAW 文件</span><strong>{scan.rawFiles}</strong></div>
          <div className="summary-ok"><span>已配对</span><strong>{scan.matchedRaws}</strong></div>
          <div className="summary-warning"><span>待清理 RAW</span><strong>{scan.missingRaws}</strong></div>
          <div><span>伴随 XMP</span><strong>{scan.sidecars}</strong></div>
          <div><span>预计释放</span><strong>{formatBytes(scan.reclaimableBytes)}</strong></div>
        </section>
      )}

      {notice && (
        <div className={`notice notice-${notice.tone}`} role={notice.tone === "error" ? "alert" : "status"}>
          {notice.tone === "success" ? <CheckCircle2 aria-hidden="true" size={18} /> : <AlertTriangle aria-hidden="true" size={18} />}
          <div><strong>{notice.title}</strong>{notice.detail && <span>{notice.detail}</span>}</div>
          <button className="notice-close" type="button" onClick={() => setNotice(null)} aria-label="关闭消息" title="关闭消息">
            <X aria-hidden="true" size={16} />
          </button>
        </div>
      )}

      <main className="workspace">
        <section className="results-pane" aria-label="扫描结果">
          <div className="results-toolbar">
            <div className="segment-control" role="tablist" aria-label="结果过滤">
              {([
                ["delete", `待清理 ${scan?.missingRaws ?? 0}`],
                ["keep", `已配对 ${scan?.matchedRaws ?? 0}`],
                ["all", `全部 ${scan?.items.length ?? 0}`],
              ] as const).map(([value, label]) => (
                <button key={value} type="button" role="tab" aria-selected={filter === value} onClick={() => setFilter(value)}>
                  {label}
                </button>
              ))}
            </div>
            <label className="search-field">
              <Search aria-hidden="true" size={16} />
              <span className="sr-only">搜索路径</span>
              <input id="path-search" name="path-search" value={search} onChange={(event) => setSearch(event.target.value)} placeholder="搜索路径" />
            </label>
          </div>

          <div className="table-region">
            {scan ? (
              <table>
                <colgroup>
                  <col className="col-check" />
                  <col className="col-status" />
                  <col />
                  <col className="col-kind" />
                  <col className="col-size" />
                </colgroup>
                <thead>
                  <tr>
                    <th>
                      <input
                        type="checkbox"
                        checked={allVisibleSelected}
                        onChange={toggleVisibleItems}
                        disabled={visibleDeleteItems.length === 0 || busy}
                        aria-label="选择或取消选择当前列表中的待清理文件"
                      />
                    </th>
                    <th>状态</th>
                    <th>相对路径</th>
                    <th>类型</th>
                    <th className="numeric">大小</th>
                  </tr>
                </thead>
                <tbody>
                  {visibleItems.map((item) => (
                    <tr key={item.id} className={selectedRowId === item.id ? "is-active" : ""}>
                      <td>
                        {item.status === "delete" ? (
                          <input
                            type="checkbox"
                            checked={selectedIds.has(item.id)}
                            onChange={() => toggleItem(item)}
                            disabled={busy}
                            aria-label={`选择 ${item.relativePath}`}
                          />
                        ) : <Check aria-hidden="true" className="keep-check" size={16} />}
                      </td>
                      <td><span className={`row-status status-${item.status}`}>{statusIcon(item)}{item.status === "keep" ? "保留" : "清理"}</span></td>
                      <td>
                        <button className="file-path-button" type="button" onClick={() => setSelectedRowId(item.id)} title={item.relativePath}>
                          {item.relativePath}
                        </button>
                      </td>
                      <td>{item.kind === "raw" ? <><FileImage aria-hidden="true" size={15} />RAW</> : <><FileCode2 aria-hidden="true" size={15} />XMP</>}</td>
                      <td className="numeric">{formatBytes(item.sizeBytes)}</td>
                    </tr>
                  ))}
                </tbody>
              </table>
            ) : (
              <div className="empty-state">
                <ScanSearch aria-hidden="true" size={34} />
                <strong>尚未扫描</strong>
                <span>0 个结果</span>
              </div>
            )}
            {scan && visibleItems.length === 0 && (
              <div className="empty-state compact"><Search aria-hidden="true" size={28} /><strong>没有匹配结果</strong></div>
            )}
          </div>

          <div className="action-bar">
            <span>已选 {selectedItems.length} 个文件，{formatBytes(selectedBytes)}</span>
            <button className="danger-command" type="button" onClick={requestDelete} disabled={busy || selectedItems.length === 0}>
              <Trash2 aria-hidden="true" size={17} />移到回收站/废纸篓
            </button>
          </div>
        </section>

        <aside className="inspector" aria-label="规则与文件详情">
          <section>
            <h2>配对规则</h2>
            <dl className="rule-list">
              <div><dt>参考格式</dt><dd>JPG, JPEG</dd></div>
              <div><dt>RAW 格式</dt><dd>NEF</dd></div>
              <div><dt>匹配键</dt><dd>相对路径 + 文件名</dd></div>
            </dl>
            <label className="toggle-row">
              <span><strong>包含 XMP</strong><small>与未配对 RAW 一起处理</small></span>
              <input type="checkbox" checked={includeSidecars} onChange={(event) => updateSidecars(event.target.checked)} disabled={busy} />
            </label>
            <label className="toggle-row">
              <span><strong>区分大小写</strong><small>默认关闭</small></span>
              <input type="checkbox" checked={caseSensitive} onChange={(event) => updateCaseSensitivity(event.target.checked)} disabled={busy} />
            </label>
          </section>

          <section className="file-detail">
            <h2>文件详情</h2>
            {selectedRow ? (
              <dl>
                <div><dt>文件</dt><dd>{selectedRow.fileName}</dd></div>
                <div><dt>相对路径</dt><dd>{selectedRow.relativePath}</dd></div>
                <div><dt>大小</dt><dd>{formatBytes(selectedRow.sizeBytes)}</dd></div>
                <div><dt>修改时间</dt><dd>{formatDate(selectedRow.modifiedMs)}</dd></div>
                <div><dt>对应参考</dt><dd>{selectedRow.matchedReference ?? "无"}</dd></div>
              </dl>
            ) : <p className="muted">未选择文件</p>}
          </section>

          {(scan?.warnings.length ?? 0) > 0 && (
            <section className="warnings-section">
              <h2>扫描警告</h2>
              {scan?.warnings.map((warning) => <p key={warning}><AlertTriangle aria-hidden="true" size={15} />{warning}</p>)}
            </section>
          )}
        </aside>
      </main>

      <dialog ref={confirmDialog} className="confirm-dialog" aria-labelledby="confirm-title">
        <div className="dialog-header">
          <div className="dialog-icon"><Trash2 aria-hidden="true" size={20} /></div>
          <div><h2 id="confirm-title">移入系统回收站/废纸篓</h2><p>文件不会被永久删除。</p></div>
          <button className="icon-button" type="button" onClick={() => confirmDialog.current?.close()} aria-label="取消并关闭" title="取消并关闭">
            <X aria-hidden="true" size={18} />
          </button>
        </div>
        <div className="dialog-summary">
          <div><span>文件数量</span><strong>{selectedItems.length}</strong></div>
          <div><span>占用空间</span><strong>{formatBytes(selectedBytes)}</strong></div>
          <div><span>RAW 目录</span><strong title={rawRoot}>{rawRoot}</strong></div>
        </div>
        <div className="dialog-actions">
          <button type="button" className="secondary-command" onClick={() => confirmDialog.current?.close()}>取消</button>
          <button type="button" className="danger-command" onClick={executeDelete}><Trash2 aria-hidden="true" size={17} />确认移入</button>
        </div>
      </dialog>
    </div>
  );
}

export default App;

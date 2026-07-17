import { invoke } from "@tauri-apps/api/core";
import { open } from "@tauri-apps/plugin-dialog";
import {
  AlertTriangle,
  Aperture,
  Check,
  CheckCircle2,
  LoaderCircle,
  X,
} from "lucide-react";
import { useMemo, useRef, useState } from "react";
import { ConfirmDialog } from "./components/ConfirmDialog";
import { ResultsWorkspace } from "./components/ResultsWorkspace";
import { SetupView } from "./components/SetupView";
import type {
  DeleteSummary,
  FilterMode,
  Notice,
  ScanItem,
  ScanSummary,
  WorkPhase,
} from "./types";
import {
  cleanableItems,
  errorMessage,
  formatBytes,
  formatDate,
  noticeAfterRescanFailure,
  scanHasBlockingIssues,
} from "./utils";

const STORAGE_KEY = "framepair.settings.v2";

interface StoredSettings {
  referenceRoot: string;
  rawRoot: string;
  includeSidecars: boolean;
  caseSensitive: boolean;
}

interface ScanRunResult {
  ok: boolean;
  error?: string;
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
  const [inspectorOpen, setInspectorOpen] = useState(false);
  const [filter, setFilter] = useState<FilterMode>("delete");
  const [search, setSearch] = useState("");
  const [notice, setNotice] = useState<Notice | null>(null);
  const [lastOperation, setLastOperation] = useState<DeleteSummary | null>(null);
  const [confirmAcknowledged, setConfirmAcknowledged] = useState(false);
  const confirmDialog = useRef<HTMLDialogElement>(null);

  const busy = phase !== "idle";
  const blocked = scanHasBlockingIssues(scan);
  const deleteItems = useMemo(
    () => cleanableItems(scan?.items ?? [], includeSidecars),
    [includeSidecars, scan],
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
      if (!includeSidecars && item.kind === "sidecar") return false;
      if (filter !== "all" && item.status !== filter) return false;
      if (!term) return true;
      return (
        item.relativePath.toLocaleLowerCase().includes(term) ||
        item.matchedReference?.toLocaleLowerCase().includes(term)
      );
    });
  }, [filter, includeSidecars, scan, search]);

  const visibleDeleteItems = visibleItems.filter((item) => item.status === "delete");
  const visibleSelectedCount = visibleDeleteItems.filter((item) => selectedIds.has(item.id)).length;
  const allVisibleSelected =
    visibleDeleteItems.length > 0 && visibleSelectedCount === visibleDeleteItems.length;
  const someVisibleSelected = visibleSelectedCount > 0 && !allVisibleSelected;

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

  function resetReview() {
    setScan(null);
    setSelectedIds(new Set());
    setSelectedRowId(null);
    setInspectorOpen(false);
    setSearch("");
    setNotice(null);
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
      resetReview();
      setLastOperation(null);
    } catch (error) {
      setNotice({ tone: "error", title: "无法打开目录选择器", detail: errorMessage(error) });
    }
  }

  async function runScan(options: { silent?: boolean } = {}): Promise<ScanRunResult> {
    if (!referenceRoot || !rawRoot) {
      const error = "目录尚未选择";
      if (!options.silent) setNotice({ tone: "warning", title: error });
      return { ok: false, error };
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
      const cleanable = cleanableItems(result.items, includeSidecars);
      setScan(result);
      setSelectedIds(new Set());
      setSelectedRowId(null);
      setInspectorOpen(false);
      setFilter(result.missingRaws > 0 ? "delete" : "all");
      setSearch("");
      persistSettings();
      if (!options.silent) {
        if (result.duplicateReferenceKeys > 0) {
          setNotice({
            tone: "error",
            title: "扫描发现重复匹配键，清理操作已暂停",
            detail: "请整理 JPG 参考目录后重新扫描",
          });
        } else if (result.missingRaws > 0) {
          setNotice({
            tone: "warning",
            title: `发现 ${result.missingRaws} 个未配对 RAW`,
            detail: `${cleanable.length} 个文件可供复核，当前未选择任何文件`,
          });
        } else {
          setNotice({ tone: "success", title: "所有 RAW 均有对应参考文件" });
        }
      }
      return { ok: true };
    } catch (error) {
      const message = errorMessage(error);
      resetReview();
      if (!options.silent) setNotice({ tone: "error", title: "扫描失败", detail: message });
      return { ok: false, error: message };
    } finally {
      setPhase("idle");
    }
  }

  function toggleItem(item: ScanItem) {
    if (item.status !== "delete" || blocked) return;
    setSelectedIds((current) => {
      const next = new Set(current);
      if (next.has(item.id)) next.delete(item.id);
      else next.add(item.id);
      return next;
    });
  }

  function toggleVisibleItems() {
    if (blocked) return;
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
    if (!checked) {
      setSelectedIds((current) => {
        const sidecarIds = new Set(
          (scan?.items ?? [])
            .filter((item) => item.kind === "sidecar")
            .map((item) => item.id),
        );
        return new Set([...current].filter((id) => !sidecarIds.has(id)));
      });
      if (selectedRow?.kind === "sidecar") {
        setSelectedRowId(null);
        setInspectorOpen(false);
      }
    }
  }

  function updateCaseSensitivity(checked: boolean) {
    setCaseSensitive(checked);
    persistSettings({ caseSensitive: checked });
    resetReview();
    setLastOperation(null);
  }

  function changeDirectories() {
    resetReview();
    setLastOperation(null);
  }

  function selectRow(id: string) {
    setSelectedRowId(id);
    setInspectorOpen(true);
  }

  function requestDelete() {
    if (blocked) {
      setNotice({ tone: "error", title: "存在重复匹配键，不能执行清理" });
      return;
    }
    if (selectedItems.length === 0) {
      setNotice({ tone: "warning", title: "请先选择需要移入回收站的文件" });
      return;
    }
    setConfirmAcknowledged(false);
    confirmDialog.current?.showModal();
  }

  function closeConfirmDialog() {
    confirmDialog.current?.close();
    setConfirmAcknowledged(false);
  }

  async function executeDelete() {
    if (!confirmAcknowledged) return;
    confirmDialog.current?.close();
    setConfirmAcknowledged(false);
    if (!scan || blocked) {
      setNotice({ tone: "warning", title: "扫描结果已失效，请重新扫描" });
      return;
    }
    setPhase("deleting");
    setNotice(null);
    try {
      const result = await invoke<DeleteSummary>("move_to_trash", {
        request: {
          planId: scan.planId,
          rawRoot,
          items: selectedItems.map((item) => ({
            relativePath: item.relativePath,
            expectedSizeBytes: item.sizeBytes,
            expectedModifiedMs: item.modifiedMs,
          })),
        },
      });
      setLastOperation(result);
      const firstFailure = result.results.find((item) => !item.success);
      const deletionNotice: Notice = result.failed > 0
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
            detail: result.logWarning ?? "可在下方打开系统回收站或查看操作日志",
          };
      setNotice(deletionNotice);
      const rescan = await runScan({ silent: true });
      if (!rescan.ok) {
        setNotice(noticeAfterRescanFailure(deletionNotice, rescan.error ?? "未知错误"));
      }
    } catch (error) {
      setNotice({ tone: "error", title: "清理失败", detail: errorMessage(error) });
    } finally {
      setPhase("idle");
    }
  }

  async function runLocationCommand(command: string, args: Record<string, unknown> = {}) {
    try {
      await invoke(command, args);
    } catch (error) {
      setNotice({ tone: "error", title: "无法打开系统位置", detail: errorMessage(error) });
    }
  }

  const currentStep = phase === "deleting" ? 3 : scan ? 2 : 1;

  return (
    <div className="app-shell">
      <header className="app-header">
        <div className="brand-lockup">
          <Aperture aria-hidden="true" size={23} strokeWidth={2.25} />
          <div><strong>FramePair</strong><span>影像配对清理</span></div>
        </div>
        <ol className="workflow-progress" aria-label="清理流程">
          {["选择目录", "复核结果", "安全执行"].map((label, index) => {
            const step = index + 1;
            return (
              <li key={label} className={step === currentStep ? "is-current" : step < currentStep ? "is-complete" : ""} aria-current={step === currentStep ? "step" : undefined}>
                <span aria-hidden="true">{step < currentStep ? <Check size={13} /> : step}</span>{label}
              </li>
            );
          })}
        </ol>
        <div className="header-state" aria-live="polite">
          {phase === "scanning" && <><LoaderCircle className="spin" aria-hidden="true" size={16} />正在只读扫描</>}
          {phase === "deleting" && <><LoaderCircle className="spin" aria-hidden="true" size={16} />正在移入回收站</>}
          {phase === "idle" && scan && <>扫描于 {formatDate(scan.scannedAtMs)}</>}
          {phase === "idle" && !scan && <>本地处理，不上传照片</>}
        </div>
      </header>

      {busy && <div className="activity-line" aria-hidden="true"><span /></div>}

      {notice && (
        <div className={`notice notice-${notice.tone}`} role={notice.tone === "error" ? "alert" : "status"}>
          {notice.tone === "success" ? <CheckCircle2 aria-hidden="true" size={18} /> : <AlertTriangle aria-hidden="true" size={18} />}
          <div><strong>{notice.title}</strong>{notice.detail && <span>{notice.detail}</span>}</div>
          <button className="notice-close" type="button" onClick={() => setNotice(null)} aria-label="关闭消息" title="关闭消息"><X aria-hidden="true" size={16} /></button>
        </div>
      )}

      {scan ? (
        <ResultsWorkspace
          scan={scan}
          referenceRoot={referenceRoot}
          rawRoot={rawRoot}
          includeSidecars={includeSidecars}
          busy={busy}
          blocked={blocked}
          filter={filter}
          search={search}
          visibleItems={visibleItems}
          cleanableCount={deleteItems.length}
          selectedIds={selectedIds}
          selectedItems={selectedItems}
          selectedBytes={selectedBytes}
          selectedRow={selectedRow}
          inspectorOpen={inspectorOpen}
          allVisibleSelected={allVisibleSelected}
          someVisibleSelected={someVisibleSelected}
          visibleDeleteCount={visibleDeleteItems.length}
          lastOperation={lastOperation}
          onFilterChange={setFilter}
          onSearchChange={setSearch}
          onToggleItem={toggleItem}
          onToggleVisibleItems={toggleVisibleItems}
          onSelectRow={selectRow}
          onInspectorOpenChange={setInspectorOpen}
          onIncludeSidecarsChange={updateSidecars}
          onChangeDirectories={changeDirectories}
          onRescan={() => void runScan()}
          onRequestDelete={requestDelete}
          onRevealItem={(item) => void runLocationCommand("reveal_scan_item", { rawRoot, relativePath: item.relativePath })}
          onOpenTrash={() => void runLocationCommand("open_system_trash")}
          onOpenLog={(logPath) => void runLocationCommand("reveal_operation_log", { logPath })}
        />
      ) : (
        <SetupView
          referenceRoot={referenceRoot}
          rawRoot={rawRoot}
          includeSidecars={includeSidecars}
          caseSensitive={caseSensitive}
          busy={busy}
          onChooseDirectory={chooseDirectory}
          onIncludeSidecarsChange={updateSidecars}
          onCaseSensitiveChange={updateCaseSensitivity}
          onScan={() => void runScan()}
        />
      )}

      <ConfirmDialog
        dialogRef={confirmDialog}
        selectedItems={selectedItems}
        selectedBytes={selectedBytes}
        rawRoot={rawRoot}
        acknowledged={confirmAcknowledged}
        busy={busy}
        onAcknowledgedChange={setConfirmAcknowledged}
        onCancel={closeConfirmDialog}
        onConfirm={() => void executeDelete()}
      />
    </div>
  );
}

export default App;

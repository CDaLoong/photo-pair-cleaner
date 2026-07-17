import { invoke } from "@tauri-apps/api/core";
import { open, save } from "@tauri-apps/plugin-dialog";
import {
  AlertTriangle,
  Check,
  CheckCircle2,
  CircleHelp,
  ListChecks,
  LoaderCircle,
  X,
} from "lucide-react";
import { useMemo, useRef, useState } from "react";
import { CleanupGuideDialog } from "./CleanupGuideDialog";
import { ConfirmDialog } from "./ConfirmDialog";
import { ResultsWorkspace } from "./ResultsWorkspace";
import { SetupView } from "./SetupView";
import type {
  CleanupDestination,
  CleanupSummary,
  DirectoryKind,
  FilterMode,
  Notice,
  QuarantineOperation,
  ReferenceSource,
  ReferenceSourceType,
  RestoreSummary,
  ScanItem,
  ScanMode,
  ScanSummary,
  WorkPhase,
} from "../../types";
import {
  cleanableItems,
  canAuditReferenceSource,
  errorMessage,
  formatBytes,
  formatDate,
  noticeAfterRescanFailure,
  scanHasBlockingIssues,
} from "../../utils";

const STORAGE_KEY = "framepair.settings.v2";
const GUIDE_STORAGE_KEY = "framepair.guide.completed.v1";

interface StoredSettings {
  referenceRoot: string;
  rawRoot: string;
  includeSidecars: boolean;
  caseSensitive: boolean;
  scanMode: ScanMode;
  referenceSourceType: ReferenceSourceType;
  manifestPath: string;
  ratingRoot: string;
  minimumRating: number;
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
        scanMode: "cleanupRaw",
        referenceSourceType: "directory",
        manifestPath: "",
        ratingRoot: "",
        minimumRating: 4,
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
    scanMode: "cleanupRaw",
    referenceSourceType: "directory",
    manifestPath: "",
    ratingRoot: "",
    minimumRating: 4,
  };
}

function shouldOpenGuide() {
  try {
    return localStorage.getItem(GUIDE_STORAGE_KEY) !== "true";
  } catch {
    return true;
  }
}

interface CleanupModuleProps {
  active: boolean;
}

export function CleanupModule({ active }: CleanupModuleProps) {
  const initial = useMemo(loadSettings, []);
  const [referenceRoot, setReferenceRoot] = useState(initial.referenceRoot);
  const [rawRoot, setRawRoot] = useState(initial.rawRoot);
  const [includeSidecars, setIncludeSidecars] = useState(initial.includeSidecars);
  const [caseSensitive, setCaseSensitive] = useState(initial.caseSensitive);
  const [scanMode, setScanMode] = useState<ScanMode>(initial.scanMode);
  const [referenceSourceType, setReferenceSourceType] = useState<ReferenceSourceType>(initial.referenceSourceType);
  const [manifestPath, setManifestPath] = useState(initial.manifestPath);
  const [ratingRoot, setRatingRoot] = useState(initial.ratingRoot);
  const [minimumRating, setMinimumRating] = useState(initial.minimumRating);
  const [phase, setPhase] = useState<WorkPhase>("idle");
  const [scan, setScan] = useState<ScanSummary | null>(null);
  const [selectedIds, setSelectedIds] = useState<Set<string>>(new Set());
  const [selectedRowId, setSelectedRowId] = useState<string | null>(null);
  const [inspectorOpen, setInspectorOpen] = useState(false);
  const [filter, setFilter] = useState<FilterMode>("unmatched");
  const [search, setSearch] = useState("");
  const [notice, setNotice] = useState<Notice | null>(null);
  const [lastOperation, setLastOperation] = useState<CleanupSummary | null>(null);
  const [cleanupDestination, setCleanupDestination] = useState<CleanupDestination>("trash");
  const [quarantineOperations, setQuarantineOperations] = useState<QuarantineOperation[]>([]);
  const [confirmAcknowledged, setConfirmAcknowledged] = useState(false);
  const [guideOpen, setGuideOpen] = useState(shouldOpenGuide);
  const confirmDialog = useRef<HTMLDialogElement>(null);

  const busy = phase !== "idle";
  const blocked = scanHasBlockingIssues(scan);
  const deleteItems = useMemo(
    () => cleanableItems(scan?.items ?? [], includeSidecars, scan?.mode ?? scanMode),
    [includeSidecars, scan, scanMode],
  );
  const selectedItems = useMemo(
    () => deleteItems.filter((item) => selectedIds.has(item.id)),
    [deleteItems, selectedIds],
  );
  const selectedBytes = selectedItems.reduce((sum, item) => sum + item.sizeBytes, 0);
  const selectedRow = scan?.items.find((item) => item.id === selectedRowId) ?? null;
  const activeReferencePath = referenceSourceType === "directory"
    ? referenceRoot
    : referenceSourceType === "manifest"
      ? manifestPath
      : ratingRoot || rawRoot;

  const visibleItems = useMemo(() => {
    const term = search.trim().toLocaleLowerCase();
    return (scan?.items ?? []).filter((item) => {
      if (!includeSidecars && item.kind === "sidecar") return false;
      if (filter !== "all" && item.matchStatus !== filter) return false;
      if (!term) return true;
      return (
        item.relativePath.toLocaleLowerCase().includes(term) ||
        item.matchedPath?.toLocaleLowerCase().includes(term)
      );
    });
  }, [filter, includeSidecars, scan, search]);

  const visibleActionableItems = visibleItems.filter((item) =>
    cleanableItems([item], includeSidecars, scan?.mode ?? scanMode).length > 0,
  );
  const visibleSelectedCount = visibleActionableItems.filter((item) => selectedIds.has(item.id)).length;
  const allVisibleSelected =
    visibleActionableItems.length > 0 && visibleSelectedCount === visibleActionableItems.length;
  const someVisibleSelected = visibleSelectedCount > 0 && !allVisibleSelected;

  function persistSettings(next?: Partial<StoredSettings>) {
    localStorage.setItem(
      STORAGE_KEY,
      JSON.stringify({
        referenceRoot,
        rawRoot,
        includeSidecars,
        caseSensitive,
        scanMode,
        referenceSourceType,
        manifestPath,
        ratingRoot,
        minimumRating,
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

  function applyDirectory(kind: DirectoryKind, path: string) {
    if (kind === "reference") {
      if (referenceSourceType === "xmpRating") {
        setRatingRoot(path);
        persistSettings({ ratingRoot: path });
      } else {
        setReferenceRoot(path);
        persistSettings({ referenceRoot: path });
      }
    } else {
      setRawRoot(path);
      persistSettings({ rawRoot: path });
    }
    resetReview();
    setLastOperation(null);
    setQuarantineOperations([]);
  }

  async function chooseDirectory(kind: DirectoryKind) {
    try {
      const selected = await open({
        directory: true,
        multiple: false,
        title: kind === "reference"
          ? referenceSourceType === "xmpRating" ? "选择 XMP 评分目录" : "选择 JPG 参考目录"
          : "选择 RAW 源目录",
      });
      if (typeof selected !== "string") return;
      applyDirectory(kind, selected);
    } catch (error) {
      setNotice({ tone: "error", title: "无法打开目录选择器", detail: errorMessage(error) });
    }
  }

  async function chooseManifest() {
    try {
      const selected = await open({
        directory: false,
        multiple: false,
        title: "选择保留文件清单",
        filters: [{ name: "UTF-8 文本清单", extensions: ["txt"] }],
      });
      if (typeof selected !== "string") return;
      setManifestPath(selected);
      persistSettings({ manifestPath: selected });
      resetReview();
      setLastOperation(null);
      setNotice({ tone: "success", title: "已添加保留文件清单", detail: selected });
    } catch (error) {
      setNotice({ tone: "error", title: "无法打开文件清单选择器", detail: errorMessage(error) });
    }
  }

  async function dropDirectories(kind: DirectoryKind, paths: string[]) {
    if (busy) return;
    if (paths.length !== 1) {
      setNotice({
        tone: "warning",
        title: "一次只能拖入一个文件夹",
        detail: `当前拖入了 ${paths.length} 个项目`,
      });
      return;
    }
    try {
      const path = await invoke<string>("validate_directory_path", { path: paths[0] });
      applyDirectory(kind, path);
      setNotice({
        tone: "success",
        title: kind === "reference"
          ? referenceSourceType === "xmpRating" ? "已添加 XMP 评分目录" : "已添加 JPG 参考目录"
          : "已添加 RAW 源目录",
        detail: path,
      });
    } catch (error) {
      setNotice({
        tone: "error",
        title: "无法添加拖入的项目",
        detail: errorMessage(error),
      });
    }
  }

  async function runScan(options: { silent?: boolean } = {}): Promise<ScanRunResult> {
    if (!activeReferencePath || !rawRoot) {
      const error = "目录尚未选择";
      if (!options.silent) setNotice({ tone: "warning", title: error });
      return { ok: false, error };
    }
    setPhase("scanning");
    if (!options.silent) setNotice(null);
    try {
      const referenceSource: ReferenceSource = referenceSourceType === "directory"
        ? { type: "directory", root: referenceRoot }
        : referenceSourceType === "manifest"
          ? { type: "manifest", path: manifestPath }
          : { type: "xmpRating", root: ratingRoot || rawRoot, minimumRating };
      const result = await invoke<ScanSummary>("scan_pairs", {
        request: {
          referenceSource,
          rawRoot,
          caseSensitive,
          mode: scanMode,
        },
      });
      const cleanable = cleanableItems(result.items, includeSidecars);
      setScan(result);
      setSelectedIds(new Set());
      setSelectedRowId(null);
      setInspectorOpen(false);
      setFilter(result.unmatched > 0 ? "unmatched" : "all");
      setSearch("");
      persistSettings();
      void refreshQuarantineOperations(rawRoot);
      if (!options.silent) {
        if (result.mode === "cleanupRaw" && result.duplicateReferenceKeys > 0) {
          setNotice({
            tone: "error",
            title: "扫描发现重复匹配键，清理操作已暂停",
            detail: "请整理当前参考源后重新扫描",
          });
        } else if (result.unmatched > 0) {
          setNotice({
            tone: result.mode === "cleanupRaw" ? "warning" : "info",
            title: result.mode === "cleanupRaw"
              ? `发现 ${result.unmatched} 个未配对 RAW`
              : `发现 ${result.unmatched} 个没有对应 RAW 的 JPG`,
            detail: result.mode === "cleanupRaw"
              ? `${cleanable.length} 个文件可供复核，当前未选择任何文件`
              : "这是只读审计结果，不会修改 JPG 文件",
          });
        } else {
          setNotice({
            tone: "success",
            title: result.mode === "cleanupRaw"
              ? "所有 RAW 均有对应参考文件"
              : "所有 JPG 均有对应 RAW",
          });
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
    if (!scan || cleanableItems([item], includeSidecars, scan.mode).length === 0 || blocked) return;
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
      for (const item of visibleActionableItems) {
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

  function updateScanMode(mode: ScanMode) {
    if (mode === "auditReference" && !canAuditReferenceSource(referenceSourceType)) {
      setNotice({ tone: "warning", title: "反向审计只支持 JPG 目录参考源" });
      return;
    }
    setScanMode(mode);
    persistSettings({ scanMode: mode });
    resetReview();
    setLastOperation(null);
  }

  function updateReferenceSourceType(source: ReferenceSourceType) {
    setReferenceSourceType(source);
    const nextMode = canAuditReferenceSource(source) ? scanMode : "cleanupRaw";
    if (nextMode !== scanMode) setScanMode(nextMode);
    persistSettings({ referenceSourceType: source, scanMode: nextMode });
    resetReview();
    setLastOperation(null);
  }

  function updateMinimumRating(value: number) {
    const next = Math.max(1, Math.min(5, Math.round(value)));
    setMinimumRating(next);
    persistSettings({ minimumRating: next });
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
    if (scan?.mode !== "cleanupRaw") {
      setNotice({ tone: "warning", title: "反向审计是只读模式，不能执行清理" });
      return;
    }
    if (blocked) {
      setNotice({ tone: "error", title: "存在重复匹配键，不能执行清理" });
      return;
    }
    if (selectedItems.length === 0) {
      setNotice({ tone: "warning", title: "请先选择需要处理的文件" });
      return;
    }
    setCleanupDestination("trash");
    setConfirmAcknowledged(false);
    confirmDialog.current?.showModal();
  }

  function closeConfirmDialog() {
    confirmDialog.current?.close();
    setConfirmAcknowledged(false);
  }

  function dismissGuide() {
    try {
      localStorage.setItem(GUIDE_STORAGE_KEY, "true");
    } catch {
      // The guide remains available from the header when storage is unavailable.
    }
    setGuideOpen(false);
  }

  async function executeCleanup() {
    if (!confirmAcknowledged) return;
    confirmDialog.current?.close();
    setConfirmAcknowledged(false);
    if (!scan || blocked) {
      setNotice({ tone: "warning", title: "扫描结果已失效，请重新扫描" });
      return;
    }
    setPhase("executing");
    setNotice(null);
    try {
      const result = await invoke<CleanupSummary>("execute_cleanup", {
        request: {
          planId: scan.planId,
          rawRoot,
          destination: cleanupDestination,
          items: selectedItems.map((item) => ({
            relativePath: item.relativePath,
            expectedSizeBytes: item.sizeBytes,
            expectedModifiedMs: item.modifiedMs,
          })),
        },
      });
      setLastOperation(result);
      const firstFailure = result.results.find((item) => !item.success);
      const cleanupNotice: Notice = result.failed > 0
        ? {
            tone: "error",
            title: `${result.succeeded} 个文件已处理，${result.failed} 个失败`,
            detail: firstFailure
              ? `${firstFailure.relativePath}：${firstFailure.message}`
              : result.logWarning ?? undefined,
          }
        : {
            tone: "success",
            title: cleanupDestination === "quarantine"
              ? `${result.succeeded} 个文件已移入 FramePair 隔离区`
              : `${result.succeeded} 个文件已移入回收站/废纸篓`,
            detail: result.logWarning ?? (cleanupDestination === "quarantine"
              ? "可在下方打开隔离目录或恢复本次文件"
              : "可在下方打开系统回收站或查看操作日志"),
          };
      setNotice(cleanupNotice);
      const rescan = await runScan({ silent: true });
      if (!rescan.ok) {
        setNotice(noticeAfterRescanFailure(cleanupNotice, rescan.error ?? "未知错误"));
      }
    } catch (error) {
      setNotice({ tone: "error", title: "清理失败", detail: errorMessage(error) });
    } finally {
      setPhase("idle");
    }
  }

  async function refreshQuarantineOperations(root = rawRoot) {
    if (!root) {
      setQuarantineOperations([]);
      return;
    }
    try {
      const operations = await invoke<QuarantineOperation[]>("list_quarantine_operations", {
        rawRoot: root,
      });
      setQuarantineOperations(operations);
    } catch {
      setQuarantineOperations([]);
    }
  }

  async function restoreQuarantineOperation(operationId: string) {
    setPhase("executing");
    setNotice(null);
    try {
      const result = await invoke<RestoreSummary>("restore_quarantine_operation", {
        rawRoot,
        operationId,
      });
      const firstFailure = result.results.find((item) => !item.success);
      const restoreNotice: Notice = result.failed > 0
        ? {
            tone: "error",
            title: `${result.succeeded} 个文件已恢复，${result.failed} 个失败`,
            detail: firstFailure
              ? `${firstFailure.relativePath}：${firstFailure.message}`
              : undefined,
          }
        : {
            tone: "success",
            title: `${result.succeeded} 个文件已恢复到原位置`,
            detail: result.succeeded === 0 ? "本次操作没有待恢复文件" : undefined,
          };
      setNotice(restoreNotice);
      setLastOperation(null);
      const rescan = await runScan({ silent: true });
      if (!rescan.ok) {
        setNotice(noticeAfterRescanFailure(restoreNotice, rescan.error ?? "未知错误"));
      }
      await refreshQuarantineOperations();
    } catch (error) {
      setNotice({ tone: "error", title: "恢复失败", detail: errorMessage(error) });
    } finally {
      setPhase("idle");
    }
  }

  async function exportAuditManifest() {
    if (!scan || scan.mode !== "auditReference") return;
    try {
      const destination = await save({
        title: "导出未配对 JPG 清单",
        defaultPath: "framepair-unmatched-jpg.txt",
        filters: [{ name: "文本清单", extensions: ["txt"] }],
      });
      if (typeof destination !== "string") return;
      await invoke("export_audit_manifest", {
        planId: scan.planId,
        rawRoot,
        destination,
      });
      setNotice({ tone: "success", title: "审计清单已导出", detail: destination });
    } catch (error) {
      setNotice({ tone: "error", title: "无法导出审计清单", detail: errorMessage(error) });
    }
  }

  async function runLocationCommand(command: string, args: Record<string, unknown> = {}) {
    try {
      await invoke(command, args);
    } catch (error) {
      setNotice({ tone: "error", title: "无法打开系统位置", detail: errorMessage(error) });
    }
  }

  const currentStep = phase === "executing" ? 3 : scan ? 2 : 1;

  return (
    <section className="cleanup-module" aria-label="配对清理">
      <header className="app-header">
        <div className="module-heading">
          <ListChecks aria-hidden="true" size={20} />
          <div><strong>配对清理</strong><span>按筛选结果安全处理 RAW</span></div>
        </div>
        <ol className="workflow-progress" aria-label="清理流程" data-tour="workflow-progress">
          {["选择目录", "复核结果", "安全执行"].map((label, index) => {
            const step = index + 1;
            return (
              <li key={label} className={step === currentStep ? "is-current" : step < currentStep ? "is-complete" : ""} aria-current={step === currentStep ? "step" : undefined}>
                <span aria-hidden="true">{step < currentStep ? <Check size={13} /> : step}</span>{label}
              </li>
            );
          })}
        </ol>
        <div className="header-utilities">
          <button className="guide-trigger" type="button" onClick={() => setGuideOpen(true)} disabled={busy}>
            <CircleHelp aria-hidden="true" size={16} />使用引导
          </button>
          <div className="header-state" aria-live="polite">
            {phase === "scanning" && <><LoaderCircle className="spin" aria-hidden="true" size={16} />正在只读扫描</>}
            {phase === "executing" && <><LoaderCircle className="spin" aria-hidden="true" size={16} />正在安全移动文件</>}
            {phase === "idle" && scan && <>扫描于 {formatDate(scan.scannedAtMs)}</>}
            {phase === "idle" && !scan && <>等待选择目录</>}
          </div>
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

      {scan && !guideOpen ? (
        <ResultsWorkspace
          scan={scan}
          referenceRoot={activeReferencePath}
          referenceSourceType={referenceSourceType}
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
          visibleDeleteCount={visibleActionableItems.length}
          lastOperation={lastOperation}
          quarantineOperations={quarantineOperations}
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
          onRevealItem={(item) => void runLocationCommand("reveal_scan_item", {
            root: item.kind === "reference" ? referenceRoot : rawRoot,
            relativePath: item.relativePath,
          })}
          onOpenTrash={() => void runLocationCommand("open_system_trash")}
          onOpenLog={(logPath) => void runLocationCommand("reveal_operation_log", { logPath })}
          onRevealQuarantine={(operationId) => void runLocationCommand("reveal_quarantine_operation", { rawRoot, operationId })}
          onRestoreQuarantine={(operationId) => void restoreQuarantineOperation(operationId)}
          onExportAudit={() => void exportAuditManifest()}
        />
      ) : (
        <SetupView
          active={active}
          referenceRoot={referenceRoot}
          rawRoot={rawRoot}
          referenceSourceType={referenceSourceType}
          manifestPath={manifestPath}
          ratingRoot={ratingRoot || rawRoot}
          minimumRating={minimumRating}
          includeSidecars={includeSidecars}
          caseSensitive={caseSensitive}
          scanMode={scanMode}
          busy={busy}
          onChooseDirectory={chooseDirectory}
          onChooseManifest={() => void chooseManifest()}
          onDropDirectories={(kind, paths) => void dropDirectories(kind, paths)}
          onDropError={(message) => setNotice({ tone: "warning", title: message })}
          onIncludeSidecarsChange={updateSidecars}
          onCaseSensitiveChange={updateCaseSensitivity}
          onScanModeChange={updateScanMode}
          onReferenceSourceTypeChange={updateReferenceSourceType}
          onMinimumRatingChange={updateMinimumRating}
          onUseRawRootForRatings={() => {
            setRatingRoot("");
            persistSettings({ ratingRoot: "" });
            resetReview();
          }}
          onScan={() => void runScan()}
        />
      )}

      <ConfirmDialog
        dialogRef={confirmDialog}
        selectedItems={selectedItems}
        selectedBytes={selectedBytes}
        rawRoot={rawRoot}
        destination={cleanupDestination}
        acknowledged={confirmAcknowledged}
        busy={busy}
        onDestinationChange={setCleanupDestination}
        onAcknowledgedChange={setConfirmAcknowledged}
        onCancel={closeConfirmDialog}
        onConfirm={() => void executeCleanup()}
      />

      <CleanupGuideDialog open={active && guideOpen} onDismiss={dismissGuide} />
    </section>
  );
}

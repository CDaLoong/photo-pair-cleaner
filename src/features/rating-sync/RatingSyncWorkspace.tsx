import { invoke, isTauri } from "@tauri-apps/api/core";
import { open } from "@tauri-apps/plugin-dialog";
import {
  AlertTriangle,
  CheckCircle2,
  FolderInput,
  FolderOpen,
  LoaderCircle,
  RefreshCw,
  ShieldCheck,
  Star,
  X,
} from "lucide-react";
import { useEffect, useMemo, useRef, useState } from "react";
import { errorMessage } from "../../utils";
import {
  readySyncAssetIds,
  syncModeNotice,
  syncStatusLabel,
  validateRatingRange,
  validateSyncTargets,
} from "./ratingSyncUtils";
import type {
  RatingConflictPolicy,
  RatingSyncExecutionSummary,
  RatingSyncPlanRequest,
  RatingSyncPlanSummary,
  RatingSyncSettings,
  RatingSyncState,
} from "./types";
import { STORAGE_KEYS } from "../../storageKeys";
import { useDirectoryDrop } from "../../hooks/useDirectoryDrop";


const DEFAULT_SETTINGS: RatingSyncSettings = {
  mode: "manual",
  targets: { rawXmp: true, jpegMetadata: false },
  conflictPolicy: "skip",
  jpegWriteConfirmed: false,
};

export interface RatingSyncWorkspaceState {
  busy: boolean;
  executing: boolean;
  hasPlan: boolean;
  detail: string;
}

interface RatingSyncWorkspaceProps {
  active: boolean;
  onStateChange: (state: RatingSyncWorkspaceState) => void;
}

function loadStoredRoot(): string {
  try {
    return localStorage.getItem(STORAGE_KEYS.ratingSyncRoot) ?? "";
  } catch {
    return "";
  }
}

function ratingText(value: number | null): string {
  if (value === null) return "-";
  if (value === -1) return "拒绝 (-1)";
  return String(value);
}

export function RatingSyncWorkspace({ active, onStateChange }: RatingSyncWorkspaceProps) {
  const [root, setRoot] = useState(loadStoredRoot);
  const [minimumRating, setMinimumRating] = useState(1);
  const [maximumRating, setMaximumRating] = useState(5);
  const [settings, setSettings] = useState(DEFAULT_SETTINGS);
  const [pendingCount, setPendingCount] = useState(0);
  const [plan, setPlan] = useState<RatingSyncPlanSummary | null>(null);
  const [selectedIds, setSelectedIds] = useState<Set<string>>(new Set());
  const [result, setResult] = useState<RatingSyncExecutionSummary | null>(null);
  const [busy, setBusy] = useState(false);
  const [executing, setExecuting] = useState(false);
  const [dropActive, setDropActive] = useState(false);
  const [message, setMessage] = useState<{ tone: "success" | "warning" | "error"; title: string; detail?: string } | null>(null);
  const loadedSettings = useRef(false);
  const readyIds = useMemo(() => readySyncAssetIds(plan?.items ?? []), [plan]);
  const allReadySelected = readyIds.length > 0 && readyIds.every((id) => selectedIds.has(id));

  useEffect(() => {
    onStateChange({
      busy,
      executing,
      hasPlan: Boolean(plan),
      detail: executing
        ? "正在写入评分元数据"
        : busy
          ? "正在生成只读同步计划"
          : plan
            ? `计划包含 ${plan.ready} 个待同步照片组`
            : root
              ? "目录已选择，等待生成计划"
              : "等待选择照片目录",
    });
  }, [busy, executing, onStateChange, plan, root]);

  useEffect(() => {
    if (!active || loadedSettings.current) return;
    loadedSettings.current = true;
    if (!isTauri()) return;
    setBusy(true);
    void invoke<RatingSyncState>("get_rating_sync_state", { root: null })
      .then((state) => {
        setSettings(state.settings);
        setPendingCount(state.pending.length);
      })
      .catch((loadError) => setMessage({ tone: "error", title: "无法读取评分同步设置", detail: errorMessage(loadError) }))
      .finally(() => setBusy(false));
  }, [active]);

  useDirectoryDrop({
    active,
    onHoverChange: setDropActive,
    onDropDirectory: (path) => void validateAndSetRoot(path),
    onRejectMultiple: () => setMessage({ tone: "warning", title: "一次只能拖入一个照片目录" }),
    onError: (dropError) => setMessage({
      tone: "error",
      title: "无法启用目录拖拽",
      detail: errorMessage(dropError),
    }),
  });

  function clearPlan() {
    setPlan(null);
    setSelectedIds(new Set());
    setResult(null);
  }

  function changeSettings(next: RatingSyncSettings) {
    setSettings(next);
    clearPlan();
    setMessage(null);
  }

  async function validateAndSetRoot(path: string) {
    try {
      const validated = await invoke<string>("validate_directory_path", { path });
      setRoot(validated);
      clearPlan();
      try {
        localStorage.setItem(STORAGE_KEYS.ratingSyncRoot, validated);
      } catch {
        // The current session remains usable when layout storage is unavailable.
      }
      const state = await invoke<RatingSyncState>("get_rating_sync_state", { root: validated });
      setSettings(state.settings);
      setPendingCount(state.pending.length);
      setMessage({ tone: "success", title: "已添加照片目录", detail: validated });
    } catch (directoryError) {
      setMessage({ tone: "error", title: "无法添加照片目录", detail: errorMessage(directoryError) });
    }
  }

  async function chooseRoot() {
    try {
      const selected = await open({ directory: true, multiple: false, title: "选择评分同步照片目录" });
      if (typeof selected === "string") await validateAndSetRoot(selected);
    } catch (chooseError) {
      setMessage({ tone: "error", title: "无法打开目录选择器", detail: errorMessage(chooseError) });
    }
  }

  function planRequest(): RatingSyncPlanRequest {
    return {
      root,
      minimumRating,
      maximumRating,
      assetIds: [],
      targets: settings.targets,
      conflictPolicy: settings.conflictPolicy,
      jpegWriteConfirmed: settings.jpegWriteConfirmed,
    };
  }

  async function generatePlan(options: { keepResult?: boolean; silentSuccess?: boolean } = {}): Promise<boolean> {
    const { keepResult = false, silentSuccess = false } = options;
    if (!root) {
      setMessage({ tone: "warning", title: "请先选择照片目录" });
      return false;
    }
    const rangeValidation = validateRatingRange(minimumRating, maximumRating);
    const targetValidation = validateSyncTargets(settings.targets, settings.jpegWriteConfirmed);
    if (!rangeValidation.valid) {
      setMessage({ tone: "warning", title: rangeValidation.message });
      return false;
    }
    if (!targetValidation.valid) {
      setMessage({ tone: "warning", title: targetValidation.message });
      return false;
    }
    setBusy(true);
    setMessage(null);
    if (!keepResult) setResult(null);
    try {
      const saved = await invoke<RatingSyncSettings>("save_rating_sync_settings", { settings });
      setSettings(saved);
      const nextPlan = await invoke<RatingSyncPlanSummary>("generate_rating_sync_plan", { request: planRequest() });
      setPlan(nextPlan);
      setSelectedIds(new Set(readySyncAssetIds(nextPlan.items)));
      if (!silentSuccess) {
        setMessage(nextPlan.conflicts > 0
          ? { tone: "warning", title: `计划已生成，${nextPlan.conflicts} 个照片组存在冲突`, detail: "冲突项不可执行，请调整策略或元数据后重新生成计划。" }
          : { tone: "success", title: `计划已生成，${nextPlan.ready} 个照片组待同步` });
      }
      return true;
    } catch (planError) {
      setPlan(null);
      setSelectedIds(new Set());
      if (!keepResult) setResult(null);
      setMessage({ tone: "error", title: "无法生成评分同步计划", detail: errorMessage(planError) });
      return false;
    } finally {
      setBusy(false);
    }
  }

  function toggleReady(id: string) {
    if (!readyIds.includes(id)) return;
    setSelectedIds((current) => {
      const next = new Set(current);
      if (next.has(id)) next.delete(id);
      else next.add(id);
      return next;
    });
  }

  function toggleAllReady() {
    setSelectedIds(allReadySelected ? new Set() : new Set(readyIds));
  }

  async function executeSelected() {
    if (!plan || selectedIds.size === 0) {
      setMessage({ tone: "warning", title: "请先选择待同步照片组" });
      return;
    }
    setExecuting(true);
    setMessage(null);
    try {
      const summary = await invoke<RatingSyncExecutionSummary>("execute_rating_sync_plan", {
        request: { planId: plan.planId, root: plan.root, assetIds: [...selectedIds] },
      });
      const executionMessage = summary.failed > 0
        ? { tone: "warning" as const, title: `${summary.succeeded} 个目标同步完成，${summary.failed} 个失败`, detail: summary.results.find((item) => !item.success)?.message }
        : { tone: "success" as const, title: `${summary.succeeded} 个评分目标同步完成` };
      setResult(summary);
      setPlan(null);
      setSelectedIds(new Set());
      const refreshed = await generatePlan({ keepResult: true, silentSuccess: true });
      if (refreshed) setMessage(executionMessage);
    } catch (executeError) {
      setMessage({ tone: "error", title: "评分同步执行失败", detail: errorMessage(executeError) });
    } finally {
      setExecuting(false);
    }
  }

  return (
    <div className="rating-sync-workspace">
      {message ? <div className={`notice notice-${message.tone}`} role={message.tone === "error" ? "alert" : "status"}>{message.tone === "success" ? <CheckCircle2 aria-hidden="true" size={18} /> : <AlertTriangle aria-hidden="true" size={18} />}<div><strong>{message.title}</strong>{message.detail ? <span>{message.detail}</span> : null}</div><button className="notice-close" type="button" onClick={() => setMessage(null)} aria-label="关闭消息" title="关闭消息"><X aria-hidden="true" size={16} /></button></div> : null}

      <section className="rating-sync-batch-setup" data-tour="rating-sync-settings">
        <div className="rating-sync-batch-heading">
          <div><h1>批量同步照片评分</h1><p>根据照片组中的评分来源生成只读计划，确认后才写入元数据。</p></div>
          <button className="secondary-command" type="button" onClick={() => void chooseRoot()} disabled={busy || executing}><FolderOpen aria-hidden="true" size={16} />选择目录</button>
        </div>

        <button className={dropActive ? "rating-sync-root-picker is-drop-target" : "rating-sync-root-picker"} type="button" onClick={() => void chooseRoot()} disabled={busy || executing} data-tour="rating-sync-root">
          <FolderInput aria-hidden="true" size={20} />
          <span><strong>{root || "选择或拖入照片根目录"}</strong><small>递归索引 JPG、RAW 与 XMP，生成计划时不会修改文件</small></span>
        </button>

        <div className="rating-sync-batch-controls">
          <label><span>最低评分</span><input type="number" min="0" max="5" step="1" value={minimumRating} onChange={(event) => { setMinimumRating(Number(event.target.value)); clearPlan(); }} /></label>
          <label><span>最高评分</span><input type="number" min="0" max="5" step="1" value={maximumRating} onChange={(event) => { setMaximumRating(Number(event.target.value)); clearPlan(); }} /></label>
          <label><span>同步方式</span><select value={settings.mode} onChange={(event) => changeSettings({ ...settings, mode: event.target.value as RatingSyncSettings["mode"] })}><option value="manual">手动同步</option><option value="automatic">自动同步评分</option></select></label>
          <label><span>冲突策略</span><select value={settings.conflictPolicy} onChange={(event) => changeSettings({ ...settings, conflictPolicy: event.target.value as RatingConflictPolicy })}><option value="skip">不覆盖并提示</option><option value="framePair">FramePair 评分优先</option><option value="external">外部评分优先</option><option value="highest">取较高评分</option></select></label>
        </div>

        <div className="rating-sync-batch-targets" role="group" aria-label="评分同步目标">
          <label><input type="checkbox" checked={settings.targets.rawXmp} onChange={(event) => changeSettings({ ...settings, targets: { ...settings.targets, rawXmp: event.target.checked } })} /><span><strong>RAW XMP</strong><small>不修改 RAW 原文件</small></span></label>
          <label><input type="checkbox" checked={settings.targets.jpegMetadata} onChange={(event) => changeSettings({ ...settings, targets: { ...settings.targets, jpegMetadata: event.target.checked }, jpegWriteConfirmed: event.target.checked ? settings.jpegWriteConfirmed : false })} /><span><strong>JPG 内嵌评分</strong><small>高级选项，默认关闭</small></span></label>
        </div>
        {settings.targets.jpegMetadata ? <label className="rating-sync-jpeg-confirm"><input type="checkbox" checked={settings.jpegWriteConfirmed} onChange={(event) => changeSettings({ ...settings, jpegWriteConfirmed: event.target.checked })} /><span>我确认允许 FramePair 修改 JPG 内嵌评分元数据</span></label> : null}
        <div className="rating-sync-batch-safety"><ShieldCheck aria-hidden="true" size={16} /><span>{syncModeNotice(settings.mode)}</span>{pendingCount > 0 ? <strong>{pendingCount} 个待处理</strong> : null}</div>
        <div className="rating-sync-batch-command"><button className="primary-command" type="button" onClick={() => void generatePlan()} disabled={busy || executing || !root}>{busy ? <LoaderCircle className="spin" aria-hidden="true" size={16} /> : <RefreshCw aria-hidden="true" size={16} />}生成只读计划</button></div>
      </section>

      {plan ? <section className="rating-sync-batch-review" data-tour="rating-sync-plan">
        <header><div><h2>复核评分同步计划</h2><p>{plan.totalItems} 个照片组 · {plan.ready} 个待同步 · {plan.unchanged} 个已一致 · {plan.conflicts} 个冲突</p></div><div><button className="secondary-command" type="button" onClick={toggleAllReady} disabled={readyIds.length === 0}>{allReadySelected ? "取消全选" : "选择全部待同步"}</button><button className="primary-command" type="button" onClick={() => void executeSelected()} disabled={executing || selectedIds.size === 0}>{executing ? <LoaderCircle className="spin" aria-hidden="true" size={16} /> : null}同步所选 {selectedIds.size} 组</button></div></header>
        <div className="rating-sync-table-scroll"><table><thead><tr><th aria-label="选择" /><th>照片组</th><th><Star aria-hidden="true" size={13} /> FP</th><th>JPG</th><th>XMP</th><th>工作评分</th><th>目标</th><th>状态</th></tr></thead><tbody>{plan.items.map((item) => <tr key={item.assetId} className={`is-${item.status}`}><td><input type="checkbox" checked={selectedIds.has(item.assetId)} disabled={item.status !== "ready"} onChange={() => toggleReady(item.assetId)} aria-label={`选择 ${item.relativeStem}`} /></td><td><strong>{item.relativeStem}</strong>{item.issues[0] ? <small>{item.issues[0]}</small> : null}</td><td>{ratingText(item.framePair)}</td><td>{ratingText(item.jpegMetadata)}</td><td>{ratingText(item.rawXmp)}</td><td>{ratingText(item.resolved)}</td><td>{item.writes.map((write) => write.target === "rawXmp" ? "XMP" : "JPG").join(" + ") || "-"}</td><td><span>{syncStatusLabel(item.status)}</span></td></tr>)}</tbody></table></div>
      </section> : null}

      {result ? <footer className="rating-sync-batch-result"><CheckCircle2 aria-hidden="true" size={17} /><span>最近执行：{result.succeeded} 个成功，{result.failed} 个失败</span></footer> : null}
    </div>
  );
}

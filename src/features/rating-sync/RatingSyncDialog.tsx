import { invoke } from "@tauri-apps/api/core";
import {
  AlertTriangle,
  CheckCircle2,
  RefreshCw,
  Settings2,
  ShieldCheck,
  X,
} from "lucide-react";
import { useEffect, useRef, useState } from "react";
import { errorMessage } from "../../utils";
import type { PhotoAsset } from "../preview/types";
import { syncModeNotice, syncStatusLabel, validateSyncTargets } from "./ratingSyncUtils";
import type {
  RatingConflictPolicy,
  RatingSyncExecutionSummary,
  RatingSyncPlanSummary,
  RatingSyncSettings,
  RatingSyncState,
} from "./types";

const DEFAULT_SETTINGS: RatingSyncSettings = {
  mode: "manual",
  targets: { rawXmp: true, jpegMetadata: false },
  conflictPolicy: "skip",
  jpegWriteConfirmed: false,
};

const CONFLICT_POLICIES: Array<{ value: RatingConflictPolicy; label: string }> = [
  { value: "skip", label: "不覆盖并提示" },
  { value: "framePair", label: "FramePair 评分优先" },
  { value: "external", label: "外部评分优先" },
  { value: "highest", label: "取较高评分" },
];

interface RatingSyncDialogProps {
  open: boolean;
  root: string;
  asset: PhotoAsset | null;
  onDismiss: () => void;
  onSynced: (summary: RatingSyncExecutionSummary) => Promise<void> | void;
}

function ratingText(value: number | null): string {
  if (value === null) return "未写入";
  if (value === -1) return "拒绝标记 (-1)";
  if (value === 0) return "未评分 (0)";
  return `${value} 星`;
}

export function RatingSyncDialog({
  open,
  root,
  asset,
  onDismiss,
  onSynced,
}: RatingSyncDialogProps) {
  const dialogRef = useRef<HTMLDialogElement>(null);
  const [settings, setSettings] = useState(DEFAULT_SETTINGS);
  const [pendingCount, setPendingCount] = useState(0);
  const [plan, setPlan] = useState<RatingSyncPlanSummary | null>(null);
  const [result, setResult] = useState<RatingSyncExecutionSummary | null>(null);
  const [busy, setBusy] = useState(false);
  const [message, setMessage] = useState<string | null>(null);
  const planItem = plan?.items.find((item) => item.assetId === asset?.id) ?? null;

  useEffect(() => {
    const dialog = dialogRef.current;
    if (!dialog) return;
    if (!open) {
      if (dialog.open) dialog.close();
      return;
    }
    if (!dialog.open) dialog.showModal();
    setPlan(null);
    setResult(null);
    setMessage(null);
    setBusy(true);
    void invoke<RatingSyncState>("get_rating_sync_state", { root: root || null })
      .then((state) => {
        setSettings(state.settings);
        setPendingCount(state.pending.length);
      })
      .catch((loadError) => setMessage(errorMessage(loadError)))
      .finally(() => setBusy(false));
  }, [asset?.id, open, root]);

  function updateSettings(next: RatingSyncSettings) {
    setSettings(next);
    setPlan(null);
    setResult(null);
    setMessage(null);
  }

  async function saveSettings(): Promise<boolean> {
    const validation = validateSyncTargets(settings.targets, settings.jpegWriteConfirmed);
    if (!validation.valid) {
      setMessage(validation.message);
      return false;
    }
    setBusy(true);
    setMessage(null);
    try {
      const saved = await invoke<RatingSyncSettings>("save_rating_sync_settings", { settings });
      setSettings(saved);
      setMessage("评分同步设置已保存");
      return true;
    } catch (saveError) {
      setMessage(errorMessage(saveError));
      return false;
    } finally {
      setBusy(false);
    }
  }

  async function generatePlan() {
    if (!asset || !root) return;
    if (!(await saveSettings())) return;
    setBusy(true);
    setMessage(null);
    setResult(null);
    try {
      const nextPlan = await invoke<RatingSyncPlanSummary>("generate_rating_sync_plan", {
        request: {
          root,
          minimumRating: 0,
          maximumRating: 5,
          assetIds: [asset.id],
          targets: settings.targets,
          conflictPolicy: settings.conflictPolicy,
          jpegWriteConfirmed: settings.jpegWriteConfirmed,
        },
      });
      setPlan(nextPlan);
    } catch (planError) {
      setPlan(null);
      setMessage(errorMessage(planError));
    } finally {
      setBusy(false);
    }
  }

  async function executePlan() {
    if (!plan || !asset || planItem?.status !== "ready") return;
    setBusy(true);
    setMessage(null);
    try {
      const summary = await invoke<RatingSyncExecutionSummary>("execute_rating_sync_plan", {
        request: { planId: plan.planId, root: plan.root, assetIds: [asset.id] },
      });
      setResult(summary);
      setPlan(null);
      setMessage(summary.failed > 0 ? "部分评分目标同步失败，可修复后重新生成计划" : "当前照片评分同步完成");
      await onSynced(summary);
    } catch (executeError) {
      setMessage(errorMessage(executeError));
    } finally {
      setBusy(false);
    }
  }

  return (
    <dialog
      ref={dialogRef}
      className="rating-sync-dialog"
      aria-labelledby="rating-sync-title"
      onCancel={(event) => {
        event.preventDefault();
        onDismiss();
      }}
      onClose={onDismiss}
    >
      <header className="rating-sync-dialog-header">
        <span><Settings2 aria-hidden="true" size={20} /></span>
        <div>
          <h2 id="rating-sync-title">评分同步</h2>
          <p>{asset ? `${asset.name} · ${asset.extensions.join(" + ")}` : "设置评分元数据同步方式"}</p>
        </div>
        <button className="icon-button" type="button" onClick={onDismiss} aria-label="关闭评分同步" title="关闭"><X aria-hidden="true" size={17} /></button>
      </header>

      <div className="rating-sync-dialog-body">
        <section className="rating-sync-settings" aria-label="同步设置">
          <div className="rating-sync-field">
            <span>同步方式</span>
            <div className="rating-sync-mode" role="group" aria-label="同步方式">
              <button type="button" aria-pressed={settings.mode === "manual"} onClick={() => updateSettings({ ...settings, mode: "manual" })}>手动同步</button>
              <button type="button" aria-pressed={settings.mode === "automatic"} onClick={() => updateSettings({ ...settings, mode: "automatic" })}>自动同步</button>
            </div>
          </div>

          <fieldset className="rating-sync-targets">
            <legend>同步目标</legend>
            <label>
              <input type="checkbox" checked={settings.targets.rawXmp} onChange={(event) => updateSettings({ ...settings, targets: { ...settings.targets, rawXmp: event.target.checked } })} />
              <span><strong>RAW XMP</strong><small>创建或更新同名侧车文件，不修改 RAW</small></span>
            </label>
            <label>
              <input type="checkbox" checked={settings.targets.jpegMetadata} onChange={(event) => updateSettings({ ...settings, targets: { ...settings.targets, jpegMetadata: event.target.checked }, jpegWriteConfirmed: event.target.checked ? settings.jpegWriteConfirmed : false })} />
              <span><strong>JPG 内嵌评分</strong><small>高级选项，默认关闭</small></span>
            </label>
          </fieldset>

          {settings.targets.jpegMetadata ? (
            <label className="rating-sync-jpeg-confirm">
              <input type="checkbox" checked={settings.jpegWriteConfirmed} onChange={(event) => updateSettings({ ...settings, jpegWriteConfirmed: event.target.checked })} />
              <span>我确认允许 FramePair 修改 JPG 内嵌评分元数据</span>
            </label>
          ) : null}

          <label className="rating-sync-policy">
            <span>评分冲突策略</span>
            <select value={settings.conflictPolicy} onChange={(event) => updateSettings({ ...settings, conflictPolicy: event.target.value as RatingConflictPolicy })}>
              {CONFLICT_POLICIES.map((policy) => <option key={policy.value} value={policy.value}>{policy.label}</option>)}
            </select>
          </label>

          <div className="rating-sync-safety"><ShieldCheck aria-hidden="true" size={16} /><span>{syncModeNotice(settings.mode)}</span></div>
          {pendingCount > 0 ? <div className="rating-sync-pending"><AlertTriangle aria-hidden="true" size={15} /><span>当前目录有 {pendingCount} 个自动同步待处理项，可为对应照片重新生成计划。</span></div> : null}
        </section>

        {asset ? (
          <section className="rating-sync-current" aria-label="当前照片评分来源">
            <div className="rating-sync-section-heading"><strong>当前照片</strong><span>{asset.relativeStem}</span></div>
            <div className="rating-source-grid">
              <span><small>FramePair</small><strong>{ratingText(asset.ratingState.framePair)}</strong></span>
              <span><small>JPG 元数据</small><strong>{ratingText(asset.ratingState.jpegMetadata)}</strong></span>
              <span><small>RAW XMP</small><strong>{ratingText(asset.ratingState.rawXmp)}</strong></span>
            </div>
          </section>
        ) : null}

        {planItem ? (
          <section className={`rating-sync-plan is-${planItem.status}`} aria-label="当前照片同步计划">
            <div className="rating-sync-section-heading">
              <strong>只读同步计划</strong>
              <span>{syncStatusLabel(planItem.status)}</span>
            </div>
            <div className="rating-sync-plan-summary">
              <span>工作评分 <strong>{ratingText(planItem.resolved)}</strong></span>
              <span>{planItem.writes.length} 个元数据目标</span>
            </div>
            {planItem.writes.length > 0 ? <ul>{planItem.writes.map((write) => <li key={`${write.target}:${write.relativePath}`}><span>{write.target === "rawXmp" ? "RAW XMP" : "JPG 元数据"}</span><strong>{write.relativePath}</strong><small>{ratingText(write.currentRating)} → {ratingText(write.targetRating)}</small></li>)}</ul> : null}
            {planItem.issues.length > 0 ? <div className="rating-sync-issues">{planItem.issues.map((issue) => <span key={issue}><AlertTriangle aria-hidden="true" size={14} />{issue}</span>)}</div> : null}
          </section>
        ) : null}

        {result ? (
          <section className="rating-sync-result" aria-label="评分同步结果">
            <CheckCircle2 aria-hidden="true" size={18} />
            <div><strong>{result.succeeded} 个目标同步完成</strong><span>{result.failed > 0 ? `${result.failed} 个失败` : "外部评分已重新校验"}</span></div>
          </section>
        ) : null}

        {message ? <div className="rating-sync-message" role="status">{message}</div> : null}
      </div>

      <footer className="rating-sync-dialog-actions">
        <button className="secondary-command" type="button" onClick={onDismiss}>关闭</button>
        <button className="secondary-command" type="button" onClick={() => void saveSettings()} disabled={busy}>{busy ? <RefreshCw className="spin" aria-hidden="true" size={15} /> : null}保存设置</button>
        {asset ? <button className="secondary-command" type="button" onClick={() => void generatePlan()} disabled={busy}>生成计划</button> : null}
        {planItem?.status === "ready" ? <button className="primary-command" type="button" onClick={() => void executePlan()} disabled={busy}>确认同步</button> : null}
      </footer>
    </dialog>
  );
}

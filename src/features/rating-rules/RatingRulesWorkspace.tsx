import { invoke, isTauri } from "@tauri-apps/api/core";
import { open, save as saveDialog } from "@tauri-apps/plugin-dialog";
import {
  AlertTriangle,
  CheckCircle2,
  FileDown,
  FileUp,
  FolderInput,
  FolderOpen,
  LoaderCircle,
  Plus,
  Save,
  ScanSearch,
  ShieldCheck,
  X,
} from "lucide-react";
import { useCallback, useEffect, useRef, useState } from "react";
import { errorMessage } from "../../utils";
import type { RatingSyncState } from "../rating-sync/types";
import { validateSyncTargets } from "../rating-sync/ratingSyncUtils";
import { OperationPlanReview } from "./OperationPlanReview";
import { RatingRuleCard } from "./RatingRuleCard";
import {
  RATING_RULE_TEMPLATES,
  createRatingRule,
  rulesForTemplate,
  validateRatingRuleDrafts,
} from "./ratingRuleUtils";
import type {
  OperationPlanRequest,
  OperationPlanSummary,
  OperationSyncPreference,
  RatingRule,
  RatingRuleState,
  RatingRuleTemplateId,
} from "./types";
import type { RatingConflictPolicy } from "../rating-sync/types";

const ROOT_STORAGE_KEY = "framepair.rating-rules.root.v1";

const DEFAULT_SYNC: OperationSyncPreference = {
  enabled: false,
  targets: { rawXmp: true, jpegMetadata: false },
  jpegWriteConfirmed: false,
  syncCleanupBefore: false,
};

export interface RatingRulesWorkspaceState {
  busy: boolean;
  hasPlan: boolean;
  detail: string;
}

interface RatingRulesWorkspaceProps {
  active: boolean;
  onStateChange: (state: RatingRulesWorkspaceState) => void;
}

function loadStoredRoot(): string {
  try {
    return localStorage.getItem(ROOT_STORAGE_KEY) ?? "";
  } catch {
    return "";
  }
}

export function RatingRulesWorkspace({ active, onStateChange }: RatingRulesWorkspaceProps) {
  const [root, setRoot] = useState(loadStoredRoot);
  const [rules, setRules] = useState<RatingRule[]>([]);
  const [template, setTemplate] = useState<RatingRuleTemplateId>("custom");
  const [conflictPolicy, setConflictPolicy] = useState<RatingConflictPolicy>("skip");
  const [sync, setSync] = useState<OperationSyncPreference>(DEFAULT_SYNC);
  const [plan, setPlan] = useState<OperationPlanSummary | null>(null);
  const [busy, setBusy] = useState(false);
  const [dropActive, setDropActive] = useState(false);
  const [message, setMessage] = useState<{
    tone: "success" | "warning" | "error";
    title: string;
    detail?: string;
  } | null>(null);
  const loaded = useRef(false);
  const idCounter = useRef(0);

  const clearPlan = useCallback(() => {
    setPlan(null);
  }, []);

  useEffect(() => {
    onStateChange({
      busy,
      hasPlan: Boolean(plan),
      detail: busy
        ? "正在生成只读模拟计划"
        : plan
          ? `模拟计划包含 ${plan.totalItems} 个照片组`
          : root
            ? "规则草稿已就绪"
            : "等待选择照片目录",
    });
  }, [busy, onStateChange, plan, root]);

  useEffect(() => {
    if (!active || loaded.current) return;
    loaded.current = true;
    if (!isTauri()) return;
    setBusy(true);
    void Promise.all([
      invoke<RatingRuleState>("get_rating_rules"),
      invoke<RatingSyncState>("get_rating_sync_state", { root: null }),
    ])
      .then(([ruleState, syncState]) => {
        setRules(ruleState.rules);
        setConflictPolicy(syncState.settings.conflictPolicy);
        setSync((current) => ({
          ...current,
          targets: syncState.settings.targets,
          jpegWriteConfirmed: syncState.settings.jpegWriteConfirmed,
        }));
      })
      .catch((loadError) => setMessage({
        tone: "error",
        title: "无法读取评分整理设置",
        detail: errorMessage(loadError),
      }))
      .finally(() => setBusy(false));
  }, [active]);

  useEffect(() => {
    if (!active || !isTauri()) return;
    let disposed = false;
    let unlisten: (() => void) | undefined;
    void import("@tauri-apps/api/webview")
      .then(({ getCurrentWebview }) => getCurrentWebview().onDragDropEvent((event) => {
        if (disposed) return;
        if (event.payload.type === "leave") {
          setDropActive(false);
          return;
        }
        if (event.payload.type === "over") {
          setDropActive(true);
          return;
        }
        setDropActive(false);
        if (event.payload.paths.length !== 1) {
          setMessage({ tone: "warning", title: "一次只能拖入一个照片目录" });
          return;
        }
        void validateAndSetRoot(event.payload.paths[0]);
      }))
      .then((stop) => {
        if (disposed) stop();
        else unlisten = stop;
      })
      .catch((dropError) => setMessage({ tone: "error", title: "无法启用目录拖拽", detail: errorMessage(dropError) }));
    return () => {
      disposed = true;
      unlisten?.();
    };
  }, [active]);

  function nextRuleId(): string {
    idCounter.current += 1;
    return `rule-${Date.now()}-${idCounter.current}`;
  }

  function replaceRules(next: RatingRule[]) {
    setRules(next);
    clearPlan();
    setMessage(null);
  }

  async function validateAndSetRoot(path: string) {
    try {
      const validated = await invoke<string>("validate_directory_path", { path });
      setRoot(validated);
      clearPlan();
      try {
        localStorage.setItem(ROOT_STORAGE_KEY, validated);
      } catch {
        // The selected directory remains valid for the current session.
      }
      setMessage({ tone: "success", title: "已添加照片目录", detail: validated });
    } catch (directoryError) {
      setMessage({ tone: "error", title: "无法添加照片目录", detail: errorMessage(directoryError) });
    }
  }

  async function chooseRoot() {
    try {
      const selected = await open({ directory: true, multiple: false, title: "选择评分整理照片目录" });
      if (typeof selected === "string") await validateAndSetRoot(selected);
    } catch (chooseError) {
      setMessage({ tone: "error", title: "无法打开目录选择器", detail: errorMessage(chooseError) });
    }
  }

  async function chooseDestination(ruleId: string) {
    try {
      const selected = await open({ directory: true, multiple: false, title: "选择规则目标目录" });
      if (typeof selected !== "string") return;
      replaceRules(rules.map((rule) => rule.id === ruleId ? { ...rule, destination: selected } : rule));
    } catch (chooseError) {
      setMessage({ tone: "error", title: "无法选择目标目录", detail: errorMessage(chooseError) });
    }
  }

  function applyTemplate() {
    replaceRules(rulesForTemplate(template, nextRuleId));
    setMessage({
      tone: "success",
      title: template === "custom" ? "已切换为空白规则草稿" : "模板已填入规则草稿",
      detail: "模板不会创建目录，也不会自动生成计划。",
    });
  }

  function updateRule(next: RatingRule) {
    replaceRules(rules.map((rule) => rule.id === next.id ? next : rule));
  }

  function moveRule(index: number, direction: -1 | 1) {
    const target = index + direction;
    if (target < 0 || target >= rules.length) return;
    const next = [...rules];
    [next[index], next[target]] = [next[target], next[index]];
    replaceRules(next);
  }

  async function saveRules(showSuccess = true): Promise<boolean> {
    const validation = validateRatingRuleDrafts(rules);
    if (!validation.valid) {
      setMessage({ tone: "warning", title: validation.message });
      return false;
    }
    setBusy(true);
    try {
      const saved = await invoke<RatingRuleState>("save_rating_rules", { rules });
      setRules(saved.rules);
      if (showSuccess) setMessage({ tone: "success", title: "评分规则已保存" });
      return true;
    } catch (saveError) {
      setMessage({ tone: "error", title: "无法保存评分规则", detail: errorMessage(saveError) });
      return false;
    } finally {
      setBusy(false);
    }
  }

  async function importRules() {
    try {
      const selected = await open({
        multiple: false,
        title: "导入 FramePair 评分规则",
        filters: [{ name: "FramePair 评分规则", extensions: ["json"] }],
      });
      if (typeof selected !== "string") return;
      const imported = await invoke<RatingRuleState>("import_rating_rules", { path: selected });
      replaceRules(imported.rules);
      setMessage({ tone: "success", title: "规则已导入为草稿", detail: "确认内容后点击保存规则。" });
    } catch (importError) {
      setMessage({ tone: "error", title: "无法导入评分规则", detail: errorMessage(importError) });
    }
  }

  async function exportRules() {
    const validation = validateRatingRuleDrafts(rules);
    if (!validation.valid) {
      setMessage({ tone: "warning", title: validation.message });
      return;
    }
    try {
      const destination = await saveDialog({
        title: "导出 FramePair 评分规则",
        defaultPath: "framepair-rating-rules.json",
        filters: [{ name: "JSON", extensions: ["json"] }],
      });
      if (!destination) return;
      const exported = await invoke<string>("export_rating_rules", { path: destination, rules });
      setMessage({ tone: "success", title: "评分规则已导出", detail: exported });
    } catch (exportError) {
      setMessage({ tone: "error", title: "无法导出评分规则", detail: errorMessage(exportError) });
    }
  }

  async function generatePlan() {
    if (!root) {
      setMessage({ tone: "warning", title: "请先选择照片目录" });
      return;
    }
    const validation = validateRatingRuleDrafts(rules);
    if (!validation.valid) {
      setMessage({ tone: "warning", title: validation.message });
      return;
    }
    if (sync.enabled) {
      const targetValidation = validateSyncTargets(sync.targets, sync.jpegWriteConfirmed);
      if (!targetValidation.valid) {
        setMessage({ tone: "warning", title: targetValidation.message });
        return;
      }
    }
    if (!(await saveRules(false))) return;
    setBusy(true);
    setPlan(null);
    setMessage(null);
    try {
      const request: OperationPlanRequest = { root, rules, conflictPolicy, sync };
      const nextPlan = await invoke<OperationPlanSummary>("generate_operation_plan", { request });
      setPlan(nextPlan);
      setMessage(nextPlan.conflicts > 0
        ? { tone: "warning", title: `模拟计划已生成，${nextPlan.conflicts} 个照片组存在冲突`, detail: "冲突项必须修改规则或目录后重新生成计划。" }
        : { tone: "success", title: `模拟计划已生成，共 ${nextPlan.totalItems} 个照片组` });
    } catch (planError) {
      setMessage({ tone: "error", title: "无法生成评分整理计划", detail: errorMessage(planError) });
    } finally {
      setBusy(false);
    }
  }

  return (
    <div className="rating-rules-workspace">
      {message ? <div className={`notice notice-${message.tone}`} role={message.tone === "error" ? "alert" : "status"}>{message.tone === "success" ? <CheckCircle2 aria-hidden="true" size={18} /> : <AlertTriangle aria-hidden="true" size={18} />}<div><strong>{message.title}</strong>{message.detail ? <span>{message.detail}</span> : null}</div><button className="notice-close" type="button" onClick={() => setMessage(null)} aria-label="关闭消息" title="关闭消息"><X aria-hidden="true" size={16} /></button></div> : null}

      <section className="rating-rules-setup">
        <header className="rating-rules-heading">
          <div><h1>评分整理与清理规则</h1><p>用评分和格式生成文件处理模拟计划，确认每个目标与冲突。</p></div>
          <div className="rating-rules-file-actions">
            <button className="icon-button" type="button" disabled={busy} onClick={() => void importRules()} aria-label="导入规则" title="导入规则"><FileDown aria-hidden="true" size={16} /></button>
            <button className="icon-button" type="button" disabled={busy || rules.length === 0} onClick={() => void exportRules()} aria-label="导出规则" title="导出规则"><FileUp aria-hidden="true" size={16} /></button>
            <button className="secondary-command" type="button" disabled={busy || rules.length === 0} onClick={() => void saveRules()}><Save aria-hidden="true" size={16} />保存规则</button>
            <button className="secondary-command" type="button" disabled={busy} onClick={() => void chooseRoot()}><FolderOpen aria-hidden="true" size={16} />选择目录</button>
          </div>
        </header>

        <button className={dropActive ? "rating-rules-root-picker is-drop-target" : "rating-rules-root-picker"} type="button" onClick={() => void chooseRoot()} disabled={busy} data-tour="rating-rules-root">
          <FolderInput aria-hidden="true" size={20} />
          <span><strong>{root || "选择或拖入照片根目录"}</strong><small>计划会递归读取 JPG、RAW、XMP 和 FramePair 评分，但不会修改文件</small></span>
        </button>

        <div className="rating-rules-template" data-tour="rating-rules-template">
          <label><span>规则模板</span><select value={template} disabled={busy} onChange={(event) => setTemplate(event.target.value as RatingRuleTemplateId)}>{RATING_RULE_TEMPLATES.map((item) => <option key={item.id} value={item.id}>{item.name} · {item.detail}</option>)}</select></label>
          <button className="secondary-command" type="button" disabled={busy} onClick={applyTemplate}>应用模板</button>
          <button className="secondary-command" type="button" disabled={busy || rules.length >= 100} onClick={() => replaceRules([...rules, createRatingRule(nextRuleId())])}><Plus aria-hidden="true" size={16} />添加规则</button>
        </div>

        <div className="rating-rules-list" data-tour="rating-rules-editor">
          {rules.length === 0 ? <div className="rating-rules-empty"><ScanSearch aria-hidden="true" size={24} /><strong>当前没有评分规则</strong><span>选择模板，或添加一条完全自定义规则。</span></div> : rules.map((rule, index) => (
            <RatingRuleCard
              key={rule.id}
              rule={rule}
              index={index}
              total={rules.length}
              busy={busy}
              onChange={updateRule}
              onChooseDestination={() => void chooseDestination(rule.id)}
              onMove={(direction) => moveRule(index, direction)}
              onRemove={() => replaceRules(rules.filter((item) => item.id !== rule.id))}
            />
          ))}
        </div>

        <section className="rating-rules-sync" data-tour="rating-rules-sync">
          <header><div><strong>同时预览评分同步</strong><span>可叠加元数据动作，但不会改变文件操作规则。</span></div><label className="switch"><input type="checkbox" checked={sync.enabled} disabled={busy} onChange={(event) => { setSync({ ...sync, enabled: event.target.checked }); clearPlan(); }} /><span /></label></header>
          {sync.enabled ? <div className="rating-rules-sync-fields">
            <label><span>冲突策略</span><select value={conflictPolicy} disabled={busy} onChange={(event) => { setConflictPolicy(event.target.value as RatingConflictPolicy); clearPlan(); }}><option value="skip">不覆盖并提示</option><option value="framePair">FramePair 评分优先</option><option value="external">外部评分优先</option><option value="highest">取较高评分</option></select></label>
            <label className="rating-rules-sync-check"><input type="checkbox" checked={sync.targets.rawXmp} disabled={busy} onChange={(event) => { setSync({ ...sync, targets: { ...sync.targets, rawXmp: event.target.checked } }); clearPlan(); }} /><span><strong>RAW XMP</strong><small>永不修改 RAW 原文件</small></span></label>
            <label className="rating-rules-sync-check"><input type="checkbox" checked={sync.targets.jpegMetadata} disabled={busy} onChange={(event) => { setSync({ ...sync, targets: { ...sync.targets, jpegMetadata: event.target.checked }, jpegWriteConfirmed: event.target.checked ? sync.jpegWriteConfirmed : false }); clearPlan(); }} /><span><strong>JPG 元数据</strong><small>高级选项，默认关闭</small></span></label>
            {sync.targets.jpegMetadata ? <label className="rating-rules-sync-confirm"><input type="checkbox" checked={sync.jpegWriteConfirmed} disabled={busy} onChange={(event) => { setSync({ ...sync, jpegWriteConfirmed: event.target.checked }); clearPlan(); }} />我确认允许修改 JPG 评分元数据</label> : null}
            <label className="rating-rules-sync-confirm"><input type="checkbox" checked={sync.syncCleanupBefore} disabled={busy} onChange={(event) => { setSync({ ...sync, syncCleanupBefore: event.target.checked }); clearPlan(); }} />待清理照片在计划中包含清理前同步</label>
          </div> : null}
        </section>

        <div className="rating-rules-safety"><ShieldCheck aria-hidden="true" size={17} /><span>当前仅生成只读模拟计划，不会移动、复制或清理照片。模板也不会创建目录。</span></div>
        <div className="rating-rules-command"><button className="primary-command" type="button" disabled={busy || !root || rules.length === 0} onClick={() => void generatePlan()}>{busy ? <LoaderCircle className="spin" aria-hidden="true" size={16} /> : <ScanSearch aria-hidden="true" size={16} />}生成只读模拟计划</button></div>
      </section>

      {plan ? <OperationPlanReview plan={plan} /> : null}
    </div>
  );
}

import {
  AlertTriangle,
  ArchiveRestore,
  Check,
  CheckCircle2,
  FileCode2,
  FileImage,
  FolderOpen,
  History,
  ListChecks,
  PanelRightClose,
  PanelRightOpen,
  RefreshCw,
  RotateCcw,
  Search,
  ShieldCheck,
  Trash2,
} from "lucide-react";
import { useEffect, useRef } from "react";
import type {
  CleanupSummary,
  FilterMode,
  QuarantineOperation,
  ReferenceSourceType,
  ScanItem,
  ScanSummary,
} from "../types";
import {
  decisionReason,
  formatBytes,
  formatDate,
  rawFormatCounts,
  reclaimableBytes,
  selectionBreakdown,
} from "../utils";

interface SelectionCheckboxProps {
  checked: boolean;
  indeterminate: boolean;
  disabled: boolean;
  onChange: () => void;
}

function SelectionCheckbox({ checked, indeterminate, disabled, onChange }: SelectionCheckboxProps) {
  const ref = useRef<HTMLInputElement>(null);
  useEffect(() => {
    if (ref.current) ref.current.indeterminate = indeterminate;
  }, [indeterminate]);
  return (
    <input
      ref={ref}
      type="checkbox"
      checked={checked}
      onChange={onChange}
      disabled={disabled}
      aria-label="选择或取消选择当前列表中的待清理文件"
    />
  );
}

interface ResultsWorkspaceProps {
  scan: ScanSummary;
  referenceRoot: string;
  referenceSourceType: ReferenceSourceType;
  rawRoot: string;
  includeSidecars: boolean;
  busy: boolean;
  blocked: boolean;
  filter: FilterMode;
  search: string;
  visibleItems: ScanItem[];
  cleanableCount: number;
  selectedIds: Set<string>;
  selectedItems: ScanItem[];
  selectedBytes: number;
  selectedRow: ScanItem | null;
  inspectorOpen: boolean;
  allVisibleSelected: boolean;
  someVisibleSelected: boolean;
  visibleDeleteCount: number;
  lastOperation: CleanupSummary | null;
  quarantineOperations: QuarantineOperation[];
  onFilterChange: (filter: FilterMode) => void;
  onSearchChange: (search: string) => void;
  onToggleItem: (item: ScanItem) => void;
  onToggleVisibleItems: () => void;
  onSelectRow: (id: string) => void;
  onInspectorOpenChange: (open: boolean) => void;
  onIncludeSidecarsChange: (checked: boolean) => void;
  onChangeDirectories: () => void;
  onRescan: () => void;
  onRequestDelete: () => void;
  onRevealItem: (item: ScanItem) => void;
  onOpenTrash: () => void;
  onOpenLog: (path: string) => void;
  onRevealQuarantine: (operationId: string) => void;
  onRestoreQuarantine: (operationId: string) => void;
  onExportAudit: () => void;
}

export function ResultsWorkspace({
  scan,
  referenceRoot,
  referenceSourceType,
  rawRoot,
  includeSidecars,
  busy,
  blocked,
  filter,
  search,
  visibleItems,
  cleanableCount,
  selectedIds,
  selectedItems,
  selectedBytes,
  selectedRow,
  inspectorOpen,
  allVisibleSelected,
  someVisibleSelected,
  visibleDeleteCount,
  lastOperation,
  quarantineOperations,
  onFilterChange,
  onSearchChange,
  onToggleItem,
  onToggleVisibleItems,
  onSelectRow,
  onInspectorOpenChange,
  onIncludeSidecarsChange,
  onChangeDirectories,
  onRescan,
  onRequestDelete,
  onRevealItem,
  onOpenTrash,
  onOpenLog,
  onRevealQuarantine,
  onRestoreQuarantine,
  onExportAudit,
}: ResultsWorkspaceProps) {
  const selectedBreakdown = selectionBreakdown(selectedItems);
  const audit = scan.mode === "auditReference";
  const referenceSourceLabel = referenceSourceType === "directory"
    ? audit ? "JPG 审计范围" : "JPG 只读参考"
    : referenceSourceType === "manifest"
      ? "保留文件清单"
      : "XMP 星级参考";
  const referenceCountLabel = referenceSourceType === "directory"
    ? "参考 JPG"
    : referenceSourceType === "manifest"
      ? "清单条目"
      : "达标 XMP";
  const visibleMatched = scan.items.filter((item) => item.matchStatus === "matched").length;
  const formatSummary = Object.entries(rawFormatCounts(scan.items))
    .sort(([left], [right]) => left.localeCompare(right))
    .map(([extension, count]) => `${extension} ${count}`)
    .join(" · ");
  const recoverableOperations = quarantineOperations.filter(
    (operation) => operation.recoverable > 0 && operation.operationId !== lastOperation?.operationId,
  );

  return (
    <main className="review-view">
      <section className="review-source-bar" aria-label="本次扫描目录">
        <div className="source-summary">
          <div><FileImage aria-hidden="true" size={17} /><span><small>{referenceSourceLabel}</small><strong title={referenceRoot}>{referenceRoot}</strong></span></div>
          <div><FolderOpen aria-hidden="true" size={17} /><span><small>{audit ? "RAW 只读参照" : "RAW 处理范围"}</small><strong title={rawRoot}>{rawRoot}</strong></span></div>
        </div>
        <div className="source-actions">
          <button className="secondary-command" type="button" onClick={onChangeDirectories} disabled={busy}>更换目录或设置</button>
          <button className="primary-command" type="button" onClick={onRescan} disabled={busy}><RefreshCw aria-hidden="true" size={17} />重新扫描</button>
        </div>
      </section>

      <section className="summary-band" aria-label="扫描汇总">
        <div><span>{referenceCountLabel}</span><strong>{scan.referenceFiles}</strong></div>
        <div><span title={formatSummary}>扫描 RAW{formatSummary ? ` · ${formatSummary}` : ""}</span><strong>{scan.rawFiles}</strong></div>
        <div className="summary-ok"><span>{audit ? "有 RAW 的 JPG" : "已配对 RAW"}</span><strong>{scan.matched}</strong></div>
        <div className="summary-warning"><span>{audit ? "无 RAW 的 JPG" : "未配对 RAW"}</span><strong>{scan.unmatched}</strong></div>
        <div><span>{audit ? "审计模式" : "可处理文件"}</span><strong>{audit ? "只读" : cleanableCount}</strong></div>
        <div><span>{audit ? "文件写入" : "预计释放"}</span><strong>{audit ? "无" : formatBytes(reclaimableBytes(scan.items, includeSidecars))}</strong></div>
      </section>

      {blocked && (
        <section className="blocking-banner" role="alert">
          <AlertTriangle aria-hidden="true" size={19} />
          <div><strong>发现 {scan.duplicateReferenceKeys} 组重复匹配键，已暂停清理</strong><span>请整理当前参考源后重新扫描，避免对歧义结果执行批量操作。</span></div>
        </section>
      )}

      {!audit && lastOperation && (
        <section className="operation-receipt" aria-label="最近一次处理结果">
          <CheckCircle2 aria-hidden="true" size={18} />
          <div><strong>最近一次处理：成功 {lastOperation.succeeded}，失败 {lastOperation.failed}</strong><span>{lastOperation.destination === "quarantine" ? "文件位于 FramePair 隔离区，可直接恢复。" : "文件位于系统回收站，可根据操作日志核对。"}</span></div>
          <div className="receipt-actions">
            {lastOperation.destination === "trash" && <button className="secondary-command" type="button" onClick={onOpenTrash}><Trash2 aria-hidden="true" size={16} />打开回收站</button>}
            {lastOperation.operationId && <button className="secondary-command" type="button" onClick={() => onRevealQuarantine(lastOperation.operationId!)}><ArchiveRestore aria-hidden="true" size={16} />打开隔离目录</button>}
            {lastOperation.operationId && <button className="secondary-command" type="button" onClick={() => onRestoreQuarantine(lastOperation.operationId!)}><RotateCcw aria-hidden="true" size={16} />恢复本次文件</button>}
            {lastOperation.logPath && <button className="secondary-command" type="button" onClick={() => onOpenLog(lastOperation.logPath!)}><History aria-hidden="true" size={16} />查看日志</button>}
          </div>
        </section>
      )}

      {!audit && recoverableOperations.length > 0 && (
        <section className="quarantine-history" aria-label="可恢复的隔离操作">
          <div className="quarantine-history-heading">
            <ArchiveRestore aria-hidden="true" size={18} />
            <div><strong>隔离历史</strong><span>{recoverableOperations.length} 次操作仍有文件可以恢复</span></div>
          </div>
          <div className="quarantine-operation-list">
            {recoverableOperations.slice(0, 3).map((operation) => (
              <div key={operation.operationId}>
                <span><strong>{formatDate(operation.createdAtMs)}</strong><small>{operation.recoverable} / {operation.moved} 个文件可恢复</small></span>
                <button className="icon-button" type="button" onClick={() => onRevealQuarantine(operation.operationId)} aria-label="打开隔离目录" title="打开隔离目录"><FolderOpen aria-hidden="true" size={16} /></button>
                <button className="secondary-command" type="button" onClick={() => onRestoreQuarantine(operation.operationId)}><RotateCcw aria-hidden="true" size={15} />恢复</button>
              </div>
            ))}
          </div>
        </section>
      )}

      <div className={inspectorOpen && selectedRow ? "workspace with-inspector" : "workspace"}>
        <section className="results-pane" aria-label="扫描结果">
          <div className="results-toolbar">
            <div className="segment-control" role="tablist" aria-label="结果过滤">
              {([
                ["unmatched", `${audit ? "无 RAW 的 JPG" : "未配对 RAW"} ${scan.unmatched}`],
                ["matched", `已配对 ${visibleMatched}`],
                ["all", `全部可见 ${scan.items.length - (!includeSidecars ? scan.sidecars : 0)}`],
              ] as const).map(([value, label]) => (
                <button key={value} type="button" role="tab" aria-selected={filter === value} onClick={() => onFilterChange(value)}>
                  {label}
                </button>
              ))}
            </div>
            <div className="toolbar-actions">
              {!audit && <label className="inline-option">
                <input type="checkbox" checked={includeSidecars} onChange={(event) => onIncludeSidecarsChange(event.target.checked)} disabled={busy} />
                包含 XMP
              </label>}
              <label className="search-field">
                <Search aria-hidden="true" size={16} />
                <span className="sr-only">搜索路径或匹配参考</span>
                <input value={search} onChange={(event) => onSearchChange(event.target.value)} placeholder={audit ? "搜索 JPG 路径或 RAW" : "搜索 RAW 路径或 JPG"} />
              </label>
              <button
                className="icon-button"
                type="button"
                onClick={() => onInspectorOpenChange(!inspectorOpen)}
                disabled={!selectedRow}
                aria-label={inspectorOpen ? "隐藏文件详情" : "显示文件详情"}
                title={inspectorOpen ? "隐藏文件详情" : "显示文件详情"}
              >
                {inspectorOpen ? <PanelRightClose aria-hidden="true" size={18} /> : <PanelRightOpen aria-hidden="true" size={18} />}
              </button>
            </div>
          </div>

          <div className="table-region">
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
                    {audit ? <ShieldCheck aria-hidden="true" size={16} /> : <SelectionCheckbox
                      checked={allVisibleSelected}
                      indeterminate={someVisibleSelected}
                      onChange={onToggleVisibleItems}
                      disabled={visibleDeleteCount === 0 || busy || blocked}
                    />}
                  </th>
                  <th>处理建议</th>
                  <th>文件与判定依据</th>
                  <th>类型</th>
                  <th className="numeric">大小</th>
                </tr>
              </thead>
              <tbody>
                {visibleItems.map((item) => (
                  <tr key={item.id} className={selectedRow?.id === item.id ? "is-active" : ""}>
                    <td>
                      {!audit && item.matchStatus === "unmatched" ? (
                        <input
                          type="checkbox"
                          checked={selectedIds.has(item.id)}
                          onChange={() => onToggleItem(item)}
                          disabled={busy || blocked}
                          aria-label={`选择 ${item.relativePath}`}
                        />
                      ) : item.matchStatus === "matched"
                        ? <Check aria-hidden="true" className="keep-check" size={17} />
                        : <AlertTriangle aria-hidden="true" className="audit-warning" size={17} />}
                    </td>
                    <td>
                      <span className={`row-status status-${item.matchStatus}`}>
                        {item.matchStatus === "matched" ? <CheckCircle2 aria-hidden="true" size={16} /> : <AlertTriangle aria-hidden="true" size={16} />}
                        {item.matchStatus === "matched" ? "已配对" : audit ? "缺少 RAW" : "可清理"}
                      </span>
                    </td>
                    <td>
                      <button className="file-path-button" type="button" onClick={() => onSelectRow(item.id)} title={item.relativePath}>
                        <strong>{item.relativePath}</strong>
                        <span>{decisionReason(item)}</span>
                      </button>
                    </td>
                    <td>{item.kind === "raw" ? <><FileImage aria-hidden="true" size={15} />RAW</> : item.kind === "reference" ? <><FileImage aria-hidden="true" size={15} />JPG</> : <><FileCode2 aria-hidden="true" size={15} />XMP</>}</td>
                    <td className="numeric">{formatBytes(item.sizeBytes)}</td>
                  </tr>
                ))}
              </tbody>
            </table>
            {visibleItems.length === 0 && (
              <div className="empty-state compact">
                {filter === "unmatched" && scan.unmatched === 0 ? <CheckCircle2 aria-hidden="true" size={30} /> : <Search aria-hidden="true" size={28} />}
                <strong>{filter === "unmatched" && scan.unmatched === 0 ? (audit ? "没有缺少 RAW 的 JPG" : "没有待清理文件") : "没有匹配结果"}</strong>
                <span>{filter === "unmatched" && scan.unmatched === 0 ? (audit ? "所有 JPG 都找到了对应 RAW" : "所有 RAW 都找到了对应 JPG") : "尝试调整筛选或搜索条件"}</span>
              </div>
            )}
          </div>

          {audit ? <div className="action-bar audit-action-bar">
            <div className="selection-summary"><strong>只读审计完成</strong><span>{scan.unmatched} 个 JPG 没有对应 RAW</span></div>
            <div className="action-safety"><ShieldCheck aria-hidden="true" size={16} />不会修改 JPG 或 RAW 文件</div>
            <button className="primary-command" type="button" onClick={onExportAudit} disabled={busy || scan.unmatched === 0}>
              <FileCode2 aria-hidden="true" size={17} />导出未配对清单
            </button>
          </div> : <div className="action-bar">
            <div className="selection-summary">
              <strong>已选 {selectedBreakdown.total} / {cleanableCount} 个文件</strong>
              <span>{selectedBreakdown.raw} RAW · {selectedBreakdown.sidecar} XMP · {formatBytes(selectedBytes)}</span>
            </div>
            <div className="action-safety"><ShieldCheck aria-hidden="true" size={16} />仅处理选中文件，去向将在最终确认时选择</div>
            <button className="danger-command" type="button" onClick={onRequestDelete} disabled={busy || blocked || selectedItems.length === 0}>
              <Trash2 aria-hidden="true" size={17} />复核并执行清理
            </button>
          </div>}
        </section>

        {inspectorOpen && selectedRow && (
          <aside className="inspector" aria-label="文件详情">
            <div className="inspector-header">
              <h2>文件详情</h2>
              <button className="icon-button" type="button" onClick={() => onInspectorOpenChange(false)} aria-label="关闭文件详情" title="关闭文件详情"><PanelRightClose aria-hidden="true" size={18} /></button>
            </div>
            <section className={`decision-panel decision-${selectedRow.matchStatus}`}>
              {selectedRow.matchStatus === "matched" ? <CheckCircle2 aria-hidden="true" size={18} /> : <AlertTriangle aria-hidden="true" size={18} />}
              <div><strong>{selectedRow.matchStatus === "matched" ? "已配对" : audit ? "缺少对应 RAW" : "进入清理候选"}</strong><span>{decisionReason(selectedRow)}</span></div>
            </section>
            <section className="file-detail">
              <dl>
                <div><dt>文件</dt><dd>{selectedRow.fileName}</dd></div>
                <div><dt>相对路径</dt><dd>{selectedRow.relativePath}</dd></div>
                <div><dt>大小</dt><dd>{formatBytes(selectedRow.sizeBytes)}</dd></div>
                <div><dt>修改时间</dt><dd>{formatDate(selectedRow.modifiedMs)}</dd></div>
                <div><dt>{selectedRow.kind === "reference" ? "对应 RAW" : "对应 JPG"}</dt><dd>{selectedRow.matchedPath ?? "未找到"}</dd></div>
              </dl>
              <button className="secondary-command full-width-command" type="button" onClick={() => onRevealItem(selectedRow)}><FolderOpen aria-hidden="true" size={16} />在文件管理器中显示</button>
            </section>
            <section className="matching-note">
              <ListChecks aria-hidden="true" size={17} />
              <div><strong>本次匹配规则</strong><span>相对路径 + 文件名，{scan.duplicateReferenceKeys > 0 ? "存在重复键" : "未发现歧义"}</span></div>
            </section>
          </aside>
        )}
      </div>
    </main>
  );
}

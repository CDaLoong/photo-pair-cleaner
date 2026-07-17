import {
  AlertTriangle,
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
  Search,
  ShieldCheck,
  Trash2,
} from "lucide-react";
import { useEffect, useRef } from "react";
import type { DeleteSummary, FilterMode, ScanItem, ScanSummary } from "../types";
import {
  decisionReason,
  formatBytes,
  formatDate,
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
  lastOperation: DeleteSummary | null;
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
}

export function ResultsWorkspace({
  scan,
  referenceRoot,
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
}: ResultsWorkspaceProps) {
  const selectedBreakdown = selectionBreakdown(selectedItems);
  const visibleKeeps = scan.items.filter((item) => item.status === "keep").length;

  return (
    <main className="review-view">
      <section className="review-source-bar" aria-label="本次扫描目录">
        <div className="source-summary">
          <div><FileImage aria-hidden="true" size={17} /><span><small>JPG 只读参考</small><strong title={referenceRoot}>{referenceRoot}</strong></span></div>
          <div><FolderOpen aria-hidden="true" size={17} /><span><small>RAW 处理范围</small><strong title={rawRoot}>{rawRoot}</strong></span></div>
        </div>
        <div className="source-actions">
          <button className="secondary-command" type="button" onClick={onChangeDirectories} disabled={busy}>更换目录或设置</button>
          <button className="primary-command" type="button" onClick={onRescan} disabled={busy}><RefreshCw aria-hidden="true" size={17} />重新扫描</button>
        </div>
      </section>

      <section className="summary-band" aria-label="扫描汇总">
        <div><span>参考 JPG</span><strong>{scan.referenceFiles}</strong></div>
        <div><span>扫描 RAW</span><strong>{scan.rawFiles}</strong></div>
        <div className="summary-ok"><span>已配对 RAW</span><strong>{scan.matchedRaws}</strong></div>
        <div className="summary-warning"><span>未配对 RAW</span><strong>{scan.missingRaws}</strong></div>
        <div><span>可处理文件</span><strong>{cleanableCount}</strong></div>
        <div><span>预计释放</span><strong>{formatBytes(reclaimableBytes(scan.items, includeSidecars))}</strong></div>
      </section>

      {blocked && (
        <section className="blocking-banner" role="alert">
          <AlertTriangle aria-hidden="true" size={19} />
          <div><strong>发现 {scan.duplicateReferenceKeys} 组重复匹配键，已暂停清理</strong><span>请整理 JPG 参考目录后重新扫描，避免对歧义结果执行批量操作。</span></div>
        </section>
      )}

      {lastOperation && (
        <section className="operation-receipt" aria-label="最近一次处理结果">
          <CheckCircle2 aria-hidden="true" size={18} />
          <div><strong>最近一次处理：成功 {lastOperation.succeeded}，失败 {lastOperation.failed}</strong><span>文件位于系统回收站，可根据操作日志核对。</span></div>
          <div className="receipt-actions">
            <button className="secondary-command" type="button" onClick={onOpenTrash}><Trash2 aria-hidden="true" size={16} />打开回收站</button>
            {lastOperation.logPath && <button className="secondary-command" type="button" onClick={() => onOpenLog(lastOperation.logPath!)}><History aria-hidden="true" size={16} />查看日志</button>}
          </div>
        </section>
      )}

      <div className={inspectorOpen && selectedRow ? "workspace with-inspector" : "workspace"}>
        <section className="results-pane" aria-label="扫描结果">
          <div className="results-toolbar">
            <div className="segment-control" role="tablist" aria-label="结果过滤">
              {([
                ["delete", `待清理文件 ${cleanableCount}`],
                ["keep", `已配对 RAW ${visibleKeeps}`],
                ["all", `全部可见 ${scan.items.length - (!includeSidecars ? scan.sidecars : 0)}`],
              ] as const).map(([value, label]) => (
                <button key={value} type="button" role="tab" aria-selected={filter === value} onClick={() => onFilterChange(value)}>
                  {label}
                </button>
              ))}
            </div>
            <div className="toolbar-actions">
              <label className="inline-option">
                <input type="checkbox" checked={includeSidecars} onChange={(event) => onIncludeSidecarsChange(event.target.checked)} disabled={busy} />
                包含 XMP
              </label>
              <label className="search-field">
                <Search aria-hidden="true" size={16} />
                <span className="sr-only">搜索路径或匹配参考</span>
                <input value={search} onChange={(event) => onSearchChange(event.target.value)} placeholder="搜索路径或 JPG" />
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
                    <SelectionCheckbox
                      checked={allVisibleSelected}
                      indeterminate={someVisibleSelected}
                      onChange={onToggleVisibleItems}
                      disabled={visibleDeleteCount === 0 || busy || blocked}
                    />
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
                      {item.status === "delete" ? (
                        <input
                          type="checkbox"
                          checked={selectedIds.has(item.id)}
                          onChange={() => onToggleItem(item)}
                          disabled={busy || blocked}
                          aria-label={`选择 ${item.relativePath}`}
                        />
                      ) : <Check aria-hidden="true" className="keep-check" size={17} />}
                    </td>
                    <td>
                      <span className={`row-status status-${item.status}`}>
                        {item.status === "keep" ? <CheckCircle2 aria-hidden="true" size={16} /> : <AlertTriangle aria-hidden="true" size={16} />}
                        {item.status === "keep" ? "保留" : "可清理"}
                      </span>
                    </td>
                    <td>
                      <button className="file-path-button" type="button" onClick={() => onSelectRow(item.id)} title={item.relativePath}>
                        <strong>{item.relativePath}</strong>
                        <span>{decisionReason(item)}</span>
                      </button>
                    </td>
                    <td>{item.kind === "raw" ? <><FileImage aria-hidden="true" size={15} />RAW</> : <><FileCode2 aria-hidden="true" size={15} />XMP</>}</td>
                    <td className="numeric">{formatBytes(item.sizeBytes)}</td>
                  </tr>
                ))}
              </tbody>
            </table>
            {visibleItems.length === 0 && (
              <div className="empty-state compact">
                {filter === "delete" && cleanableCount === 0 ? <CheckCircle2 aria-hidden="true" size={30} /> : <Search aria-hidden="true" size={28} />}
                <strong>{filter === "delete" && cleanableCount === 0 ? "没有待清理文件" : "没有匹配结果"}</strong>
                <span>{filter === "delete" && cleanableCount === 0 ? "所有 RAW 都找到了对应 JPG" : "尝试调整筛选或搜索条件"}</span>
              </div>
            )}
          </div>

          <div className="action-bar">
            <div className="selection-summary">
              <strong>已选 {selectedBreakdown.total} / {cleanableCount} 个文件</strong>
              <span>{selectedBreakdown.raw} RAW · {selectedBreakdown.sidecar} XMP · {formatBytes(selectedBytes)}</span>
            </div>
            <div className="action-safety"><ShieldCheck aria-hidden="true" size={16} />仅选中文件会被移入系统回收站</div>
            <button className="danger-command" type="button" onClick={onRequestDelete} disabled={busy || blocked || selectedItems.length === 0}>
              <Trash2 aria-hidden="true" size={17} />复核并移入回收站
            </button>
          </div>
        </section>

        {inspectorOpen && selectedRow && (
          <aside className="inspector" aria-label="文件详情">
            <div className="inspector-header">
              <h2>文件详情</h2>
              <button className="icon-button" type="button" onClick={() => onInspectorOpenChange(false)} aria-label="关闭文件详情" title="关闭文件详情"><PanelRightClose aria-hidden="true" size={18} /></button>
            </div>
            <section className={`decision-panel decision-${selectedRow.status}`}>
              {selectedRow.status === "keep" ? <CheckCircle2 aria-hidden="true" size={18} /> : <AlertTriangle aria-hidden="true" size={18} />}
              <div><strong>{selectedRow.status === "keep" ? "建议保留" : "进入清理候选"}</strong><span>{decisionReason(selectedRow)}</span></div>
            </section>
            <section className="file-detail">
              <dl>
                <div><dt>文件</dt><dd>{selectedRow.fileName}</dd></div>
                <div><dt>相对路径</dt><dd>{selectedRow.relativePath}</dd></div>
                <div><dt>大小</dt><dd>{formatBytes(selectedRow.sizeBytes)}</dd></div>
                <div><dt>修改时间</dt><dd>{formatDate(selectedRow.modifiedMs)}</dd></div>
                <div><dt>对应 JPG</dt><dd>{selectedRow.matchedReference ?? "未找到"}</dd></div>
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

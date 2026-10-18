import { useMemo, useRef, useState } from 'react';
import {
  createColumnHelper,
  createSortedRowModel,
  rowSortingFeature,
  sortFns,
  tableFeatures,
  useTable,
  type SortingState,
} from '@tanstack/react-table';
import { useVirtualizer } from '@tanstack/react-virtual';

import * as ipc from '../ipc';
import { filterIssues, useScanStore } from '../state';
import {
  CONFIDENCE_LABELS,
  RULE_ORDER,
  RULE_TITLES,
  SEVERITY_LABELS,
  SEVERITY_ORDER,
  type Issue,
  type RuleId,
  type Severity,
} from '../types';
import { CellContextPanel } from './cell-context';

const ROW_HEIGHT = 44;

/**
 * TanStack Table v9 需要明確註冊使用到的 feature。
 * 只啟用「排序」（篩選由前端純函式 + Rust 查詢處理），並註冊全部內建 sortFn，
 * 讓欄位的預設 `sortFn: 'auto'` 能依值型別自動選擇（數值、文字、日期）。
 */
const features = tableFeatures({
  rowSortingFeature,
  sortedRowModel: createSortedRowModel(),
  sortFns,
});

/** 供測試重用（排序行為由 tests/table-sorting.test.tsx 驗證）。 */
export const TABLE_FEATURES = features;

const column = createColumnHelper<typeof features, Issue>();

/** 供測試重用。 */
export const ISSUE_COLUMNS = column.columns([
  column.accessor('severity', {
    header: '嚴重度',
    cell: (info) => (
      <span className={`pill severity-${info.getValue()}`}>
        {SEVERITY_LABELS[info.getValue() as Severity]}
      </span>
    ),
  }),
  column.accessor('ruleId', {
    header: '規則',
    cell: (info) => <span title={RULE_TITLES[info.getValue() as RuleId]}>{info.getValue()}</span>,
  }),
  column.accessor('sheetName', { header: '工作表' }),
  column.accessor('cellAddress', {
    header: '儲存格',
    cell: (info) => <span className="mono">{info.getValue()}</span>,
  }),
  column.accessor('message', { header: '說明' }),
  column.display({
    id: 'content',
    header: '公式或值',
    cell: (info) => (
      <span className="mono truncate">
        {info.row.original.formula ?? info.row.original.displayValue ?? ''}
      </span>
    ),
  }),
  column.accessor('confidence', {
    header: '信心',
    cell: (info) => CONFIDENCE_LABELS[info.getValue()],
  }),
]);

/** 問題清單：篩選、排序、虛擬列表、明細與匯出（SPEC §4.3）。 */
export function IssuesTable() {
  const payload = useScanStore((s) => s.payload);
  const scanId = useScanStore((s) => s.scanId);
  const filters = useScanStore((s) => s.filters);
  const setFilters = useScanStore((s) => s.setFilters);
  const resetFilters = useScanStore((s) => s.resetFilters);
  const selectedIssueId = useScanStore((s) => s.selectedIssueId);
  const selectIssue = useScanStore((s) => s.selectIssue);
  const setNotice = useScanStore((s) => s.setNotice);
  const [sorting, setSorting] = useState<SortingState>([]);
  const scrollRef = useRef<HTMLDivElement>(null);

  const allIssues = useMemo(() => payload?.issues ?? [], [payload]);
  const filtered = useMemo(() => filterIssues(allIssues, filters), [allIssues, filters]);

  const table = useTable({
    features,
    columns: ISSUE_COLUMNS,
    data: filtered,
    state: { sorting },
    onSortingChange: setSorting,
  });

  const rows = table.getRowModel().rows;

  const virtualizer = useVirtualizer({
    count: rows.length,
    getScrollElement: () => scrollRef.current,
    estimateSize: () => ROW_HEIGHT,
    overscan: 12,
  });

  const selectedIssue = allIssues.find((issue) => issue.id === selectedIssueId) ?? null;

  function toggleSeverity(severity: Severity) {
    const next = filters.severities.includes(severity)
      ? filters.severities.filter((value) => value !== severity)
      : [...filters.severities, severity];
    setFilters({ severities: next });
  }

  function toggleRule(rule: RuleId) {
    const next = filters.rules.includes(rule)
      ? filters.rules.filter((value) => value !== rule)
      : [...filters.rules, rule];
    setFilters({ rules: next });
  }

  async function exportIssues(format: 'csv' | 'json', scope: 'filtered' | 'all') {
    try {
      const result =
        scope === 'all' && scanId
          ? await ipc.exportAllIssues(scanId, format)
          : await ipc.exportFilteredIssues(
              scope === 'all' ? allIssues : rows.map((row) => row.original),
              format,
            );
      if (!result) return;
      setNotice(`已匯出 ${result.count} 筆問題至 ${result.path}`);
    } catch (raw) {
      setNotice(ipc.describeError(raw).message);
    }
  }

  return (
    <div className="issues-layout">
      <div className="issues-main">
        <div className="filter-bar">
          <div className="filter-group" role="group" aria-label="嚴重度">
            <span className="filter-label">嚴重度</span>
            {SEVERITY_ORDER.map((severity) => (
              <label key={severity} className="check">
                <input
                  type="checkbox"
                  checked={filters.severities.includes(severity)}
                  onChange={() => toggleSeverity(severity)}
                />
                {SEVERITY_LABELS[severity]}
              </label>
            ))}
          </div>

          <div className="filter-group" role="group" aria-label="規則">
            <span className="filter-label">規則</span>
            {RULE_ORDER.map((rule) => (
              <label key={rule} className="check" title={RULE_TITLES[rule]}>
                <input
                  type="checkbox"
                  checked={filters.rules.includes(rule)}
                  onChange={() => toggleRule(rule)}
                />
                {rule}
              </label>
            ))}
          </div>

          <div className="filter-group">
            <span className="filter-label">工作表</span>
            <select
              value={filters.sheetName ?? ''}
              onChange={(event) =>
                setFilters({ sheetName: event.target.value === '' ? null : event.target.value })
              }
            >
              <option value="">全部</option>
              {(payload?.sheets ?? []).map((sheet) => (
                <option key={sheet.name} value={sheet.name}>
                  {sheet.name}
                </option>
              ))}
            </select>
          </div>

          <div className="filter-group grow">
            <span className="filter-label">關鍵字</span>
            <input
              type="search"
              value={filters.keyword}
              placeholder="搜尋說明、公式、儲存格…"
              onChange={(event) => setFilters({ keyword: event.target.value })}
            />
          </div>

          <button type="button" className="btn btn-small" onClick={resetFilters}>
            清除篩選
          </button>
        </div>

        <div className="toolbar">
          <span className="count">
            顯示 {rows.length.toLocaleString('en-US')} / {allIssues.length.toLocaleString('en-US')} 筆問題
          </span>
          <div className="toolbar-actions">
            <button
              type="button"
              className="btn btn-small"
              onClick={() => void exportIssues('csv', 'filtered')}
              disabled={rows.length === 0}
            >
              匯出目前篩選 CSV
            </button>
            <button
              type="button"
              className="btn btn-small"
              onClick={() => void exportIssues('json', 'filtered')}
              disabled={rows.length === 0}
            >
              匯出目前篩選 JSON
            </button>
            <button
              type="button"
              className="btn btn-small"
              onClick={() => void exportIssues('csv', 'all')}
              disabled={allIssues.length === 0 && !scanId}
            >
              匯出全部 CSV
            </button>
            <button
              type="button"
              className="btn btn-small"
              onClick={() => void exportIssues('json', 'all')}
              disabled={allIssues.length === 0 && !scanId}
            >
              匯出全部 JSON
            </button>
          </div>
        </div>

        <div className="grid-head issues-grid-cols">
          {table.getHeaderGroups().map((headerGroup) =>
            headerGroup.headers.map((header) => (
              <button
                key={header.id}
                type="button"
                className="grid-head-cell"
                onClick={header.column.getToggleSortingHandler()}
                disabled={!header.column.getCanSort()}
              >
                <table.FlexRender header={header} />
                {header.column.getIsSorted() === 'asc' && ' ▲'}
                {header.column.getIsSorted() === 'desc' && ' ▼'}
              </button>
            )),
          )}
        </div>

        <div className="grid-body" ref={scrollRef}>
          {rows.length === 0 ? (
            <p className="empty">
              沒有符合目前篩選條件的問題。
              {allIssues.length > 0 && '（可試著清除篩選條件）'}
            </p>
          ) : (
            <div style={{ height: virtualizer.getTotalSize(), position: 'relative' }}>
              {virtualizer.getVirtualItems().map((virtualRow) => {
                const row = rows[virtualRow.index];
                const issue = row.original;
                return (
                  <div
                    key={row.id}
                    className={
                      issue.id === selectedIssueId
                        ? 'grid-row issues-grid-cols grid-row-selected'
                        : 'grid-row issues-grid-cols'
                    }
                    style={{ transform: `translateY(${virtualRow.start}px)`, height: ROW_HEIGHT }}
                    onClick={() => selectIssue(issue.id)}
                    role="button"
                    tabIndex={0}
                    onKeyDown={(event) => {
                      if (event.key === 'Enter' || event.key === ' ') {
                        event.preventDefault();
                        selectIssue(issue.id);
                      }
                    }}
                  >
                    {row.getAllCells().map((cell) => (
                      <div key={cell.id} className="grid-cell">
                        <table.FlexRender cell={cell} />
                      </div>
                    ))}
                  </div>
                );
              })}
            </div>
          )}
        </div>
      </div>

      <aside className="issues-detail">
        {selectedIssue ? (
          <>
            <div className="detail-summary">
              <h3>
                {selectedIssue.ruleId} · {RULE_TITLES[selectedIssue.ruleId]}
              </h3>
              <p className="muted">
                信心程度：{CONFIDENCE_LABELS[selectedIssue.confidence]}（啟發式結果，請人工確認）
              </p>
            </div>
            <CellContextPanel
              sheetName={selectedIssue.sheetName}
              address={selectedIssue.cellAddress}
            />
          </>
        ) : (
          <p className="detail muted">從左側清單選擇一筆問題，即可查看判斷依據與相鄰公式比較。</p>
        )}
      </aside>
    </div>
  );
}

/**
 * 驗證 TanStack Table v9 的 feature 設定可實際排序（v9 需明確註冊 feature 與 sortFns）。
 * 這裡不渲染虛擬列表（jsdom 無實際尺寸），只驗證表格模型本身。
 */

import { describe, expect, it, vi } from 'vitest';
import { cleanup, fireEvent, render, screen } from '@testing-library/react';
import { useTable, type SortingState } from '@tanstack/react-table';
import { useState } from 'react';

import { ISSUE_COLUMNS, TABLE_FEATURES } from '../src/ui/issues-table';
import type { Issue } from '../src/types';

vi.mock('../src/ipc', () => ({
  pickWorkbook: vi.fn(),
  startScan: vi.fn(),
  cancelScan: vi.fn(),
  clearAll: vi.fn(),
  exportFilteredIssues: vi.fn(),
  exportAllIssues: vi.fn(),
  getCellContext: vi.fn(),
  getFormulas: vi.fn(),
  getSheetWindow: vi.fn(),
  describeError: vi.fn(),
  onScanProgress: vi.fn(),
  onScanDone: vi.fn(),
  onScanError: vi.fn(),
  onFileDrop: vi.fn(),
  EVENTS: { progress: 'scan-progress', done: 'scan-done', error: 'scan-error' },
}));

const DATA: Issue[] = [
  {
    id: 'b',
    ruleId: 'F003',
    severity: 'medium',
    confidence: 'medium',
    sheetName: 'Sheet1',
    cellAddress: 'C6',
    message: '疑似公式模式不一致',
    evidence: ['證據'],
  },
  {
    id: 'a',
    ruleId: 'F001',
    severity: 'high',
    confidence: 'high',
    sheetName: 'Sheet1',
    cellAddress: 'B2',
    message: '錯誤值 #DIV/0!',
    evidence: ['證據'],
  },
  {
    id: 'c',
    ruleId: 'F006',
    severity: 'info',
    confidence: 'high',
    sheetName: 'Inputs',
    cellAddress: 'D2',
    message: '高風險函數',
    evidence: ['證據'],
  },
];

/** 以正式設定渲染表格模型（僅顯示欄位與第一欄值）。 */
function Harness({ data }: { data: Issue[] }) {
  const [sorting, setSorting] = useState<SortingState>([]);
  const table = useTable({
    features: TABLE_FEATURES,
    columns: ISSUE_COLUMNS,
    data,
    state: { sorting },
    onSortingChange: setSorting,
  });

  return (
    <div>
      <div data-testid="headers">
        {table.getHeaderGroups().map((group) =>
          group.headers.map((header) => (
            <button
              key={header.id}
              type="button"
              data-testid={`sort-${header.column.id}`}
              onClick={header.column.getToggleSortingHandler()}
              disabled={!header.column.getCanSort()}
            >
              <table.FlexRender header={header} />
              {header.column.getIsSorted() === 'asc' ? ' ▲' : ''}
              {header.column.getIsSorted() === 'desc' ? ' ▼' : ''}
            </button>
          )),
        )}
      </div>
      <ul data-testid="rows">
        {table.getRowModel().rows.map((row) => (
          <li key={row.id} data-testid="row">
            {row.original.id}:{row.original.cellAddress}
          </li>
        ))}
      </ul>
    </div>
  );
}

function rowIds(): string[] {
  return screen
    .getAllByTestId('row')
    .map((element) => element.textContent?.split(':')[0] ?? '');
}

describe('問題清單表格（TanStack Table v9）', () => {
  it('未排序時維持輸入順序', () => {
    render(<Harness data={DATA} />);
    expect(rowIds()).toEqual(['b', 'a', 'c']);
    cleanup();
  });

  it('點擊「儲存格」欄可升冪／降冪排序', () => {
    render(<Harness data={DATA} />);

    fireEvent.click(screen.getByTestId('sort-cellAddress'));
    expect(rowIds()).toEqual(['a', 'b', 'c']); // B2 < C6 < D2
    expect(screen.getByTestId('sort-cellAddress').textContent).toContain('▲');

    fireEvent.click(screen.getByTestId('sort-cellAddress'));
    expect(rowIds()).toEqual(['c', 'b', 'a']);
    expect(screen.getByTestId('sort-cellAddress').textContent).toContain('▼');

    cleanup();
  });

  it('可依工作表排序（相同鍵維持原順序）', () => {
    render(<Harness data={DATA} />);

    // 輸入順序為 b, a, c；依工作表升冪後 Inputs(c) 在前，
    // 兩筆 Sheet1（b、a）為相同鍵，維持原本相對順序 → c, b, a
    fireEvent.click(screen.getByTestId('sort-sheetName'));
    expect(rowIds()).toEqual(['c', 'b', 'a']);
    cleanup();
  });

  it('顯示欄位的標題為「公式或值」且不可排序', () => {
    render(<Harness data={DATA} />);
    const contentHeader = screen.getByTestId('sort-content');
    expect(contentHeader.textContent).toContain('公式或值');
    expect(contentHeader).toBeDisabled();
    cleanup();
  });
});

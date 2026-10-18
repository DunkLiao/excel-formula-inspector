import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { act, cleanup, fireEvent, render, screen, waitFor } from '@testing-library/react';

import * as ipc from '../src/ipc';
import { EMPTY_FILTERS, useScanStore } from '../src/state';
import type { Issue, ScanErrorEvent, ScanPayload } from '../src/types';
import App from '../src/App';
import { UploadPanel } from '../src/ui/upload';
import { Dashboard } from '../src/ui/dashboard';
import { Header } from '../src/ui/header';

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
  describeError: (error: unknown) =>
    error && typeof error === 'object' && 'code' in error
      ? error
      : { code: 'UNKNOWN', message: String(error) },
  onScanProgress: vi.fn(),
  onScanDone: vi.fn(),
  onScanError: vi.fn(),
  onFileDrop: vi.fn(),
  EVENTS: { progress: 'scan://progress', done: 'scan://done', error: 'scan://error' },
}));

const mockedIpc = vi.mocked(ipc);

function issue(partial: Partial<Issue> & { id: string }): Issue {
  return {
    ruleId: 'F001',
    severity: 'high',
    confidence: 'high',
    sheetName: 'Sheet1',
    cellAddress: 'A1',
    message: '錯誤值',
    evidence: ['證據'],
    ...partial,
  };
}

const PAYLOAD: ScanPayload = {
  scanId: 'scan-test',
  fileName: 'sample.xlsx',
  fileSize: 31_457,
  summary: {
    sheetCount: 2,
    scannedCellCount: 120,
    formulaCount: 10,
    constantCount: 110,
    issuesBySeverity: { high: 1, medium: 1, low: 0, info: 1 },
    durationMs: 42,
    warnings: ['測試警告'],
  },
  sheets: [
    {
      index: 0,
      name: 'Inputs',
      usedRange: 'A1:B7',
      rowCount: 7,
      colCount: 2,
      formulaCount: 0,
      constantCount: 14,
      errorCount: 0,
      issueCount: 1,
    },
    {
      index: 1,
      name: 'Report',
      usedRange: 'A1:B11',
      rowCount: 11,
      colCount: 2,
      formulaCount: 10,
      constantCount: 2,
      errorCount: 1,
      issueCount: 2,
    },
  ],
  issues: [
    issue({ id: 'i1', ruleId: 'F001', severity: 'high', cellAddress: 'B2' }),
    issue({ id: 'i2', ruleId: 'F003', severity: 'medium', cellAddress: 'C6', message: '疑似公式模式不一致' }),
    issue({ id: 'i3', ruleId: 'F006', severity: 'info', cellAddress: 'B3', message: '公式包含高風險函數：INDIRECT' }),
  ],
  issuesTruncated: false,
};

beforeEach(() => {
  vi.clearAllMocks();
  delete document.documentElement.dataset.theme;
  useScanStore.setState({
    status: 'idle',
    scanId: null,
    fileName: null,
    fileSize: 0,
    progress: null,
    payload: null,
    error: null,
    pendingLargeFile: null,
    notice: null,
    filters: EMPTY_FILTERS,
    tab: 'summary',
    theme: 'light',
    selectedIssueId: null,
    selectedCell: null,
    listenersReady: false,
    lastAttemptName: null,
  });
});

afterEach(cleanup);

describe('UploadPanel', () => {
  it('顯示隱私聲明與支援格式', () => {
    render(<UploadPanel />);
    expect(screen.getByText('匯入 Excel 工作簿')).toBeInTheDocument();
    expect(screen.getByText(/不會發出任何網路連線/)).toBeInTheDocument();
    expect(screen.getByText(/\.xlsx 與 \.xlsm/)).toBeInTheDocument();
  });

  it('選擇檔案後呼叫 startScan', async () => {
    mockedIpc.pickWorkbook.mockResolvedValue('C:\\tmp\\sample.xlsx');
    mockedIpc.startScan.mockResolvedValue('scan-1');
    render(<UploadPanel />);

    fireEvent.click(screen.getByRole('button', { name: '選擇檔案' }));

    await waitFor(() => {
      expect(mockedIpc.startScan).toHaveBeenCalledWith('C:\\tmp\\sample.xlsx', false);
    });
    expect(useScanStore.getState().scanId).toBe('scan-1');
    expect(useScanStore.getState().status).toBe('scanning');
  });

  it('超過 20 MB 時先要求確認，確認後才以實驗性模式啟動', async () => {
    mockedIpc.pickWorkbook.mockResolvedValue('D:\\big\\huge.xlsx');
    mockedIpc.startScan.mockRejectedValueOnce({
      code: 'LARGE_FILE_NEEDS_CONFIRMATION',
      message: '檔案大小 30.0 MB 超過預設上限 20 MB。',
      extra: { kind: 'size', sizeBytes: 31_457_280 },
    });
    mockedIpc.startScan.mockResolvedValueOnce('scan-2');

    render(<UploadPanel />);
    fireEvent.click(screen.getByRole('button', { name: '選擇檔案' }));

    const confirmButton = await screen.findByRole('button', { name: '以實驗性模式繼續' });
    expect(screen.getByText(/30\.0 MB/)).toBeInTheDocument();
    // 尚未以實驗性模式啟動
    expect(mockedIpc.startScan).toHaveBeenCalledTimes(1);

    fireEvent.click(confirmButton);

    await waitFor(() => {
      expect(mockedIpc.startScan).toHaveBeenCalledWith('D:\\big\\huge.xlsx', true);
    });
    expect(useScanStore.getState().status).toBe('scanning');
  });

  it('顯示後端回報的錯誤訊息與錯誤碼', () => {
    useScanStore.setState({
      status: 'error',
      error: { code: 'CORRUPT', message: '檔案損毀或格式不正確，無法解析。' },
    });
    render(<UploadPanel />);
    expect(screen.getByText('檔案損毀或格式不正確，無法解析。')).toBeInTheDocument();
    expect(screen.getByText(/CORRUPT/)).toBeInTheDocument();
  });

  it('錯誤訊息會顯示失敗的檔名，並針對損毀檔提供提示', () => {
    useScanStore.setState({
      status: 'error',
      lastAttemptName: 'UAT-09-large-workbook-over-20MB.xlsx',
      error: { code: 'CORRUPT', message: '檔案損毀或格式不正確，無法解析。' },
    });
    render(<UploadPanel />);

    expect(screen.getByText(/UAT-09-large-workbook-over-20MB\.xlsx/)).toBeInTheDocument();
    expect(screen.getByText(/可能不是標準的 \.xlsx/)).toBeInTheDocument();
  });

  it('掃描失敗時清掉實驗性模式通知，避免與錯誤訊息互相矛盾', async () => {
    let onError: ((event: ScanErrorEvent) => void) | undefined;
    mockedIpc.onScanError.mockImplementation((handler: (event: ScanErrorEvent) => void) => {
      onError = handler;
      return Promise.resolve(() => {});
    });
    mockedIpc.onScanProgress.mockResolvedValue(() => {});
    mockedIpc.onScanDone.mockResolvedValue(() => {});
    mockedIpc.onFileDrop.mockResolvedValue(() => {});
    mockedIpc.startScan.mockResolvedValue('scan-x');

    await useScanStore.getState().init();
    await useScanStore
      .getState()
      .requestScan('D:\\UAT\\UAT-09-large-workbook-over-20MB.xlsx');

    // 模擬「已進入實驗性模式」之後才失敗
    useScanStore.setState({
      notice: '已啟動實驗性模式：大檔可能花費較長時間，且記憶體用量較高。',
    });
    expect(useScanStore.getState().lastAttemptName).toBe(
      'UAT-09-large-workbook-over-20MB.xlsx',
    );

    act(() => {
      onError?.({
        scanId: 'scan-x',
        code: 'CORRUPT',
        message: '檔案損毀或格式不正確，無法解析。',
      });
    });

    const state = useScanStore.getState();
    expect(state.status).toBe('error');
    expect(state.notice).toBeNull();
    expect(state.error?.code).toBe('CORRUPT');
  });
});

describe('Dashboard', () => {
  it('顯示 KPI 與警告', () => {
    useScanStore.setState({ status: 'done', payload: PAYLOAD });
    render(<Dashboard />);

    const kpiValue = (label: string) =>
      screen
        .getByText(label, { selector: '.kpi-label' })
        .parentElement?.querySelector('.kpi-value')?.textContent;

    expect(kpiValue('工作表')).toBe('2');
    expect(kpiValue('公式儲存格')).toBe('10');
    expect(kpiValue('錯誤儲存格')).toBe('1');
    expect(kpiValue('問題總數')).toBe('3');
    expect(screen.getByText(/測試警告/)).toBeInTheDocument();
    expect(screen.getByText(/不會重新計算公式/)).toBeInTheDocument();
  });

  it('點擊嚴重度可直接跳轉到問題清單並帶入篩選', () => {
    useScanStore.setState({ status: 'done', payload: PAYLOAD });
    render(<Dashboard />);

    fireEvent.click(screen.getByText('中').closest('button') as HTMLElement);
    expect(useScanStore.getState().filters.severities).toEqual(['medium']);
    expect(useScanStore.getState().tab).toBe('issues');
  });

  it('點擊規則列帶入規則篩選', () => {
    useScanStore.setState({ status: 'done', payload: PAYLOAD });
    render(<Dashboard />);

    fireEvent.click(screen.getByText(/F003/));
    expect(useScanStore.getState().filters.rules).toEqual(['F003']);
  });
});

describe('Header', () => {
  it('顯示本機處理標記與檔名', () => {
    useScanStore.setState({ status: 'done', fileName: 'sample.xlsx', fileSize: 31_457, payload: PAYLOAD });
    render(<Header />);
    expect(screen.getByText(/在此裝置本機處理/)).toBeInTheDocument();
    expect(screen.getByText(/sample\.xlsx/)).toBeInTheDocument();
  });

  it('掃描中提供取消按鈕', async () => {
    mockedIpc.cancelScan.mockResolvedValue(true);
    useScanStore.setState({
      status: 'scanning',
      scanId: 'scan-x',
      progress: { scanId: 'scan-x', phase: 'reading', done: 1, total: 2, label: 'Inputs' },
    });
    render(<Header />);

    fireEvent.click(screen.getByRole('button', { name: '取消' }));
    await waitFor(() => expect(mockedIpc.cancelScan).toHaveBeenCalledWith('scan-x'));
    expect(useScanStore.getState().status).toBe('idle');
  });

  it('清除資料會呼叫後端並回到初始狀態（AC12）', async () => {
    mockedIpc.clearAll.mockResolvedValue(1);
    useScanStore.setState({ status: 'done', scanId: 'scan-x', payload: PAYLOAD, fileName: 'sample.xlsx' });
    render(<Header />);

    fireEvent.click(screen.getByRole('button', { name: '清除資料' }));

    await waitFor(() => expect(mockedIpc.clearAll).toHaveBeenCalled());
    const state = useScanStore.getState();
    expect(state.status).toBe('idle');
    expect(state.payload).toBeNull();
    expect(state.scanId).toBeNull();
  });

  it('預設為淺色模式，可切換深色再切回淺色', () => {
    render(<Header />);

    expect(useScanStore.getState().theme).toBe('light');
    const lightButton = screen.getByRole('button', { name: /淺色/ });
    const darkButton = screen.getByRole('button', { name: /深色/ });
    expect(lightButton).toHaveAttribute('aria-pressed', 'true');
    expect(darkButton).toHaveAttribute('aria-pressed', 'false');

    fireEvent.click(darkButton);
    expect(useScanStore.getState().theme).toBe('dark');
    expect(screen.getByRole('button', { name: /深色/ })).toHaveAttribute('aria-pressed', 'true');

    fireEvent.click(screen.getByRole('button', { name: /淺色/ }));
    expect(useScanStore.getState().theme).toBe('light');
  });

  it('切換主題不會影響掃描狀態或篩選條件', () => {
    useScanStore.setState({ scanId: 'scan-keep', filters: { ...EMPTY_FILTERS, keyword: 'DIV' } });
    render(<Header />);

    fireEvent.click(screen.getByRole('button', { name: /深色/ }));

    const state = useScanStore.getState();
    expect(state.theme).toBe('dark');
    expect(state.scanId).toBe('scan-keep');
    expect(state.filters.keyword).toBe('DIV');
  });
});

describe('佈景主題', () => {
  it('App 會把主題套用到文件根節點，預設為淺色', async () => {
    render(<App />);

    await waitFor(() => expect(document.documentElement.dataset.theme).toBe('light'));

    fireEvent.click(screen.getByRole('button', { name: /深色/ }));
    await waitFor(() => expect(document.documentElement.dataset.theme).toBe('dark'));

    fireEvent.click(screen.getByRole('button', { name: /淺色/ }));
    await waitFor(() => expect(document.documentElement.dataset.theme).toBe('light'));
  });
});

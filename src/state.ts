/**
 * 前端狀態（Zustand，SPEC §7）。
 *
 * 只保存 UI 狀態與 Rust 回傳的結果；不做任何規則判斷。
 */

import { create } from 'zustand';

import * as ipc from './ipc';
import type {
  ErrorPayload,
  Issue,
  RuleId,
  ScanPayload,
  ScanProgress,
  Severity,
} from './types';

export type ScanStatus = 'idle' | 'scanning' | 'done' | 'error';
export type TabKey = 'summary' | 'issues' | 'formulas' | 'grid';
/** 佈景主題；預設淺色（SPEC §4.1）。僅存在於本次執行，不寫入任何儲存空間。 */
export type Theme = 'light' | 'dark';

export type Filters = {
  severities: Severity[];
  rules: RuleId[];
  sheetName: string | null;
  /** 全文關鍵字：比對說明、公式、儲存格、工作表。 */
  keyword: string;
};

export const EMPTY_FILTERS: Filters = {
  severities: [],
  rules: [],
  sheetName: null,
  keyword: '',
};

export type PendingLargeFile = {
  path: string;
  sizeBytes: number;
};

type ScanStore = {
  status: ScanStatus;
  scanId: string | null;
  fileName: string | null;
  fileSize: number;
  /** 最近一次嘗試匯入的檔名（失敗時顯示，讓使用者知道是哪個檔案出問題）。 */
  lastAttemptName: string | null;
  progress: ScanProgress | null;
  payload: ScanPayload | null;
  error: ErrorPayload | null;
  pendingLargeFile: PendingLargeFile | null;
  notice: string | null;
  filters: Filters;
  tab: TabKey;
  theme: Theme;
  selectedIssueId: string | null;
  selectedCell: { sheetName: string; address: string } | null;
  listenersReady: boolean;

  init: () => Promise<void>;
  requestScan: (path: string) => Promise<void>;
  confirmLargeScan: () => Promise<void>;
  cancelScan: () => Promise<void>;
  clearData: () => Promise<void>;
  setFilters: (patch: Partial<Filters>) => void;
  resetFilters: () => void;
  setTab: (tab: TabKey) => void;
  setTheme: (theme: Theme) => void;
  toggleTheme: () => void;
  selectIssue: (issueId: string | null) => void;
  selectCell: (sheetName: string, address: string) => void;
  setNotice: (message: string | null) => void;
};

export const useScanStore = create<ScanStore>((set, get) => ({
  status: 'idle',
  scanId: null,
  fileName: null,
  fileSize: 0,
  lastAttemptName: null,
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

  /** 註冊後端事件（僅一次）。任何註冊失敗都不應阻斷應用程式啟動。 */
  init: async () => {
    if (get().listenersReady) return;
    set({ listenersReady: true });

    const belongsToCurrentScan = (scanId: string) => {
      const current = get().scanId;
      return current === null || current === scanId;
    };

    try {
      await ipc.onScanProgress((event) => {
        if (!belongsToCurrentScan(event.scanId)) return;
        set({ progress: event, status: 'scanning' });
      });

      await ipc.onScanDone((payload) => {
        if (!belongsToCurrentScan(payload.scanId)) return;
        set({
          status: 'done',
          scanId: payload.scanId,
          fileName: payload.fileName,
          fileSize: payload.fileSize,
          payload,
          progress: null,
          error: null,
          filters: EMPTY_FILTERS,
          selectedIssueId: null,
          selectedCell: null,
          tab: 'summary',
        });
      });

      await ipc.onScanError((event) => {
        if (!belongsToCurrentScan(event.scanId)) return;
        // 掃描失敗時必須清掉「已啟動實驗性模式」之類的前置通知，
        // 否則錯誤訊息旁邊會留著互相矛盾的提示。
        set({
          status: 'error',
          progress: null,
          notice: null,
          error: { code: event.code, message: event.message, extra: event.extra },
        });
      });
    } catch {
      set({ notice: '無法註冊掃描事件，請重新啟動程式。' });
    }

    // 拖放匯入（僅接受 .xlsx / .xlsm）；若環境不支援則靜默略過，使用者仍可用「選擇檔案」。
    try {
      await ipc.onFileDrop(
        (path) => {
          void get().requestScan(path);
        },
        (reason) => set({ notice: reason }),
      );
    } catch {
      // 拖放不可用不影響其他功能。
    }
  },

  requestScan: async (path) => {
    set({
      status: 'scanning',
      scanId: null,
      fileName: null,
      fileSize: 0,
      lastAttemptName: baseName(path),
      payload: null,
      error: null,
      progress: null,
      pendingLargeFile: null,
      notice: null,
      selectedIssueId: null,
      selectedCell: null,
    });
    try {
      const scanId = await ipc.startScan(path, false);
      set({ scanId });
    } catch (raw) {
      const error = ipc.describeError(raw);
      if (error.code === 'LARGE_FILE_NEEDS_CONFIRMATION') {
        const sizeBytes =
          error.extra && error.extra.kind === 'size' ? error.extra.sizeBytes : 0;
        set({ status: 'idle', scanId: null, pendingLargeFile: { path, sizeBytes } });
        return;
      }
      set({ status: 'error', scanId: null, error });
    }
  },

  /** 使用者確認後以實驗性模式繼續（SPEC §8.2）。 */
  confirmLargeScan: async () => {
    const pending = get().pendingLargeFile;
    if (!pending) return;
    set({
      status: 'scanning',
      pendingLargeFile: null,
      error: null,
      progress: null,
      scanId: null,
      lastAttemptName: baseName(pending.path),
      notice: '已啟動實驗性模式：大檔可能花費較長時間，且記憶體用量較高。',
    });
    try {
      const scanId = await ipc.startScan(pending.path, true);
      set({ scanId });
    } catch (raw) {
      set({ status: 'error', scanId: null, error: ipc.describeError(raw) });
    }
  },

  cancelScan: async () => {
    const scanId = get().scanId;
    if (scanId) {
      await ipc.cancelScan(scanId);
    }
    set({
      status: 'idle',
      scanId: null,
      progress: null,
      pendingLargeFile: null,
      notice: '已取消掃描。',
    });
  },

  /** 「清除資料」：取消掃描並清除 Rust 與前端記憶體中的所有結果（AC12）。 */
  clearData: async () => {
    await ipc.clearAll();
    set({
      status: 'idle',
      scanId: null,
      fileName: null,
      fileSize: 0,
      lastAttemptName: null,
      progress: null,
      payload: null,
      error: null,
      pendingLargeFile: null,
      notice: '已清除載入的工作簿與檢查結果。',
      filters: EMPTY_FILTERS,
      tab: 'summary',
      selectedIssueId: null,
      selectedCell: null,
    });
  },

  setFilters: (patch) => set({ filters: { ...get().filters, ...patch } }),
  resetFilters: () => set({ filters: EMPTY_FILTERS }),
  setTab: (tab) => set({ tab }),
  setTheme: (theme) => set({ theme }),
  toggleTheme: () => set({ theme: get().theme === 'light' ? 'dark' : 'light' }),
  selectIssue: (selectedIssueId) => set({ selectedIssueId }),
  selectCell: (sheetName, address) =>
    set({ selectedCell: { sheetName, address }, tab: 'formulas' }),
  setNotice: (notice) => set({ notice }),
}));

/** 由路徑取出檔名（僅供顯示；不做任何檔案存取）。 */
export function baseName(path: string): string {
  const parts = path.split(/[\\/]/);
  return parts[parts.length - 1] || path;
}

/** 依篩選條件過濾問題（純函式，可單元測試）。 */
export function filterIssues(issues: Issue[], filters: Filters): Issue[] {
  const keyword = filters.keyword.trim().toLowerCase();
  return issues.filter((issue) => {
    if (filters.severities.length > 0 && !filters.severities.includes(issue.severity)) {
      return false;
    }
    if (filters.rules.length > 0 && !filters.rules.includes(issue.ruleId)) {
      return false;
    }
    if (filters.sheetName && issue.sheetName !== filters.sheetName) {
      return false;
    }
    if (keyword) {
      const haystack = [
        issue.message,
        issue.formula ?? '',
        issue.displayValue ?? '',
        issue.cellAddress,
        issue.sheetName,
        issue.ruleId,
        issue.evidence.join(' '),
      ]
        .join(' ')
        .toLowerCase();
      if (!haystack.includes(keyword)) return false;
    }
    return true;
  });
}

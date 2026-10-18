/**
 * 與 Rust 溝通的唯一入口（SPEC §7）。
 *
 * 前端**不**掛載 fs／http／shell plugin：檔案讀寫一律由 Rust command 完成，
 * 這裡只負責呼叫命令與接收事件。
 */

import { invoke } from '@tauri-apps/api/core';
import { listen, type UnlistenFn } from '@tauri-apps/api/event';
import { open, save } from '@tauri-apps/plugin-dialog';
import { getCurrentWebview } from '@tauri-apps/api/webview';

import type {
  CellContext,
  ErrorPayload,
  ExportFormat,
  ExportResult,
  FormulaPage,
  FormulaQuery,
  GridWindow,
  Issue,
  ScanErrorEvent,
  ScanPayload,
  ScanProgress,
} from './types';

export const EVENTS = {
  progress: 'scan://progress',
  done: 'scan://done',
  error: 'scan://error',
} as const;

/** 系統檔案選擇對話框；取消時回傳 null。 */
export async function pickWorkbook(): Promise<string | null> {
  const selected = await open({
    multiple: false,
    directory: false,
    title: '選擇 Excel 工作簿',
    filters: [{ name: 'Excel 工作簿', extensions: ['xlsx', 'xlsm'] }],
  });
  if (typeof selected === 'string') return selected;
  return null;
}

export async function startScan(path: string, allowLarge: boolean): Promise<string> {
  return invoke<string>('start_scan', { path, allowLarge });
}

export async function cancelScan(scanId: string): Promise<boolean> {
  return invoke<boolean>('cancel_scan', { scanId });
}

export async function getScanPayload(scanId: string): Promise<ScanPayload> {
  return invoke<ScanPayload>('get_scan_payload', { scanId });
}

export async function getIssues(scanId: string): Promise<Issue[]> {
  return invoke<Issue[]>('get_issues', { scanId });
}

export async function getFormulas(scanId: string, query: FormulaQuery): Promise<FormulaPage> {
  return invoke<FormulaPage>('get_formulas', { scanId, query });
}

export async function getSheetWindow(
  scanId: string,
  sheetName: string,
  startRow: number,
  startCol: number,
  rowCount: number,
  colCount: number,
): Promise<GridWindow> {
  return invoke<GridWindow>('get_sheet_window', {
    scanId,
    sheetName,
    startRow,
    startCol,
    rowCount,
    colCount,
  });
}

export async function getCellContext(
  scanId: string,
  sheetName: string,
  address: string,
  span = 3,
): Promise<CellContext> {
  return invoke<CellContext>('get_cell_context', { scanId, sheetName, address, span });
}

/** 匯出前端目前的篩選結果（使用者先選擇輸出路徑）。 */
export async function exportFilteredIssues(
  issues: Issue[],
  format: ExportFormat,
): Promise<ExportResult | null> {
  const path = await save({
    title: '匯出檢查結果',
    defaultPath: format === 'csv' ? 'issues.csv' : 'issues.json',
    filters: [
      {
        name: format === 'csv' ? 'CSV（支援 Excel 開啟）' : 'JSON',
        extensions: [format],
      },
    ],
  });
  if (!path) return null;
  return invoke<ExportResult>('export_issues', { request: { format, path, issues } });
}

/** 匯出完整結果（不受 IPC 傳輸上限影響，由 Rust 直接寫檔）。 */
export async function exportAllIssues(
  scanId: string,
  format: ExportFormat,
): Promise<ExportResult | null> {
  const path = await save({
    title: '匯出全部檢查結果',
    defaultPath: format === 'csv' ? 'issues-all.csv' : 'issues-all.json',
    filters: [
      {
        name: format === 'csv' ? 'CSV（支援 Excel 開啟）' : 'JSON',
        extensions: [format],
      },
    ],
  });
  if (!path) return null;
  return invoke<ExportResult>('export_from_scan', { scanId, format, path });
}

export async function clearScan(scanId: string): Promise<boolean> {
  return invoke<boolean>('clear_scan', { scanId });
}

export async function clearAll(): Promise<number> {
  return invoke<number>('clear_all');
}

export function onScanProgress(handler: (event: ScanProgress) => void): Promise<UnlistenFn> {
  return listen<ScanProgress>(EVENTS.progress, (event) => handler(event.payload));
}

export function onScanDone(handler: (payload: ScanPayload) => void): Promise<UnlistenFn> {
  return listen<ScanPayload>(EVENTS.done, (event) => handler(event.payload));
}

export function onScanError(handler: (error: ScanErrorEvent) => void): Promise<UnlistenFn> {
  return listen<ScanErrorEvent>(EVENTS.error, (event) => handler(event.payload));
}

/** 拖放檔案；僅接受 .xlsx / .xlsm。 */
export async function onFileDrop(
  handler: (path: string) => void,
  onRejected: (reason: string) => void,
): Promise<UnlistenFn> {
  const webview = getCurrentWebview();
  return webview.onDragDropEvent((event) => {
    if (event.payload.type !== 'drop') return;
    const paths = event.payload.paths ?? [];
    if (paths.length === 0) return;
    if (paths.length > 1) {
      onRejected('一次僅能匯入一個工作簿。');
      return;
    }
    const candidate = paths[0];
    const lower = candidate.toLowerCase();
    if (!lower.endsWith('.xlsx') && !lower.endsWith('.xlsm')) {
      onRejected('僅支援 .xlsx 與 .xlsm 檔案。');
      return;
    }
    handler(candidate);
  });
}

/** 將後端錯誤轉為可顯示文字（後端已提供中文訊息）。 */
export function describeError(error: unknown): ErrorPayload {
  if (error && typeof error === 'object' && 'code' in error && 'message' in error) {
    return error as ErrorPayload;
  }
  return {
    code: 'UNKNOWN',
    message: typeof error === 'string' ? error : '發生未預期的錯誤。',
  };
}

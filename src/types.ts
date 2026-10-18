/**
 * 對應 Rust 端 `src-tauri/src/model.rs` 的型別鏡像。
 * 僅供 UI 使用；所有規則邏輯都在 Rust，前端不重複實作。
 */

export type Severity = 'high' | 'medium' | 'low' | 'info';
export type Confidence = 'high' | 'medium' | 'low';
export type RuleId = 'F001' | 'F002' | 'F003' | 'F004' | 'F005' | 'F006';

export type RuleId2 = RuleId;

export const RULE_ORDER: RuleId[] = ['F001', 'F002', 'F003', 'F004', 'F005', 'F006'];
export const SEVERITY_ORDER: Severity[] = ['high', 'medium', 'low', 'info'];

export const RULE_TITLES: Record<RuleId, string> = {
  F001: 'Excel 錯誤值',
  F002: '無效參照',
  F003: '公式模式不一致',
  F004: '疑似硬編碼覆寫',
  F005: '疑似公式缺口',
  F006: '高風險函數提示',
};

export const SEVERITY_LABELS: Record<Severity, string> = {
  high: '高',
  medium: '中',
  low: '低',
  info: '提示',
};

export const CONFIDENCE_LABELS: Record<Confidence, string> = {
  high: '高',
  medium: '中',
  low: '低',
};

export type Issue = {
  id: string;
  ruleId: RuleId;
  severity: Severity;
  confidence: Confidence;
  sheetName: string;
  cellAddress: string;
  message: string;
  evidence: string[];
  formula?: string;
  displayValue?: string;
};

export type ScanSummary = {
  sheetCount: number;
  scannedCellCount: number;
  formulaCount: number;
  constantCount: number;
  issuesBySeverity: Record<Severity, number>;
  durationMs: number;
  warnings: string[];
};

export type SheetInfo = {
  index: number;
  name: string;
  usedRange?: string;
  rowCount: number;
  colCount: number;
  formulaCount: number;
  constantCount: number;
  errorCount: number;
  issueCount: number;
};

export type ScanPayload = {
  scanId: string;
  fileName: string;
  fileSize: number;
  summary: ScanSummary;
  sheets: SheetInfo[];
  issues: Issue[];
  issuesTruncated: boolean;
};

export type ValueKind =
  | 'empty'
  | 'text'
  | 'number'
  | 'bool'
  | 'dateTime'
  | 'duration'
  | 'error'
  | 'unsupported';

export const VALUE_KIND_LABELS: Record<ValueKind, string> = {
  empty: '空白',
  text: '文字',
  number: '數值',
  bool: '布林',
  dateTime: '日期/時間',
  duration: '期間',
  error: '錯誤',
  unsupported: '無法判讀',
};

export type GridCell = {
  row: number;
  col: number;
  address: string;
  display?: string;
  kind: ValueKind;
  formula?: string;
  /** 公式簽章（無法判讀的公式不會有值）。 */
  signature?: string;
  issueIds: string[];
};

export type GridWindow = {
  sheetName: string;
  startRow: number;
  startCol: number;
  rowCount: number;
  colCount: number;
  cells: GridCell[];
};

export type FormulaRecord = {
  sheetName: string;
  cellAddress: string;
  row: number;
  col: number;
  formula: string;
  displayValue?: string;
  kind: ValueKind;
  issueIds: string[];
  signature?: string;
};

export type FormulaPage = {
  total: number;
  offset: number;
  limit: number;
  items: FormulaRecord[];
};

export type FormulaQuery = {
  sheetName?: string;
  address?: string;
  text?: string;
  onlyIssues?: boolean;
  offset?: number;
  limit?: number;
};

export type CellContext = {
  sheetName: string;
  cell: GridCell;
  issues: Issue[];
  neighbours: GridCell[];
  dominantSignature?: string;
  refs: string[];
  unsupportedReason?: string;
};

export type ExportFormat = 'csv' | 'json';

export type ExportResult = {
  path: string;
  count: number;
};

export type ScanProgress = {
  scanId: string;
  phase: string;
  done: number;
  total: number;
  label?: string;
};

export type ErrorExtra =
  | { kind: 'size'; sizeBytes: number }
  | { kind: 'limit'; limit: number };

export type ErrorPayload = {
  code: string;
  message: string;
  extra?: ErrorExtra;
};

export type ScanErrorEvent = {
  scanId: string;
  code: string;
  message: string;
  extra?: ErrorExtra;
};

export const PHASE_LABELS: Record<string, string> = {
  reading: '讀取工作表',
  rules: '執行規則檢查',
};

/** 位址轉 0-based 行列（供網格定位使用）。 */
export function parseAddress(address: string): { row: number; col: number } | null {
  const match = /^\$?([A-Za-z]{1,3})\$?(\d{1,7})$/.exec(address.trim());
  if (!match) return null;
  const letters = match[1].toUpperCase();
  let col = 0;
  for (const char of letters) {
    col = col * 26 + (char.charCodeAt(0) - 64);
  }
  const row = Number(match[2]);
  if (row < 1) return null;
  return { row: row - 1, col: col - 1 };
}

/** 0-based 欄索引 → 欄字母。 */
export function columnLetters(col: number): string {
  let value = col;
  let out = '';
  for (;;) {
    out = String.fromCharCode(65 + (value % 26)) + out;
    if (value < 26) break;
    value = Math.floor(value / 26) - 1;
  }
  return out;
}

export function formatBytes(bytes: number): string {
  const mb = bytes / (1024 * 1024);
  if (mb >= 1) return `${mb.toFixed(1)} MB`;
  return `${Math.max(1, Math.round(bytes / 1024))} KB`;
}

export function formatDuration(milliseconds: number): string {
  if (milliseconds < 1000) return `${milliseconds} ms`;
  return `${(milliseconds / 1000).toFixed(2)} 秒`;
}

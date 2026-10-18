//! 掃描編排（SPEC §3、§7、§8.2）。
//!
//! - 解析與規則掃描在**獨立執行緒**執行，不阻塞 UI。
//! - 進度以 Tauri 事件回報；取消以 `AtomicBool` 旗標於安全點中止。
//! - 結果保存在 Rust 記憶體（`ScanStore`），前端只取需要的分頁／視窗，避免一次搬移整份工作簿。

use std::collections::{BTreeMap, HashMap};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Instant, SystemTime, UNIX_EPOCH};

use serde::Serialize;
use tauri::{AppHandle, Emitter};

use crate::errors::{ScanError, ScanResult};
use crate::formula_parser::{self, display_formula, format_address};
use crate::model::{
    CellContext, FormulaPage, FormulaQuery, FormulaRecord, GridCell, GridWindow, Issue, ScanPayload,
    ScanSummary, SheetInfo, ValueKind,
};
use crate::rules;
use crate::workbook_reader::{self, ProgressUpdate, ReadOptions, SheetData, WorkbookData};

/// 事件名稱（前端 `listen` 使用）。
pub const EVENT_PROGRESS: &str = "scan://progress";
pub const EVENT_DONE: &str = "scan://done";
pub const EVENT_ERROR: &str = "scan://error";

/// 單次 IPC 可攜帶的問題上限；超過時僅截斷傳輸，統計仍以完整結果計算。
pub const MAX_ISSUES_IN_PAYLOAD: usize = 20_000;

/// 網格視窗單次最大列／欄數，避免前端一次要求過大範圍。
pub const MAX_WINDOW_ROWS: u32 = 200;
pub const MAX_WINDOW_COLS: u32 = 60;

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProgressEvent {
    pub scan_id: String,
    pub phase: String,
    pub done: usize,
    pub total: usize,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub label: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ScanErrorEvent {
    pub scan_id: String,
    pub code: String,
    pub message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub extra: Option<crate::errors::ErrorExtra>,
}

/// 一次掃描的完整結果（保存在記憶體，直到使用者清除或關閉程式）。
pub struct ScanEntry {
    pub scan_id: String,
    pub workbook: WorkbookData,
    pub issues: Vec<Issue>,
    pub payload: ScanPayload,
    /// (工作表索引, 列, 欄) → 問題 ID，供網格與清單標記。
    issue_index: HashMap<(usize, u32, u32), Vec<String>>,
}

impl ScanEntry {
    pub fn issues_at(&self, sheet_index: usize, row: u32, col: u32) -> &[String] {
        self.issue_index
            .get(&(sheet_index, row, col))
            .map(Vec::as_slice)
            .unwrap_or(&[])
    }

    pub fn sheet_by_name(&self, name: &str) -> Option<&SheetData> {
        self.workbook.sheets.iter().find(|s| s.name == name)
    }
}

/// Tauri managed state：所有掃描結果與進行中的掃描。
#[derive(Default)]
pub struct ScanStore {
    entries: Mutex<HashMap<String, Arc<ScanEntry>>>,
    /// 進行中的掃描 → 取消旗標。
    running: Mutex<HashMap<String, Arc<AtomicBool>>>,
    counter: AtomicU64,
}

impl ScanStore {
    fn lock(&self) -> std::sync::MutexGuard<'_, HashMap<String, Arc<ScanEntry>>> {
        // 不使用 unwrap：互斥鎖中毒時仍取回內部資料，避免整個程式崩潰（SPEC §8.2）。
        self.entries.lock().unwrap_or_else(|e| e.into_inner())
    }

    fn lock_running(&self) -> std::sync::MutexGuard<'_, HashMap<String, Arc<AtomicBool>>> {
        self.running.lock().unwrap_or_else(|e| e.into_inner())
    }

    pub fn insert(&self, entry: Arc<ScanEntry>) {
        self.lock().insert(entry.scan_id.clone(), entry);
    }

    pub fn get(&self, scan_id: &str) -> Option<Arc<ScanEntry>> {
        self.lock().get(scan_id).cloned()
    }

    pub fn remove(&self, scan_id: &str) -> Option<Arc<ScanEntry>> {
        self.cancel(scan_id);
        self.lock().remove(scan_id)
    }

    pub fn all(&self) -> Vec<Arc<ScanEntry>> {
        self.lock().values().cloned().collect()
    }

    /// 登記進行中的掃描，回傳取消旗標。
    pub fn register_running(&self, scan_id: &str) -> Arc<AtomicBool> {
        let flag = Arc::new(AtomicBool::new(false));
        self.lock_running()
            .insert(scan_id.to_string(), Arc::clone(&flag));
        flag
    }

    pub fn unregister_running(&self, scan_id: &str) {
        self.lock_running().remove(scan_id);
    }

    /// 要求取消指定掃描；回傳是否真的有進行中的掃描被取消（AC07）。
    pub fn cancel(&self, scan_id: &str) -> bool {
        match self.lock_running().get(scan_id) {
            Some(flag) => {
                flag.store(true, Ordering::Relaxed);
                true
            }
            None => false,
        }
    }

    /// 取消所有進行中的掃描並清除結果（SPEC §2.1-8「清除資料」、AC12）。
    pub fn clear_all(&self) -> usize {
        for flag in self.lock_running().values() {
            flag.store(true, Ordering::Relaxed);
        }
        let mut guard = self.lock();
        let count = guard.len();
        guard.clear();
        count
    }

    pub fn next_scan_id(&self) -> String {
        let sequence = self.counter.fetch_add(1, Ordering::Relaxed);
        let stamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_millis())
            .unwrap_or(0);
        format!("scan-{stamp}-{sequence}")
    }
}

/// 啟動掃描：先同步驗證（讓錯誤可立即回報），再於背景執行緒解析與檢查。
pub fn start_scan(
    app: AppHandle,
    store: Arc<ScanStore>,
    path: String,
    allow_large: bool,
) -> ScanResult<String> {
    let path_buf = workbook_reader::absolute_path(&path);

    // 同步前置驗證：檔案存在、副檔名、大小限制。
    workbook_reader::validate_path(&path_buf, allow_large)?;

    let scan_id = store.next_scan_id();
    let cancel = store.register_running(&scan_id);
    let thread_id = scan_id.clone();
    let thread_cancel = Arc::clone(&cancel);

    std::thread::Builder::new()
        .name(format!("scan-{thread_id}"))
        .spawn(move || {
            let result = run_scan(
                &app,
                &store,
                &thread_id,
                path_buf,
                allow_large,
                thread_cancel,
            );
            store.unregister_running(&thread_id);
            if let Err(error) = result {
                let _ = app.emit(
                    EVENT_ERROR,
                    ScanErrorEvent {
                        scan_id: thread_id.clone(),
                        code: error.code().to_string(),
                        message: error.to_string(),
                        extra: error.payload().extra,
                    },
                );
            }
        })
        .map_err(|e| ScanError::Internal {
            detail: format!("無法建立掃描執行緒：{e}"),
        })?;

    Ok(scan_id)
}

fn run_scan(
    app: &AppHandle,
    store: &Arc<ScanStore>,
    scan_id: &str,
    path: std::path::PathBuf,
    allow_large: bool,
    cancel: Arc<AtomicBool>,
) -> ScanResult<()> {
    let started = Instant::now();
    let emit_progress = |update: ProgressUpdate| {
        let _ = app.emit(
            EVENT_PROGRESS,
            ProgressEvent {
                scan_id: scan_id.to_string(),
                phase: update.phase,
                done: update.done,
                total: update.total,
                label: update.label,
            },
        );
    };

    let mut on_progress = |update: ProgressUpdate| emit_progress(update);

    let workbook = workbook_reader::read_workbook(
        &path,
        ReadOptions { allow_large },
        &cancel,
        &mut on_progress,
    )?;

    let issues = rules::run_rules(&workbook, &cancel, &mut on_progress)?;

    if cancel.load(Ordering::Relaxed) {
        return Err(ScanError::Cancelled);
    }

    let entry = Arc::new(build_entry(scan_id, workbook, issues, started));
    let payload = entry.payload.clone();
    store.insert(Arc::clone(&entry));

    let _ = app.emit(EVENT_DONE, payload);
    Ok(())
}

/// 將解析結果與規則結果組裝成可查詢、可傳送的掃描結果。
///
/// 由掃描執行緒呼叫；整合測試亦直接使用（`src-tauri/tests/engine_queries.rs`）。
pub fn build_entry(
    scan_id: &str,
    workbook: WorkbookData,
    issues: Vec<Issue>,
    started: Instant,
) -> ScanEntry {
    let mut issue_index: HashMap<(usize, u32, u32), Vec<String>> = HashMap::new();
    let mut per_sheet_issue_count: HashMap<String, usize> = HashMap::new();
    let sheet_index_by_name: HashMap<&str, usize> = workbook
        .sheets
        .iter()
        .map(|s| (s.name.as_str(), s.index))
        .collect();

    for issue in &issues {
        *per_sheet_issue_count
            .entry(issue.sheet_name.clone())
            .or_insert(0) += 1;
        if let (Some(&sheet_index), Some((row, col))) = (
            sheet_index_by_name.get(issue.sheet_name.as_str()),
            formula_parser::parse_address(&issue.cell_address),
        ) {
            issue_index
                .entry((sheet_index, row, col))
                .or_default()
                .push(issue.id.clone());
        }
    }

    let severity_counts: HashMap<crate::model::Severity, usize> = rules::count_by_severity(&issues);
    let mut issues_by_severity: BTreeMap<crate::model::Severity, usize> = BTreeMap::new();
    for rule_severity in [
        crate::model::Severity::High,
        crate::model::Severity::Medium,
        crate::model::Severity::Low,
        crate::model::Severity::Info,
    ] {
        issues_by_severity.insert(
            rule_severity,
            severity_counts.get(&rule_severity).copied().unwrap_or(0),
        );
    }

    let mut warnings = workbook.warnings.clone();
    let issues_truncated = issues.len() > MAX_ISSUES_IN_PAYLOAD;
    if issues_truncated {
        warnings.push(format!(
            "問題數量超過單次傳輸上限 {MAX_ISSUES_IN_PAYLOAD}，清單僅顯示前 {MAX_ISSUES_IN_PAYLOAD} 筆（統計仍為完整數量）。"
        ));
    }

    let sheets: Vec<SheetInfo> = workbook
        .sheets
        .iter()
        .map(|sheet| SheetInfo {
            index: sheet.index,
            name: sheet.name.clone(),
            used_range: sheet.used_range_string(),
            row_count: sheet
                .used_end
                .map(|(row, _)| row + 1)
                .unwrap_or_default(),
            col_count: sheet
                .used_end
                .map(|(_, col)| col + 1)
                .unwrap_or_default(),
            formula_count: sheet.formula_count,
            constant_count: sheet.constant_count,
            error_count: sheet.error_count,
            issue_count: per_sheet_issue_count
                .get(&sheet.name)
                .copied()
                .unwrap_or(0),
        })
        .collect();

    let summary = ScanSummary {
        sheet_count: sheets.len(),
        scanned_cell_count: workbook.cell_count,
        formula_count: sheets.iter().map(|s| s.formula_count).sum(),
        constant_count: sheets.iter().map(|s| s.constant_count).sum(),
        issues_by_severity,
        duration_ms: started.elapsed().as_millis() as u64,
        warnings: warnings.clone(),
    };

    let payload = ScanPayload {
        scan_id: scan_id.to_string(),
        file_name: workbook.file_name.clone(),
        file_size: workbook.file_size,
        summary,
        sheets,
        issues: issues
            .iter()
            .take(MAX_ISSUES_IN_PAYLOAD)
            .cloned()
            .collect(),
        issues_truncated,
    };

    ScanEntry {
        scan_id: scan_id.to_string(),
        workbook,
        issues,
        payload,
        issue_index,
    }
}

// ---------------------------------------------------------------------------
// 查詢：公式清單 / 網格視窗 / 儲存格上下文
// ---------------------------------------------------------------------------

/// 依查詢條件分頁取得公式清單（SPEC §2.1-3、§4.4）。
pub fn query_formulas(entry: &ScanEntry, query: &FormulaQuery) -> FormulaPage {
    let address_filter = query
        .address
        .as_ref()
        .map(|a| a.trim().to_ascii_uppercase())
        .filter(|a| !a.is_empty());
    let text_filter = query
        .text
        .as_ref()
        .map(|t| t.trim().to_ascii_uppercase())
        .filter(|t| !t.is_empty());

    let mut matched: Vec<FormulaRecord> = Vec::new();

    for sheet in &entry.workbook.sheets {
        if let Some(name) = &query.sheet_name {
            if &sheet.name != name {
                continue;
            }
        }
        for (row, col) in sheet.formula_positions() {
            let Some(cell) = sheet.cell(row, col) else {
                continue;
            };
            let Some(formula) = cell.formula.as_deref() else {
                continue;
            };
            let address = format_address(row, col);
            let issue_ids: Vec<String> = entry.issues_at(sheet.index, row, col).to_vec();

            if query.only_issues && issue_ids.is_empty() {
                continue;
            }
            if let Some(filter) = &address_filter {
                if !address.to_ascii_uppercase().starts_with(filter.as_str()) {
                    continue;
                }
            }
            if let Some(filter) = &text_filter {
                if !formula.to_ascii_uppercase().contains(filter.as_str()) {
                    continue;
                }
            }

            matched.push(FormulaRecord {
                sheet_name: sheet.name.clone(),
                cell_address: address,
                row,
                col,
                formula: display_formula(formula),
                display_value: cell.display.clone(),
                kind: cell.kind,
                issue_ids,
                signature: Some(
                    crate::formula_signature::signature_at(formula, &sheet.name, (row, col)).value,
                ),
            });
        }
    }

    let total = matched.len();
    let offset = query.offset.min(total);
    let limit = query.normalized_limit();
    let items = matched.into_iter().skip(offset).take(limit).collect();

    FormulaPage {
        total,
        offset,
        limit,
        items,
    }
}

/// 取得工作表的一段網格（虛擬化前端依可見範圍分批呼叫）。
pub fn sheet_window(
    entry: &ScanEntry,
    sheet_name: &str,
    start_row: u32,
    start_col: u32,
    row_count: u32,
    col_count: u32,
) -> ScanResult<GridWindow> {
    let sheet = entry
        .sheet_by_name(sheet_name)
        .ok_or(ScanError::ScanNotFound)?;
    let row_count = row_count.clamp(1, MAX_WINDOW_ROWS);
    let col_count = col_count.clamp(1, MAX_WINDOW_COLS);

    let mut cells = Vec::with_capacity((row_count * col_count) as usize);
    for row in start_row..start_row.saturating_add(row_count) {
        for col in start_col..start_col.saturating_add(col_count) {
            cells.push(grid_cell(entry, sheet, row, col));
        }
    }

    Ok(GridWindow {
        sheet_name: sheet.name.clone(),
        start_row,
        start_col,
        row_count,
        col_count,
        cells,
    })
}

/// 建立單一網格儲存格（含公式簽章與問題標記）。
fn grid_cell(entry: &ScanEntry, sheet: &SheetData, row: u32, col: u32) -> GridCell {
    let cell = sheet.cell(row, col);
    let formula = cell.and_then(|c| c.formula.as_deref());
    let signature = formula.and_then(|raw| {
        let signature = crate::formula_signature::signature_at(raw, &sheet.name, (row, col));
        (!signature.unsupported).then_some(signature.value)
    });
    GridCell {
        row,
        col,
        address: format_address(row, col),
        display: cell.and_then(|c| c.display.clone()),
        kind: cell.map(|c| c.kind).unwrap_or(ValueKind::Empty),
        formula: formula.map(display_formula),
        signature,
        issue_ids: entry.issues_at(sheet.index, row, col).to_vec(),
    }
}

/// 儲存格上下文：儲存格本身、問題、同欄鄰近公式與主流簽章（SPEC §4.4）。
pub fn cell_context(
    entry: &ScanEntry,
    sheet_name: &str,
    address: &str,
    span: u32,
) -> ScanResult<CellContext> {
    let sheet = entry
        .sheet_by_name(sheet_name)
        .ok_or(ScanError::ScanNotFound)?;
    let (row, col) = formula_parser::parse_address(address).ok_or(ScanError::ScanNotFound)?;
    let span = span.clamp(1, 10);

    let issues_at_cell: Vec<Issue> = entry
        .issues
        .iter()
        .filter(|issue| issue.sheet_name == sheet.name && issue.cell_address == address)
        .cloned()
        .collect();

    let start_row = row.saturating_sub(span);
    let end_row = row.saturating_add(span);
    let mut neighbours = Vec::new();
    for neighbour_row in start_row..=end_row {
        if neighbour_row == row {
            continue;
        }
        if sheet.cell(neighbour_row, col).is_none() {
            continue;
        }
        neighbours.push(grid_cell(entry, sheet, neighbour_row, col));
    }

    let cell = sheet.cell(row, col);
    let dominant_signature = rules::column_block_signature(sheet, row, col)
        .map(|(signature, count, total)| format!("{signature}（{count}/{total}）"));

    // 直接參照：只列出可靜態解析的語法；含不支援語法時明確說明「無法判讀」。
    let (refs, unsupported_reason) = match cell.and_then(|c| c.formula.as_deref()) {
        Some(formula) => {
            let parsed = formula_parser::parse(formula);
            let refs = parsed
                .refs
                .iter()
                .map(|reference| match &reference.sheet {
                    Some(name) => format!("{name}!{}", reference.address()),
                    None => reference.address(),
                })
                .collect();
            let reason = parsed.unsupported_reasons.first().cloned();
            (refs, reason)
        }
        None => (Vec::new(), None),
    };

    Ok(CellContext {
        sheet_name: sheet.name.clone(),
        cell: grid_cell(entry, sheet, row, col),
        issues: issues_at_cell,
        neighbours,
        dominant_signature,
        refs,
        unsupported_reason,
    })
}

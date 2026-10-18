//! Tauri IPC 進入點（SPEC §7）。
//!
//! 前端可呼叫的命令極少且明確：不提供任意檔案系統存取、shell 執行或網路 API。

use std::path::PathBuf;
use std::sync::Arc;

use serde::Serialize;
use tauri::{AppHandle, State};

use crate::engine::{self, ScanStore};
use crate::errors::ErrorPayload;
use crate::exporter;
use crate::model::{
    CellContext, ExportFormat, ExportRequest, FormulaPage, FormulaQuery, GridWindow, Issue,
    ScanPayload,
};

type Store<'a> = State<'a, Arc<ScanStore>>;

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ExportResult {
    pub path: String,
    pub count: usize,
}

/// 開始掃描：立即回傳 scanId，結果透過 `scan://progress` / `scan://done` / `scan://error` 事件送出。
///
/// `allowLarge` 由前端在使用者確認實驗性模式後帶入（SPEC §8.2）。
#[tauri::command(rename_all = "camelCase")]
pub async fn start_scan(
    app: AppHandle,
    store: Store<'_>,
    path: String,
    allow_large: bool,
) -> Result<String, ErrorPayload> {
    let store = Arc::clone(store.inner());
    engine::start_scan(app, store, path, allow_large).map_err(Into::into)
}

/// 取消進行中的掃描（AC07）。
#[tauri::command(rename_all = "camelCase")]
pub async fn cancel_scan(store: Store<'_>, scan_id: String) -> Result<bool, ErrorPayload> {
    Ok(store.cancel(&scan_id))
}

/// 取回掃描結果（重新載入 UI 時使用；內容與 `scan://done` 相同）。
#[tauri::command(rename_all = "camelCase")]
pub async fn get_scan_payload(
    store: Store<'_>,
    scan_id: String,
) -> Result<ScanPayload, ErrorPayload> {
    let entry = store
        .get(&scan_id)
        .ok_or_else(|| crate::errors::ScanError::ScanNotFound)?;
    Ok(entry.payload.clone())
}

/// 取得問題清單（完整結果，供匯出與明細使用）。
#[tauri::command(rename_all = "camelCase")]
pub async fn get_issues(store: Store<'_>, scan_id: String) -> Result<Vec<Issue>, ErrorPayload> {
    let entry = store
        .get(&scan_id)
        .ok_or_else(|| crate::errors::ScanError::ScanNotFound)?;
    Ok(entry.issues.clone())
}

/// 分頁取得公式清單（SPEC §2.1-3）。
#[tauri::command(rename_all = "camelCase")]
pub async fn get_formulas(
    store: Store<'_>,
    scan_id: String,
    query: Option<FormulaQuery>,
) -> Result<FormulaPage, ErrorPayload> {
    let entry = store
        .get(&scan_id)
        .ok_or_else(|| crate::errors::ScanError::ScanNotFound)?;
    Ok(engine::query_formulas(
        &entry,
        &query.unwrap_or_default(),
    ))
}

/// 取得工作表的一段網格（虛擬化網格分批呼叫）。
#[tauri::command(rename_all = "camelCase")]
pub async fn get_sheet_window(
    store: Store<'_>,
    scan_id: String,
    sheet_name: String,
    start_row: u32,
    start_col: u32,
    row_count: u32,
    col_count: u32,
) -> Result<GridWindow, ErrorPayload> {
    let entry = store
        .get(&scan_id)
        .ok_or_else(|| crate::errors::ScanError::ScanNotFound)?;
    engine::sheet_window(&entry, &sheet_name, start_row, start_col, row_count, col_count)
        .map_err(Into::into)
}

/// 取得儲存格上下文與相鄰公式比較（SPEC §4.4）。
#[tauri::command(rename_all = "camelCase")]
pub async fn get_cell_context(
    store: Store<'_>,
    scan_id: String,
    sheet_name: String,
    address: String,
    span: Option<u32>,
) -> Result<CellContext, ErrorPayload> {
    let entry = store
        .get(&scan_id)
        .ok_or_else(|| crate::errors::ScanError::ScanNotFound)?;
    engine::cell_context(&entry, &sheet_name, &address, span.unwrap_or(3)).map_err(Into::into)
}

/// 匯出目前篩選結果（SPEC §2.1-6）。`path` 由前端檔案對話框取得。
#[tauri::command(rename_all = "camelCase")]
pub async fn export_issues(request: ExportRequest) -> Result<ExportResult, ErrorPayload> {
    let count = request.issues.len();
    let path = exporter::write_export(
        &PathBuf::from(&request.path),
        request.format,
        &request.issues,
    )?;
    Ok(ExportResult { path, count })
}

/// 匯出並由 Rust 直接產生檔案（格式由 `format` 決定）。
#[tauri::command(rename_all = "camelCase")]
pub async fn export_from_scan(
    store: Store<'_>,
    scan_id: String,
    format: ExportFormat,
    path: String,
) -> Result<ExportResult, ErrorPayload> {
    let entry = store
        .get(&scan_id)
        .ok_or_else(|| crate::errors::ScanError::ScanNotFound)?;
    let count = entry.issues.len();
    let written = exporter::write_export(&PathBuf::from(path), format, &entry.issues)?;
    Ok(ExportResult {
        path: written,
        count,
    })
}

/// 清除單次掃描結果（SPEC §2.1-8）。
#[tauri::command(rename_all = "camelCase")]
pub async fn clear_scan(store: Store<'_>, scan_id: String) -> Result<bool, ErrorPayload> {
    Ok(store.remove(&scan_id).is_some())
}

/// 清除所有掃描結果並取消進行中的掃描（AC12）。
#[tauri::command(rename_all = "camelCase")]
pub async fn clear_all(store: Store<'_>) -> Result<usize, ErrorPayload> {
    Ok(store.clear_all())
}

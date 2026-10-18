//! Excel Formula Inspector — Rust 後端。
//!
//! 版面（SPEC §7）：
//! - `model`   對外資料模型與 IPC 型別
//! - `errors`  結構化錯誤與錯誤碼
//! - `workbook_reader` calamine 讀檔（值／型別／公式）
//! - `formula_parser` / `formula_signature` 公式 tokenizer 與參照簽章
//! - `rules`   F001–F006 靜態規則
//! - `engine`  掃描編排、進度事件、取消、結果儲存
//! - `commands` Tauri IPC 進入點
//! - `exporter` CSV / JSON 匯出
//!
//! 安全邊界（SPEC §8.1）：不引入 http／shell／fs plugin，前端無法發起網路請求或任意讀寫檔案。

pub mod commands;
pub mod engine;
pub mod errors;
pub mod exporter;
pub mod formula_parser;
pub mod formula_signature;
pub mod model;
pub mod rules;
pub mod workbook_reader;

use std::sync::Arc;

use engine::ScanStore;

/// 啟動桌面應用程式。
pub fn run() {
    let outcome = tauri::Builder::default()
        // 唯一允許的 plugin：系統檔案對話框（開啟／儲存）。不加入 http、shell、fs 或 updater。
        .plugin(tauri_plugin_dialog::init())
        .manage(Arc::new(ScanStore::default()))
        .invoke_handler(tauri::generate_handler![
            commands::start_scan,
            commands::cancel_scan,
            commands::get_scan_payload,
            commands::get_issues,
            commands::get_formulas,
            commands::get_sheet_window,
            commands::get_cell_context,
            commands::export_issues,
            commands::export_from_scan,
            commands::clear_scan,
            commands::clear_all,
        ])
        .run(tauri::generate_context!());

    // 啟動失敗不 panic（SPEC §12）：印出訊息並以非零碼結束。
    if let Err(error) = outcome {
        eprintln!("Excel Formula Inspector 啟動失敗：{error}");
        std::process::exit(1);
    }
}

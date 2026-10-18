//! 整合測試：掃描引擎的查詢層（SPEC §4.3、§4.4、§2.1-5）。
//!
//! 驗證公式清單的篩選／分頁、網格視窗與儲存格上下文（含「無法判讀」的處理）。

mod common;

use std::sync::atomic::AtomicBool;
use std::time::Instant;

use excel_formula_inspector_lib::engine::{self, ScanEntry};
use excel_formula_inspector_lib::model::{FormulaQuery, ValueKind};
use excel_formula_inspector_lib::rules;
use excel_formula_inspector_lib::workbook_reader::{self, ProgressUpdate, ReadOptions};

fn build_entry(path: &std::path::Path) -> ScanEntry {
    let cancel = AtomicBool::new(false);
    let workbook = workbook_reader::read_workbook(
        path,
        ReadOptions { allow_large: false },
        &cancel,
        &mut |_: ProgressUpdate| {},
    )
    .expect("讀取 fixture");
    let issues = rules::run_rules(&workbook, &cancel, &mut |_: ProgressUpdate| {}).expect("規則");
    engine::build_entry("scan-test", workbook, issues, Instant::now())
}

fn query(entry: &ScanEntry, query: FormulaQuery) -> excel_formula_inspector_lib::model::FormulaPage {
    engine::query_formulas(entry, &query)
}

#[test]
fn formula_list_returns_every_formula_with_address_and_text() {
    let entry = build_entry(&common::normal_formulas());
    let page = query(&entry, FormulaQuery::default());

    assert_eq!(page.total, 10);
    assert_eq!(page.items.len(), 10);
    let first = &page.items[0];
    assert_eq!(first.sheet_name, "Report");
    assert_eq!(first.cell_address, "B2");
    assert!(first.formula.starts_with('='), "對外顯示需帶前導 =");
    assert!(first.signature.is_some(), "應附簽章供比對");
}

#[test]
fn formula_list_paginates_without_losing_the_total() {
    let entry = build_entry(&common::normal_formulas());

    let first = query(
        &entry,
        FormulaQuery {
            limit: 3,
            ..Default::default()
        },
    );
    assert_eq!(first.total, 10);
    assert_eq!(first.items.len(), 3);

    let second = query(
        &entry,
        FormulaQuery {
            limit: 3,
            offset: 3,
            ..Default::default()
        },
    );
    assert_eq!(second.total, 10);
    assert_eq!(second.offset, 3);
    assert_ne!(first.items[0].cell_address, second.items[0].cell_address);

    // 超過上限的 limit 會被夾到 MAX_PAGE_SIZE，不會一次搬移整份清單
    let huge = query(
        &entry,
        FormulaQuery {
            limit: 100_000,
            ..Default::default()
        },
    );
    assert_eq!(huge.limit, excel_formula_inspector_lib::model::MAX_PAGE_SIZE);
}

#[test]
fn formula_list_filters_by_sheet_address_and_text() {
    let entry = build_entry(&common::normal_formulas());

    let by_sheet = query(
        &entry,
        FormulaQuery {
            sheet_name: Some("Inputs".to_string()),
            ..Default::default()
        },
    );
    assert_eq!(by_sheet.total, 0, "Inputs 沒有公式");

    let by_prefix = query(
        &entry,
        FormulaQuery {
            address: Some("B1".to_string()),
            ..Default::default()
        },
    );
    assert_eq!(by_prefix.total, 2, "B10、B11 兩個位址");
    assert!(by_prefix
        .items
        .iter()
        .all(|item| item.cell_address.starts_with("B1")));

    let by_text = query(
        &entry,
        FormulaQuery {
            text: Some("sum".to_string()),
            ..Default::default()
        },
    );
    assert_eq!(by_text.total, 1);
    assert_eq!(by_text.items[0].cell_address, "B2");
}

#[test]
fn formula_list_can_show_only_flagged_formulas() {
    let entry = build_entry(&common::pattern_anomaly());
    let all = query(&entry, FormulaQuery::default());
    let flagged = query(
        &entry,
        FormulaQuery {
            only_issues: true,
            ..Default::default()
        },
    );

    assert!(flagged.total < all.total);
    assert!(flagged.items.iter().all(|item| !item.issue_ids.is_empty()));
    let addresses: Vec<&str> = flagged
        .items
        .iter()
        .map(|item| item.cell_address.as_str())
        .collect();
    assert!(addresses.contains(&"C6"), "F003 的儲存格應被標記：{addresses:?}");
}

#[test]
fn sheet_window_returns_requested_block_with_issue_markers() {
    let entry = build_entry(&common::pattern_anomaly());
    let window = engine::sheet_window(&entry, "Patterns", 3, 2, 4, 2).expect("網格視窗");

    assert_eq!(window.row_count, 4);
    assert_eq!(window.col_count, 2);
    assert_eq!(window.cells.len(), 8);

    // 0-based (5,2) = C6 → F003 的問題儲存格
    let flagged = window
        .cells
        .iter()
        .find(|cell| cell.address == "C6")
        .expect("C6 應在視窗內");
    assert!(!flagged.issue_ids.is_empty(), "C6 應帶問題標記");
    assert_eq!(flagged.formula.as_deref(), Some("=B11+1"));
    assert!(flagged.signature.is_some());

    // 常數覆寫的儲存格（F004）也應帶標記並保留快取值
    let override_cell = window
        .cells
        .iter()
        .find(|cell| cell.address == "D4")
        .expect("D4 應在視窗內");
    assert_eq!(override_cell.display.as_deref(), Some("123"));
    assert!(!override_cell.issue_ids.is_empty(), "常數覆寫應帶問題標記");

    // 空白儲存格仍需回傳，讓網格可以畫出完整範圍
    let empty = window
        .cells
        .iter()
        .find(|cell| cell.address == "C7")
        .expect("C7 應在視窗內");
    assert_eq!(empty.display, None);
    assert_eq!(empty.formula, None);
    assert_eq!(empty.kind, ValueKind::Empty);
}

#[test]
fn sheet_window_clamps_oversized_requests() {
    let entry = build_entry(&common::normal_formulas());
    let window = engine::sheet_window(&entry, "Report", 0, 0, 5_000, 5_000).expect("網格視窗");
    assert_eq!(window.row_count, engine::MAX_WINDOW_ROWS);
    assert_eq!(window.col_count, engine::MAX_WINDOW_COLS);
}

#[test]
fn sheet_window_rejects_unknown_sheet() {
    let entry = build_entry(&common::normal_formulas());
    let error = engine::sheet_window(&entry, "NotThere", 0, 0, 5, 5).expect_err("未知工作表");
    assert_eq!(error.code(), "SCAN_NOT_FOUND");
}

#[test]
fn cell_context_exposes_references_and_neighbours() {
    let entry = build_entry(&common::pattern_anomaly());
    let context = engine::cell_context(&entry, "Patterns", "C6", 3).expect("上下文");

    assert_eq!(context.cell.address, "C6");
    assert_eq!(context.cell.formula.as_deref(), Some("=B11+1"));
    assert_eq!(context.refs, vec!["B11".to_string()]);
    assert!(context.unsupported_reason.is_none());
    assert!(
        context.dominant_signature.is_some(),
        "同欄應可判定主流簽章"
    );
    assert!(!context.neighbours.is_empty(), "應提供上下相鄰列");
    assert!(context.neighbours.iter().all(|cell| cell.col == 2));
    assert_eq!(context.issues.len(), 1);
    assert_eq!(context.issues[0].rule_id, excel_formula_inspector_lib::model::RuleId::F003);
}

#[test]
fn cell_context_reports_unsupported_syntax_as_unreadable() {
    let entry = build_entry(&common::special_formulas());
    let context = engine::cell_context(&entry, "Special", "B4", 2).expect("上下文");

    assert!(context.refs.is_empty(), "結構化參照不應被當成一般參照");
    let reason = context.unsupported_reason.expect("應說明無法判讀");
    assert!(reason.contains("結構化參照"), "實際：{reason}");
    assert!(context.issues.is_empty(), "無法判讀不得產生問題");
    assert!(context.dominant_signature.is_some(), "相鄰公式仍可比較");
}

#[test]
fn cell_context_rejects_invalid_address() {
    let entry = build_entry(&common::normal_formulas());
    assert!(engine::cell_context(&entry, "Report", "not-an-address", 3).is_err());
}

#[test]
fn issues_at_lookup_matches_grid_markers() {
    let entry = build_entry(&common::invalid_reference());
    // 0-based (1,1) = B2 → F001 與 F002 兩筆
    let ids = entry.issues_at(0, 1, 1);
    assert_eq!(ids.len(), 2, "B2 應同時是錯誤值與無效參照：{ids:?}");

    let payload = &entry.payload;
    assert_eq!(payload.summary.issues_by_severity.values().sum::<usize>(), entry.issues.len());
    assert!(!payload.issues_truncated);
    assert!(
        payload.file_name.ends_with("invalid-reference.xlsx"),
        "實際：{}",
        payload.file_name
    );
}

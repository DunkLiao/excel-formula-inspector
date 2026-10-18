//! 整合測試：解析器（SPEC §10）。
//! 這些測試直接操作 Rust 公開 API，不經 UI。

mod common;

use std::sync::atomic::AtomicBool;

use excel_formula_inspector_lib::errors::ScanError;
use excel_formula_inspector_lib::formula_parser::format_address;
use excel_formula_inspector_lib::model::ValueKind;
use excel_formula_inspector_lib::workbook_reader::{
    self, ReadOptions, ProgressUpdate, DEFAULT_SIZE_LIMIT, HARD_SIZE_LIMIT,
};

fn read(path: &std::path::Path) -> excel_formula_inspector_lib::workbook_reader::WorkbookData {
    let cancel = AtomicBool::new(false);
    workbook_reader::read_workbook(
        path,
        ReadOptions { allow_large: false },
        &cancel,
        &mut |_: ProgressUpdate| {},
    )
    .expect("讀取 fixture")
}

fn read_with_options(
    path: &std::path::Path,
    allow_large: bool,
) -> Result<excel_formula_inspector_lib::workbook_reader::WorkbookData, ScanError> {
    let cancel = AtomicBool::new(false);
    workbook_reader::read_workbook(
        path,
        ReadOptions { allow_large },
        &cancel,
        &mut |_: ProgressUpdate| {},
    )
}

#[test]
fn ac01_reads_two_sheets_and_ten_formulas() {
    let path = common::normal_formulas();
    let data = read(&path);

    assert_eq!(data.sheets.len(), 2, "應讀到 2 個工作表");
    assert_eq!(data.sheets[0].name, "Inputs");
    assert_eq!(data.sheets[1].name, "Report");

    let formulas: Vec<String> = data.sheets[1]
        .formula_positions()
        .iter()
        .map(|(row, col)| format_address(*row, *col))
        .collect();
    assert_eq!(formulas.len(), 10, "Report 應有 10 個公式：{formulas:?}");
    assert_eq!(data.sheets[1].formula_count, 10);
    assert!(
        data.sheets[0].formula_count == 0,
        "Inputs 沒有公式"
    );
    // 每個位置都可列出
    assert!(formulas.contains(&"B2".to_string()));
    assert!(formulas.contains(&"B11".to_string()));
}

#[test]
fn formula_addresses_are_absolute_not_relative_to_range_start() {
    // 這是 calamine 的陷阱：used_cells() 回傳相對座標，必須加上 range.start()。
    // Fixture 的公式集中在 Report!B2:B11，若未修正會回報為 A1:A10。
    let path = common::normal_formulas();
    let data = read(&path);
    let report = &data.sheets[1];

    assert!(report.formula_of(1, 1).is_some(), "B2 應為公式");
    assert!(report.formula_of(0, 0).is_none(), "A1 不應被誤認為公式");
    assert_eq!(
        data.sheets[1].used_range_string().as_deref(),
        Some("A1:B11"),
        "已使用範圍應由絕對座標計算"
    );
}

#[test]
fn ac09_rejects_unsupported_and_damaged_files() {
    let xls = common::unsupported_xls();
    let error = read_with_options(&xls, false).expect_err("xls 必須被拒絕");
    assert_eq!(error.code(), "UNSUPPORTED_FORMAT");

    let damaged = common::damaged_xlsx();
    let error = read_with_options(&damaged, false).expect_err("損毀檔必須被拒絕");
    assert_eq!(error.code(), "CORRUPT");

    let missing = std::path::Path::new("definitely-not-here-12345.xlsx");
    let error = read_with_options(missing, false).expect_err("不存在檔案必須被拒絕");
    assert_eq!(error.code(), "FILE_NOT_FOUND");
}

#[test]
fn large_file_requires_confirmation_then_allows_experimental_mode() {
    // 兩段式策略：> 20 MB 需確認；> 硬上限直接拒絕（SPEC §8.2）。
    let needs_confirmation = common::write_sized("big.xlsx", DEFAULT_SIZE_LIMIT + 1);
    let error = read_with_options(&needs_confirmation, false).expect_err("需先確認");
    assert_eq!(error.code(), "LARGE_FILE_NEEDS_CONFIRMATION");
    let payload = error.payload();
    assert!(
        payload.extra.is_some(),
        "錯誤需附帶結構化大小資訊供 UI 顯示"
    );

    // 確認後（實驗性模式）不再因大小被拒；此檔內容為全零，因此預期解析失敗而非大小錯誤。
    let result = read_with_options(&needs_confirmation, true);
    match result {
        Err(error) => assert_ne!(
            error.code(),
            "LARGE_FILE_NEEDS_CONFIRMATION",
            "使用者確認後不應再要求確認"
        ),
        Ok(_) => {}
    }

    let too_large = common::write_sized("huge.xlsx", HARD_SIZE_LIMIT + 1);
    let error = read_with_options(&too_large, true).expect_err("超過硬上限必須拒絕");
    assert_eq!(error.code(), "TOO_LARGE");
}

#[test]
fn ac11_sparse_sheet_with_data_at_the_far_end() {
    let last_row = 99_999u32;
    let path = common::large_sparse(last_row);
    let started = std::time::Instant::now();
    let data = read(&path);
    let elapsed = started.elapsed();

    let sheet = &data.sheets[0];
    assert_eq!(sheet.cells.len(), 4, "只走訪實際存在的儲存格");
    assert_eq!(
        sheet.used_range_string().as_deref(),
        Some("A1:C100000"),
        "尾端資料必須被找到"
    );
    assert_eq!(sheet.cell(last_row, 2).map(|c| c.kind), Some(ValueKind::Number));
    assert!(
        elapsed.as_secs() < 10,
        "稀疏表不應因虛擬網格而變慢（實際 {elapsed:?}）"
    );
}

#[test]
fn formulas_are_read_without_leading_equals_but_displayed_with_it() {
    let path = common::normal_formulas();
    let data = read(&path);
    let raw = data.sheets[1].formula_of(1, 1).expect("B2 公式");
    assert!(!raw.starts_with('='), "calamine 回傳的原文不含前導 =");
    assert_eq!(
        excel_formula_inspector_lib::formula_parser::display_formula(raw),
        "=SUM(Inputs!B2:B7)"
    );
}

#[test]
fn error_cells_keep_type_and_error_code() {
    let path = common::invalid_reference();
    let data = read(&path);
    let sheet = &data.sheets[0];

    let div0 = sheet.cell(2, 1).expect("B3");
    assert_eq!(div0.kind, ValueKind::Error);
    assert_eq!(div0.error.as_deref(), Some("#DIV/0!"));

    // 純文字常值的 "#REF!" 不是錯誤值
    let text = sheet.cell(4, 1).expect("B5");
    assert_eq!(text.kind, ValueKind::Text);
    assert!(text.error.is_none());
}

#[test]
fn merged_regions_are_captured_and_extend_used_range() {
    let path = common::intentional_breaks();
    let data = read(&path);
    let sheet = &data.sheets[0];
    assert_eq!(sheet.merge_regions.len(), 1);
    assert!(sheet.is_merged(0, 1), "合併區域內的儲存格應被識別");
    assert!(!sheet.is_merged(1, 1));
}

#[test]
fn sheet_visibility_is_reported() {
    let path = common::normal_formulas();
    let data = read(&path);
    assert!(data.sheets.iter().all(|s| s.visible));
}

#[test]
fn cancellation_stops_reading() {
    let path = common::large_sparse(99_999);
    let cancel = AtomicBool::new(true); // 已取消
    let result = workbook_reader::read_workbook(
        &path,
        ReadOptions { allow_large: false },
        &cancel,
        &mut |_: ProgressUpdate| {},
    );
    assert!(matches!(result, Err(ScanError::Cancelled)));
}

#[test]
fn progress_is_reported_per_sheet() {
    let path = common::normal_formulas();
    let cancel = AtomicBool::new(false);
    let mut updates = Vec::new();
    workbook_reader::read_workbook(
        &path,
        ReadOptions { allow_large: false },
        &cancel,
        &mut |update: ProgressUpdate| updates.push(update),
    )
    .expect("讀取");

    let reading: Vec<_> = updates.iter().filter(|u| u.phase == "reading").collect();
    assert!(reading.len() >= 3, "兩個工作表至少 2 次 + 收尾 1 次");
    assert!(reading.iter().all(|u| u.total == 2));
    assert!(reading.iter().any(|u| u.label.as_deref() == Some("Inputs")));
}

//! 產生 UAT 用的 Excel 測試檔案（SPEC §10 Fixture 的 UAT 版本）。
//!
//! 執行：
//!   cargo run --example make_uat_fixtures              # 輸出到 <repo>\uat-fixtures
//!   cargo run --example make_uat_fixtures <output-dir> # 指定輸出目錄
//!
//! 產生後會**實際用本工具的解析器與規則引擎跑一遍**，並印出每個檔案的預期結果，
//! 供 UAT 人員比對（不確定的預期不寫入報告，一切以引擎輸出為準）。

use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::AtomicBool;

use excel_formula_inspector_lib::errors::ScanError;
use excel_formula_inspector_lib::model::{RuleId, Severity};
use excel_formula_inspector_lib::rules;
use excel_formula_inspector_lib::workbook_reader::{self, ProgressUpdate, ReadOptions};
use rust_xlsxwriter::{Format, Formula, Workbook, XlsxError};

fn main() {
    let mut args = std::env::args().skip(1);
    let output_dir = args
        .next()
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("..").join("uat-fixtures"));

    if let Err(error) = fs::create_dir_all(&output_dir) {
        eprintln!("無法建立輸出目錄 {output_dir:?}：{error}");
        std::process::exit(1);
    }

    println!("UAT test workbooks");
    println!("==================");
    println!("Output folder: {}", output_dir.display());
    println!();

    let mut generated: Vec<PathBuf> = Vec::new();
    let mut failures: Vec<String> = Vec::new();

    let builders: Vec<(&str, fn(&Path) -> Result<PathBuf, XlsxError>)> = vec![
        ("UAT-01-clean-monthly-report.xlsx", write_clean_report),
        ("UAT-02-invalid-references.xlsx", write_invalid_references),
        ("UAT-03-pattern-anomaly.xlsx", write_pattern_anomaly),
        ("UAT-04-intentional-breaks.xlsx", write_intentional_breaks),
        ("UAT-05-special-formulas.xlsx", write_special_formulas),
        ("UAT-06-large-sparse.xlsx", write_large_sparse),
    ];

    for (name, build) in builders {
        let path = output_dir.join(name);
        match build(&path) {
            Ok(path) => {
                println!("[written] {}", path.display());
                generated.push(path);
            }
            Err(error) => failures.push(format!("{name}: {error}")),
        }
    }

    // 非工作簿檔（不支援格式與損毀檔）
    let unsupported = output_dir.join("UAT-07-unsupported-legacy.xls");
    match fs::write(&unsupported, [0xD0u8, 0xCF, 0x11, 0xE0, 0xA1, 0xB1, 0x1A, 0xE1, 0x00, 0x00, 0x00, 0x00])
    {
        Ok(()) => {
            println!("[written] {} (legacy OLE2 container, must be rejected)", unsupported.display());
            generated.push(unsupported);
        }
        Err(error) => failures.push(format!("UAT-07: {error}")),
    }

    let damaged = output_dir.join("UAT-08-damaged-file.xlsx");
    let mut damaged_bytes = vec![0x50u8, 0x4B, 0x03, 0x04];
    damaged_bytes.extend((0..2048).map(|index| ((index * 7 + 13) % 251) as u8));
    match fs::write(&damaged, &damaged_bytes) {
        Ok(()) => {
            println!("[written] {} (corrupted archive, must be rejected)", damaged.display());
            generated.push(damaged);
        }
        Err(error) => failures.push(format!("UAT-08: {error}")),
    }

    // 大小上限流程：真正有效且超過 20 MB 的工作簿（可完整走完「確認 → 實驗性模式 → 成功掃描」）
    let oversized = output_dir.join("UAT-09-large-workbook-over-20MB.xlsx");
    let stale = output_dir.join("UAT-09-over-20MB-size-gate.xlsx");
    if stale.exists() {
        match fs::remove_file(&stale) {
            Ok(()) => println!("[removed] {} (replaced by the valid large workbook)", stale.display()),
            Err(error) => println!("[warn] could not remove {}: {error}", stale.display()),
        }
    }
    match write_large_workbook(&oversized) {
        Ok(path) => {
            println!("[written] {}", path.display());
            generated.push(path);
        }
        Err(error) => failures.push(format!("UAT-09: {error}")),
    }

    if !failures.is_empty() {
        println!();
        println!("FAILED to generate:");
        for failure in &failures {
            println!("  - {failure}");
        }
    }

    println!();
    println!("Expected results (produced by the real parser and rule engine)");
    println!("=============================================================");

    for path in &generated {
        report_expectation(path);
    }

    println!("Done. See docs/UAT-GUIDE.md for the checklist and how to record results.");
}

// ---------------------------------------------------------------------------
// 檔案產生器
// ---------------------------------------------------------------------------

/// UAT-01：乾淨的月報（3 工作表、常見函數、跨表／絕對／相對參照）→ 預期 0 問題。
///
/// 設計原則：同一欄的公式必須是「複製貼上式」——只有相對參照會位移、絕對參照固定、
/// 逐列字串常值相同。這樣才不會被 F003 判定為模式不一致（UAT-03 會故意打破這個規則）。
fn write_clean_report(path: &Path) -> Result<PathBuf, XlsxError> {
    let mut workbook = Workbook::new();
    let bold = Format::new().set_bold();

    let inputs = workbook.add_worksheet();
    inputs.set_name("Inputs")?;
    for (col, title) in ["Item", "Qty", "Unit Price", "Amount"].iter().enumerate() {
        inputs.write_string_with_format(0, col as u16, *title, &bold)?;
    }
    for row in 1..=10u32 {
        inputs.write_string(row, 0, format!("PRODUCT-{row:02}"))?;
        inputs.write_number(row, 1, f64::from(row) * 3.0)?;
        inputs.write_number(row, 2, 25.0 + f64::from(row) * 2.5)?;
        // 相對參照複製向下：=B2*C2、=B3*C3 …
        inputs.write_formula(
            row,
            3,
            Formula::new(format!("=B{}*C{}", row + 1, row + 1).as_str()),
        )?;
    }

    let report = workbook.add_worksheet();
    report.set_name("Monthly Report")?;
    report.merge_range(0, 0, 0, 5, "Monthly Report - January", &bold)?;
    for (col, title) in [
        "Month",
        "Units",
        "Unit Price",
        "Lookup (VLOOKUP)",
        "Lookup (XLOOKUP)",
        "Status",
    ]
    .iter()
    .enumerate()
    {
        report.write_string_with_format(2, col as u16, *title, &bold)?;
    }
    for row in 3..=7u32 {
        report.write_string(row, 0, format!("M{}", row - 2))?;
        // 相對區間位移
        report.write_formula(
            row,
            1,
            Formula::new(format!("=SUM(Inputs!B{}:B{})", row, row + 2).as_str()),
        )?;
        // 絕對參照：所有列都指向同一個费率
        report.write_formula(row, 2, Formula::new("=Inputs!$C$2"))?;
        // 相對鍵值 + 絕對查表範圍
        report.write_formula(
            row,
            3,
            Formula::new(
                format!("=VLOOKUP(A{},Inputs!$A$2:$D$11,4,FALSE)", row + 1).as_str(),
            ),
        )?;
        report.write_formula(
            row,
            4,
            Formula::new(
                format!(
                    "=XLOOKUP(A{},Inputs!$A$2:$A$11,Inputs!$D$2:$D$11)",
                    row + 1
                )
                .as_str(),
            ),
        )?;
        // 相對參照 + 每列相同的字串常值
        report.write_formula(
            row,
            5,
            Formula::new(format!("=IF(B{}>0,\"OK\",\"CHECK\")", row + 1).as_str()),
        )?;
    }

    // 摘要：每個欄位的連續公式都只有 1 筆（不會形成可比較的區塊），確保為「乾淨」樣本
    let summary = workbook.add_worksheet();
    summary.set_name("Summary")?;
    summary.write_string_with_format(0, 0, "Metric", &bold)?;
    summary.write_string_with_format(0, 1, "Value", &bold)?;
    summary.write_string(1, 0, "Total units (row 4)")?;
    summary.write_formula(1, 1, Formula::new("='Monthly Report'!B4"))?;
    summary.write_string(2, 0, "Average unit price")?;
    summary.write_formula(2, 2, Formula::new("=AVERAGE(Inputs!C2:C11)"))?;
    summary.write_string(3, 0, "Rounded average")?;
    summary.write_formula(3, 1, Formula::new("=ROUND(C3,2)"))?;
    summary.write_string(4, 0, "Formatted average")?;
    summary.write_formula(4, 2, Formula::new("=TEXT(C3,\"0.00\")"))?;

    workbook.save(path)?;
    Ok(path.to_path_buf())
}

/// UAT-02：錯誤值與無效參照（含字串常值內的 `#REF!`，不得誤報）。
fn write_invalid_references(path: &Path) -> Result<PathBuf, XlsxError> {
    let mut workbook = Workbook::new();
    let bold = Format::new().set_bold();
    let sheet = workbook.add_worksheet();
    sheet.set_name("Errors")?;

    sheet.write_string_with_format(0, 0, "Case", &bold)?;
    sheet.write_string_with_format(0, 1, "Formula", &bold)?;
    sheet.write_string_with_format(0, 2, "Cached value", &bold)?;

    sheet.write_string(1, 0, "Deleted column")?;
    sheet.write_formula(1, 1, Formula::new("=B2+#REF!").set_result("#REF!"))?;

    sheet.write_string(2, 0, "Divide by zero")?;
    sheet.write_formula(2, 1, Formula::new("=1/0").set_result("#DIV/0!"))?;

    sheet.write_string(3, 0, "Lookup not found")?;
    sheet.write_formula(
        3,
        1,
        Formula::new("=VLOOKUP(\"MISSING\",A1:B2,2,FALSE)").set_result("#N/A"),
    )?;

    sheet.write_string(4, 0, "Text literal (must NOT raise F002)")?;
    sheet.write_formula(
        4,
        1,
        Formula::new("=B2&\"#REF! is just text\"").set_result("value #REF! is just text"),
    )?;

    sheet.write_string(5, 0, "Plain text cell")?;
    sheet.write_string(5, 1, "#REF!")?;

    sheet.write_string(6, 0, "Healthy formula")?;
    sheet.write_formula(6, 1, Formula::new("=B2+10"))?;

    workbook.save(path)?;
    Ok(path.to_path_buf())
}

/// UAT-03：同欄模式異常（F003）、常數覆寫（F004）與序列缺口（F005）。
fn write_pattern_anomaly(path: &Path) -> Result<PathBuf, XlsxError> {
    let mut workbook = Workbook::new();
    let bold = Format::new().set_bold();
    let sheet = workbook.add_worksheet();
    sheet.set_name("Patterns")?;

    sheet.write_string_with_format(0, 0, "Base", &bold)?;
    sheet.write_string_with_format(0, 1, "Copied down", &bold)?;
    sheet.write_string_with_format(0, 2, "Doubled (override)", &bold)?;
    sheet.write_string_with_format(0, 3, "Gap", &bold)?;
    sheet.write_string_with_format(0, 7, "Notes", &bold)?;

    for row in 1..=8u32 {
        sheet.write_number(row, 0, f64::from(row) * 100.0)?;
    }

    // B 欄：7 列相同模式，第 6 列（B7）列位移偏離 → F003
    for row in 1..=8u32 {
        let target = if row == 6 { 20 } else { row + 1 };
        sheet.write_formula(row, 1, Formula::new(format!("=A{target}*2").as_str()))?;
    }

    // C 欄：中間 C5 被常數覆寫 → F004
    for row in 1..=3u32 {
        sheet.write_formula(row, 2, Formula::new(format!("=B{}+1", row + 1).as_str()))?;
    }
    sheet.write_number(4, 2, 999.0)?;
    for row in 5..=8u32 {
        sheet.write_formula(row, 2, Formula::new(format!("=B{}+1", row + 1).as_str()))?;
    }

    // D 欄：D5 空白但同一列其他欄有內容 → F005
    for row in [1u32, 2, 3, 5, 6, 7] {
        sheet.write_formula(row, 3, Formula::new(format!("=B{}+5", row + 1).as_str()))?;
    }
    sheet.write_string(4, 7, "intentional note on the same row")?;

    workbook.save(path)?;
    Ok(path.to_path_buf())
}

/// UAT-04：刻意中斷（合併標題、完全空白列、硬編碼小計）→ 誤報控制。
///
/// 預期：只有「被硬編碼覆寫的小計」被標記為 F004（並附上可能是小計的提示）；
/// 合併標題與完全空白列都不得產生任何問題。
fn write_intentional_breaks(path: &Path) -> Result<PathBuf, XlsxError> {
    let mut workbook = Workbook::new();
    let bold = Format::new().set_bold();
    let sheet = workbook.add_worksheet();
    sheet.set_name("Report")?;

    sheet.merge_range(0, 0, 0, 3, "Quarterly Budget", &bold)?;
    for (col, title) in ["Month", "Budget", "Actual", "Variance"].iter().enumerate() {
        sheet.write_string_with_format(2, col as u16, *title, &bold)?;
    }

    // 區段一：4 列同模式公式
    for row in 3..=6u32 {
        sheet.write_string(row, 0, format!("Q1-M{}", row - 2))?;
        sheet.write_number(row, 1, 1_000.0 * f64::from(row))?;
        sheet.write_formula(row, 2, Formula::new(format!("=B{}*2", row + 1).as_str()))?;
        sheet.write_formula(
            row,
            3,
            Formula::new(format!("=C{}-B{}", row + 1, row + 1).as_str()),
        )?;
    }

    // 第 8 列（0-based 7）完全空白 → 區段切開，不得跨列推定
    // 區段二：3 列同模式公式
    for row in 8..=10u32 {
        sheet.write_string(row, 0, format!("Q2-M{}", row - 7))?;
        sheet.write_number(row, 1, 2_000.0 * f64::from(row))?;
        sheet.write_formula(row, 2, Formula::new(format!("=B{}*2", row + 1).as_str()))?;
        sheet.write_formula(
            row,
            3,
            Formula::new(format!("=C{}-B{}", row + 1, row + 1).as_str()),
        )?;
    }

    // 硬編碼小計：常數夾在公式之間 → F004（附「可能是小計」提示）
    sheet.write_string_with_format(11, 0, "Subtotal", &bold)?;
    sheet.write_number(11, 1, 99_999.0)?;
    sheet.write_number(11, 2, 60_000.0)?;
    for row in 12..=13u32 {
        sheet.write_string(row, 0, format!("X{}", row - 11))?;
        sheet.write_number(row, 1, 3_000.0 * f64::from(row))?;
        sheet.write_formula(row, 2, Formula::new(format!("=B{}*2", row + 1).as_str()))?;
    }

    workbook.save(path)?;
    Ok(path.to_path_buf())
}

/// UAT-05：暫不支援的語法（結構化參照、外部活頁簿、3D 參照、動態陣列）→ 必須「無法判讀」。
fn write_special_formulas(path: &Path) -> Result<PathBuf, XlsxError> {
    let mut workbook = Workbook::new();
    let bold = Format::new().set_bold();
    let sheet = workbook.add_worksheet();
    sheet.set_name("Special")?;

    sheet.write_string_with_format(0, 0, "Case", &bold)?;
    sheet.write_string_with_format(0, 1, "Formula", &bold)?;

    for row in 1..=3u32 {
        sheet.write_number(row, 0, f64::from(row))?;
    }

    // 3 列相同模式（若工具誤把下方不支援語法納入比對，就會產生假 F003）
    sheet.write_formula(1, 1, Formula::new("=A2+1"))?;
    sheet.write_formula(2, 1, Formula::new("=A3+1"))?;
    sheet.write_formula(3, 1, Formula::new("=A4+1"))?;

    sheet.write_string(5, 0, "Structured reference")?;
    sheet.write_formula(5, 1, Formula::new("=SUM(Table1[Amount])"))?;

    sheet.write_string(6, 0, "External workbook")?;
    sheet.write_formula(6, 1, Formula::new("='[Budget 2024.xlsx]Sheet1'!A1"))?;

    sheet.write_string(7, 0, "3D reference")?;
    sheet.write_formula(7, 1, Formula::new("=SUM(Sheet1:Sheet3!A1)"))?;

    sheet.write_string(8, 0, "Spill operator")?;
    sheet.write_formula(8, 1, Formula::new("=A2#"))?;

    sheet.write_string(9, 0, "LAMBDA (supported syntax)")?;
    sheet.write_formula(9, 1, Formula::new("=LAMBDA(x,x*2)"))?;

    workbook.save(path)?;
    Ok(path.to_path_buf())
}

/// UAT-06：稀疏工作表，資料落在很後面的列。
fn write_large_sparse(path: &Path) -> Result<PathBuf, XlsxError> {
    let mut workbook = Workbook::new();
    let bold = Format::new().set_bold();
    let sheet = workbook.add_worksheet();
    sheet.set_name("Sparse")?;

    sheet.write_string_with_format(0, 0, "Item", &bold)?;
    sheet.write_string_with_format(0, 1, "Amount", &bold)?;
    sheet.write_string_with_format(0, 2, "Note", &bold)?;
    sheet.write_string_with_format(0, 3, "Calc", &bold)?;

    sheet.write_number(1, 1, 100.0)?;
    sheet.write_formula(1, 3, Formula::new("=B2*2"))?;

    // 中段只有零星資料
    for row in [10_000u32, 50_000, 99_999] {
        sheet.write_number(row, 1, f64::from(row))?;
        sheet.write_string(row, 2, format!("row {row}"))?;
        sheet.write_formula(row, 3, Formula::new(format!("=B{}*2", row + 1).as_str()))?;
    }

    workbook.save(path)?;
    Ok(path.to_path_buf())
}

/// UAT-09：**有效**且超過 20 MB 的工作簿，用來完整驗證「大小上限 → 確認 → 實驗性模式 → 成功掃描」。
///
/// 做法：大量「互不重複且不易壓縮」的字串（deflate 對 base64 亂數幾乎無法壓縮），
/// 讓壓縮後的 .xlsx 仍超過 20 MB，同時把已使用儲存格控制在硬上限（500,000）以內。
/// Excel 可以正常開啟這個檔案（它是標準工作簿，不是佔位檔）。
fn write_large_workbook(path: &Path) -> Result<PathBuf, XlsxError> {
    const ROWS: u32 = 280_000;
    const TOKEN_LEN: usize = 96;

    let mut workbook = Workbook::new();
    let bold = Format::new().set_bold();
    let sheet = workbook.add_worksheet();
    sheet.set_name("Ledger")?;

    sheet.write_string_with_format(0, 0, "Reference", &bold)?;
    sheet.write_string_with_format(0, 1, "Formula check", &bold)?;

    // 少量公式，讓工具仍有公式可檢查（同時不觸發任何規則）
    for row in 1..=20u32 {
        sheet.write_formula(row, 1, Formula::new(format!("=LEN(A{})", row + 1).as_str()))?;
    }

    for row in 1..=ROWS {
        sheet.write_string(row, 0, unique_token(row, TOKEN_LEN))?;
    }

    workbook.save(path)?;

    let size = fs::metadata(path).map(|meta| meta.len()).unwrap_or(0);
    if size <= 20 * 1024 * 1024 {
        println!(
            "  [warn] {ROWS} rows produced only {:.1} MB - increase ROWS in write_large_workbook",
            size as f64 / (1024.0 * 1024.0)
        );
    }
    Ok(path.to_path_buf())
}

/// 產生第 `row` 列專屬的高熵字串（以列號為種子的 xorshift，不引入額外相依）。
fn unique_token(row: u32, length: usize) -> String {
    const ALPHABET: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut state = 0x9E37_79B9u32 ^ row.wrapping_mul(0x85EB_CA6B);
    let mut token = String::with_capacity(length + 12);
    token.push_str(&format!("{row:08}"));
    for _ in 0..length {
        // xorshift32
        state ^= state << 13;
        state ^= state >> 17;
        state ^= state << 5;
        let index = (state as usize) % ALPHABET.len();
        token.push(ALPHABET[index] as char);
    }
    token
}

// ---------------------------------------------------------------------------
// 以真實引擎產生預期結果
// ---------------------------------------------------------------------------

fn report_expectation(path: &Path) {
    println!();
    println!("{}", path.file_name().and_then(|n| n.to_str()).unwrap_or("?"));
    println!("  size: {} bytes", fs::metadata(path).map(|m| m.len()).unwrap_or(0));

    let cancel = AtomicBool::new(false);
    let started = std::time::Instant::now();
    let workbook = match workbook_reader::read_workbook(
        path,
        ReadOptions { allow_large: false },
        &cancel,
        &mut |_: ProgressUpdate| {},
    ) {
        Ok(workbook) => workbook,
        Err(error) => {
            report_rejection(&error);
            // 大檔流程：使用者確認後應可繼續讀取，並回報真正結果（而不是直接失敗）
            if !matches!(error, ScanError::LargeFileNeedsConfirmation { .. }) {
                return;
            }
            match workbook_reader::read_workbook(
                path,
                ReadOptions { allow_large: true },
                &cancel,
                &mut |_: ProgressUpdate| {},
            ) {
                Ok(workbook) => {
                    println!("  after confirmation: parsed successfully (experimental mode)");
                    workbook
                }
                Err(second) => {
                    println!("  after confirmation: {} ({second})", second.code());
                    return;
                }
            }
        }
    };

    let issues = match rules::run_rules(&workbook, &cancel, &mut |_: ProgressUpdate| {}) {
        Ok(issues) => issues,
        Err(error) => {
            println!("  rule engine error: {}", error.code());
            return;
        }
    };
    let elapsed = started.elapsed();

    println!(
        "  sheets: {}  cells: {}  formulas: {}  constants: {}  error cells: {}",
        workbook.sheets.len(),
        workbook.cell_count,
        workbook.sheets.iter().map(|s| s.formula_count).sum::<usize>(),
        workbook.sheets.iter().map(|s| s.constant_count).sum::<usize>(),
        workbook.sheets.iter().map(|s| s.error_count).sum::<usize>()
    );
    println!(
        "  read + rules took {} ms",
        elapsed.as_millis()
    );
    for sheet in &workbook.sheets {
        println!(
            "    - {:<16} used: {:<14} formulas: {:>4}  issues: {}",
            sheet.name,
            sheet.used_range_string().unwrap_or_else(|| "-".to_string()),
            sheet.formula_count,
            issues.iter().filter(|i| i.sheet_name == sheet.name).count()
        );
    }

    println!("  expectation: {} issue(s)", issues.len());
    for rule in RuleId::ALL {
        let matching: Vec<&excel_formula_inspector_lib::model::Issue> =
            issues.iter().filter(|issue| issue.rule_id == rule).collect();
        if matching.is_empty() {
            continue;
        }
        let severity = matching[0].severity;
        let addresses: Vec<String> = matching
            .iter()
            .take(6)
            .map(|issue| format!("{}!{}", issue.sheet_name, issue.cell_address))
            .collect();
        let suffix = if matching.len() > 6 { ", …" } else { "" };
        println!(
            "    {} ({}) x{}  at {}{}",
            rule.as_str(),
            severity_label(severity),
            matching.len(),
            addresses.join(", "),
            suffix
        );
    }
}

fn report_rejection(error: &ScanError) {
    println!("  expectation: rejected with code {}", error.code());
    println!("    message: {error}");
}

fn severity_label(severity: Severity) -> &'static str {
    match severity {
        Severity::High => "high",
        Severity::Medium => "medium",
        Severity::Low => "low",
        Severity::Info => "info",
    }
}

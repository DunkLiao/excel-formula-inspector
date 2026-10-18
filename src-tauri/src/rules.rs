//! 靜態規則引擎（SPEC §5）。
//!
//! 所有啟發式規則一律使用「疑似」措辭，並附 confidence 與可追溯 evidence。
//! 不支援的公式語法一律「無法判讀」，不當作異常（SPEC §5.1、§13.3）。

use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, Ordering};

use crate::errors::{ScanError, ScanResult};
use crate::formula_parser::{self, format_address};
use crate::formula_signature::{self, Signature};
use crate::model::{Confidence, Issue, RuleId, Severity};
use crate::workbook_reader::{CellData, ProgressUpdate, SheetData, WorkbookData};

/// 主流簽章最低佔比；未達門檻視為「無明顯主流」，不報警（降低誤報）。
pub const MIN_DOMINANCE_RATIO: f64 = 0.6;
/// 區塊內可判讀公式的最低樣本數（SPEC §5.1：不足 3 筆公式不報警）。
pub const MIN_SAMPLE: usize = 3;

/// 高風險函數清單（SPEC §5 F006「可設定清單」；數量少且可稽核爭議大者）。
pub const HIGH_RISK_FUNCTIONS: [&str; 13] = [
    "INDIRECT",
    "OFFSET",
    "NOW",
    "TODAY",
    "RAND",
    "RANDBETWEEN",
    "RANDARRAY",
    "CELL",
    "INFO",
    "WEBSERVICE",
    "FILTERXML",
    "HYPERLINK",
    "GETPIVOTDATA",
];

/// 執行所有規則。`on_progress` 會依工作表回報進度。
pub fn run_rules(
    workbook: &WorkbookData,
    cancel: &AtomicBool,
    on_progress: &mut dyn FnMut(ProgressUpdate),
) -> ScanResult<Vec<Issue>> {
    let total = workbook.sheets.len();
    let mut issues: Vec<Issue> = Vec::new();

    for (index, sheet) in workbook.sheets.iter().enumerate() {
        if cancel.load(Ordering::Relaxed) {
            return Err(ScanError::Cancelled);
        }
        on_progress(ProgressUpdate {
            phase: "rules".to_string(),
            done: index,
            total,
            label: Some(sheet.name.clone()),
        });

        collect_cell_rules(sheet, &mut issues);
        collect_pattern_rules(sheet, &mut issues);
    }

    on_progress(ProgressUpdate {
        phase: "rules".to_string(),
        done: total,
        total,
        label: None,
    });

    sort_issues(&mut issues);
    Ok(issues)
}

/// 穩定排序：工作表順序 → 列 → 欄 → 規則。
pub fn sort_issues(issues: &mut [Issue]) {
    issues.sort_by(|a, b| {
        a.sheet_name
            .cmp(&b.sheet_name)
            .then_with(|| address_sort_key(&a.cell_address).cmp(&address_sort_key(&b.cell_address)))
            .then_with(|| a.rule_id.cmp(&b.rule_id))
    });
}

fn address_sort_key(address: &str) -> (u32, u32) {
    formula_parser::parse_address(address).unwrap_or((u32::MAX, u32::MAX))
}

fn issue_id(rule: RuleId, sheet: &str, address: &str) -> String {
    format!("{}:{}!{}", rule.as_str(), sheet, address)
}

// ---------------------------------------------------------------------------
// 逐格規則：F001 / F002 / F006
// ---------------------------------------------------------------------------

fn collect_cell_rules(sheet: &SheetData, issues: &mut Vec<Issue>) {
    let mut positions: Vec<(u32, u32)> = sheet.cells.keys().copied().collect();
    positions.sort_unstable();

    for (row, col) in positions {
        let Some(cell) = sheet.cell(row, col) else {
            continue;
        };
        let address = format_address(row, col);

        check_error_cell(sheet, cell, row, col, &address, issues);
        check_invalid_reference(sheet, cell, &address, issues);
        check_high_risk_functions(sheet, cell, &address, issues);
    }
}

/// F001 — Excel 錯誤值。
fn check_error_cell(
    sheet: &SheetData,
    cell: &CellData,
    row: u32,
    col: u32,
    address: &str,
    issues: &mut Vec<Issue>,
) {
    let Some(error) = cell.error.as_deref() else {
        return;
    };

    let mut evidence = vec![
        format!("儲存格型別為錯誤值：{error}"),
        "快取錯誤值可能與公式目前的實際計算結果不一致".to_string(),
    ];
    if sheet.is_merged(row, col) {
        evidence.push("此儲存格屬於合併區域，錯誤可能來自合併前的輸入".to_string());
    }

    // 有公式時錯誤來自公式計算結果，信心較高；純錯誤常值則需保留彈性。
    let confidence = if cell.is_formula() {
        Confidence::High
    } else {
        Confidence::Medium
    };

    issues.push(
        Issue::new(
            issue_id(RuleId::F001, &sheet.name, address),
            RuleId::F001,
            confidence,
            &sheet.name,
            address,
            format!("儲存格為 Excel 錯誤值 {error}"),
        )
        .with_evidence(evidence)
        .with_formula(cell.formula.clone())
        .with_display_value(cell.display.clone()),
    );
}

/// F002 — 無效參照（真正的 `#REF!`）。
fn check_invalid_reference(
    sheet: &SheetData,
    cell: &CellData,
    address: &str,
    issues: &mut Vec<Issue>,
) {
    let Some(formula) = cell.formula.as_deref() else {
        return;
    };
    let parsed = formula_parser::parse(formula);
    if !parsed.has_error_literal("#REF!") {
        return;
    }

    issues.push(
        Issue::new(
            issue_id(RuleId::F002, &sheet.name, address),
            RuleId::F002,
            Confidence::High,
            &sheet.name,
            address,
            "公式包含無效參照 #REF!".to_string(),
        )
        .with_evidence(vec![
            "在公式的語法位置（非字串常值內）找到 #REF!".to_string(),
            "引號內的文字常值（例如 \"#REF!\"）不列入判斷".to_string(),
        ])
        .with_formula(Some(formula_parser::display_formula(formula)))
        .with_display_value(cell.display.clone()),
    );
}

/// F006 — 高風險函數提示（僅提示，不是錯誤）。
fn check_high_risk_functions(
    sheet: &SheetData,
    cell: &CellData,
    address: &str,
    issues: &mut Vec<Issue>,
) {
    let Some(formula) = cell.formula.as_deref() else {
        return;
    };
    let parsed = formula_parser::parse(formula);
    let mut found: Vec<&str> = HIGH_RISK_FUNCTIONS
        .iter()
        .copied()
        .filter(|name| parsed.has_function(name))
        .collect();
    if found.is_empty() {
        return;
    }
    found.sort_unstable();

    issues.push(
        Issue::new(
            issue_id(RuleId::F006, &sheet.name, address),
            RuleId::F006,
            Confidence::High,
            &sheet.name,
            address,
            format!("公式包含高風險函數：{}", found.join("、")),
        )
        .with_evidence(vec![
            format!("使用的函數：{}", found.join("、")),
            "這類函數不易靜態稽核、結果易變動或會存取外部資源，僅供審閱提示".to_string(),
            "此規則不代表公式有錯誤".to_string(),
        ])
        .with_formula(Some(formula_parser::display_formula(formula)))
        .with_display_value(cell.display.clone()),
    );
}

// ---------------------------------------------------------------------------
// 欄位模式規則：F003 / F004 / F005
// ---------------------------------------------------------------------------

/// 同一欄內的連續區段（run）：以「完全空白整列」為界。
struct Run {
    rows: Vec<u32>,
}

struct BlockEntry {
    row: u32,
    signature: Option<Signature>,
    formula: String,
    is_formula: bool,
}

fn collect_pattern_rules(sheet: &SheetData, issues: &mut Vec<Issue>) {
    let (Some(start), Some(end)) = (sheet.used_start, sheet.used_end) else {
        return;
    };

    // 一次建立「欄 → 列」索引，避免逐欄重掃整張表：
    // 成本為 O(儲存格數) 而非 O(儲存格數 × 使用欄數)。
    let mut rows_by_col: HashMap<u32, Vec<u32>> = HashMap::new();
    for (row, col) in sheet.cells.keys() {
        if *col >= start.1 && *col <= end.1 {
            rows_by_col.entry(*col).or_default().push(*row);
        }
    }

    for (col, mut rows) in rows_by_col {
        rows.sort_unstable();
        rows.dedup();
        for run in runs_from_rows(sheet, &rows) {
            analyse_run(sheet, col, &run, issues);
        }
    }
}

/// 依「不跨完全空白整列」切出連續區段（SPEC §5.2）。
fn runs_from_rows(sheet: &SheetData, filled_rows: &[u32]) -> Vec<Run> {
    let mut runs: Vec<Run> = Vec::new();
    let mut current: Vec<u32> = Vec::new();

    for row in filled_rows.iter().copied() {
        if let Some(&previous) = current.last() {
            let breaks = (previous + 1..row).any(|r| sheet.is_row_empty(r));
            if breaks {
                runs.push(Run {
                    rows: std::mem::take(&mut current),
                });
            }
        }
        current.push(row);
    }
    if !current.is_empty() {
        runs.push(Run { rows: current });
    }
    runs
}

fn entry_for(sheet: &SheetData, row: u32, col: u32) -> Option<BlockEntry> {
    let cell = sheet.cell(row, col)?;
    match cell.formula.as_deref() {
        Some(formula) => Some(BlockEntry {
            row,
            signature: Some(formula_signature::signature_at(
                formula,
                &sheet.name,
                (row, col),
            )),
            formula: formula.to_string(),
            is_formula: true,
        }),
        None => Some(BlockEntry {
            row,
            signature: None,
            formula: String::new(),
            is_formula: false,
        }),
    }
}

/// 找出主流簽章；平手或樣本不足時回傳 `None`（不報警）。
fn dominant_signature(signatures: &[String]) -> Option<(String, usize, usize)> {
    let total = signatures.len();
    if total < MIN_SAMPLE {
        return None;
    }
    // 主流簽章：取最多數者；平手或樣本不足一律回傳 None（不報警）。
    let mut tally: HashMap<&str, usize> = HashMap::new();
    for signature in signatures {
        *tally.entry(signature.as_str()).or_insert(0) += 1;
    }
    let mut best_count = 0usize;
    let mut best: Option<&str> = None;
    let mut tie = false;
    for (signature, count) in &tally {
        if *count > best_count {
            best_count = *count;
            best = Some(signature);
            tie = false;
        } else if *count == best_count {
            tie = true;
        }
    }
    let signature = best?;
    if tie || best_count < 2 {
        return None;
    }
    let ratio = best_count as f64 / total as f64;
    if ratio < MIN_DOMINANCE_RATIO {
        return None;
    }
    Some((signature.to_string(), best_count, total))
}

fn analyse_run(sheet: &SheetData, col: u32, run: &Run, issues: &mut Vec<Issue>) {
    let entries: Vec<BlockEntry> = run
        .rows
        .iter()
        .filter_map(|row| entry_for(sheet, *row, col))
        .collect();

    // 主流簽章以整個區段的可判讀公式計算（不支援語法不列入）。
    let all_signatures: Vec<String> = entries
        .iter()
        .filter_map(|e| e.signature.as_ref())
        .filter(|s| !s.unsupported)
        .map(|s| s.value.clone())
        .collect();
    let dominant = dominant_signature(&all_signatures);

    // F003：連續公式區塊
    let mut block: Vec<&BlockEntry> = Vec::new();
    let flush = |block: &mut Vec<&BlockEntry>, issues: &mut Vec<Issue>| {
        if !block.is_empty() {
            analyse_block(sheet, col, block, issues);
        }
        block.clear();
    };
    for entry in &entries {
        if entry.is_formula {
            block.push(entry);
        } else {
            flush(&mut block, issues);
        }
    }
    flush(&mut block, issues);

    if let Some((dominant_signature, dominant_count, dominant_total)) = &dominant {
        collect_constant_overrides(
            sheet,
            col,
            &entries,
            dominant_signature,
            *dominant_count,
            *dominant_total,
            issues,
        );
        collect_sequence_gaps(
            sheet,
            col,
            &entries,
            dominant_signature,
            *dominant_count,
            *dominant_total,
            issues,
        );
    }
}

/// F003 — 公式模式不一致。
fn analyse_block(sheet: &SheetData, col: u32, block: &[&BlockEntry], issues: &mut Vec<Issue>) {
    let readable: Vec<&BlockEntry> = block
        .iter()
        .copied()
        .filter(|e| e.signature.as_ref().is_some_and(|s| !s.unsupported))
        .collect();
    let signatures: Vec<String> = readable
        .iter()
        .filter_map(|e| e.signature.as_ref())
        .map(|s| s.value.clone())
        .collect();
    let Some((dominant, count, total)) = dominant_signature(&signatures) else {
        return;
    };

    let ratio = count as f64 / total as f64;
    let confidence = if ratio >= 0.8 && count >= 5 {
        Confidence::High
    } else {
        Confidence::Medium
    };

    let first = block.first().map(|e| e.row).unwrap_or_default();
    let last = block.last().map(|e| e.row).unwrap_or_default();
    let block_range = format!(
        "{}:{}",
        format_address(first, col),
        format_address(last, col)
    );

    for entry in &readable {
        let Some(signature) = entry.signature.as_ref() else {
            continue;
        };
        if signature.value == dominant {
            continue;
        }
        let address = format_address(entry.row, col);
        let mut evidence = vec![
            format!("同欄連續區塊 {block_range} 的主流簽章：{dominant}（{count}/{total} 筆）"),
            format!("本儲存格簽章：{}", signature.value),
            format!("公式：{}", formula_parser::display_formula(&entry.formula)),
            "僅為靜態模式比對，無法判斷是否為刻意變更".to_string(),
        ];
        if sheet.is_merged(entry.row, col) {
            evidence.push("此儲存格屬於合併區域，可能造成誤報".to_string());
        }

        issues.push(
            Issue::new(
                issue_id(RuleId::F003, &sheet.name, &address),
                RuleId::F003,
                confidence,
                &sheet.name,
                &address,
                "疑似公式模式不一致".to_string(),
            )
            .with_evidence(evidence)
            .with_formula(Some(formula_parser::display_formula(&entry.formula)))
            .with_display_value(
                sheet
                    .cell(entry.row, col)
                    .and_then(|c| c.display.clone()),
            ),
        );
    }
}

/// 找出區段內的主流公式簽章（供 F004/F005 判斷「上下同類公式」）。
#[allow(clippy::too_many_arguments)]
fn collect_constant_overrides(
    sheet: &SheetData,
    col: u32,
    entries: &[BlockEntry],
    dominant: &str,
    dominant_count: usize,
    dominant_total: usize,
    issues: &mut Vec<Issue>,
) {
    // F004 — 疑似硬編碼覆寫：常數被夾在同簽章的公式之間。
    for (index, entry) in entries.iter().enumerate() {
        if entry.is_formula || index == 0 {
            continue;
        }
        let previous = &entries[index - 1];
        let Some(next) = entries.get(index + 1) else {
            continue;
        };
        if !previous.is_formula || !next.is_formula {
            continue;
        }
        let previous_signature = previous
            .signature
            .as_ref()
            .filter(|s| !s.unsupported)
            .map(|s| s.value.as_str());
        let next_signature = next
            .signature
            .as_ref()
            .filter(|s| !s.unsupported)
            .map(|s| s.value.as_str());
        if previous_signature != Some(dominant) || next_signature != Some(dominant) {
            continue;
        }

        let address = format_address(entry.row, col);
        let cell = sheet.cell(entry.row, col);
        let mut evidence = vec![
            format!(
                "同欄上下相鄰公式簽章相同：{}（{dominant_count}/{dominant_total} 筆）",
                dominant
            ),
            format!(
                "上一列 {} 與下一列 {} 為同模式公式，本列為常數",
                format_address(previous.row, col),
                format_address(next.row, col)
            ),
        ];
        if let Some(value) = cell.and_then(|c| c.display.as_deref()) {
            evidence.push(format!("本列內容：{value}"));
        }
        evidence.push("可能是合法的小計、註解或輸入欄".to_string());
        if sheet.is_merged(entry.row, col) {
            evidence.push("此儲存格屬於合併區域，可能造成誤報".to_string());
        }

        issues.push(
            Issue::new(
                issue_id(RuleId::F004, &sheet.name, &address),
                RuleId::F004,
                Confidence::Medium,
                &sheet.name,
                &address,
                "疑似硬編碼覆寫（公式區塊中夾雜常數）".to_string(),
            )
            .with_evidence(evidence)
            .with_formula(None)
            .with_display_value(cell.and_then(|c| c.display.clone())),
        );
    }
}

#[allow(clippy::too_many_arguments)]
fn collect_sequence_gaps(
    sheet: &SheetData,
    col: u32,
    entries: &[BlockEntry],
    dominant: &str,
    dominant_count: usize,
    dominant_total: usize,
    issues: &mut Vec<Issue>,
) {
    // F005 — 疑似公式缺口：同一序列中被略過的空白儲存格（列本身有其他內容）。
    for window in entries.windows(2) {
        let previous = &window[0];
        let next = &window[1];
        if !previous.is_formula || !next.is_formula {
            continue;
        }
        if next.row != previous.row + 2 {
            continue;
        }
        let gap_row = previous.row + 1;
        if sheet.cell(gap_row, col).is_some() {
            continue;
        }
        // 「不跨完全空白整列推定」：該列在其他欄也沒有內容時，區段早已被切開，不得報警。
        if sheet.is_row_empty(gap_row) {
            continue;
        }
        let previous_signature = previous
            .signature
            .as_ref()
            .filter(|s| !s.unsupported)
            .map(|s| s.value.as_str());
        let next_signature = next
            .signature
            .as_ref()
            .filter(|s| !s.unsupported)
            .map(|s| s.value.as_str());
        if previous_signature != Some(dominant) || next_signature != Some(dominant) {
            continue;
        }

        let address = format_address(gap_row, col);
        issues.push(
            Issue::new(
                issue_id(RuleId::F005, &sheet.name, &address),
                RuleId::F005,
                Confidence::Low,
                &sheet.name,
                &address,
                "疑似公式缺口（序列中的空白儲存格）".to_string(),
            )
            .with_evidence(vec![
                format!(
                    "同欄序列 {} 與 {} 為同模式公式（{dominant_count}/{dominant_total} 筆）",
                    format_address(previous.row, col),
                    format_address(next.row, col)
                ),
                format!("{address} 為空白，但上下列為同模式公式"),
                "分組、小計或刻意留白常造成此情形，僅供審閱".to_string(),
            ])
            .with_formula(None)
            .with_display_value(None),
        );
    }
}

/// 由 issues 統計各嚴重度（SPEC §6 ScanSummary）。
pub fn count_by_severity(issues: &[Issue]) -> HashMap<Severity, usize> {
    let mut counts: HashMap<Severity, usize> = HashMap::new();
    for issue in issues {
        *counts.entry(issue.severity).or_insert(0) += 1;
    }
    counts
}

/// 取得包含指定儲存格的同欄**連續公式區塊**之主流簽章，供 Formula Explorer 顯示比較基準。
pub fn column_block_signature(
    sheet: &SheetData,
    row: u32,
    col: u32,
) -> Option<(String, usize, usize)> {
    let (Some(start), Some(end)) = (sheet.used_start, sheet.used_end) else {
        return None;
    };
    let formula_rows: Vec<u32> = (start.0..=end.0)
        .filter(|r| sheet.is_formula(*r, col))
        .collect();
    if formula_rows.is_empty() {
        return None;
    }

    let anchor = if formula_rows.contains(&row) {
        row
    } else {
        *formula_rows.iter().find(|r| **r > row)?
    };
    let mut block = vec![anchor];
    let mut current = anchor;
    while current > 0 && formula_rows.binary_search(&(current - 1)).is_ok() {
        current -= 1;
        block.push(current);
    }
    let mut current = anchor;
    while formula_rows.binary_search(&(current + 1)).is_ok() {
        current += 1;
        block.push(current);
    }
    block.sort_unstable();

    let mut signatures = Vec::new();
    for block_row in block {
        if let Some(formula) = sheet.formula_of(block_row, col) {
            let signature = formula_signature::signature_at(formula, &sheet.name, (block_row, col));
            if !signature.unsupported {
                signatures.push(signature.value);
            }
        }
    }
    dominant_signature(&signatures)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::workbook_reader::CellData;
    use crate::model::ValueKind;

    fn formula_cell(formula: &str) -> CellData {
        CellData {
            kind: ValueKind::Number,
            display: Some("0".to_string()),
            error: None,
            formula: Some(formula.to_string()),
        }
    }

    fn value_cell(display: &str) -> CellData {
        CellData {
            kind: ValueKind::Number,
            display: Some(display.to_string()),
            error: None,
            formula: None,
        }
    }

    fn sheet_with(cells: Vec<((u32, u32), CellData)>) -> SheetData {
        let mut map = HashMap::new();
        for (position, cell) in cells {
            map.insert(position, cell);
        }
        let used_start = map.keys().copied().reduce(|a, b| (a.0.min(b.0), a.1.min(b.1)));
        let used_end = map.keys().copied().reduce(|a, b| (a.0.max(b.0), a.1.max(b.1)));
        let formula_count = map.values().filter(|c| c.is_formula()).count();
        let constant_count = map.len() - formula_count;
        SheetData {
            index: 0,
            name: "Sheet1".to_string(),
            visible: true,
            cells: map,
            used_start,
            used_end,
            merge_regions: Vec::new(),
            formula_count,
            constant_count,
            error_count: 0,
        }
    }

    #[test]
    fn f003_flags_only_the_deviating_formula() {
        // 同一欄：連續四列公式，其中一列列位移不同。
        let sheet = sheet_with(vec![
            ((1, 2), formula_cell("B2+1")),
            ((2, 2), formula_cell("B3+1")),
            ((3, 2), formula_cell("B4+1")),
            ((4, 2), formula_cell("B9+1")),
        ]);
        let mut issues = Vec::new();
        collect_pattern_rules(&sheet, &mut issues);
        let f003: Vec<_> = issues
            .iter()
            .filter(|i| i.rule_id == RuleId::F003)
            .collect();
        assert_eq!(f003.len(), 1, "只應標記偏離主流的那一列");
        assert_eq!(f003[0].cell_address, "C5");
        assert!(f003[0].evidence.iter().any(|e| e.contains("主流簽章")));
    }

    #[test]
    fn f003_ignores_blocks_smaller_than_three_formulas() {
        let sheet = sheet_with(vec![
            ((1, 2), formula_cell("B2+1")),
            ((2, 2), formula_cell("B99+1")),
        ]);
        let mut issues = Vec::new();
        collect_pattern_rules(&sheet, &mut issues);
        assert!(issues.iter().all(|i| i.rule_id != RuleId::F003));
    }

    #[test]
    fn f003_does_not_flag_unsupported_formulas() {
        let sheet = sheet_with(vec![
            ((1, 2), formula_cell("B2+1")),
            ((2, 2), formula_cell("B3+1")),
            ((3, 2), formula_cell("B4+1")),
            ((4, 2), formula_cell("SUM(Table1[Amount])")),
        ]);
        let mut issues = Vec::new();
        collect_pattern_rules(&sheet, &mut issues);
        assert!(
            issues.iter().all(|i| i.rule_id != RuleId::F003),
            "不支援的語法必須『無法判讀』而非異常"
        );
    }

    #[test]
    fn f003_skips_when_there_is_no_dominant_pattern() {
        // 三個完全不同的簽章 → 平手，不得報警。
        let sheet = sheet_with(vec![
            ((1, 2), formula_cell("B2")),
            ((2, 2), formula_cell("C3")),
            ((3, 2), formula_cell("D4")),
        ]);
        let mut issues = Vec::new();
        collect_pattern_rules(&sheet, &mut issues);
        assert!(issues.iter().all(|i| i.rule_id != RuleId::F003));
    }

    #[test]
    fn f004_flags_constant_sandwiched_between_formulas() {
        let sheet = sheet_with(vec![
            ((1, 2), formula_cell("B2+1")),
            ((2, 2), formula_cell("B3+1")),
            ((3, 2), value_cell("123")),
            ((4, 2), formula_cell("B5+1")),
            ((5, 2), formula_cell("B6+1")),
        ]);
        let mut issues = Vec::new();
        collect_pattern_rules(&sheet, &mut issues);
        let f004: Vec<_> = issues
            .iter()
            .filter(|i| i.rule_id == RuleId::F004)
            .collect();
        assert_eq!(f004.len(), 1);
        assert_eq!(f004[0].cell_address, "C4");
        assert_eq!(f004[0].severity, Severity::Medium);
        assert!(f004[0]
            .evidence
            .iter()
            .any(|e| e.contains("小計") || e.contains("註解")));
    }

    #[test]
    fn f004_does_not_fire_at_the_edge_of_a_block() {
        let sheet = sheet_with(vec![
            ((1, 2), value_cell("header")),
            ((2, 2), formula_cell("B2+1")),
            ((3, 2), formula_cell("B3+1")),
            ((4, 2), formula_cell("B4+1")),
        ]);
        let mut issues = Vec::new();
        collect_pattern_rules(&sheet, &mut issues);
        assert!(issues.iter().all(|i| i.rule_id != RuleId::F004));
    }

    #[test]
    fn f005_flags_gap_only_when_row_is_not_empty_elsewhere() {
        // 中間列完全空白 → 區段被切開，不應產生 F005。
        let sheet = sheet_with(vec![
            ((1, 2), formula_cell("B2+1")),
            ((2, 2), formula_cell("B3+1")),
            ((4, 2), formula_cell("B5+1")),
            ((5, 2), formula_cell("B6+1")),
        ]);
        let mut issues = Vec::new();
        collect_pattern_rules(&sheet, &mut issues);
        assert!(
            issues.iter().all(|i| i.rule_id != RuleId::F005),
            "不得跨完全空白整列推定"
        );
    }

    #[test]
    fn f005_flags_gap_when_row_has_other_content() {
        // 中間列在別的欄有內容（例如註解），本欄空白 → F005。
        let sheet = sheet_with(vec![
            ((1, 2), formula_cell("B2+1")),
            ((2, 2), formula_cell("B3+1")),
            ((3, 5), value_cell("note")),
            ((4, 2), formula_cell("B5+1")),
            ((5, 2), formula_cell("B6+1")),
        ]);
        let mut issues = Vec::new();
        collect_pattern_rules(&sheet, &mut issues);
        let f005: Vec<_> = issues
            .iter()
            .filter(|i| i.rule_id == RuleId::F005)
            .collect();
        assert_eq!(f005.len(), 1);
        assert_eq!(f005[0].cell_address, "C4");
        assert_eq!(f005[0].confidence, Confidence::Low);
    }

    #[test]
    fn f005_never_fires_across_a_fully_blank_row() {
        let sheet = sheet_with(vec![
            ((0, 2), formula_cell("B1+1")),
            ((3, 2), formula_cell("B4+1")),
            ((4, 2), formula_cell("B5+1")),
            ((5, 2), formula_cell("B6+1")),
        ]);
        let mut issues = Vec::new();
        collect_pattern_rules(&sheet, &mut issues);
        assert!(issues.iter().all(|i| i.rule_id != RuleId::F005));
    }

    #[test]
    fn severity_counts_are_aggregated() {
        let issues = vec![
            Issue::new("a", RuleId::F001, Confidence::High, "S", "A1", "x"),
            Issue::new("b", RuleId::F006, Confidence::High, "S", "A2", "x"),
        ];
        let counts = count_by_severity(&issues);
        assert_eq!(counts.get(&Severity::High), Some(&1));
        assert_eq!(counts.get(&Severity::Info), Some(&1));
    }

    #[test]
    fn issue_sorting_is_stable_by_sheet_and_address() {
        let mut issues = vec![
            Issue::new("z", RuleId::F001, Confidence::High, "B", "A1", "x"),
            Issue::new("a", RuleId::F001, Confidence::High, "A", "D12", "x"),
            Issue::new("b", RuleId::F001, Confidence::High, "A", "A1", "x"),
        ];
        sort_issues(&mut issues);
        let keys: Vec<(String, String)> = issues
            .iter()
            .map(|i| (i.sheet_name.clone(), i.cell_address.clone()))
            .collect();
        assert_eq!(
            keys,
            vec![
                ("A".to_string(), "A1".to_string()),
                ("A".to_string(), "D12".to_string()),
                ("B".to_string(), "A1".to_string()),
            ]
        );
    }
}

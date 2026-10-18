//! 整合測試：規則引擎（SPEC §9 AC02–AC06、AC11）。
//!
//! 每個測試都使用 SPEC §10 的 Fixture，並驗證「不得誤報」的邊界行為。

mod common;

use std::sync::atomic::AtomicBool;

use excel_formula_inspector_lib::model::{Confidence, Issue, RuleId, Severity};
use excel_formula_inspector_lib::rules;
use excel_formula_inspector_lib::workbook_reader::{
    self, ProgressUpdate, ReadOptions, WorkbookData,
};

fn scan(path: &std::path::Path) -> Vec<Issue> {
    let cancel = AtomicBool::new(false);
    let workbook: WorkbookData = workbook_reader::read_workbook(
        path,
        ReadOptions { allow_large: false },
        &cancel,
        &mut |_: ProgressUpdate| {},
    )
    .expect("讀取 fixture");
    rules::run_rules(&workbook, &cancel, &mut |_: ProgressUpdate| {}).expect("執行規則")
}

fn by_rule(issues: &[Issue], rule: RuleId) -> Vec<&Issue> {
    issues.iter().filter(|i| i.rule_id == rule).collect()
}

fn addresses(issues: &[&Issue]) -> Vec<String> {
    issues.iter().map(|i| i.cell_address.clone()).collect()
}

#[test]
fn ac02_error_cells_produce_f001_with_class_and_position() {
    let issues = scan(&common::invalid_reference());
    let f001 = by_rule(&issues, RuleId::F001);
    let mut found = addresses(&f001);
    found.sort();
    assert_eq!(found, vec!["B2".to_string(), "B3".to_string()]);

    let ref_error = f001
        .iter()
        .find(|i| i.cell_address == "B2")
        .expect("B2 錯誤");
    assert_eq!(ref_error.severity, Severity::High);
    assert!(ref_error.message.contains("#REF!"));
    assert!(ref_error
        .evidence
        .iter()
        .any(|e| e.contains("快取錯誤值可能")));
}

#[test]
fn ac03_ref_error_detected_but_not_inside_string_literals() {
    let issues = scan(&common::invalid_reference());
    let f002 = by_rule(&issues, RuleId::F002);
    assert_eq!(addresses(&f002), vec!["B2".to_string()], "只有真正的 #REF! 觸發 F002");
    assert!(f002[0]
        .evidence
        .iter()
        .any(|e| e.contains("引號內的文字常值")));

    // B4 的公式 =B3&"#REF! literal" 不得被標記
    assert!(!issues
        .iter()
        .any(|i| i.rule_id == RuleId::F002 && i.cell_address == "B4"));
    // 純文字常值 "#REF!" 也不得被標記
    assert!(!issues
        .iter()
        .any(|i| i.rule_id == RuleId::F002 && i.cell_address == "B5"));
}

#[test]
fn ac04_pattern_deviation_produces_f003_with_dominant_evidence() {
    let issues = scan(&common::pattern_anomaly());
    let f003 = by_rule(&issues, RuleId::F003);
    assert_eq!(addresses(&f003), vec!["C6".to_string()]);
    assert_eq!(f003[0].severity, Severity::Medium);
    assert!(f003[0].message.contains("疑似"));
    assert!(f003[0]
        .evidence
        .iter()
        .any(|e| e.contains("主流簽章") && e.contains("4/5")));
    assert!(f003[0].formula.as_deref().unwrap_or_default().starts_with('='));
}

#[test]
fn ac05_constant_override_produces_f004_with_confidence_and_caveat() {
    let issues = scan(&common::pattern_anomaly());
    let f004 = by_rule(&issues, RuleId::F004);
    assert_eq!(addresses(&f004), vec!["D4".to_string()]);
    assert_eq!(f004[0].severity, Severity::Medium);
    assert_eq!(f004[0].confidence, Confidence::Medium);
    assert!(f004[0].message.contains("疑似"));
    assert!(f004[0]
        .evidence
        .iter()
        .any(|e| e.contains("小計") || e.contains("註解") || e.contains("輸入欄")));
}

#[test]
fn ac06_gap_rule_respects_fully_blank_rows() {
    let issues = scan(&common::intentional_breaks());
    let f005 = by_rule(&issues, RuleId::F005);
    let found = addresses(&f005);
    assert_eq!(
        found,
        vec!["C9".to_string()],
        "只標記同列其他欄有內容的缺口：{found:?}"
    );
    assert!(f005[0].message.contains("疑似"));
    assert_eq!(f005[0].confidence, Confidence::Low);

    // 完全空白列（第 7 列）不得被推定為缺口
    assert!(!found.contains(&"C7".to_string()));
}

#[test]
fn intentional_breaks_do_not_produce_pattern_false_positives() {
    let issues = scan(&common::intentional_breaks());
    let f003 = by_rule(&issues, RuleId::F003);
    assert!(
        f003.is_empty(),
        "所有公式同模式，不應產生 F003：{:?}",
        addresses(&f003)
    );

    // 合計列（C11）確實是常數夾在公式之間 → 允許 F004，但必須明示「疑似」
    let f004 = by_rule(&issues, RuleId::F004);
    assert_eq!(addresses(&f004), vec!["C11".to_string()]);
    assert!(f004[0].message.contains("疑似"));
}

#[test]
fn ac11_unsupported_syntax_is_not_reported_as_anomaly() {
    let issues = scan(&common::special_formulas());
    assert!(
        by_rule(&issues, RuleId::F003).is_empty(),
        "不支援語法必須『無法判讀』而非異常"
    );
    assert!(by_rule(&issues, RuleId::F001).is_empty());
    assert!(by_rule(&issues, RuleId::F002).is_empty());
}

#[test]
fn ac07_cancellation_aborts_rule_execution() {
    let cancel = AtomicBool::new(false);
    let workbook = workbook_reader::read_workbook(
        &common::pattern_anomaly(),
        ReadOptions { allow_large: false },
        &cancel,
        &mut |_: ProgressUpdate| {},
    )
    .expect("讀取");
    cancel.store(true, std::sync::atomic::Ordering::Relaxed);
    let result = rules::run_rules(&workbook, &cancel, &mut |_: ProgressUpdate| {});
    assert!(
        matches!(
            result,
            Err(excel_formula_inspector_lib::errors::ScanError::Cancelled)
        ),
        "取消後應立即停止"
    );
}

#[test]
fn f006_high_risk_functions_are_reported_as_info() {
    let path = common::write_workbook("high-risk.xlsx", |workbook| {
        let sheet = workbook.add_worksheet();
        sheet.set_name("Risk")?;
        sheet.write_number(0, 0, 1.0)?;
        sheet.write_formula(0, 1, common::formula("=INDIRECT(\"A1\")"))?;
        sheet.write_formula(1, 1, common::formula("=OFFSET(A1,1,1)"))?;
        sheet.write_formula(2, 1, common::formula("=A1+NOW()"))?;
        sheet.write_formula(3, 1, common::formula("=SUM(A1:A2)"))?;
        Ok(())
    });
    let issues = scan(&path);
    let f006 = by_rule(&issues, RuleId::F006);
    assert_eq!(f006.len(), 3, "三個高風險函數公式各一筆提示");
    assert!(f006.iter().all(|i| i.severity == Severity::Info));
    assert!(f006
        .iter()
        .any(|i| i.message.contains("INDIRECT")));
    assert!(f006.iter().any(|i| i.message.contains("OFFSET")));
    assert!(f006.iter().any(|i| i.message.contains("NOW")));
    assert!(f006
        .iter()
        .all(|i| i.evidence.iter().any(|e| e.contains("不代表公式有錯誤"))));
    // INDIRECT/OFFSET/NOW 位於同一欄且都是不同函數，不應觸發 F003
    assert!(by_rule(&issues, RuleId::F003).is_empty());
}

#[test]
fn normal_workbook_has_no_issues_at_all() {
    let issues = scan(&common::normal_formulas());
    assert!(
        issues.is_empty(),
        "一般公式不應產生任何問題：{:?}",
        issues
            .iter()
            .map(|i| (i.rule_id, i.cell_address.clone()))
            .collect::<Vec<_>>()
    );
}

#[test]
fn every_issue_carries_evidence_and_traceable_id() {
    let issues = scan(&common::pattern_anomaly());
    assert!(!issues.is_empty());
    for issue in &issues {
        assert!(!issue.evidence.is_empty(), "{} 缺少證據", issue.id);
        assert!(issue.id.starts_with(issue.rule_id.as_str()));
        assert!(issue.id.contains(&issue.cell_address));
        assert!(!issue.message.is_empty());
    }
}

#[test]
fn severity_counts_cover_all_levels() {
    let issues = scan(&common::intentional_breaks());
    let counts = rules::count_by_severity(&issues);
    assert!(
        counts.values().sum::<usize>() == issues.len(),
        "統計總數需等於問題數"
    );
}

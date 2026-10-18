//! 匯出（SPEC §2.1-6、§8.1）。
//!
//! - CSV 使用 UTF-8 BOM，讓 Excel 直接以正確編碼開啟。
//! - 防止 CSV 公式注入：以 `=`, `+`, `-`, `@`, Tab, CR 開頭的欄位加上 `'` 前綴。
//! - 匯出內容僅為檢查結果，不含未選擇的原始工作簿資料（SPEC §1.3）。

use std::fs;
use std::path::{Path, PathBuf};

use crate::errors::{ScanError, ScanResult};
use crate::model::{ExportFormat, Issue};

/// CSV 欄位順序（固定，避免欄位漂移）。
pub const CSV_HEADERS: [&str; 9] = [
    "嚴重度(high/medium/low/info)",
    "規則ID",
    "工作表",
    "儲存格",
    "信心程度(high/medium/low)",
    "說明",
    "公式",
    "快取值",
    "判斷依據",
];

pub fn severity_code(issue: &Issue) -> &'static str {
    match issue.severity {
        crate::model::Severity::High => "high",
        crate::model::Severity::Medium => "medium",
        crate::model::Severity::Low => "low",
        crate::model::Severity::Info => "info",
    }
}

pub fn confidence_code(issue: &Issue) -> &'static str {
    match issue.confidence {
        crate::model::Confidence::High => "high",
        crate::model::Confidence::Medium => "medium",
        crate::model::Confidence::Low => "low",
    }
}

/// 產生 CSV 文字（含 BOM）。
pub fn to_csv(issues: &[Issue]) -> String {
    let mut out = String::from("\u{feff}");
    out.push_str(&CSV_HEADERS.join(","));
    out.push_str("\r\n");

    for issue in issues {
        let fields = [
            severity_code(issue).to_string(),
            issue.rule_id.as_str().to_string(),
            issue.sheet_name.clone(),
            issue.cell_address.clone(),
            confidence_code(issue).to_string(),
            issue.message.clone(),
            issue.formula.clone().unwrap_or_default(),
            issue.display_value.clone().unwrap_or_default(),
            issue.evidence.join(" | "),
        ];
        let row: Vec<String> = fields.iter().map(|field| escape_csv_field(field)).collect();
        out.push_str(&row.join(","));
        out.push_str("\r\n");
    }
    out
}

/// 單一 CSV 欄位：先做公式注入防護，再處理引號／逗號／換行。
pub fn escape_csv_field(value: &str) -> String {
    let dangerous = value
        .chars()
        .next()
        .is_some_and(|c| matches!(c, '=' | '+' | '-' | '@' | '\t' | '\r'));
    let guarded = if dangerous {
        format!("'{value}")
    } else {
        value.to_string()
    };

    if guarded.contains([',', '"', '\n', '\r']) {
        format!("\"{}\"", guarded.replace('"', "\"\""))
    } else {
        guarded
    }
}

/// 產生 JSON 文字（UTF-8，不含 BOM）。
pub fn to_json(issues: &[Issue]) -> ScanResult<String> {
    serde_json::to_string_pretty(issues).map_err(|e| ScanError::Internal {
        detail: format!("JSON 序列化失敗：{e}"),
    })
}

/// 依格式補上副檔名。
pub fn ensure_extension(path: &Path, format: ExportFormat) -> PathBuf {
    let wanted = match format {
        ExportFormat::Csv => "csv",
        ExportFormat::Json => "json",
    };
    let has_extension = path
        .extension()
        .and_then(|e| e.to_str())
        .map(|e| e.eq_ignore_ascii_case(wanted))
        .unwrap_or(false);
    if has_extension {
        path.to_path_buf()
    } else {
        path.with_extension(wanted)
    }
}

/// 寫出檔案，回傳實際寫入路徑。
pub fn write_export(path: &Path, format: ExportFormat, issues: &[Issue]) -> ScanResult<String> {
    let target = ensure_extension(path, format);
    let contents = match format {
        ExportFormat::Csv => to_csv(issues),
        ExportFormat::Json => to_json(issues)?,
    };

    fs::write(&target, contents.as_bytes()).map_err(|e| ScanError::Io {
        detail: format!("寫入匯出檔失敗：{}", e.kind()),
    })?;

    Ok(target.to_string_lossy().to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{Confidence, RuleId};

    fn sample_issue() -> Issue {
        Issue::new(
            "F001:Sheet1!A1",
            RuleId::F001,
            Confidence::High,
            "Sheet1",
            "A1",
            "儲存格為 Excel 錯誤值 #DIV/0!",
        )
        .with_evidence(vec!["型別為錯誤值".to_string(), "快取值可能過期".to_string()])
        .with_formula(Some("=1/0".to_string()))
        .with_display_value(Some("#DIV/0!".to_string()))
    }

    #[test]
    fn csv_starts_with_bom_and_header() {
        let csv = to_csv(&[sample_issue()]);
        assert!(csv.starts_with('\u{feff}'), "CSV 需有 UTF-8 BOM");
        let header = csv.lines().next().unwrap();
        assert!(header.contains("規則ID"));
        assert!(header.contains("判斷依據"));
    }

    #[test]
    fn csv_has_one_row_per_issue_with_all_columns() {
        let csv = to_csv(&[sample_issue()]);
        let lines: Vec<&str> = csv.lines().collect();
        assert_eq!(lines.len(), 2);
        let fields: Vec<&str> = lines[1].split(',').collect();
        assert_eq!(fields.len(), CSV_HEADERS.len());
        assert_eq!(fields[0], "high");
        assert_eq!(fields[1], "F001");
        assert_eq!(fields[2], "Sheet1");
        assert_eq!(fields[3], "A1");
    }

    #[test]
    fn csv_guards_against_formula_injection() {
        assert_eq!(escape_csv_field("=1+1"), "'=1+1");
        assert_eq!(escape_csv_field("+SUM(A1)"), "'+SUM(A1)");
        assert_eq!(escape_csv_field("-2"), "'-2");
        assert_eq!(escape_csv_field("@cmd"), "'@cmd");
        assert_eq!(escape_csv_field("正常文字"), "正常文字");
    }

    #[test]
    fn csv_quotes_commas_quotes_and_newlines() {
        assert_eq!(escape_csv_field("a,b"), "\"a,b\"");
        assert_eq!(escape_csv_field("say \"hi\""), "\"say \"\"hi\"\"\"");
        assert_eq!(escape_csv_field("line1\nline2"), "\"line1\nline2\"");
        // 危險前綴 + 逗號需同時處理
        assert_eq!(escape_csv_field("=a,b"), "\"'=a,b\"");
    }

    #[test]
    fn json_export_is_utf8_without_bom_and_has_camel_case() {
        let json = to_json(&[sample_issue()]).expect("json");
        assert!(!json.starts_with('\u{feff}'));
        let value: serde_json::Value = serde_json::from_str(&json).expect("parse");
        assert_eq!(value[0]["ruleId"], "F001");
        assert_eq!(value[0]["confidence"], "high");
    }

    #[test]
    fn extension_is_normalized() {
        assert_eq!(
            ensure_extension(Path::new("out"), ExportFormat::Csv),
            PathBuf::from("out.csv")
        );
        assert_eq!(
            ensure_extension(Path::new("out.CSV"), ExportFormat::Csv),
            PathBuf::from("out.CSV")
        );
        assert_eq!(
            ensure_extension(Path::new("out.txt"), ExportFormat::Json),
            PathBuf::from("out.json")
        );
    }

    #[test]
    fn evidence_is_joined_readably() {
        let csv = to_csv(&[sample_issue()]);
        assert!(csv.contains("型別為錯誤值 | 快取值可能過期"));
    }

    #[test]
    fn empty_issue_list_still_produces_header() {
        let csv = to_csv(&[]);
        assert_eq!(csv.lines().count(), 1);
    }
}

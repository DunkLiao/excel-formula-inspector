//! 對外資料模型（SPEC §6）與 IPC 傳輸型別。
//!
//! 前端只持有對應的 TypeScript 鏡像（`src/types.ts`），規則邏輯一律在本模組所屬的
//! Rust 端完成。

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Severity {
    High,
    Medium,
    Low,
    Info,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Confidence {
    High,
    Medium,
    Low,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
pub enum RuleId {
    F001,
    F002,
    F003,
    F004,
    F005,
    F006,
}

impl RuleId {
    pub const ALL: [RuleId; 6] = [
        RuleId::F001,
        RuleId::F002,
        RuleId::F003,
        RuleId::F004,
        RuleId::F005,
        RuleId::F006,
    ];

    pub fn as_str(self) -> &'static str {
        match self {
            RuleId::F001 => "F001",
            RuleId::F002 => "F002",
            RuleId::F003 => "F003",
            RuleId::F004 => "F004",
            RuleId::F005 => "F005",
            RuleId::F006 => "F006",
        }
    }

    /// 預設嚴重度（SPEC §5）。
    pub fn default_severity(self) -> Severity {
        match self {
            RuleId::F001 | RuleId::F002 => Severity::High,
            RuleId::F003 | RuleId::F004 => Severity::Medium,
            RuleId::F005 => Severity::Low,
            RuleId::F006 => Severity::Info,
        }
    }

    pub fn title(self) -> &'static str {
        match self {
            RuleId::F001 => "Excel 錯誤值",
            RuleId::F002 => "無效參照",
            RuleId::F003 => "公式模式不一致",
            RuleId::F004 => "疑似硬編碼覆寫",
            RuleId::F005 => "疑似公式缺口",
            RuleId::F006 => "高風險函數提示",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Issue {
    pub id: String,
    pub rule_id: RuleId,
    pub severity: Severity,
    pub confidence: Confidence,
    pub sheet_name: String,
    pub cell_address: String,
    pub message: String,
    pub evidence: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub formula: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub display_value: Option<String>,
}

impl Issue {
    pub fn new(
        id: impl Into<String>,
        rule_id: RuleId,
        confidence: Confidence,
        sheet_name: impl Into<String>,
        cell_address: impl Into<String>,
        message: impl Into<String>,
    ) -> Self {
        Self {
            id: id.into(),
            rule_id,
            severity: rule_id.default_severity(),
            confidence,
            sheet_name: sheet_name.into(),
            cell_address: cell_address.into(),
            message: message.into(),
            evidence: Vec::new(),
            formula: None,
            display_value: None,
        }
    }

    pub fn with_evidence<I, S>(mut self, evidence: I) -> Self
    where
        I: IntoIterator<Item = S>,
        S: Into<String>,
    {
        self.evidence = evidence.into_iter().map(Into::into).collect();
        self
    }

    pub fn with_formula(mut self, formula: Option<String>) -> Self {
        self.formula = formula;
        self
    }

    pub fn with_display_value(mut self, value: Option<String>) -> Self {
        self.display_value = value;
        self
    }

    pub fn with_severity(mut self, severity: Severity) -> Self {
        self.severity = severity;
        self
    }
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ScanSummary {
    pub sheet_count: usize,
    pub scanned_cell_count: usize,
    pub formula_count: usize,
    pub constant_count: usize,
    pub issues_by_severity: BTreeMap<Severity, usize>,
    pub duration_ms: u64,
    pub warnings: Vec<String>,
}

/// 儲存格值的種類；僅供檢視，不用於驗證計算結果。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum ValueKind {
    Empty,
    Text,
    Number,
    Bool,
    DateTime,
    Duration,
    Error,
    Unsupported,
}

/// 工作表摘要（SPEC §4.2 工作表清單所需，非 SPEC §6 的核心模型）。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SheetInfo {
    pub index: usize,
    pub name: String,
    /// 已使用範圍（A1 記法字串，例如 `A1:F1200`）；空工作表為 None。
    pub used_range: Option<String>,
    pub row_count: u32,
    pub col_count: u32,
    pub formula_count: usize,
    pub constant_count: usize,
    pub error_count: usize,
    pub issue_count: usize,
}

/// 掃描完成後回傳給前端的統包（issues 有上限，超過時 `issues_truncated` 為 true）。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ScanPayload {
    pub scan_id: String,
    pub file_name: String,
    pub file_size: u64,
    pub summary: ScanSummary,
    pub sheets: Vec<SheetInfo>,
    pub issues: Vec<Issue>,
    pub issues_truncated: bool,
}

/// 網格中的單一儲存格（SPEC §2.1-5 網格預覽、§4.4 鄰近公式比較）。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GridCell {
    pub row: u32,
    pub col: u32,
    pub address: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub display: Option<String>,
    pub kind: ValueKind,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub formula: Option<String>,
    /// 公式簽章（供鄰近公式模式比較；無法判讀的公式為 `None`）。
    #[serde(skip_serializing_if = "Option::is_none")]
    pub signature: Option<String>,
    pub issue_ids: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GridWindow {
    pub sheet_name: String,
    pub start_row: u32,
    pub start_col: u32,
    pub row_count: u32,
    pub col_count: u32,
    pub cells: Vec<GridCell>,
}

/// 公式清單的一筆（SPEC §2.1-3）。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FormulaRecord {
    pub sheet_name: String,
    pub cell_address: String,
    pub row: u32,
    pub col: u32,
    pub formula: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub display_value: Option<String>,
    pub kind: ValueKind,
    pub issue_ids: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub signature: Option<String>,
}

/// 公式清單查詢（分頁 + 篩選；在 Rust 端過濾以避免前端持有整份清單）。
#[derive(Debug, Clone, Default, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct FormulaQuery {
    pub sheet_name: Option<String>,
    /// 位址關鍵字（不分大小寫，比對開頭，例如 `D12`、`D`）。
    pub address: Option<String>,
    /// 公式原文關鍵字（不分大小寫，substring）。
    pub text: Option<String>,
    /// 僅列出被任一條規則標記的公式。
    pub only_issues: bool,
    pub offset: usize,
    pub limit: usize,
}

impl FormulaQuery {
    /// 未指定 `limit`（0）時使用預設分頁大小，避免一次回傳整份清單。
    pub fn normalized_limit(&self) -> usize {
        if self.limit == 0 {
            DEFAULT_PAGE_SIZE
        } else {
            self.limit.clamp(1, MAX_PAGE_SIZE)
        }
    }
}

pub const MAX_PAGE_SIZE: usize = 500;
/// 未指定 limit 時的分頁大小。
pub const DEFAULT_PAGE_SIZE: usize = 100;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FormulaPage {
    pub total: usize,
    pub offset: usize,
    pub limit: usize,
    pub items: Vec<FormulaRecord>,
}

/// 儲存格上下文（SPEC §4.4：問題解釋與相鄰 3–5 列公式模式比較）。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CellContext {
    pub sheet_name: String,
    pub cell: GridCell,
    pub issues: Vec<Issue>,
    /// 同一欄、上下相鄰的公式列（最多 ±`span` 列），供模式比較。
    pub neighbours: Vec<GridCell>,
    /// 主流簽章（若該欄區塊可判定）。
    #[serde(skip_serializing_if = "Option::is_none")]
    pub dominant_signature: Option<String>,
    /// 公式的直接參照（僅列出可靜態解析的語法；無法判讀時為空）。
    pub refs: Vec<String>,
    /// 無法判讀的原因（若公式含不支援語法）。
    #[serde(skip_serializing_if = "Option::is_none")]
    pub unsupported_reason: Option<String>,
}

/// 匯出格式（SPEC §2.1-6）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ExportFormat {
    Csv,
    Json,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ExportRequest {
    pub format: ExportFormat,
    /// 使用者選擇的輸出路徑（由前端 dialog 取得）。
    pub path: String,
    pub issues: Vec<Issue>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn severity_serializes_lowercase_as_map_key() {
        // SPEC §6 的 `Record<Severity, number>` 需與前端鏡像完全一致。
        let mut map: BTreeMap<Severity, usize> = BTreeMap::new();
        map.insert(Severity::High, 2);
        map.insert(Severity::Info, 1);
        let json = serde_json::to_string(&map).expect("serialize");
        assert_eq!(json, r#"{"high":2,"info":1}"#);
    }

    #[test]
    fn issue_serializes_camel_case() {
        let issue = Issue::new(
            "F001:Sheet1!A1",
            RuleId::F001,
            Confidence::High,
            "Sheet1",
            "A1",
            "#DIV/0!",
        )
        .with_display_value(Some("#DIV/0!".into()));
        let value = serde_json::to_value(&issue).expect("serialize");
        assert_eq!(value["ruleId"], "F001");
        assert_eq!(value["sheetName"], "Sheet1");
        assert_eq!(value["cellAddress"], "A1");
        assert_eq!(value["displayValue"], "#DIV/0!");
        assert!(value.get("formula").is_none(), "None 欄位不應序列化");
    }

    #[test]
    fn rule_id_defaults_match_spec_table() {
        assert_eq!(RuleId::F001.default_severity(), Severity::High);
        assert_eq!(RuleId::F002.default_severity(), Severity::High);
        assert_eq!(RuleId::F003.default_severity(), Severity::Medium);
        assert_eq!(RuleId::F004.default_severity(), Severity::Medium);
        assert_eq!(RuleId::F005.default_severity(), Severity::Low);
        assert_eq!(RuleId::F006.default_severity(), Severity::Info);
    }

    #[test]
    fn query_limit_is_clamped() {
        // 未指定 limit 時使用預設分頁大小，避免回傳整份清單。
        assert_eq!(FormulaQuery::default().normalized_limit(), DEFAULT_PAGE_SIZE);
        let q = FormulaQuery {
            limit: 100_000,
            ..Default::default()
        };
        assert_eq!(q.normalized_limit(), MAX_PAGE_SIZE);
        let q = FormulaQuery {
            limit: 25,
            ..Default::default()
        };
        assert_eq!(q.normalized_limit(), 25);
    }
}

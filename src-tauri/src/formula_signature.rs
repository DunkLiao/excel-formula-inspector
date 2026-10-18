//! 公式參照簽章（SPEC §5.1）。
//!
//! 將公式中**同一欄的相對參照**正規化為「相對於公式儲存格」的行列位移，絕對參照保留絕對索引，
//! 混合參照分別處理，藉此讓「複製貼上式」的公式產生相同簽章，偏離者才被標記。
//!
//! 正規化決策（已寫入測試）：
//! - 數字常值一律取代為 `#NUM`：同一欄的稅率、門檻值等常數本來就可能逐列不同，不宜視為異常。
//! - 字串常值**保留原文**：字串通常帶有語意（標籤、關鍵字、查表鍵），變更值得提示。
//! - 工作表前置詞：同表 → `~`，跨表 → `@工作表名稱`，未指明 → 無標記。

use crate::formula_parser::{self, CellRef, ParsedFormula, Token};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Signature {
    pub value: String,
    /// 公式含不支援語法；規則層必須輸出「無法判讀」而非異常（SPEC §5.1、§13.3）。
    pub unsupported: bool,
    pub unsupported_reason: Option<String>,
}

impl Signature {
    fn unsupported(reason: String) -> Self {
        Self {
            value: "<unsupported>".to_string(),
            unsupported: true,
            unsupported_reason: Some(reason),
        }
    }
}

/// 產生公式簽章。`current_sheet` 為公式所在工作表，用於區分同表／跨表參照。
pub fn signature(formula: &str, current_sheet: &str) -> Signature {
    let parsed = formula_parser::parse(formula);
    signature_of(&parsed, current_sheet)
}

/// 由已解析的公式產生簽章（避免重複解析）。
pub fn signature_of(parsed: &ParsedFormula, current_sheet: &str) -> Signature {
    if parsed.has_unsupported {
        let reason = parsed
            .unsupported_reasons
            .first()
            .cloned()
            .unwrap_or_else(|| "未支援的公式語法".to_string());
        return Signature::unsupported(reason);
    }

    let mut out = String::new();
    for token in &parsed.tokens {
        match token {
            Token::Number(_) => out.push_str("#NUM"),
            Token::Text(value) => {
                out.push('"');
                out.push_str(value);
                out.push('"');
            }
            Token::ErrorLiteral(value) => {
                out.push_str("#ERR");
                out.push_str(value);
            }
            Token::Function(name) => {
                out.push_str(name);
                out.push('(');
            }
            Token::Name(name) => {
                out.push_str("name:");
                out.push_str(name);
            }
            Token::Cell(cell) => out.push_str(&cell_signature(cell, current_sheet)),
            Token::CellRange { start, end } => {
                out.push_str(&cell_signature(start, current_sheet));
                out.push(':');
                out.push_str(&cell_signature(end, current_sheet));
            }
            Token::WholeColRange {
                sheet,
                start_col,
                end_col,
                start_absolute,
                end_absolute,
            } => {
                out.push_str(&sheet_marker(sheet.as_deref(), current_sheet));
                out.push_str(&col_part(*start_col, *start_absolute, None));
                out.push(':');
                out.push_str(&sheet_marker(sheet.as_deref(), current_sheet));
                out.push_str(&col_part(*end_col, *end_absolute, None));
            }
            Token::WholeRowRange {
                sheet,
                start_row,
                end_row,
                start_absolute,
                end_absolute,
            } => {
                out.push_str(&sheet_marker(sheet.as_deref(), current_sheet));
                out.push_str(&row_part(*start_row, *start_absolute, None));
                out.push(':');
                out.push_str(&sheet_marker(sheet.as_deref(), current_sheet));
                out.push_str(&row_part(*end_row, *end_absolute, None));
            }
            Token::Unsupported { .. } => return Signature::unsupported("未支援的公式語法".to_string()),
            Token::Op(op) => {
                // 空白（交集運算子）以單一符號表示，避免多空白造成簽章不同。
                if op.trim().is_empty() {
                    out.push(' ');
                } else {
                    out.push_str(op);
                }
            }
        }
    }

    Signature {
        value: out,
        unsupported: false,
        unsupported_reason: None,
    }
}

fn sheet_marker(sheet: Option<&str>, current_sheet: &str) -> String {
    match sheet {
        None => String::new(),
        Some(name) if name.eq_ignore_ascii_case(current_sheet) => "~".to_string(),
        Some(name) => format!("@{name}"),
    }
}

fn col_part(col: u32, absolute: bool, origin_col: Option<u32>) -> String {
    if absolute {
        format!("C${col}")
    } else {
        let delta = i64::from(col) - i64::from(origin_col.unwrap_or(0));
        format!("C{delta:+}")
    }
}

fn row_part(row: u32, absolute: bool, origin_row: Option<u32>) -> String {
    if absolute {
        format!("R${row}")
    } else {
        let delta = i64::from(row) - i64::from(origin_row.unwrap_or(0));
        format!("R{delta:+}")
    }
}

/// 相對位移需以「公式所在儲存格」為原點；因此簽章計算需要 caller 提供原點座標。
pub fn signature_at(formula: &str, current_sheet: &str, origin: (u32, u32)) -> Signature {
    let parsed = formula_parser::parse(formula);
    signature_at_of(&parsed, current_sheet, origin)
}

pub fn signature_at_of(
    parsed: &ParsedFormula,
    current_sheet: &str,
    origin: (u32, u32),
) -> Signature {
    if parsed.has_unsupported {
        let reason = parsed
            .unsupported_reasons
            .first()
            .cloned()
            .unwrap_or_else(|| "未支援的公式語法".to_string());
        return Signature::unsupported(reason);
    }
    let mut out = String::new();
    for token in &parsed.tokens {
        match token {
            Token::Cell(cell) => out.push_str(&cell_signature_at(cell, current_sheet, origin)),
            Token::CellRange { start, end } => {
                out.push_str(&cell_signature_at(start, current_sheet, origin));
                out.push(':');
                out.push_str(&cell_signature_at(end, current_sheet, origin));
            }
            Token::WholeColRange {
                sheet,
                start_col,
                end_col,
                start_absolute,
                end_absolute,
            } => {
                out.push_str(&sheet_marker(sheet.as_deref(), current_sheet));
                out.push_str(&col_part(*start_col, *start_absolute, Some(origin.1)));
                out.push(':');
                out.push_str(&sheet_marker(sheet.as_deref(), current_sheet));
                out.push_str(&col_part(*end_col, *end_absolute, Some(origin.1)));
            }
            Token::WholeRowRange {
                sheet,
                start_row,
                end_row,
                start_absolute,
                end_absolute,
            } => {
                out.push_str(&sheet_marker(sheet.as_deref(), current_sheet));
                out.push_str(&row_part(*start_row, *start_absolute, Some(origin.0)));
                out.push(':');
                out.push_str(&sheet_marker(sheet.as_deref(), current_sheet));
                out.push_str(&row_part(*end_row, *end_absolute, Some(origin.0)));
            }
            Token::Number(_) => out.push_str("#NUM"),
            Token::Text(value) => {
                out.push('"');
                out.push_str(value);
                out.push('"');
            }
            Token::ErrorLiteral(value) => {
                out.push_str("#ERR");
                out.push_str(value);
            }
            Token::Function(name) => {
                out.push_str(name);
                out.push('(');
            }
            Token::Name(name) => {
                out.push_str("name:");
                out.push_str(name);
            }
            Token::Unsupported { .. } => {
                return Signature::unsupported("未支援的公式語法".to_string())
            }
            Token::Op(op) => {
                if op.trim().is_empty() {
                    out.push(' ');
                } else {
                    out.push_str(op);
                }
            }
        }
    }
    Signature {
        value: out,
        unsupported: false,
        unsupported_reason: None,
    }
}

fn cell_signature(cell: &CellRef, current_sheet: &str) -> String {
    cell_signature_at(cell, current_sheet, (cell.row, cell.col))
}

fn cell_signature_at(cell: &CellRef, current_sheet: &str, origin: (u32, u32)) -> String {
    let mut out = sheet_marker(cell.sheet.as_deref(), current_sheet);
    out.push_str(&col_part(cell.col, cell.col_absolute, Some(origin.1)));
    out.push_str(&row_part(cell.row, cell.row_absolute, Some(origin.0)));
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::formula_parser::format_address;

    /// 便利函式：以實際位址計算簽章，模擬同欄不同列的公式。
    fn sig(formula: &str, sheet: &str, address: &str) -> Signature {
        let origin = formula_parser::parse_address(address).expect("valid address");
        signature_at(formula, sheet, origin)
    }

    #[test]
    fn copied_down_relative_formula_has_identical_signature() {
        let a = sig("=B2+1", "Sheet1", "C2");
        let b = sig("=B3+1", "Sheet1", "C3");
        let c = sig("=B4+1", "Sheet1", "C4");
        assert!(!a.unsupported);
        assert_eq!(a.value, b.value);
        assert_eq!(b.value, c.value);
        assert_eq!(a.value, "C-1R+0+#NUM");
    }

    #[test]
    fn absolute_reference_is_stable_across_rows() {
        let a = sig("=$B$2*2", "Sheet1", "C2");
        let b = sig("=$B$2*2", "Sheet1", "C9");
        assert_eq!(a.value, b.value);
        assert_eq!(a.value, "C$1R$1*#NUM");
    }

    #[test]
    fn mixed_reference_only_normalizes_relative_axis() {
        let a = sig("=B$2", "Sheet1", "C2");
        let b = sig("=B$2", "Sheet1", "C3");
        assert_eq!(a.value, b.value, "列為絕對，因此跨列仍相同");

        let x = sig("=$B2", "Sheet1", "C2");
        let y = sig("=$B2", "Sheet1", "C3");
        assert_ne!(x.value, y.value, "欄為絕對、列為相對，跨列應不同");
    }

    #[test]
    fn relative_row_deviation_is_detectable() {
        let expected = sig("=B2+1", "Sheet1", "C2");
        let deviant = sig("=B5+1", "Sheet1", "C3");
        assert_ne!(expected.value, deviant.value);
    }

    #[test]
    fn cross_sheet_references_are_distinguished() {
        let same_sheet = sig("=Sheet1!B2", "Sheet1", "C2");
        let other_sheet = sig("=Sheet2!B2", "Sheet1", "C2");
        assert_ne!(same_sheet.value, other_sheet.value);
        assert!(same_sheet.value.starts_with('~'));
        assert!(other_sheet.value.starts_with("@Sheet2"));
    }

    #[test]
    fn range_shapes_are_compared() {
        let a = sig("=SUM(B2:B6)", "Sheet1", "C2");
        let b = sig("=SUM(B2:B4)", "Sheet1", "C2");
        assert_ne!(a.value, b.value);
        let c = sig("=SUM(B2:B6)", "Sheet1", "C3");
        assert_ne!(a.value, c.value, "區間也會隨列位移");
    }

    #[test]
    fn number_literals_are_masked_strings_are_not() {
        let a = sig("=B2*1.05", "Sheet1", "C2");
        let b = sig("=B2*1.1", "Sheet1", "C2");
        assert_eq!(a.value, b.value, "數字常值正規化為 #NUM");

        let x = sig(r#"=IF(A1="x",1,0)"#, "Sheet1", "C2");
        let y = sig(r#"=IF(A1="y",1,0)"#, "Sheet1", "C2");
        assert_ne!(x.value, y.value, "字串常值保留語意");
    }

    #[test]
    fn whitespace_is_normalized() {
        let a = sig("=SUM( B2 )", "Sheet1", "C2");
        let b = sig("=SUM(  B2  )", "Sheet1", "C2");
        assert_eq!(a.value, b.value);
    }

    #[test]
    fn unsupported_formula_is_marked_not_flagged_as_anomaly() {
        let s = sig("=SUM(Table1[Amount])", "Sheet1", "C2");
        assert!(s.unsupported);
        assert!(s.unsupported_reason.unwrap().contains("結構化參照"));
    }

    #[test]
    fn whole_column_reference_is_stable() {
        let a = sig("=SUM(A:A)", "Sheet1", "C2");
        let b = sig("=SUM(A:A)", "Sheet1", "C3");
        assert_eq!(a.value, b.value);
        assert!(!a.unsupported);
    }

    #[test]
    fn signature_is_deterministic_and_round_trips_addresses() {
        // 簽章不得依賴絕對位址，但同一位置的兩次計算必須一致。
        let origin = formula_parser::parse_address("C2").unwrap();
        assert_eq!(format_address(origin.0, origin.1), "C2");
        let a = sig("=A1+B2", "Sheet1", "C2");
        let b = sig("=A1+B2", "Sheet1", "C2");
        assert_eq!(a.value, b.value);
    }
}

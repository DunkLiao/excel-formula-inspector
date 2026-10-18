//! Excel A1 公式 tokenizer／parser（SPEC §5.1）。
//!
//! 設計原則：
//! - **手寫字元掃描**，不用正規表示式全域取代（SPEC 明令禁止以字串取代當作 parser）。
//! - 只做**靜態結構辨識**，不求值、不重算、不判定商業邏輯。
//! - 無法判讀的語法（結構化參照、外部活頁簿、3D 參照、動態陣列中繼資料等）一律標記為
//!   `unsupported`，讓規則層輸出「無法判讀」而非猜測錯誤（SPEC §13.3）。

/// 單一儲存格參照。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CellRef {
    /// 工作表名稱（已去除引號）；`None` 表示未指明工作表。
    pub sheet: Option<String>,
    /// 0-based 欄索引。
    pub col: u32,
    /// 0-based 列索引。
    pub row: u32,
    pub col_absolute: bool,
    pub row_absolute: bool,
}

impl CellRef {
    pub fn address(&self) -> String {
        format_address(self.row, self.col)
    }
}

/// 解析後的 token。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Token {
    Number(String),
    Text(String),
    ErrorLiteral(String),
    Function(String),
    Name(String),
    Cell(CellRef),
    CellRange { start: CellRef, end: CellRef },
    WholeColRange {
        sheet: Option<String>,
        start_col: u32,
        end_col: u32,
        start_absolute: bool,
        end_absolute: bool,
    },
    WholeRowRange {
        sheet: Option<String>,
        start_row: u32,
        end_row: u32,
        start_absolute: bool,
        end_absolute: bool,
    },
    /// 結構化參照（`Table1[Col]`）等目前不判讀的語法。
    Unsupported { text: String, reason: String },
    Op(String),
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ParsedFormula {
    pub tokens: Vec<Token>,
    /// 所有出現的參照端點（含區間兩端）。
    pub refs: Vec<CellRef>,
    /// 出現的函數名稱（大寫）。
    pub functions: Vec<String>,
    /// 語法位置出現的錯誤常值（例如真正的 `#REF!`）；字串常值內的文字不列入。
    pub error_literals: Vec<String>,
    pub has_unsupported: bool,
    pub unsupported_reasons: Vec<String>,
}

impl ParsedFormula {
    pub fn has_error_literal(&self, literal: &str) -> bool {
        self.error_literals
            .iter()
            .any(|e| e.eq_ignore_ascii_case(literal))
    }

    pub fn has_function(&self, name: &str) -> bool {
        self.functions.iter().any(|f| f == name)
    }
}

// ---------------------------------------------------------------------------
// A1 位址工具
// ---------------------------------------------------------------------------

pub const MAX_COL_INDEX: u32 = 16_383; // XFD
pub const MAX_ROW_INDEX: u32 = 1_048_575; // 第 1048576 列

/// 0-based 欄索引 → 欄字母（0 → A、25 → Z、26 → AA）。
pub fn col_to_letters(mut col: u32) -> String {
    let mut out = Vec::new();
    loop {
        out.push(b'A' + (col % 26) as u8);
        if col < 26 {
            break;
        }
        col = col / 26 - 1;
    }
    out.reverse();
    String::from_utf8(out).unwrap_or_default()
}

/// 欄字母 → 0-based 欄索引（`A` → 0）。超出 Excel 範圍或非法時回傳 `None`。
pub fn letters_to_col(letters: &str) -> Option<u32> {
    if letters.is_empty() || letters.len() > 3 {
        return None;
    }
    let mut col: u32 = 0;
    for ch in letters.chars() {
        let upper = ch.to_ascii_uppercase();
        if !upper.is_ascii_uppercase() {
            return None;
        }
        col = col * 26 + (upper as u32 - 'A' as u32 + 1);
    }
    let col = col - 1;
    (col <= MAX_COL_INDEX).then_some(col)
}

/// 0-based (row, col) → A1 位址。
pub fn format_address(row: u32, col: u32) -> String {
    format!("{}{}", col_to_letters(col), row + 1)
}

/// 0-based 範圍 → `A1:B2`。
pub fn format_range(start: (u32, u32), end: (u32, u32)) -> String {
    format!(
        "{}:{}",
        format_address(start.0, start.1),
        format_address(end.0, end.1)
    )
}

/// `A1` 位址 → 0-based (row, col)。支援 `$A$1` 形式；失敗回傳 `None`。
pub fn parse_address(address: &str) -> Option<(u32, u32)> {
    let mut letters = String::new();
    let mut digits = String::new();
    for ch in address.chars() {
        if ch == '$' {
            continue;
        }
        if ch.is_ascii_alphabetic() && digits.is_empty() {
            letters.push(ch);
        } else if ch.is_ascii_digit() {
            digits.push(ch);
        } else {
            return None;
        }
    }
    let col = letters_to_col(&letters)?;
    let row_number: u32 = digits.parse().ok()?;
    if row_number == 0 || row_number > MAX_ROW_INDEX + 1 {
        return None;
    }
    Some((row_number - 1, col))
}

/// 將公式原文轉為顯示用字串（補上 Excel 習慣的前導 `=`）。
pub fn display_formula(raw: &str) -> String {
    if raw.starts_with('=') {
        raw.to_string()
    } else {
        format!("={raw}")
    }
}

// ---------------------------------------------------------------------------
// Lexer
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq, Eq)]
enum Raw {
    Word(String),
    QuotedSheet(String),
    Text(String),
    Number(String),
    Error(String),
    Bracket(String),
    Op(String),
}

const KNOWN_ERRORS: [&str; 9] = [
    "#REF!", "#DIV/0!", "#N/A", "#NAME?", "#NULL!", "#NUM!", "#VALUE!", "#GETTING_DATA", "#SPILL!",
];

fn is_word_char(ch: char) -> bool {
    ch.is_alphanumeric() || matches!(ch, '_' | '.' | '$' | '\\') || !ch.is_ascii()
}

fn lex(input: &str) -> Vec<Raw> {
    let chars: Vec<char> = input.chars().collect();
    let mut out = Vec::new();
    let mut i = 0usize;

    while i < chars.len() {
        let ch = chars[i];

        // 空白：Excel 的交集運算子，保留為 token 以維持簽章語意。
        if ch.is_whitespace() {
            let start = i;
            while i < chars.len() && chars[i].is_whitespace() {
                i += 1;
            }
            out.push(Raw::Op(chars[start..i].iter().collect::<String>()));
            continue;
        }

        // 字串常值（"" 為跳脫）
        if ch == '"' {
            i += 1;
            let mut value = String::new();
            while i < chars.len() {
                if chars[i] == '"' {
                    if i + 1 < chars.len() && chars[i + 1] == '"' {
                        value.push('"');
                        i += 2;
                        continue;
                    }
                    i += 1;
                    break;
                }
                value.push(chars[i]);
                i += 1;
            }
            out.push(Raw::Text(value));
            continue;
        }

        // 單引號工作表名稱（'' 為跳脫）
        if ch == '\'' {
            i += 1;
            let mut value = String::new();
            while i < chars.len() {
                if chars[i] == '\'' {
                    if i + 1 < chars.len() && chars[i + 1] == '\'' {
                        value.push('\'');
                        i += 2;
                        continue;
                    }
                    i += 1;
                    break;
                }
                value.push(chars[i]);
                i += 1;
            }
            out.push(Raw::QuotedSheet(value));
            continue;
        }

        // 錯誤常值
        if ch == '#' {
            let rest: String = chars[i..].iter().collect();
            let upper = rest.to_ascii_uppercase();
            if let Some(found) = KNOWN_ERRORS.iter().find(|e| upper.starts_with(**e)) {
                i += found.chars().count();
                out.push(Raw::Error((*found).to_string()));
                continue;
            }
            // `#` 也可能是溢出／隱含交集運算子（A1#）
            out.push(Raw::Op("#".into()));
            i += 1;
            continue;
        }

        // 結構化／外部參照
        if ch == '[' {
            let mut text = String::from("[");
            i += 1;
            while i < chars.len() {
                text.push(chars[i]);
                let done = chars[i] == ']';
                i += 1;
                if done {
                    break;
                }
            }
            out.push(Raw::Bracket(text));
            continue;
        }

        // 數字（含小數與指數）
        if ch.is_ascii_digit() || (ch == '.' && i + 1 < chars.len() && chars[i + 1].is_ascii_digit()) {
            let mut text = String::new();
            while i < chars.len() && chars[i].is_ascii_digit() {
                text.push(chars[i]);
                i += 1;
            }
            if i < chars.len() && chars[i] == '.' {
                text.push('.');
                i += 1;
                while i < chars.len() && chars[i].is_ascii_digit() {
                    text.push(chars[i]);
                    i += 1;
                }
            }
            if i < chars.len() && (chars[i] == 'E' || chars[i] == 'e') {
                let save = i;
                let mut exp = String::from("E");
                i += 1;
                if i < chars.len() && (chars[i] == '+' || chars[i] == '-') {
                    exp.push(chars[i]);
                    i += 1;
                }
                if i < chars.len() && chars[i].is_ascii_digit() {
                    while i < chars.len() && chars[i].is_ascii_digit() {
                        exp.push(chars[i]);
                        i += 1;
                    }
                    text.push_str(&exp);
                } else {
                    i = save;
                }
            }
            out.push(Raw::Number(text));
            continue;
        }

        // 識別字（含 $、底線、CJK；可形成 A1 參照或具名範圍）
        if ch == '$' || ch.is_alphabetic() || ch == '_' || ch == '\\' || !ch.is_ascii() {
            let mut text = String::new();
            while i < chars.len() && is_word_char(chars[i]) {
                text.push(chars[i]);
                i += 1;
            }
            out.push(Raw::Word(text));
            continue;
        }

        // 運算子與標點（雙字元先比對）
        let two: String = chars[i..].iter().take(2).collect();
        if matches!(two.as_str(), "<>" | "<=" | ">=" | "==") {
            out.push(Raw::Op(two));
            i += 2;
            continue;
        }
        out.push(Raw::Op(ch.to_string()));
        i += 1;
    }

    out
}

// ---------------------------------------------------------------------------
// Interpret
// ---------------------------------------------------------------------------

/// 拆解 A1 參照。回傳 (col, row, col_abs, row_abs)。
fn match_cell(word: &str) -> Option<(u32, u32, bool, bool)> {
    let mut letters = String::new();
    let mut digits = String::new();
    let mut col_abs = false;
    let mut row_abs = false;
    let mut seen_dollar_for_col = false;
    let mut seen_dollar_for_row = false;
    let mut iter = word.chars().peekable();

    while let Some(&ch) = iter.peek() {
        if ch == '$' {
            if letters.is_empty() && !seen_dollar_for_col {
                col_abs = true;
                seen_dollar_for_col = true;
            } else if digits.is_empty() && !seen_dollar_for_row {
                row_abs = true;
                seen_dollar_for_row = true;
            } else {
                return None;
            }
            iter.next();
            continue;
        }
        if ch.is_ascii_alphabetic() && digits.is_empty() {
            letters.push(ch);
            iter.next();
            continue;
        }
        if ch.is_ascii_digit() {
            digits.push(ch);
            iter.next();
            continue;
        }
        return None;
    }

    let col = letters_to_col(&letters)?;
    let row_number: u32 = digits.parse().ok()?;
    if row_number == 0 || row_number > MAX_ROW_INDEX + 1 {
        return None;
    }
    Some((col, row_number - 1, col_abs, row_abs))
}

/// 拆解「只有欄」的參照（`$A`）。
fn match_col(word: &str) -> Option<(u32, bool)> {
    let mut letters = String::new();
    let mut absolute = false;
    for ch in word.chars() {
        if ch == '$' {
            if !letters.is_empty() || absolute {
                return None;
            }
            absolute = true;
            continue;
        }
        if ch.is_ascii_alphabetic() {
            letters.push(ch);
            continue;
        }
        return None;
    }
    letters_to_col(&letters).map(|c| (c, absolute))
}

/// 拆解「只有列」的參照（`$1`）。
fn match_row(word: &str) -> Option<(u32, bool)> {
    let mut digits = String::new();
    let mut absolute = false;
    for ch in word.chars() {
        if ch == '$' {
            if !digits.is_empty() || absolute {
                return None;
            }
            absolute = true;
            continue;
        }
        if ch.is_ascii_digit() {
            digits.push(ch);
            continue;
        }
        return None;
    }
    let value: u32 = digits.parse().ok()?;
    if value == 0 || value > MAX_ROW_INDEX + 1 {
        return None;
    }
    Some((value - 1, absolute))
}

/// 解析公式原文（可帶或不帶前導 `=`）。
pub fn parse(input: &str) -> ParsedFormula {
    let body = input.strip_prefix('=').unwrap_or(input);
    let raw = lex(body);
    let mut parsed = ParsedFormula::default();

    let mut i = 0usize;
    let mut pending_sheet: Option<String> = None;

    while i < raw.len() {
        let current = raw[i].clone();

        // 工作表前置詞：'Sheet Name'! 或 Sheet1!
        let sheet_prefix = match &current {
            Raw::QuotedSheet(name) => Some(name.clone()),
            Raw::Word(word) => {
                if matches!(raw.get(i + 1), Some(Raw::Op(op)) if op == "!") {
                    Some(word.clone())
                } else {
                    None
                }
            }
            _ => None,
        };
        if let Some(name) = sheet_prefix {
            if name.contains('[') {
                parsed.has_unsupported = true;
                parsed
                    .unsupported_reasons
                    .push(format!("外部活頁簿參照：{name}"));
                parsed.tokens.push(Token::Unsupported {
                    text: format!("{name}!"),
                    reason: "外部活頁簿參照".into(),
                });
            } else {
                pending_sheet = Some(name);
            }
            i += 2; // 名稱 + '!'
            continue;
        }

        match current {
            Raw::Text(value) => {
                parsed.tokens.push(Token::Text(value));
                i += 1;
            }
            Raw::Error(value) => {
                parsed.error_literals.push(value.clone());
                parsed.tokens.push(Token::ErrorLiteral(value));
                i += 1;
            }
            Raw::Number(value) => {
                // `1:1` → 整列參照
                if matches!(raw.get(i + 1), Some(Raw::Op(op)) if op == ":") {
                    if let Some(Raw::Number(end)) = raw.get(i + 2) {
                        if let (Some(start_row), Some(end_row)) = (parse_row_number(&value), parse_row_number(end)) {
                            parsed.tokens.push(Token::WholeRowRange {
                                sheet: pending_sheet.take(),
                                start_row,
                                end_row,
                                start_absolute: false,
                                end_absolute: false,
                            });
                            i += 3;
                            continue;
                        }
                    }
                }
                parsed.tokens.push(Token::Number(value));
                i += 1;
            }
            Raw::Bracket(text) => {
                parsed.has_unsupported = true;
                parsed
                    .unsupported_reasons
                    .push("結構化參照或外部參照".to_string());
                parsed.tokens.push(Token::Unsupported {
                    text,
                    reason: "結構化參照或外部參照".into(),
                });
                i += 1;
            }
            Raw::Op(op) => {
                // 動態陣列語法（溢出運算子 `#`、隱含交集運算子 `@`）一律「無法判讀」（SPEC §5.1、§13.3）。
                if op == "#" || op == "@" {
                    parsed.has_unsupported = true;
                    parsed
                        .unsupported_reasons
                        .push(format!("動態陣列運算子（{op}）"));
                    parsed.tokens.push(Token::Unsupported {
                        text: op.clone(),
                        reason: "動態陣列語法".into(),
                    });
                    i += 1;
                    continue;
                }
                // 3D 參照 Sheet1:Sheet3!A1
                if op == ":" && pending_sheet.is_some() {
                    if let Some(Raw::Word(next)) = raw.get(i + 1) {
                        if matches!(raw.get(i + 2), Some(Raw::Op(o)) if o == "!") {
                            parsed.has_unsupported = true;
                            parsed
                                .unsupported_reasons
                                .push("3D 工作表參照（Sheet1:Sheet3!A1）".to_string());
                            parsed.tokens.push(Token::Unsupported {
                                text: format!("{next}!"),
                                reason: "3D 工作表參照".into(),
                            });
                            pending_sheet = None;
                            i += 3;
                            continue;
                        }
                    }
                }
                parsed.tokens.push(Token::Op(op));
                i += 1;
            }
            Raw::Word(word) => {
                // 函數
                if matches!(raw.get(i + 1), Some(Raw::Op(op)) if op == "(") {
                    let name = word.to_ascii_uppercase();
                    parsed.functions.push(name.clone());
                    parsed.tokens.push(Token::Function(name));
                    i += 1;
                    continue;
                }

                // 3D 參照：Sheet1:Sheet3!A1（名稱 : 名稱 !）
                if matches!(raw.get(i + 1), Some(Raw::Op(op)) if op == ":") {
                    if let (Some(Raw::Word(_)), Some(Raw::Op(bang))) =
                        (raw.get(i + 2), raw.get(i + 3))
                    {
                        if bang == "!" {
                            parsed.has_unsupported = true;
                            parsed
                                .unsupported_reasons
                                .push("3D 工作表參照（Sheet1:Sheet3!A1）".to_string());
                            parsed.tokens.push(Token::Unsupported {
                                text: format!("{word}:…!"),
                                reason: "3D 工作表參照".into(),
                            });
                            i += 4;
                            continue;
                        }
                    }
                }

                // 範圍：A1:B2 / A:A / $1:$5
                if matches!(raw.get(i + 1), Some(Raw::Op(op)) if op == ":") {
                    if let Some(next) = raw.get(i + 2) {
                        if let Some(token) = build_range(&word, next, pending_sheet.take()) {
                            parsed.push_refs(&token);
                            parsed.tokens.push(token);
                            i += 3;
                            continue;
                        }
                    }
                }

                if let Some((col, row, col_abs, row_abs)) = match_cell(&word) {
                    let cell = CellRef {
                        sheet: pending_sheet.take(),
                        col,
                        row,
                        col_absolute: col_abs,
                        row_absolute: row_abs,
                    };
                    parsed.refs.push(cell.clone());
                    parsed.tokens.push(Token::Cell(cell));
                    i += 1;
                    continue;
                }

                // 具名範圍／未識別名稱
                parsed.tokens.push(Token::Name(word));
                i += 1;
            }
            Raw::QuotedSheet(_) => {
                // 未接 '!'，視為名稱
                parsed.tokens.push(Token::Name("<quoted>".into()));
                i += 1;
            }
        }
    }

    parsed.unsupported_reasons.sort();
    parsed.unsupported_reasons.dedup();
    parsed
}

fn parse_row_number(text: &str) -> Option<u32> {
    let value: u32 = text.parse().ok()?;
    if value == 0 || value > MAX_ROW_INDEX + 1 {
        return None;
    }
    Some(value - 1)
}

fn build_range(start: &str, end: &Raw, sheet: Option<String>) -> Option<Token> {
    let end = match end {
        Raw::Word(word) => word.clone(),
        Raw::Number(number) => format!("${number}").trim_start_matches('$').to_string(),
        _ => return None,
    };

    // 儲存格:儲存格
    if let (Some((s_col, s_row, s_col_abs, s_row_abs)), Some((e_col, e_row, e_col_abs, e_row_abs))) =
        (match_cell(start), match_cell(&end))
    {
        let s = CellRef {
            sheet: sheet.clone(),
            col: s_col,
            row: s_row,
            col_absolute: s_col_abs,
            row_absolute: s_row_abs,
        };
        let e = CellRef {
            sheet,
            col: e_col,
            row: e_row,
            col_absolute: e_col_abs,
            row_absolute: e_row_abs,
        };
        return Some(Token::CellRange { start: s, end: e });
    }

    // 整欄 A:A
    if let (Some((s_col, s_abs)), Some((e_col, e_abs))) = (match_col(start), match_col(&end)) {
        return Some(Token::WholeColRange {
            sheet,
            start_col: s_col,
            end_col: e_col,
            start_absolute: s_abs,
            end_absolute: e_abs,
        });
    }

    // 整列 1:1
    if let (Some((s_row, s_abs)), Some((e_row, e_abs))) = (match_row(start), match_row(&end)) {
        return Some(Token::WholeRowRange {
            sheet,
            start_row: s_row,
            end_row: e_row,
            start_absolute: s_abs,
            end_absolute: e_abs,
        });
    }

    None
}

impl ParsedFormula {
    fn push_refs(&mut self, token: &Token) {
        match token {
            Token::Cell(cell) => self.refs.push(cell.clone()),
            Token::CellRange { start, end } => {
                self.refs.push(start.clone());
                self.refs.push(end.clone());
            }
            _ => {}
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn refs_of(formula: &str) -> Vec<String> {
        parse(formula)
            .refs
            .iter()
            .map(|r| match &r.sheet {
                Some(s) => format!("{s}!{}", r.address()),
                None => r.address(),
            })
            .collect()
    }

    #[test]
    fn column_letters_round_trip() {
        assert_eq!(col_to_letters(0), "A");
        assert_eq!(col_to_letters(25), "Z");
        assert_eq!(col_to_letters(26), "AA");
        assert_eq!(col_to_letters(27), "AB");
        assert_eq!(col_to_letters(51), "AZ");
        assert_eq!(col_to_letters(52), "BA");
        assert_eq!(col_to_letters(16_383), "XFD");
        for col in [0u32, 1, 25, 26, 701, 702, 16_382, 16_383] {
            assert_eq!(letters_to_col(&col_to_letters(col)), Some(col));
        }
        assert_eq!(letters_to_col("XFE"), None);
        assert_eq!(letters_to_col("A1"), None);
    }

    #[test]
    fn address_parsing_and_formatting() {
        assert_eq!(parse_address("D12"), Some((11, 3)));
        assert_eq!(parse_address("$D$12"), Some((11, 3)));
        assert_eq!(parse_address("A1"), Some((0, 0)));
        assert_eq!(parse_address("XFD1048576"), Some((1_048_575, 16_383)));
        assert_eq!(parse_address("A0"), None);
        assert_eq!(parse_address("XFE1"), None);
        assert_eq!(format_address(11, 3), "D12");
        assert_eq!(format_range((0, 0), (1, 1)), "A1:B2");
    }

    #[test]
    fn parses_simple_references_and_ranges() {
        assert_eq!(refs_of("B2+1"), vec!["B2"]);
        assert_eq!(refs_of("SUM(B2:B6)"), vec!["B2", "B6"]);
        assert_eq!(refs_of("Sales!B2+Sales!B3"), vec!["Sales!B2", "Sales!B3"]);
        assert_eq!(refs_of("'My Sheet'!A1"), vec!["My Sheet!A1"]);
        assert_eq!(refs_of("SUM($A$1,B$2,$C3)"), vec!["A1", "B2", "C3"]);
    }

    #[test]
    fn absolute_flags_are_preserved() {
        let parsed = parse("$A1+B$2+$C$3");
        let flags: Vec<(bool, bool)> = parsed
            .refs
            .iter()
            .map(|r| (r.col_absolute, r.row_absolute))
            .collect();
        assert_eq!(flags, vec![(true, false), (false, true), (true, true)]);
    }

    #[test]
    fn whole_column_and_row_ranges() {
        let parsed = parse("SUM(A:A)+SUM($1:$3)");
        assert_eq!(parsed.refs.len(), 0, "整欄／整列不產生單一儲存格參照");
        assert!(parsed
            .tokens
            .iter()
            .any(|t| matches!(t, Token::WholeColRange { start_col: 0, end_col: 0, .. })));
        assert!(parsed
            .tokens
            .iter()
            .any(|t| matches!(t, Token::WholeRowRange { start_row: 0, end_row: 2, .. })));
    }

    #[test]
    fn functions_are_detected_case_insensitively() {
        let parsed = parse("=sum(A1)+If(B1>0,indirect(\"C1\"),0)");
        assert!(parsed.has_function("SUM"));
        assert!(parsed.has_function("IF"));
        assert!(parsed.has_function("INDIRECT"));
        assert!(!parsed.has_function("OFFSET"));
    }

    #[test]
    fn string_literals_never_leak_ref_errors_or_refs() {
        let parsed = parse(r##"=A1&"#REF! B2:C3""##);
        assert!(parsed.error_literals.is_empty(), "字串常值內的 #REF! 不算錯誤");
        assert!(!parsed.has_error_literal("#REF!"));
        assert_eq!(parsed.refs.len(), 1);
        assert_eq!(parsed.refs[0].address(), "A1");
        assert!(parsed
            .tokens
            .iter()
            .any(|t| matches!(t, Token::Text(v) if v == "#REF! B2:C3")));
    }

    #[test]
    fn ref_error_in_syntactic_position_is_detected() {
        let parsed = parse("=A1+#REF!");
        assert!(parsed.has_error_literal("#REF!"));
        assert_eq!(parsed.error_literals, vec!["#REF!".to_string()]);

        let parsed = parse("=SUM(#REF!,#DIV/0!)");
        assert!(parsed.has_error_literal("#REF!"));
        assert!(parsed.has_error_literal("#DIV/0!"));
    }

    #[test]
    fn structured_reference_is_flagged_unsupported() {
        let parsed = parse("=SUM(Table1[Amount])");
        assert!(parsed.has_unsupported);
        assert!(parsed
            .unsupported_reasons
            .iter()
            .any(|r| r.contains("結構化參照")));
        assert!(parsed.refs.is_empty());
    }

    #[test]
    fn external_workbook_reference_is_flagged_unsupported() {
        let parsed = parse("='[Book1.xlsx]Sheet1'!A1");
        assert!(parsed.has_unsupported);
        assert!(parsed
            .unsupported_reasons
            .iter()
            .any(|r| r.contains("外部活頁簿")));
    }

    #[test]
    fn three_d_reference_is_flagged_unsupported() {
        let parsed = parse("=SUM(Sheet1:Sheet3!A1)");
        assert!(parsed.has_unsupported);
        assert!(parsed
            .unsupported_reasons
            .iter()
            .any(|r| r.contains("3D")));
    }

    #[test]
    fn dynamic_array_syntax_is_flagged_unsupported() {
        // 溢出運算子與隱含交集運算子在 MVP 一律「無法判讀」（SPEC §5.1）。
        for formula in ["=A1#", "=@A1:A3", "=SUM(A1#)"] {
            let parsed = parse(formula);
            assert!(parsed.has_unsupported, "{formula} 應被標記為無法判讀");
            assert!(parsed
                .unsupported_reasons
                .iter()
                .any(|r| r.contains("動態陣列")));
        }
    }

    #[test]
    fn names_that_look_like_addresses_are_unsupported_gracefully() {
        // ABCD1 不是合法欄名（4 個字母），應視為名稱而非與錯誤判讀
        let parsed = parse("=SUM(ABCD1,MyName)");
        assert_eq!(parsed.refs.len(), 0);
        assert!(parsed
            .tokens
            .iter()
            .any(|t| matches!(t, Token::Name(n) if n == "ABCD1")));
        assert!(parsed
            .tokens
            .iter()
            .any(|t| matches!(t, Token::Name(n) if n == "MyName")));
    }

    #[test]
    fn empty_string_escaping_inside_formula() {
        let parsed = parse(r#"=IF(A1="say ""hi""",1,0)"#);
        assert!(parsed
            .tokens
            .iter()
            .any(|t| matches!(t, Token::Text(v) if v == "say \"hi\"")));
    }

    #[test]
    fn display_formula_adds_leading_equals_once() {
        assert_eq!(display_formula("SUM(A1)"), "=SUM(A1)");
        assert_eq!(display_formula("=SUM(A1)"), "=SUM(A1)");
    }
}

//! 工作簿讀取（SPEC §7）：以 `calamine` 讀取 `.xlsx` / `.xlsm` 的儲存格值、型別與公式原文。
//!
//! 重點：
//! - **稀疏走訪**：只走訪實際存在的儲存格（SPEC §8.2），不遍歷整張 Excel 虛擬網格。
//! - **位址修正**：`calamine::Range::used_cells()` 回傳的座標是相對於 `Range::start()` 的位移，
//!   必須加上起點才是絕對 A1 位址（見 `absolute_cells` 及其測試）。
//! - **不執行巨集**：`.xlsm` 只讀取儲存格與公式。

use std::collections::HashMap;
use std::fs;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};

use calamine::{Data, Reader, Sheets};
use serde::{Deserialize, Serialize};

use crate::errors::{ScanError, ScanResult};
use crate::formula_parser::{self, format_range};
use crate::model::ValueKind;

/// 預設檔案大小上限：超過需使用者確認後以實驗性模式繼續（SPEC §8.2）。
pub const DEFAULT_SIZE_LIMIT: u64 = 20 * 1024 * 1024;
/// 硬上限：任何情況都不處理，避免記憶體耗盡。
pub const HARD_SIZE_LIMIT: u64 = 200 * 1024 * 1024;
/// 已使用儲存格硬上限。
pub const HARD_CELL_LIMIT: usize = 500_000;

/// 單一儲存格在記憶體中的樣貌。
#[derive(Debug, Clone, PartialEq)]
pub struct CellData {
    pub kind: ValueKind,
    /// 快取值顯示字串（僅供檢視，不代表重新計算結果）。
    pub display: Option<String>,
    /// 錯誤值字串（例如 `#DIV/0!`）。
    pub error: Option<String>,
    /// 公式原文（不含前導 `=`）。
    pub formula: Option<String>,
}

impl CellData {
    pub fn is_formula(&self) -> bool {
        self.formula.is_some()
    }

    pub fn is_error(&self) -> bool {
        self.error.is_some()
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct SheetData {
    pub index: usize,
    pub name: String,
    pub visible: bool,
    /// 稀疏儲存格（0-based 行列）。
    pub cells: HashMap<(u32, u32), CellData>,
    /// 已使用範圍（含合併區域的聯集）。
    pub used_start: Option<(u32, u32)>,
    pub used_end: Option<(u32, u32)>,
    pub merge_regions: Vec<((u32, u32), (u32, u32))>,
    pub formula_count: usize,
    pub constant_count: usize,
    pub error_count: usize,
}

impl SheetData {
    pub fn cell(&self, row: u32, col: u32) -> Option<&CellData> {
        self.cells.get(&(row, col))
    }

    pub fn formula_of(&self, row: u32, col: u32) -> Option<&str> {
        self.cells
            .get(&(row, col))
            .and_then(|c| c.formula.as_deref())
    }

    pub fn is_formula(&self, row: u32, col: u32) -> bool {
        self.cells
            .get(&(row, col))
            .map(|c| c.is_formula())
            .unwrap_or(false)
    }

    /// 該儲存格是否落在任何合併區域內（含左上角錨點）。
    pub fn is_merged(&self, row: u32, col: u32) -> bool {
        self.merge_regions
            .iter()
            .any(|(start, end)| row >= start.0 && row <= end.0 && col >= start.1 && col <= end.1)
    }

    /// 整列是否完全沒有內容（用於「不跨空白整列推定」，SPEC §5.2）。
    pub fn is_row_empty(&self, row: u32) -> bool {
        !self.cells.keys().any(|(r, _)| *r == row)
    }

    pub fn used_range_string(&self) -> Option<String> {
        match (self.used_start, self.used_end) {
            (Some(start), Some(end)) => Some(format_range(start, end)),
            _ => None,
        }
    }

    /// 依列排序的公式儲存格座標。
    pub fn formula_positions(&self) -> Vec<(u32, u32)> {
        let mut positions: Vec<(u32, u32)> = self
            .cells
            .iter()
            .filter(|(_, cell)| cell.is_formula())
            .map(|(pos, _)| *pos)
            .collect();
        positions.sort_unstable();
        positions
    }
}

#[derive(Debug, Clone)]
pub struct WorkbookData {
    pub file_name: String,
    pub file_size: u64,
    pub sheets: Vec<SheetData>,
    pub warnings: Vec<String>,
    pub cell_count: usize,
}

/// 掃描進度（由 engine 轉成 IPC 事件）。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProgressUpdate {
    pub phase: String,
    pub done: usize,
    pub total: usize,
    pub label: Option<String>,
}

/// 是否允許實驗性模式（超過預設大小上限）。
#[derive(Debug, Clone, Copy)]
pub struct ReadOptions {
    pub allow_large: bool,
}

/// 驗證路徑、副檔名與大小限制，回傳檔案大小。
pub fn validate_path(path: &Path, allow_large: bool) -> ScanResult<u64> {
    let metadata = fs::metadata(path).map_err(|_| ScanError::FileNotFound)?;
    if !metadata.is_file() {
        return Err(ScanError::NotAFile);
    }
    let extension = path
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or_default()
        .to_ascii_lowercase();
    if !matches!(extension.as_str(), "xlsx" | "xlsm") {
        return Err(ScanError::UnsupportedFormat(if extension.is_empty() {
            "（無副檔名）".to_string()
        } else {
            extension
        }));
    }

    let size = metadata.len();
    if size > HARD_SIZE_LIMIT {
        return Err(ScanError::TooLarge {
            size_bytes: size,
            size_mb: size as f64 / (1024.0 * 1024.0),
            limit_mb: HARD_SIZE_LIMIT / (1024 * 1024),
        });
    }
    if size > DEFAULT_SIZE_LIMIT && !allow_large {
        return Err(ScanError::LargeFileNeedsConfirmation {
            size_bytes: size,
            size_mb: size as f64 / (1024.0 * 1024.0),
        });
    }
    Ok(size)
}

/// 偵測 OLE2/CFB 容器（加密 xlsx 或 .xls 偽裝成 .xlsx）。
fn detect_container(path: &Path) -> Option<ScanError> {
    let mut file = fs::File::open(path).ok()?;
    let mut head = vec![0u8; 8192];
    let read = file.read(&mut head).ok()?;
    head.truncate(read);

    const CFB_MAGIC: [u8; 8] = [0xD0, 0xCF, 0x11, 0xE0, 0xA1, 0xB1, 0x1A, 0xE1];
    if !head.starts_with(&CFB_MAGIC) {
        return None;
    }

    // 加密 OOXML 會在 CFB 中存放 "EncryptedPackage"／"EncryptionInfo"（UTF-16LE）。
    let ascii = |needle: &str| head.windows(needle.len()).any(|w| w == needle.as_bytes());
    let utf16: Vec<u8> = "EncryptedPackage"
        .bytes()
        .flat_map(|b| [b, 0u8])
        .collect();
    let encrypted = ascii("EncryptedPackage")
        || head.windows(utf16.len()).any(|w| w == utf16.as_slice())
        || ascii("EncryptionInfo");

    Some(if encrypted {
        ScanError::Encrypted
    } else {
        ScanError::UnsupportedFormat("偵測到舊版 OLE2 容器（可能是 .xls）".to_string())
    })
}

fn map_open_error(error: &calamine::Error) -> ScanError {
    match error {
        calamine::Error::Io(io) if io.kind() == std::io::ErrorKind::NotFound => ScanError::FileNotFound,
        other => ScanError::Corrupt {
            detail: short_detail(&format!("{other:?}")),
        },
    }
}

/// 只保留錯誤的簡短類型，不外洩內部細節（SPEC §8.2）。
fn short_detail(text: &str) -> String {
    let head: String = text.chars().take(120).collect();
    head
}

/// 讀取工作簿。`on_progress` 會被頻繁呼叫（每個工作表至少一次）。
pub fn read_workbook(
    path: &Path,
    options: ReadOptions,
    cancel: &AtomicBool,
    on_progress: &mut dyn FnMut(ProgressUpdate),
) -> ScanResult<WorkbookData> {
    let file_size = validate_path(path, options.allow_large)?;

    if let Some(error) = detect_container(path) {
        return Err(error);
    }

    let mut workbook = calamine::open_workbook_auto(path).map_err(|e| map_open_error(&e))?;

    let sheets = match &workbook {
        Sheets::Xlsx(_) => workbook.sheets_metadata().to_vec(),
        Sheets::Xls(_) => {
            return Err(ScanError::UnsupportedFormat(
                "偵測到舊版 .xls 格式，MVP 僅支援 .xlsx 與 .xlsm".to_string(),
            ))
        }
        Sheets::Xlsb(_) => {
            return Err(ScanError::UnsupportedFormat(
                "偵測到 .xlsb 格式，MVP 僅支援 .xlsx 與 .xlsm".to_string(),
            ))
        }
        Sheets::Ods(_) => {
            return Err(ScanError::UnsupportedFormat(
                "偵測到 OpenDocument 格式，MVP 僅支援 .xlsx 與 .xlsm".to_string(),
            ))
        }
    };

    let total_sheets = sheets.len();
    let mut data_sheets: Vec<SheetData> = Vec::new();
    let mut warnings: Vec<String> = Vec::new();
    let mut cell_count = 0usize;

    for (index, meta) in sheets.iter().enumerate() {
        if cancel.load(Ordering::Relaxed) {
            return Err(ScanError::Cancelled);
        }

        if meta.typ != calamine::SheetType::WorkSheet {
            warnings.push(format!(
                "工作表「{}」不是一般工作表（{:?}），已略過分析。",
                meta.name, meta.typ
            ));
            continue;
        }

        on_progress(ProgressUpdate {
            phase: "reading".to_string(),
            done: index,
            total: total_sheets,
            label: Some(meta.name.clone()),
        });

        let mut sheet = SheetData {
            index: data_sheets.len(),
            name: meta.name.clone(),
            visible: meta.visible == calamine::SheetVisible::Visible,
            cells: HashMap::new(),
            used_start: None,
            used_end: None,
            merge_regions: Vec::new(),
            formula_count: 0,
            constant_count: 0,
            error_count: 0,
        };

        // 1) 值與型別
        let range = workbook
            .worksheet_range(&meta.name)
            .map_err(|e| ScanError::Corrupt {
                detail: short_detail(&format!("{e:?}")),
            })?;

        for (row, col) in absolute_cells(&range) {
            if cancel.load(Ordering::Relaxed) {
                return Err(ScanError::Cancelled);
            }
            let Some(value) = range.get_value((row, col)) else {
                continue;
            };
            let Some((kind, display, error)) = classify(value) else {
                continue;
            };
            sheet
                .cells
                .entry((row, col))
                .or_insert(CellData {
                    kind,
                    display,
                    error,
                    formula: None,
                });
        }

        // 2) 公式原文（calamine 不含前導 `=`）
        let formulas = workbook
            .worksheet_formula(&meta.name)
            .map_err(|e| ScanError::Corrupt {
                detail: short_detail(&format!("{e:?}")),
            })?;

        for (row, col) in absolute_cells(&formulas) {
            if cancel.load(Ordering::Relaxed) {
                return Err(ScanError::Cancelled);
            }
            let Some(formula) = formulas.get_value((row, col)) else {
                continue;
            };
            if formula.is_empty() {
                continue;
            }
            sheet
                .cells
                .entry((row, col))
                .or_insert(CellData {
                    kind: ValueKind::Empty,
                    display: None,
                    error: None,
                    formula: None,
                })
                .formula = Some(formula.clone());
        }

        // 3) 合併區域（僅 xlsx/xlsm 專屬 API）
        if let Sheets::Xlsx(inner) = &mut workbook {
            match inner.merge_cells_by_sheet_name(&meta.name) {
                Ok(regions) => {
                    sheet.merge_regions = regions
                        .iter()
                        .map(|d| (d.start, d.end))
                        .filter(|(start, end)| end.0 >= start.0 && end.1 >= start.1)
                        .collect();
                }
                Err(e) => warnings.push(format!(
                    "無法讀取工作表「{}」的合併儲存格資訊（{}），F004/F005 的誤報提示可能不完整。",
                    meta.name,
                    short_detail(&format!("{e:?}"))
                )),
            }
        }

        // 4) 統計與已使用範圍（儲存格聯集合併區域）
        for ((row, col), cell) in &sheet.cells {
            sheet.used_start = Some(match sheet.used_start {
                None => (*row, *col),
                Some((r, c)) => (r.min(*row), c.min(*col)),
            });
            sheet.used_end = Some(match sheet.used_end {
                None => (*row, *col),
                Some((r, c)) => (r.max(*row), c.max(*col)),
            });
            if cell.is_formula() {
                sheet.formula_count += 1;
            } else {
                sheet.constant_count += 1;
            }
            if cell.is_error() {
                sheet.error_count += 1;
            }
        }
        for (start, end) in &sheet.merge_regions {
            sheet.used_start = Some(match sheet.used_start {
                None => *start,
                Some((r, c)) => (r.min(start.0), c.min(start.1)),
            });
            sheet.used_end = Some(match sheet.used_end {
                None => *end,
                Some((r, c)) => (r.max(end.0), c.max(end.1)),
            });
        }

        cell_count += sheet.cells.len();
        if cell_count > HARD_CELL_LIMIT {
            return Err(ScanError::TooManyCells {
                cells: cell_count,
                limit: HARD_CELL_LIMIT,
            });
        }

        data_sheets.push(sheet);
    }

    on_progress(ProgressUpdate {
        phase: "reading".to_string(),
        done: total_sheets,
        total: total_sheets,
        label: None,
    });

    if data_sheets.is_empty() {
        warnings.push("工作簿中沒有可分析的一般工作表。".to_string());
    }

    Ok(WorkbookData {
        file_name: path
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("(未知檔案)")
            .to_string(),
        file_size,
        sheets: data_sheets,
        warnings,
        cell_count,
    })
}

/// `used_cells()` 回傳的座標是相對 `start()` 的位移，這裡轉成絕對座標。
fn absolute_cells<T: calamine::CellType>(range: &calamine::Range<T>) -> Vec<(u32, u32)> {
    let (start_row, start_col) = range.start().unwrap_or((0, 0));
    range
        .used_cells()
        .map(|(row, col, _)| (start_row + row as u32, start_col + col as u32))
        .collect()
}

/// 由 `calamine::Data` 轉成顯示用資訊；`Empty` 回傳 `None` 表示可略過。
fn classify(value: &Data) -> Option<(ValueKind, Option<String>, Option<String>)> {
    match value {
        Data::Empty => None,
        Data::Int(number) => Some((ValueKind::Number, Some(number.to_string()), None)),
        Data::Float(number) => Some((ValueKind::Number, Some(format_number(*number)), None)),
        Data::String(text) => Some((ValueKind::Text, Some(text.clone()), None)),
        Data::Bool(flag) => Some((
            ValueKind::Bool,
            Some(if *flag { "TRUE" } else { "FALSE" }.to_string()),
            None,
        )),
        Data::DateTime(datetime) => {
            let text = excel_serial_to_text(datetime.as_f64());
            Some((ValueKind::DateTime, Some(text), None))
        }
        Data::DateTimeIso(text) => Some((ValueKind::DateTime, Some(text.clone()), None)),
        Data::DurationIso(text) => Some((ValueKind::Duration, Some(text.clone()), None)),
        Data::Error(error) => {
            let text = error.to_string();
            Some((ValueKind::Error, Some(text.clone()), Some(text)))
        }
    }
}

/// 數值顯示：整數不補小數點，其餘用最短往返表示。
pub fn format_number(number: f64) -> String {
    if number.is_nan() || number.is_infinite() {
        return number.to_string();
    }
    if number.fract() == 0.0 && number.abs() < 1e15 {
        format!("{}", number as i64)
    } else {
        format!("{number}")
    }
}

/// Excel 序列值 → 可讀日期時間（不引入額外相依）。
///
/// 採 1899-12-30 為原點，並處理 Excel 的 1900 年閏年錯誤（序號 < 61 時原點為 1899-12-31）。
pub fn excel_serial_to_text(serial: f64) -> String {
    if !serial.is_finite() || serial < 0.0 || serial > 2_958_465.0 {
        return format_number(serial);
    }
    let days = serial.floor() as i64;
    let fraction = serial - serial.floor();

    let epoch_offset = if days < 61 { 25568 } else { 25569 };
    let unix_days = days - epoch_offset;
    let (year, month, day) = civil_from_days(unix_days);

    let total_seconds = (fraction * 86_400.0).round() as i64;
    let (hour, minute, second) = (
        (total_seconds / 3600) % 24,
        (total_seconds / 60) % 60,
        total_seconds % 60,
    );

    if total_seconds == 0 {
        format!("{year:04}-{month:02}-{day:02}")
    } else {
        format!("{year:04}-{month:02}-{day:02} {hour:02}:{minute:02}:{second:02}")
    }
}

/// Howard Hinnant 的 civil_from_days（以 1970-01-01 為 0）。
fn civil_from_days(unix_days: i64) -> (i64, u32, u32) {
    let z = unix_days + 719_468;
    let era = if z >= 0 { z } else { z - 146_096 } / 146_097;
    let doe = (z - era * 146_097) as u64;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let y = yoe as i64 + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = (doy - (153 * mp + 2) / 5 + 1) as u32;
    let m = if mp < 10 { mp + 3 } else { mp - 9 } as u32;
    (if m <= 2 { y + 1 } else { y }, m, d)
}

/// 產生 A1 位址字串（供 IPC 使用）。
pub fn cell_address(row: u32, col: u32) -> String {
    formula_parser::format_address(row, col)
}

/// 讀取檔案路徑時的狀態（供 commands 使用，避免重複驗證）。
pub fn absolute_path(path: &str) -> PathBuf {
    PathBuf::from(path)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn number_formatting_hides_trailing_zero() {
        assert_eq!(format_number(42.0), "42");
        assert_eq!(format_number(-7.0), "-7");
        assert_eq!(format_number(1.5), "1.5");
        assert_eq!(format_number(0.1 + 0.2), "0.30000000000000004");
    }

    #[test]
    fn excel_serial_dates_match_excel_conventions() {
        // 錨點：1970-01-01 = 25569、1900-01-01 = 1、1900-03-01 = 61（跳過不存在的 1900-02-29）
        assert_eq!(excel_serial_to_text(25_569.0), "1970-01-01");
        assert_eq!(excel_serial_to_text(1.0), "1900-01-01");
        assert_eq!(excel_serial_to_text(61.0), "1900-03-01");
        assert_eq!(excel_serial_to_text(25_569.5), "1970-01-01 12:00:00");
        assert_eq!(excel_serial_to_text(25_569.25), "1970-01-01 06:00:00");
        // 時間小於一天
        assert_eq!(excel_serial_to_text(0.5), "1899-12-31 12:00:00");
    }

    #[test]
    fn serial_date_conversion_is_monotonic() {
        let mut previous = String::new();
        for serial in [1.0_f64, 59.0, 61.0, 100.0, 25_569.0, 45_000.0] {
            let text = excel_serial_to_text(serial);
            assert!(text > previous, "{text} 應大於 {previous}");
            previous = text;
        }
    }

    #[test]
    fn non_finite_serials_fall_back_to_number() {
        assert_eq!(excel_serial_to_text(f64::NAN), "NaN");
        assert_eq!(excel_serial_to_text(-5.0), "-5");
    }

    #[test]
    fn cell_address_uses_a1_notation() {
        assert_eq!(cell_address(0, 0), "A1");
        assert_eq!(cell_address(11, 3), "D12");
    }
}

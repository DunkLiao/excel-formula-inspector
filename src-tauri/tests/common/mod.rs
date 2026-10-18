//! 測試共用工具：以 `rust_xlsxwriter` 程式化產生 SPEC §10 的 Fixture。
//!
//! 全部在工作簿暫存目錄即時產生，不將任何真實資料或二進位檔案納入版控。

use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};

use rust_xlsxwriter::{Formula, Format, Workbook, XlsxError};

static COUNTER: AtomicU64 = AtomicU64::new(0);

pub fn temp_dir() -> PathBuf {
    let dir = std::env::temp_dir().join(format!("efi-fixtures-{}", std::process::id()));
    std::fs::create_dir_all(&dir).expect("建立暫存目錄");
    dir
}

/// 產生唯一檔名並寫出工作簿。
pub fn write_workbook(
    name: &str,
    build: impl FnOnce(&mut Workbook) -> Result<(), XlsxError>,
) -> PathBuf {
    let sequence = COUNTER.fetch_add(1, Ordering::Relaxed);
    let path = temp_dir().join(format!("{sequence:04}-{name}"));
    let mut workbook = Workbook::new();
    build(&mut workbook).expect("建立 fixture");
    workbook.save(&path).expect("寫出 fixture");
    path
}

pub fn write_raw(name: &str, bytes: &[u8]) -> PathBuf {
    let sequence = COUNTER.fetch_add(1, Ordering::Relaxed);
    let path = temp_dir().join(format!("{sequence:04}-{name}"));
    std::fs::write(&path, bytes).expect("寫出原始檔案");
    path
}

/// 建立指定大小的稀疏檔案（用於大小上限測試，不實際佔用磁碟）。
pub fn write_sized(name: &str, size: u64) -> PathBuf {
    let sequence = COUNTER.fetch_add(1, Ordering::Relaxed);
    let path = temp_dir().join(format!("{sequence:04}-{name}"));
    let file = std::fs::File::create(&path).expect("建立檔案");
    file.set_len(size).expect("設定檔案大小");
    path
}

/// 公式儲存格（含可供檢視的快取值）。
pub fn formula_with_result(formula: &str, result: &str) -> Formula {
    Formula::new(formula).set_result(result)
}

/// 一般公式。
pub fn formula(formula: &str) -> Formula {
    Formula::new(formula)
}

pub fn bold() -> Format {
    Format::new().set_bold()
}

/// Fixture 1：一般公式（2 工作表、10 個公式，AC01）。
pub fn normal_formulas() -> PathBuf {
    write_workbook("normal-formulas.xlsx", |workbook| {
        let inputs = workbook.add_worksheet();
        inputs.set_name("Inputs")?;
        inputs.write_string(0, 0, "Item")?;
        inputs.write_string(0, 1, "Amount")?;
        for row in 1..=6u32 {
            inputs.write_string(row, 0, format!("Item {row}"))?;
            inputs.write_number(row, 1, f64::from(row) * 1_000.0)?;
        }

        let report = workbook.add_worksheet();
        report.set_name("Report")?;
        report.write_string(0, 0, "Metric")?;
        report.write_string(0, 1, "Value")?;

        // 10 個公式，分散在不同欄以避免形成同欄連續區塊（不觸發 F003）。
        report.write_string(1, 0, "SUM")?;
        report.write_formula_with_format(1, 1, formula("=SUM(Inputs!B2:B7)"), &bold())?;
        report.write_string(2, 0, "IF")?;
        report.write_formula(2, 1, formula("=IF(B2>0,\"positive\",\"other\")"))?;
        report.write_string(3, 0, "VLOOKUP")?;
        report.write_formula(3, 1, formula("=VLOOKUP(\"Item 3\",Inputs!A2:B7,2,FALSE)"))?;
        report.write_string(4, 0, "XLOOKUP")?;
        report.write_formula(4, 1, formula("=XLOOKUP(\"Item 4\",Inputs!A2:A7,Inputs!B2:B7)"))?;
        report.write_string(5, 0, "Mixed ref")?;
        report.write_formula(5, 1, formula("=$B$2+Inputs!$B$2"))?;
        report.write_string(6, 0, "Cross sheet")?;
        report.write_formula(6, 1, formula("=Inputs!B3*2"))?;
        report.write_string(7, 0, "AVERAGE")?;
        report.write_formula(7, 1, formula("=AVERAGE(Inputs!B2:B7)"))?;
        report.write_string(8, 0, "ROUND")?;
        report.write_formula(8, 1, formula("=ROUND(Inputs!B2/3,2)"))?;
        report.write_string(9, 0, "TEXT")?;
        report.write_formula(9, 1, formula("=TEXT(Inputs!B2,\"0.00\")"))?;
        report.write_string(10, 0, "CONCAT")?;
        report.write_formula(10, 1, formula("=CONCAT(Inputs!A2,\"-\",Inputs!A3)"))?;
        Ok(())
    })
}

/// Fixture 2：無效參照與錯誤值（含字串常值內 `#REF!`）。
pub fn invalid_reference() -> PathBuf {
    write_workbook("invalid-reference.xlsx", |workbook| {
        let sheet = workbook.add_worksheet();
        sheet.set_name("Errors")?;
        sheet.write_string(0, 0, "Class")?;
        sheet.write_string(0, 1, "Value")?;

        // 真正的 #REF!（語法位置）→ F001 + F002
        sheet.write_string(1, 0, "invalid ref")?;
        sheet.write_formula(1, 1, formula_with_result("=B2+#REF!", "#REF!"))?;
        // 計算錯誤 #DIV/0! → F001（不是 F002）
        sheet.write_string(2, 0, "div0")?;
        sheet.write_formula(2, 1, formula_with_result("=1/0", "#DIV/0!"))?;
        // 字串常值含 #REF! → 不得產生 F002
        sheet.write_string(3, 0, "text literal")?;
        sheet.write_formula(
            3,
            1,
            formula_with_result("=B2&\"#REF! literal\"", "value #REF! literal"),
        )?;
        // 純文字常值
        sheet.write_string(4, 0, "plain text")?;
        sheet.write_string(4, 1, "#REF!")?;
        Ok(())
    })
}

/// Fixture 3：同欄公式模式異常與常數覆寫。
pub fn pattern_anomaly() -> PathBuf {
    write_workbook("pattern-anomaly.xlsx", |workbook| {
        let sheet = workbook.add_worksheet();
        sheet.set_name("Patterns")?;
        sheet.write_string(0, 1, "Base")?;
        sheet.write_string(0, 2, "Copied")?;
        sheet.write_string(0, 3, "Doubled")?;
        for row in 1..=6u32 {
            sheet.write_number(row, 1, f64::from(row) * 10.0)?;
        }

        // C2:C6 — 四列相同模式、一列相對列位移偏離
        sheet.write_formula(1, 2, formula("=B2+1"))?;
        sheet.write_formula(2, 2, formula("=B3+1"))?;
        sheet.write_formula(3, 2, formula("=B4+1"))?;
        sheet.write_formula(4, 2, formula("=B5+1"))?;
        sheet.write_formula(5, 2, formula("=B11+1"))?;

        // D2:D6 — 中間一列被常數覆寫
        sheet.write_formula(1, 3, formula("=C2*2"))?;
        sheet.write_formula(2, 3, formula("=C3*2"))?;
        sheet.write_number(3, 3, 123.0)?;
        sheet.write_formula(4, 3, formula("=C5*2"))?;
        sheet.write_formula(5, 3, formula("=C6*2"))?;
        Ok(())
    })
}

/// Fixture 4：刻意中斷（標題列、合計列、空白列、合併儲存格）——誤報控制。
pub fn intentional_breaks() -> PathBuf {
    write_workbook("intentional-breaks.xlsx", |workbook| {
        let sheet = workbook.add_worksheet();
        sheet.set_name("Breaks")?;
        // 標題列（合併儲存格）
        sheet.merge_range(0, 0, 0, 2, "Monthly Report", &bold())?;
        sheet.write_string(2, 0, "Month")?;
        sheet.write_string(2, 1, "Base")?;
        sheet.write_string(2, 2, "Calc")?;

        for row in 3..=5u32 {
            sheet.write_string(row, 0, format!("M{row}"))?;
            sheet.write_number(row, 1, f64::from(row) * 100.0)?;
        }
        sheet.write_formula(3, 2, formula("=B4*2"))?;
        sheet.write_formula(4, 2, formula("=B5*2"))?;
        sheet.write_formula(5, 2, formula("=B6*2"))?;

        // 第 7 列完全空白 → 區段切開，不得跨列推定
        for row in 7..=9u32 {
            sheet.write_string(row, 0, format!("N{row}"))?;
            sheet.write_number(row, 1, f64::from(row) * 100.0)?;
        }
        sheet.write_formula(7, 2, formula("=B8*2"))?;
        // 第 9 列（0-based 8）本欄空白，但其他欄有註解 → 序列缺口
        sheet.write_string(8, 4, "note")?;
        sheet.write_formula(9, 2, formula("=B10*2"))?;

        // 合計列（常數）夾在公式之間
        sheet.write_string(10, 0, "Total")?;
        sheet.write_number(10, 1, 999.0)?;
        sheet.write_number(10, 2, 1_000.0)?;
        sheet.write_formula(11, 2, formula("=B12*2"))?;
        sheet.write_formula(12, 2, formula("=B13*2"))?;
        Ok(())
    })
}

/// Fixture 5：暫不支援的特殊語法（結構化參照、外部活頁簿、3D 參照、溢出運算子）。
pub fn special_formulas() -> PathBuf {
    write_workbook("special-formulas.xlsx", |workbook| {
        let sheet = workbook.add_worksheet();
        sheet.set_name("Special")?;
        sheet.write_number(0, 0, 1.0)?;
        sheet.write_number(1, 0, 2.0)?;
        sheet.write_number(2, 0, 3.0)?;

        // 三列相同模式（主流），其餘為不支援語法 → 不得報 F003
        sheet.write_formula(0, 1, formula("=A1+1"))?;
        sheet.write_formula(1, 1, formula("=A2+1"))?;
        sheet.write_formula(2, 1, formula("=A3+1"))?;
        sheet.write_formula(3, 1, formula("=SUM(Table1[Amount])"))?;
        sheet.write_formula(4, 1, formula("='[Book1.xlsx]Sheet1'!A1"))?;
        sheet.write_formula(5, 1, formula("=SUM(Sheet1:Sheet3!A1)"))?;
        // 溢出／隱含交集運算子
        sheet.write_formula(6, 1, formula("=A1#"))?;
        Ok(())
    })
}

/// Fixture 6：稀疏工作表，資料落在很後面的列。
pub fn large_sparse(last_row: u32) -> PathBuf {
    write_workbook("large-sparse.xlsx", move |workbook| {
        let sheet = workbook.add_worksheet();
        sheet.set_name("Sparse")?;
        sheet.write_string(0, 0, "start")?;
        sheet.write_number(0, 1, 1.0)?;
        sheet.write_string(last_row, 0, "end")?;
        sheet.write_number(last_row, 2, 42.0)?;
        Ok(())
    })
}

/// Fixture 7：損毀檔案（非 CFB、非 zip 的隨機位元組）。
pub fn damaged_xlsx() -> PathBuf {
    let mut bytes = vec![0x50u8, 0x4b, 0x03, 0x04];
    bytes.extend((0..512).map(|i| (i % 251) as u8));
    write_raw("damaged.xlsx", &bytes)
}

/// Fixture 7：不支援的舊版格式（僅有副檔名，內容無關）。
pub fn unsupported_xls() -> PathBuf {
    write_raw("unsupported.xls", &[0xd0, 0xcf, 0x11, 0xe0])
}

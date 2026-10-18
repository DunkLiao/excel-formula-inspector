# 驗收條件對照（AC01–AC12）

本文件將 SPEC §9 的每一條驗收條件對應到**實際執行過的測試**或**需人工執行的步驟**。

執行環境：Windows、Rust 1.96.0、Node.js 24.16.0、WebView2 Runtime 155.0.4283.60。

## 自動化測試涵蓋

| AC | 內容 | 涵蓋測試 | 結果 |
|---|---|---|---|
| AC01 | 匯入含 2 工作表、10 公式的 xlsx | `src-tauri/tests/reader.rs::ac01_reads_two_sheets_and_ten_formulas` | ✅ 2 工作表、10 公式、位置可逐一列出 |
| AC01 | 位址正確（非相對位移） | `reader.rs::formula_addresses_are_absolute_not_relative_to_range_start` | ✅ 已使用範圍 `A1:B11`，`B2` 為公式、`A1` 不是 |
| AC02 | 錯誤型儲存格 → F001 | `tests/rules_integration.rs::ac02_error_cells_produce_f001_with_class_and_position` | ✅ F001 命中 `B2`/`B3`，附錯誤類別與「快取值可能過期」提示 |
| AC03 | `#REF!` → F002，字串常值不算 | `rules_integration.rs::ac03_ref_error_detected_but_not_inside_string_literals`；`formula_parser::tests::string_literals_never_leak_ref_errors_or_refs` | ✅ 只命中 `B2`；`=B3&"#REF! literal"` 與純文字 `#REF!` 皆未誤報 |
| AC04 | 同欄 5 筆公式，1 筆相對參照偏離 → F003 | `rules_integration.rs::ac04_pattern_deviation_produces_f003_with_dominant_evidence` | ✅ 命中 `C6`，證據含主流簽章 `4/5` |
| AC05 | 公式列中間被常數取代 → F004 + 信心程度 | `rules_integration.rs::ac05_constant_override_produces_f004_with_confidence_and_caveat` | ✅ 命中 `D4`，標示「疑似」、Medium，並提示可能是小計／輸入欄 |
| AC06 | 同欄公式中間空白 → F005，且不跨空白整列誤判 | `rules_integration.rs::ac06_gap_rule_respects_fully_blank_rows`、`f005_flags_gap_only_when_row_is_not_empty_elsewhere`、`f005_never_fires_across_a_fully_blank_row` | ✅ 只命中同列其他欄有內容的缺口 `C9`；完全空白列不推定 |
| AC07 | 取消掃描 | `reader.rs::cancellation_stops_reading`、`rules_integration.rs::ac07_cancellation_aborts_rule_execution`、`tests/ui.test.tsx::Header > 掃描中提供取消按鈕` | ✅ 讀取與規則階段皆可中止；UI 取消後回到可重新匯入狀態 |
| AC08 | 篩選 High + F001 匯出 CSV | `exporter::tests`（BOM、欄位順序、公式注入防護、中文可讀）、`tests/filters.test.ts::filterIssues`（篩選結果即匯出內容） | ✅ CSV 僅含篩選後問題；UTF-8 BOM + 9 欄完整 |
| AC09 | xls / xlsb / 加密 / 損毀檔 | `reader.rs::ac09_rejects_unsupported_and_damaged_files`、`large_file_requires_confirmation_then_allows_experimental_mode` | ✅ 分別回報 `UNSUPPORTED_FORMAT` / `CORRUPT` / `FILE_NOT_FOUND` / `LARGE_FILE_NEEDS_CONFIRMATION` / `TOO_LARGE`，不崩潰 |
| AC11 | 不支援的特殊語法 | `rules_integration.rs::ac11_unsupported_syntax_is_not_reported_as_anomaly`、`formula_parser::tests::dynamic_array_syntax_is_flagged_unsupported`、`structured_reference_is_flagged_unsupported`、`external_workbook_reference_is_flagged_unsupported`、`three_d_reference_is_flagged_unsupported` | ✅ 標記「無法判讀」，不假報錯誤 |
| AC03/AC06 | 誤報控制（刻意中斷 fixture） | `rules_integration.rs::intentional_breaks_do_not_produce_pattern_false_positives`、`normal_workbook_has_no_issues_at_all` | ✅ 標題列／空白列／合併儲存格不產生 F003；一般工作簿零問題 |
| — | 大檔兩段式策略 | `reader.rs::large_file_requires_confirmation_then_allows_experimental_mode`、`tests/ui.test.tsx::超過 20 MB 時先要求確認…` | ✅ >20 MB 先確認，確認後以實驗性模式啟動；>200 MB 直接拒絕 |
| — | 稀疏工作表效能 | `reader.rs::ac11_sparse_sheet_with_data_at_the_far_end` | ✅ 尾端第 100,000 列可找到，僅走訪 4 個實際存在的儲存格 |

執行命令與結果：

```text
cd src-tauri; cargo test
  56 passed（單元）
  11 passed（engine_queries 整合）
  11 passed（reader 整合）
  12 passed（rules 整合）
  0 failed
npm run test
  24 passed / 0 failed
```

## 需人工執行的驗收步驟

| AC | 步驟 | 預期 |
|---|---|---|
| 啟動 | 以 `src-tauri/target/release/excel-formula-inspector.exe`（或安裝檔）啟動。 | 視窗正常顯示匯入畫面（表示 CSP 未阻擋 UI 與 IPC），無白畫面 |
| AC10 | 1. 開啟資源監視器（`resmon`）→「網路」分頁，記下目前連線。<br>2. 啟動本工具並匯入含敏感字串的測試檔。<br>3. 觀察 `excel-formula-inspector.exe` 的 TCP 連線。 | 連線數維持 0；`%LOCALAPPDATA%`／`%APPDATA%` 無本工具寫出的工作簿暫存檔 |
| AC10（輔助） | 以 Wireshark 或 `netsh trace start capture=yes` 擷取，重複上述流程。 | 沒有包含檔案內容的對外請求 |
| AC12 | 匯入檔案 → 檢查 →「清除資料」。 | 回到初始畫面；`get_scan_payload` 回 `SCAN_NOT_FOUND`；記憶體中的結果已釋放 |
| AC01–AC09、AC11（UI 層） | 依 README 操作流程，以 `tests/fixtures` 等價的檔案（可由 `src-tauri/tests/common/mod.rs` 的產生器產生）逐項操作。 | 與上表自動化測試一致 |

### 產生驗收用的 Fixture 檔案

`src-tauri/tests/common/mod.rs` 以 `rust_xlsxwriter` 程式化產生 SPEC §10 的七個 Fixture
（`normal-formulas`、`invalid-reference`、`pattern-anomaly`、`intentional-breaks`、
`special-formulas`、`large-sparse`、`unsupported.xls`／`damaged.xlsx`）。
執行整合測試時會寫入 `%TEMP%\efi-fixtures-<pid>\`，可直接取用該目錄中的檔案做手動驗收：

```powershell
cd src-tauri; cargo test --test reader   # 產生 Fixture 後可至 %TEMP%\efi-fixtures-* 取檔
```

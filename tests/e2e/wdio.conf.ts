/**
 * WebdriverIO 設定：以 tauri-driver 驅動建置後的桌面應用程式。
 *
 * 前置需求與執行方式見 docs/e2e-runbook.md。
 * 需要互動式 Windows 工作階段（WebView2 視窗必須能開啟）。
 */

import { spawn, type ChildProcess } from 'node:child_process';
import { existsSync } from 'node:fs';
import path from 'node:path';
import { fileURLToPath } from 'node:url';

const here = path.dirname(fileURLToPath(import.meta.url));
const application = path.resolve(
  here,
  '../../src-tauri/target/release/excel-formula-inspector.exe',
);

let driver: ChildProcess | null = null;

export const config = {
  hostname: '127.0.0.1',
  port: 4444,
  specs: ['./app.e2e.ts'],
  maxInstances: 1,
  capabilities: {
    'tauri:options': {
      application,
    },
  },
  framework: 'mocha',
  mochaOpts: {
    ui: 'bdd',
    timeout: 120_000,
  },
  reporters: ['spec'],
  logLevel: 'warn',

  onPrepare: () => {
    if (!existsSync(application)) {
      throw new Error(
        `找不到建置產物：${application}\n請先執行 npm run tauri build（詳見 docs/e2e-runbook.md）`,
      );
    }
    driver = spawn('tauri-driver', [], {
      stdio: [null as unknown as 'pipe', process.stdout, process.stderr],
    });
  },

  onComplete: () => {
    driver?.kill();
  },
};

export default config;

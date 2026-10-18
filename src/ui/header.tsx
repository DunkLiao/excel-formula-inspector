import { PHASE_LABELS, formatBytes } from '../types';
import { useScanStore } from '../state';

/** 產品標題、本機處理標記、掃描進度與清除資料（SPEC §4.1）。 */
export function Header() {
  const status = useScanStore((s) => s.status);
  const progress = useScanStore((s) => s.progress);
  const fileName = useScanStore((s) => s.fileName);
  const pending = useScanStore((s) => s.pendingLargeFile);
  const fileSize = useScanStore((s) => s.fileSize);
  const cancelScan = useScanStore((s) => s.cancelScan);
  const clearData = useScanStore((s) => s.clearData);
  const theme = useScanStore((s) => s.theme);
  const setTheme = useScanStore((s) => s.setTheme);

  const scanning = status === 'scanning';
  const percent =
    progress && progress.total > 0
      ? Math.min(100, Math.round((progress.done / progress.total) * 100))
      : 0;

  return (
    <header className="app-header">
      <div className="brand">
        <span className="brand-mark" aria-hidden="true">
          fx
        </span>
        <div>
          <h1>Excel Formula Inspector</h1>
          <p className="brand-sub">公式靜態檢查器 v1.0.0</p>
        </div>
      </div>

      <div className="header-middle">
        {scanning ? (
          <div className="scan-status" role="status">
            <div className="scan-line">
              <strong>{pending ? '準備開始' : '掃描中'}</strong>
              <span className="scan-file">{fileName ?? '（讀取中）'}</span>
              <button type="button" className="btn btn-ghost" onClick={() => void cancelScan()}>
                取消
              </button>
            </div>
            <div className="progress" aria-hidden="true">
              <div className="progress-fill" style={{ width: `${percent}%` }} />
            </div>
            <div className="scan-detail">
              <span>
                {PHASE_LABELS[progress?.phase ?? ''] ?? progress?.phase ?? '準備中'}
                {progress?.label ? `：${progress.label}` : ''}
              </span>
              <span>
                {progress ? `${progress.done} / ${progress.total} 個工作表` : ''}
              </span>
            </div>
          </div>
        ) : (
          <div className="local-badge" title="所有處理都在這台電腦完成">
            <span className="dot" aria-hidden="true" />
            在此裝置本機處理{fileName ? `：${fileName}` : ''}
            {fileSize ? `（${formatBytes(fileSize)}）` : ''}
          </div>
        )}
      </div>

      <div className="header-actions">
        <div className="theme-switch" role="group" aria-label="佈景主題">
          <button
            type="button"
            className={theme === 'light' ? 'theme-option theme-option-active' : 'theme-option'}
            aria-pressed={theme === 'light'}
            title="淺色模式"
            onClick={() => setTheme('light')}
          >
            ☀ 淺色
          </button>
          <button
            type="button"
            className={theme === 'dark' ? 'theme-option theme-option-active' : 'theme-option'}
            aria-pressed={theme === 'dark'}
            title="深色模式"
            onClick={() => setTheme('dark')}
          >
            ☾ 深色
          </button>
        </div>

        <button
          type="button"
          className="btn"
          onClick={() => void clearData()}
          disabled={status === 'idle' && !fileName}
        >
          清除資料
        </button>
      </div>
    </header>
  );
}

import { formatBytes } from '../types';
import { useScanStore } from '../state';
import * as ipc from '../ipc';

/** 匯入頁：檔案選擇、拖放提示、隱私聲明、大檔確認與錯誤訊息（SPEC §3、§8.1）。 */
export function UploadPanel() {
  const status = useScanStore((s) => s.status);
  const error = useScanStore((s) => s.error);
  const pending = useScanStore((s) => s.pendingLargeFile);
  const notice = useScanStore((s) => s.notice);
  const lastAttemptName = useScanStore((s) => s.lastAttemptName);
  const requestScan = useScanStore((s) => s.requestScan);
  const confirmLargeScan = useScanStore((s) => s.confirmLargeScan);
  const cancelScan = useScanStore((s) => s.cancelScan);
  const setNotice = useScanStore((s) => s.setNotice);

  const scanning = status === 'scanning';

  async function chooseFile() {
    try {
      const path = await ipc.pickWorkbook();
      if (!path) return;
      await requestScan(path);
    } catch (raw) {
      setNotice(ipc.describeError(raw).message);
    }
  }

  return (
    <section className="upload">
      <div className="upload-card">
        <h2>匯入 Excel 工作簿</h2>
        <p className="upload-hint">
          支援 .xlsx 與 .xlsm（保留巨集但<strong>不執行</strong>；.xlsm 只分析儲存格與公式，
          <strong>無法檢查 VBA 邏輯</strong>）。
        </p>

        <div className="upload-actions">
          <button
            type="button"
            className="btn btn-primary"
            onClick={() => void chooseFile()}
            disabled={scanning}
          >
            選擇檔案
          </button>
          <span className="upload-or">或將檔案拖曳到視窗</span>
        </div>

        {scanning && (
          <div className="upload-scanning" role="status">
            <span>正在解析，請稍候…</span>
            <button type="button" className="btn btn-ghost" onClick={() => void cancelScan()}>
              取消
            </button>
          </div>
        )}

        {pending && (
          <div className="callout callout-warn" role="alert">
            <strong>檔案較大（{formatBytes(pending.sizeBytes)}）</strong>
            <p>
              超過預設上限 20 MB。實驗性模式會使用較多記憶體，處理時間也可能明顯變長；建議先關閉其他大型程式。
            </p>
            <div className="callout-actions">
              <button
                type="button"
                className="btn btn-primary"
                onClick={() => void confirmLargeScan()}
              >
                以實驗性模式繼續
              </button>
              <button type="button" className="btn" onClick={() => void cancelScan()}>
                取消
              </button>
            </div>
          </div>
        )}

        {status === 'error' && error && (
          <div className="callout callout-error" role="alert">
            <strong>
              無法完成匯入{lastAttemptName ? `：${lastAttemptName}` : ''}
            </strong>
            <p>{error.message}</p>
            <p className="mono">錯誤代碼：{error.code}</p>
            {error.code === 'CORRUPT' && (
              <p className="muted">
                檔案可能不是標準的 .xlsx / .xlsm（例如被改名、或是用其他工具產生的佔位檔）。
                可改用 Excel 另存新檔後再試一次。
              </p>
            )}
          </div>
        )}

        {notice && <div className="callout">{notice}</div>}

        <ul className="upload-privacy">
          <li>所有解析與檢查都在這台電腦完成，不會發出任何網路連線。</li>
          <li>不執行巨集、不讀取外部連結、不重新計算或修改公式。</li>
          <li>清除資料後，記憶體中的工作簿內容與檢查結果會一併移除。</li>
        </ul>
      </div>
    </section>
  );
}

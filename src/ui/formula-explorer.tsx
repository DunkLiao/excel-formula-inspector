import { useCallback, useEffect, useState } from 'react';

import * as ipc from '../ipc';
import { useScanStore } from '../state';
import { VALUE_KIND_LABELS, parseAddress, type FormulaPage, type FormulaRecord } from '../types';
import { CellContextPanel } from './cell-context';

const PAGE_SIZE = 200;

/** 公式檢視：工作表切換、位址跳轉、公式搜尋與明細（SPEC §4.4）。 */
export function FormulaExplorer() {
  const scanId = useScanStore((s) => s.scanId);
  const payload = useScanStore((s) => s.payload);
  const selectedCell = useScanStore((s) => s.selectedCell);
  const setNotice = useScanStore((s) => s.setNotice);

  const [sheetName, setSheetName] = useState<string>('');
  const [addressInput, setAddressInput] = useState('');
  const [textInput, setTextInput] = useState('');
  const [onlyIssues, setOnlyIssues] = useState(false);
  const [offset, setOffset] = useState(0);
  const [page, setPage] = useState<FormulaPage | null>(null);
  const [active, setActive] = useState<{ sheetName: string; address: string } | null>(null);
  const [loading, setLoading] = useState(false);

  useEffect(() => {
    const first = payload?.sheets[0]?.name;
    if (first && sheetName === '') setSheetName(first);
  }, [payload, sheetName]);

  // 由其他檢視（問題清單／網格）指定要查看的儲存格
  useEffect(() => {
    if (!selectedCell) return;
    setSheetName(selectedCell.sheetName);
    setAddressInput(selectedCell.address);
    setActive(selectedCell);
  }, [selectedCell]);

  const load = useCallback(async () => {
    if (!scanId) return;
    setLoading(true);
    try {
      const result = await ipc.getFormulas(scanId, {
        sheetName: sheetName || undefined,
        address: addressInput.trim() || undefined,
        text: textInput.trim() || undefined,
        onlyIssues,
        offset,
        limit: PAGE_SIZE,
      });
      setPage(result);
    } catch (raw) {
      setNotice(ipc.describeError(raw).message);
    } finally {
      setLoading(false);
    }
  }, [scanId, sheetName, addressInput, textInput, onlyIssues, offset, setNotice]);

  useEffect(() => {
    void load();
  }, [load]);

  function jumpToAddress() {
    const value = addressInput.trim();
    if (!value) return;
    const parsed = parseAddress(value);
    if (!parsed) {
      setNotice(`無法識別的儲存格位址：${value}`);
      return;
    }
    setOffset(0);
    setActive({ sheetName, address: value.toUpperCase() });
  }

  return (
    <div className="explorer-layout">
      <div className="explorer-main">
        <div className="filter-bar">
          <div className="filter-group">
            <span className="filter-label">工作表</span>
            <select
              value={sheetName}
              onChange={(event) => {
                setSheetName(event.target.value);
                setOffset(0);
              }}
            >
              {(payload?.sheets ?? []).map((sheet) => (
                <option key={sheet.name} value={sheet.name}>
                  {sheet.name}
                </option>
              ))}
            </select>
          </div>

          <div className="filter-group">
            <span className="filter-label">位址跳轉</span>
            <input
              type="text"
              className="mono"
              placeholder="例如 D12"
              value={addressInput}
              onChange={(event) => {
                setAddressInput(event.target.value);
                setOffset(0);
              }}
              onKeyDown={(event) => {
                if (event.key === 'Enter') jumpToAddress();
              }}
            />
            <button type="button" className="btn btn-small" onClick={jumpToAddress}>
              跳轉
            </button>
          </div>

          <div className="filter-group grow">
            <span className="filter-label">公式搜尋</span>
            <input
              type="search"
              placeholder="例如 SUM、#REF!、Inputs!"
              value={textInput}
              onChange={(event) => {
                setTextInput(event.target.value);
                setOffset(0);
              }}
            />
          </div>

          <label className="check">
            <input
              type="checkbox"
              checked={onlyIssues}
              onChange={(event) => {
                setOnlyIssues(event.target.checked);
                setOffset(0);
              }}
            />
            僅顯示被標記的公式
          </label>
        </div>

        <div className="toolbar">
          <span className="count">
            {loading
              ? '載入中…'
              : `共 ${(page?.total ?? 0).toLocaleString('en-US')} 筆公式${
                  page && page.total > 0
                    ? `，顯示第 ${page.offset + 1}–${Math.min(
                        page.offset + page.limit,
                        page.total,
                      )} 筆`
                    : ''
                }`}
          </span>
          <div className="toolbar-actions">
            <button
              type="button"
              className="btn btn-small"
              disabled={offset === 0}
              onClick={() => setOffset(Math.max(0, offset - PAGE_SIZE))}
            >
              上一頁
            </button>
            <button
              type="button"
              className="btn btn-small"
              disabled={!page || page.offset + page.limit >= page.total}
              onClick={() => setOffset(offset + PAGE_SIZE)}
            >
              下一頁
            </button>
          </div>
        </div>

        <div className="table-scroll grow-scroll">
          <table className="data-table">
            <thead>
              <tr>
                <th>儲存格</th>
                <th>公式</th>
                <th>快取值</th>
                <th>型別</th>
                <th>標記</th>
              </tr>
            </thead>
            <tbody>
              {(page?.items ?? []).map((item) => (
                <FormulaRow
                  key={`${item.sheetName}!${item.cellAddress}`}
                  item={item}
                  selected={
                    active?.address === item.cellAddress && active?.sheetName === item.sheetName
                  }
                  onSelect={() => setActive({ sheetName: item.sheetName, address: item.cellAddress })}
                />
              ))}
              {page && page.items.length === 0 && (
                <tr>
                  <td colSpan={5} className="muted">
                    沒有符合條件的公式。
                  </td>
                </tr>
              )}
            </tbody>
          </table>
        </div>
      </div>

      <aside className="explorer-detail">
        {active ? (
          <CellContextPanel sheetName={active.sheetName} address={active.address} />
        ) : (
          <p className="detail muted">
            選擇一筆公式或輸入位址跳轉，即可查看直接參照、快取值與同欄模式比較。
          </p>
        )}
      </aside>
    </div>
  );
}

function FormulaRow({
  item,
  selected,
  onSelect,
}: {
  item: FormulaRecord;
  selected: boolean;
  onSelect: () => void;
}) {
  return (
    <tr
      className={selected ? 'row-selected' : item.issueIds.length > 0 ? 'row-flagged' : undefined}
      onClick={onSelect}
    >
      <td className="mono nowrap">{item.cellAddress}</td>
      <td className="mono truncate" title={item.formula}>
        {item.formula}
      </td>
      <td className="mono truncate">{item.displayValue ?? '—'}</td>
      <td>{VALUE_KIND_LABELS[item.kind]}</td>
      <td>{item.issueIds.length > 0 ? `⚑ ${item.issueIds.length}` : '—'}</td>
    </tr>
  );
}

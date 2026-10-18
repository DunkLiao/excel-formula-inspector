import { useCallback, useEffect, useMemo, useRef, useState } from 'react';

import * as ipc from '../ipc';
import { useScanStore } from '../state';
import { columnLetters, parseAddress, type GridCell } from '../types';
import { CellContextPanel } from './cell-context';

const ROW_HEIGHT = 28;
const COL_WIDTH = 120;
const ROW_HEADER_WIDTH = 64;
const HEADER_HEIGHT = 28;
const BLOCK_ROWS = 100;
const BLOCK_COLS = 30;
const OVERSCAN = 6;

/**
 * 工作表網格：只覆蓋已使用範圍，並以虛擬捲動 + 區塊載入（`get_sheet_window`）呈現（SPEC §2.1-5、§8.2）。
 */
export function SheetGrid() {
  const scanId = useScanStore((s) => s.scanId);
  const payload = useScanStore((s) => s.payload);
  const selectedCell = useScanStore((s) => s.selectedCell);
  const selectCell = useScanStore((s) => s.selectCell);
  const setNotice = useScanStore((s) => s.setNotice);

  const [sheetName, setSheetName] = useState('');
  const [jumpInput, setJumpInput] = useState('');
  const [scrollTop, setScrollTop] = useState(0);
  const [scrollLeft, setScrollLeft] = useState(0);
  const [viewport, setViewport] = useState({ width: 900, height: 460 });
  const [cells, setCells] = useState<Map<string, GridCell>>(new Map());
  const [active, setActive] = useState<{ sheetName: string; address: string } | null>(null);
  const scrollerRef = useRef<HTMLDivElement>(null);
  const loadingBlocks = useRef<Set<string>>(new Set());

  const sheet = useMemo(
    () => payload?.sheets.find((s) => s.name === sheetName) ?? payload?.sheets[0],
    [payload, sheetName],
  );

  useEffect(() => {
    const first = payload?.sheets[0]?.name;
    if (!sheetName && first) setSheetName(first);
  }, [payload, sheetName]);

  useEffect(() => {
    if (selectedCell) {
      setSheetName(selectedCell.sheetName);
      setActive(selectedCell);
    }
  }, [selectedCell]);

  // 切換工作表時清空快取
  useEffect(() => {
    setCells(new Map());
    loadingBlocks.current.clear();
  }, [sheetName]);

  useEffect(() => {
    const element = scrollerRef.current;
    if (!element) return;
    const observer = new ResizeObserver(() => {
      setViewport({ width: element.clientWidth, height: element.clientHeight });
    });
    observer.observe(element);
    setViewport({ width: element.clientWidth, height: element.clientHeight });
    return () => observer.disconnect();
  }, []);

  const totalRows = sheet?.rowCount ?? 0;
  const totalCols = sheet?.colCount ?? 0;
  const canvasWidth = ROW_HEADER_WIDTH + totalCols * COL_WIDTH;
  const canvasHeight = totalRows * ROW_HEIGHT + HEADER_HEIGHT;

  const firstRow = Math.max(0, Math.floor(scrollTop / ROW_HEIGHT) - OVERSCAN);
  const lastRow = Math.min(
    totalRows,
    Math.ceil((scrollTop + viewport.height) / ROW_HEIGHT) + OVERSCAN,
  );
  const firstCol = Math.max(0, Math.floor(scrollLeft / COL_WIDTH) - 1);
  const lastCol = Math.min(totalCols, Math.ceil((scrollLeft + viewport.width) / COL_WIDTH) + 1);

  const loadWindow = useCallback(
    async (startRow: number, startCol: number) => {
      if (!scanId || !sheet) return;
      const rows = Math.min(BLOCK_ROWS, Math.max(1, totalRows - startRow));
      const cols = Math.min(BLOCK_COLS, Math.max(1, totalCols - startCol));
      if (rows <= 0 || cols <= 0) return;
      const key = `${startRow}:${startCol}`;
      if (loadingBlocks.current.has(key)) return;
      loadingBlocks.current.add(key);
      try {
        const window = await ipc.getSheetWindow(scanId, sheet.name, startRow, startCol, rows, cols);
        setCells((previous) => {
          const next = new Map(previous);
          for (const cell of window.cells) {
            next.set(`${cell.row}:${cell.col}`, cell);
          }
          return next;
        });
      } catch (raw) {
        setNotice(ipc.describeError(raw).message);
      } finally {
        loadingBlocks.current.delete(key);
      }
    },
    [scanId, sheet, totalRows, totalCols, setNotice],
  );

  // 依可見範圍載入缺少的區塊（含鄰近區塊預取）
  useEffect(() => {
    if (!sheet) return;
    const startBlockRow = Math.floor(firstRow / BLOCK_ROWS) * BLOCK_ROWS;
    const startBlockCol = Math.floor(firstCol / BLOCK_COLS) * BLOCK_COLS;
    const endBlockRow = Math.floor(Math.max(0, lastRow - 1) / BLOCK_ROWS) * BLOCK_ROWS;
    const endBlockCol = Math.floor(Math.max(0, lastCol - 1) / BLOCK_COLS) * BLOCK_COLS;
    for (let row = startBlockRow; row <= endBlockRow; row += BLOCK_ROWS) {
      for (let col = startBlockCol; col <= endBlockCol; col += BLOCK_COLS) {
        void loadWindow(row, col);
      }
    }
  }, [firstRow, lastRow, firstCol, lastCol, loadWindow, sheet]);

  const visibleRows = useMemo(() => {
    const rows: number[] = [];
    for (let row = firstRow; row < lastRow; row += 1) rows.push(row);
    return rows;
  }, [firstRow, lastRow]);

  const visibleCols = useMemo(() => {
    const cols: number[] = [];
    for (let col = firstCol; col < lastCol; col += 1) cols.push(col);
    return cols;
  }, [firstCol, lastCol]);

  function jump() {
    const raw = jumpInput.trim();
    if (!raw) return;
    const parsed = parseAddress(raw);
    if (!parsed) {
      setNotice(`無法識別的儲存格位址：${raw}`);
      return;
    }
    const element = scrollerRef.current;
    if (element) {
      element.scrollTop = parsed.row * ROW_HEIGHT;
      element.scrollLeft = Math.max(0, parsed.col * COL_WIDTH - COL_WIDTH);
    }
    setActive({ sheetName: sheet?.name ?? '', address: raw.toUpperCase() });
  }

  if (!payload || !sheet) {
    return <p className="empty">尚無可顯示的工作表。</p>;
  }

  return (
    <div className="grid-layout">
      <div className="grid-main">
        <div className="filter-bar">
          <div className="filter-group">
            <span className="filter-label">工作表</span>
            <select
              value={sheet.name}
              onChange={(event) => {
                setSheetName(event.target.value);
                setActive(null);
                setScrollTop(0);
                setScrollLeft(0);
                const element = scrollerRef.current;
                if (element) {
                  element.scrollTop = 0;
                  element.scrollLeft = 0;
                }
              }}
            >
              {payload.sheets.map((item) => (
                <option key={item.name} value={item.name}>
                  {item.name}
                </option>
              ))}
            </select>
          </div>
          <div className="filter-group">
            <span className="filter-label">已使用範圍</span>
            <span className="mono">{sheet.usedRange ?? '（空白）'}</span>
          </div>
          <div className="filter-group">
            <span className="filter-label">前往儲存格</span>
            <input
              type="text"
              className="mono"
              placeholder="例如 D120"
              value={jumpInput}
              onChange={(event) => setJumpInput(event.target.value)}
              onKeyDown={(event) => {
                if (event.key === 'Enter') jump();
              }}
            />
            <button type="button" className="btn btn-small" onClick={jump}>
              前往
            </button>
          </div>

          <div className="filter-group grow">
            <span className="filter-label">目前位置</span>
            <span className="mono">{active?.address ?? '—'}</span>
          </div>
        </div>

        <div className="grid-legend">
          <span className="legend legend-flag">⚑ 有問題標記</span>
          <span className="legend legend-formula">fx 公式</span>
          <span className="muted">
            網格僅顯示已使用範圍；資料以區塊向 Rust 分批取得，不一次載入整張表。
          </span>
        </div>

        <div
          className="grid-scroller"
          ref={scrollerRef}
          onScroll={(event) => {
            setScrollTop(event.currentTarget.scrollTop);
            setScrollLeft(event.currentTarget.scrollLeft);
          }}
        >
          <div className="grid-canvas" style={{ width: canvasWidth, height: canvasHeight }}>
            <div className="sheet-col-header">
              <div className="sheet-corner" style={{ width: ROW_HEADER_WIDTH }} />
              {visibleCols.map((col) => (
                <div
                  key={col}
                  className="sheet-col-cell"
                  style={{
                    left: ROW_HEADER_WIDTH + col * COL_WIDTH,
                    width: COL_WIDTH,
                    height: HEADER_HEIGHT,
                  }}
                >
                  {columnLetters(col)}
                </div>
              ))}
            </div>

            {visibleRows.map((row) => (
              <div
                key={row}
                className="sheet-row"
                style={{
                  top: HEADER_HEIGHT + row * ROW_HEIGHT,
                  height: ROW_HEIGHT,
                  width: canvasWidth,
                }}
              >
                <div className="sheet-row-head" style={{ width: ROW_HEADER_WIDTH }}>
                  {row + 1}
                </div>
                {visibleCols.map((col) => {
                  const cell = cells.get(`${row}:${col}`);
                  const address = cell?.address ?? `${columnLetters(col)}${row + 1}`;
                  const flagged = (cell?.issueIds.length ?? 0) > 0;
                  const isActive =
                    active?.address === address && active?.sheetName === sheet.name;
                  const classes = ['sheet-cell'];
                  if (flagged) classes.push('sheet-cell-flagged');
                  if (cell?.formula) classes.push('sheet-cell-formula');
                  if (isActive) classes.push('sheet-cell-active');
                  return (
                    <div
                      key={col}
                      className={classes.join(' ')}
                      style={{
                        left: ROW_HEADER_WIDTH + col * COL_WIDTH,
                        width: COL_WIDTH,
                        height: ROW_HEIGHT,
                      }}
                      title={
                        cell
                          ? `${address}${cell.formula ? `  ${cell.formula}` : ''}${
                              cell.display ? `\n值：${cell.display}` : ''
                            }`
                          : address
                      }
                      onClick={() => setActive({ sheetName: sheet.name, address })}
                    >
                      <span className="sheet-cell-text">
                        {cell?.formula ? `fx ${cell.display ?? ''}` : (cell?.display ?? '')}
                      </span>
                      {flagged && <span className="flag">⚑</span>}
                    </div>
                  );
                })}
              </div>
            ))}
          </div>
        </div>
      </div>

      <aside className="grid-detail">
        {active ? (
          <>
            <button
              type="button"
              className="btn btn-small"
              onClick={() => selectCell(active.sheetName, active.address)}
            >
              在公式檢視中開啟
            </button>
            <CellContextPanel sheetName={active.sheetName} address={active.address} />
          </>
        ) : (
          <p className="detail muted">點選任一儲存格，即可查看公式、快取值與問題判斷依據。</p>
        )}
      </aside>
    </div>
  );
}

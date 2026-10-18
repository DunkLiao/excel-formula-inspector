import { useEffect, useState } from 'react';

import * as ipc from '../ipc';
import { useScanStore } from '../state';
import { CONFIDENCE_LABELS, RULE_TITLES, VALUE_KIND_LABELS, type CellContext, type Issue } from '../types';

type Props = {
  sheetName: string;
  address: string;
  /** 顯示相鄰列數（SPEC §4.4：3–5 列）。 */
  span?: number;
};

/**
 * 儲存格明細：問題、判斷依據、公式、快取值、直接參照與相鄰公式模式比較（SPEC §4.3、§4.4）。
 */
export function CellContextPanel({ sheetName, address, span = 3 }: Props) {
  const scanId = useScanStore((s) => s.scanId);
  const selectCell = useScanStore((s) => s.selectCell);
  const [context, setContext] = useState<CellContext | null>(null);
  const [errorText, setErrorText] = useState<string | null>(null);
  const [loading, setLoading] = useState(false);

  useEffect(() => {
    if (!scanId) return;
    let cancelled = false;
    setLoading(true);
    setErrorText(null);
    ipc
      .getCellContext(scanId, sheetName, address, span)
      .then((value) => {
        if (!cancelled) setContext(value);
      })
      .catch((raw) => {
        if (!cancelled) {
          setContext(null);
          setErrorText(ipc.describeError(raw).message);
        }
      })
      .finally(() => {
        if (!cancelled) setLoading(false);
      });
    return () => {
      cancelled = true;
    };
  }, [scanId, sheetName, address, span]);

  if (loading && !context) {
    return <div className="detail muted">載入儲存格資訊…</div>;
  }
  if (errorText) {
    return <div className="detail callout callout-error">{errorText}</div>;
  }
  if (!context) return null;

  const dominant = context.dominantSignature?.replace(/（.*）$/, '') ?? null;

  return (
    <div className="detail">
      <div className="detail-head">
        <h3>
          {context.sheetName}!{context.cell.address}
        </h3>
        <span className={`pill pill-kind kind-${context.cell.kind}`}>
          {VALUE_KIND_LABELS[context.cell.kind]}
        </span>
        <button
          type="button"
          className="btn btn-small"
          onClick={() => selectCell(context.sheetName, context.cell.address)}
          title="在工作表網格中檢視"
        >
          在網格中檢視
        </button>
      </div>

      <dl className="detail-grid">
        <dt>公式</dt>
        <dd className="mono">{context.cell.formula ?? '（非常式）'}</dd>
        <dt>快取值</dt>
        <dd className="mono">{context.cell.display ?? '（無）'}</dd>
        <dt>直接參照</dt>
        <dd className="mono">
          {context.unsupportedReason
            ? '無法判讀'
            : context.refs.length > 0
              ? context.refs.join('、')
              : '（無）'}
        </dd>
        {context.unsupportedReason && (
          <>
            <dt>無法判讀原因</dt>
            <dd>{context.unsupportedReason}</dd>
          </>
        )}
        {context.dominantSignature && (
          <>
            <dt>同欄主流簽章</dt>
            <dd className="mono">{context.dominantSignature}</dd>
          </>
        )}
      </dl>

      {context.issues.length > 0 && (
        <div className="detail-block">
          <h4>本儲存格問題（{context.issues.length}）</h4>
          {context.issues.map((issue) => (
            <IssueBrief key={issue.id} issue={issue} />
          ))}
        </div>
      )}

      <div className="detail-block">
        <h4>相鄰公式模式比較（同欄 ±{span} 列）</h4>
        <div className="table-scroll">
          <table className="data-table compact">
            <thead>
              <tr>
                <th>儲存格</th>
                <th>內容</th>
                <th>與主流</th>
              </tr>
            </thead>
            <tbody>
              {context.neighbours.map((neighbour) => {
                const same =
                  dominant !== null &&
                  neighbour.signature !== undefined &&
                  neighbour.signature === dominant;
                const differs =
                  dominant !== null &&
                  neighbour.signature !== undefined &&
                  neighbour.signature !== dominant;
                return (
                  <tr
                    key={neighbour.address}
                    className={neighbour.issueIds.length > 0 ? 'row-flagged' : undefined}
                  >
                    <td className="mono">{neighbour.address}</td>
                    <td className="mono">{neighbour.formula ?? neighbour.display ?? '（空白）'}</td>
                    <td>
                      {neighbour.formula === undefined
                        ? '—'
                        : same
                          ? '相同'
                          : differs
                            ? '不同'
                            : '無法判讀'}
                    </td>
                  </tr>
                );
              })}
              {context.neighbours.length === 0 && (
                <tr>
                  <td colSpan={3} className="muted">
                    同一欄上下沒有其他儲存格。
                  </td>
                </tr>
              )}
            </tbody>
          </table>
        </div>
      </div>
    </div>
  );
}

function IssueBrief({ issue }: { issue: Issue }) {
  return (
    <article className={`issue-brief severity-border-${issue.severity}`}>
      <header>
        <strong>
          {issue.ruleId} · {RULE_TITLES[issue.ruleId]}
        </strong>
        <span className="muted">信心：{CONFIDENCE_LABELS[issue.confidence]}</span>
      </header>
      <p>{issue.message}</p>
      <ul className="evidence">
        {issue.evidence.map((line, index) => (
          <li key={index}>{line}</li>
        ))}
      </ul>
      {issue.formula && <p className="mono">{issue.formula}</p>}
    </article>
  );
}

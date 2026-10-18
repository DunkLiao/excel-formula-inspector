import { useMemo } from 'react';

import {
  RULE_ORDER,
  RULE_TITLES,
  SEVERITY_LABELS,
  SEVERITY_ORDER,
  formatBytes,
  formatDuration,
  type Severity,
} from '../types';
import { useScanStore } from '../state';

/** 摘要 Dashboard：KPI、規則／工作表分布與警告（SPEC §4.2）。 */
export function Dashboard() {
  const payload = useScanStore((s) => s.payload);
  const setTab = useScanStore((s) => s.setTab);
  const setFilters = useScanStore((s) => s.setFilters);

  const issues = payload?.issues ?? [];

  const byRule = useMemo(() => {
    const counts = new Map<string, number>();
    for (const issue of issues) {
      counts.set(issue.ruleId, (counts.get(issue.ruleId) ?? 0) + 1);
    }
    return counts;
  }, [issues]);

  if (!payload) return null;
  const { summary, sheets } = payload;
  const maxRule = Math.max(1, ...RULE_ORDER.map((rule) => byRule.get(rule) ?? 0));
  const totalIssues = SEVERITY_ORDER.reduce(
    (sum, severity) => sum + (summary.issuesBySeverity[severity] ?? 0),
    0,
  );

  return (
    <div className="dashboard">
      <div className="kpis">
        <Kpi label="工作表" value={String(summary.sheetCount)} />
        <Kpi label="已掃描儲存格" value={summary.scannedCellCount.toLocaleString('en-US')} />
        <Kpi label="公式儲存格" value={summary.formulaCount.toLocaleString('en-US')} />
        <Kpi label="常數儲存格" value={summary.constantCount.toLocaleString('en-US')} />
        <Kpi
          label="錯誤儲存格"
          value={sheets
            .reduce((sum, sheet) => sum + sheet.errorCount, 0)
            .toLocaleString('en-US')}
        />
        <Kpi label="問題總數" value={totalIssues.toLocaleString('en-US')} emphasis />
        <Kpi label="掃描耗時" value={formatDuration(summary.durationMs)} />
        <Kpi label="檔案大小" value={formatBytes(payload.fileSize)} />
      </div>

      <div className="panel">
        <h2>依嚴重度</h2>
        <div className="severity-row">
          {SEVERITY_ORDER.map((severity) => (
            <button
              key={severity}
              type="button"
              className={`severity-chip severity-${severity}`}
              onClick={() => {
                setFilters({ severities: [severity], rules: [], sheetName: null, keyword: '' });
                setTab('issues');
              }}
            >
              <span className="severity-name">{SEVERITY_LABELS[severity]}</span>
              <span className="severity-count">
                {(summary.issuesBySeverity[severity] ?? 0).toLocaleString('en-US')}
              </span>
            </button>
          ))}
        </div>
        {totalIssues === 0 && (
          <p className="muted">未發現任何問題。這只代表靜態檢查未命中規則，不代表公式的商業邏輯正確。</p>
        )}
      </div>

      <div className="panel">
        <h2>依規則分布</h2>
        <ul className="bar-list">
          {RULE_ORDER.map((rule) => {
            const count = byRule.get(rule) ?? 0;
            return (
              <li key={rule}>
                <button
                  type="button"
                  className="bar-row"
                  onClick={() => {
                    setFilters({ rules: [rule], severities: [], sheetName: null, keyword: '' });
                    setTab('issues');
                  }}
                >
                  <span className="bar-label">
                    {rule} · {RULE_TITLES[rule]}
                  </span>
                  <span className="bar-track">
                    <span
                      className="bar-fill"
                      style={{ width: `${Math.round((count / maxRule) * 100)}%` }}
                    />
                  </span>
                  <span className="bar-count">{count}</span>
                </button>
              </li>
            );
          })}
        </ul>
      </div>

      <div className="panel">
        <h2>工作表</h2>
        <div className="table-scroll">
          <table className="data-table">
            <thead>
              <tr>
                <th>工作表</th>
                <th>已使用範圍</th>
                <th className="num">公式</th>
                <th className="num">常數</th>
                <th className="num">錯誤值</th>
                <th className="num">問題</th>
              </tr>
            </thead>
            <tbody>
              {sheets.map((sheet) => (
                <tr key={sheet.name}>
                  <td>
                    <button
                      type="button"
                      className="link"
                      onClick={() => {
                        setFilters({
                          sheetName: sheet.name,
                          severities: [],
                          rules: [],
                          keyword: '',
                        });
                        setTab('issues');
                      }}
                    >
                      {sheet.name}
                    </button>
                  </td>
                  <td className="mono">{sheet.usedRange ?? '（空白）'}</td>
                  <td className="num">{sheet.formulaCount}</td>
                  <td className="num">{sheet.constantCount}</td>
                  <td className="num">{sheet.errorCount}</td>
                  <td className="num">{sheet.issueCount}</td>
                </tr>
              ))}
            </tbody>
          </table>
        </div>
      </div>

      <div className="panel">
        <h2>掃描資訊與限制</h2>
        <ul className="notes">
          <li>
            本工具為靜態檢查器：<strong>不會重新計算公式</strong>，顯示的「快取值」是檔案中既有的
            Excel 計算結果，可能已過期。
          </li>
          <li>啟發式規則使用「疑似」措辭並附信心程度；被覆寫的常數與刻意輸入值可能無法區分。</li>
          {payload.issuesTruncated && (
            <li className="warn">
              問題數量超過單次傳輸上限，清單僅顯示部分內容；匯出「全部問題」時仍會輸出完整結果。
            </li>
          )}
          {summary.warnings.map((warning, index) => (
            <li key={index} className="warn">
              {warning}
            </li>
          ))}
        </ul>
      </div>
    </div>
  );
}

function Kpi({
  label,
  value,
  emphasis,
}: {
  label: string;
  value: string;
  emphasis?: boolean;
}) {
  return (
    <div className={emphasis ? 'kpi kpi-emphasis' : 'kpi'}>
      <span className="kpi-label">{label}</span>
      <span className="kpi-value">{value}</span>
    </div>
  );
}

/** 供其他檢視重用的嚴重度標籤。 */
export function severityClass(severity: Severity): string {
  return `severity-${severity}`;
}

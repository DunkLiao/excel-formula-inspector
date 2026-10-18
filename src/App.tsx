import { useEffect } from 'react';

import { useScanStore, type TabKey } from './state';
import { Header } from './ui/header';
import { UploadPanel } from './ui/upload';
import { Dashboard } from './ui/dashboard';
import { IssuesTable } from './ui/issues-table';
import { FormulaExplorer } from './ui/formula-explorer';
import { SheetGrid } from './ui/sheet-grid';

const TABS: Array<{ key: TabKey; label: string }> = [
  { key: 'summary', label: '摘要' },
  { key: 'issues', label: '問題清單' },
  { key: 'formulas', label: '公式檢視' },
  { key: 'grid', label: '工作表網格' },
];

export default function App() {
  const init = useScanStore((s) => s.init);
  const status = useScanStore((s) => s.status);
  const payload = useScanStore((s) => s.payload);
  const tab = useScanStore((s) => s.tab);
  const setTab = useScanStore((s) => s.setTab);
  const theme = useScanStore((s) => s.theme);

  useEffect(() => {
    void init();
  }, [init]);

  // 佈景主題只作用在文件根節點；預設淺色（SPEC §4.1）
  useEffect(() => {
    document.documentElement.dataset.theme = theme;
  }, [theme]);

  const hasResult = status === 'done' && payload !== null;

  return (
    <div className="app">
      <Header />
      <main className="app-body">
        {!hasResult ? (
          <UploadPanel />
        ) : (
          <>
            <nav className="tabs" aria-label="檢視切換">
              {TABS.map((item) => (
                <button
                  key={item.key}
                  type="button"
                  className={tab === item.key ? 'tab tab-active' : 'tab'}
                  aria-current={tab === item.key ? 'page' : undefined}
                  onClick={() => setTab(item.key)}
                >
                  {item.label}
                  {item.key === 'issues' && payload.summary.issuesBySeverity
                    ? ` (${totalIssues(payload.summary.issuesBySeverity)})`
                    : ''}
                </button>
              ))}
            </nav>
            <div className="view">
              {tab === 'summary' && <Dashboard />}
              {tab === 'issues' && <IssuesTable />}
              {tab === 'formulas' && <FormulaExplorer />}
              {tab === 'grid' && <SheetGrid />}
            </div>
          </>
        )}
      </main>
    </div>
  );
}

function totalIssues(counts: Record<string, number>): number {
  return Object.values(counts).reduce((sum, value) => sum + value, 0);
}

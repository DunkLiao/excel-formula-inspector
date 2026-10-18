import { describe, expect, it } from 'vitest';

import { EMPTY_FILTERS, filterIssues, type Filters } from '../src/state';
import { columnLetters, parseAddress, type Issue } from '../src/types';

function issue(partial: Partial<Issue> & { id: string }): Issue {
  return {
    ruleId: 'F001',
    severity: 'high',
    confidence: 'high',
    sheetName: 'Sheet1',
    cellAddress: 'A1',
    message: '訊息',
    evidence: ['證據'],
    ...partial,
  };
}

const ISSUES: Issue[] = [
  issue({ id: '1', ruleId: 'F001', severity: 'high', sheetName: 'Sheet1', cellAddress: 'A1', message: '錯誤值 #DIV/0!' }),
  issue({ id: '2', ruleId: 'F003', severity: 'medium', sheetName: 'Sheet1', cellAddress: 'C6', message: '疑似公式模式不一致', formula: '=B11+1' }),
  issue({ id: '3', ruleId: 'F006', severity: 'info', sheetName: 'Inputs', cellAddress: 'D2', message: '公式包含高風險函數：INDIRECT' }),
];

describe('filterIssues', () => {
  it('未設定篩選時回傳全部', () => {
    expect(filterIssues(ISSUES, EMPTY_FILTERS)).toHaveLength(3);
  });

  it('可依嚴重度篩選', () => {
    const filters: Filters = { ...EMPTY_FILTERS, severities: ['high', 'info'] };
    expect(filterIssues(ISSUES, filters).map((i) => i.id)).toEqual(['1', '3']);
  });

  it('可依規則篩選', () => {
    const filters: Filters = { ...EMPTY_FILTERS, rules: ['F003'] };
    expect(filterIssues(ISSUES, filters).map((i) => i.id)).toEqual(['2']);
  });

  it('可依工作表篩選', () => {
    const filters: Filters = { ...EMPTY_FILTERS, sheetName: 'Inputs' };
    expect(filterIssues(ISSUES, filters).map((i) => i.id)).toEqual(['3']);
  });

  it('關鍵字可比對公式與說明，且不分大小寫', () => {
    expect(filterIssues(ISSUES, { ...EMPTY_FILTERS, keyword: 'indirect' }).map((i) => i.id)).toEqual(['3']);
    expect(filterIssues(ISSUES, { ...EMPTY_FILTERS, keyword: 'b11' }).map((i) => i.id)).toEqual(['2']);
    expect(filterIssues(ISSUES, { ...EMPTY_FILTERS, keyword: 'C6' }).map((i) => i.id)).toEqual(['2']);
  });

  it('條件為 AND 關係', () => {
    const filters: Filters = {
      ...EMPTY_FILTERS,
      severities: ['medium'],
      rules: ['F003'],
      sheetName: 'Sheet1',
      keyword: '模式',
    };
    expect(filterIssues(ISSUES, filters).map((i) => i.id)).toEqual(['2']);
    expect(filterIssues(ISSUES, { ...filters, sheetName: 'Inputs' })).toHaveLength(0);
  });
});

describe('位址工具', () => {
  it('parseAddress 支援絕對符號並回傳 0-based 座標', () => {
    expect(parseAddress('A1')).toEqual({ row: 0, col: 0 });
    expect(parseAddress('D12')).toEqual({ row: 11, col: 3 });
    expect(parseAddress('$D$12')).toEqual({ row: 11, col: 3 });
    expect(parseAddress('XFD1048576')).toEqual({ row: 1048575, col: 16383 });
  });

  it('parseAddress 拒絕無效位址', () => {
    expect(parseAddress('A0')).toBeNull();
    expect(parseAddress('12')).toBeNull();
    expect(parseAddress('')).toBeNull();
    expect(parseAddress('A1:B2')).toBeNull();
  });

  it('columnLetters 與 Excel 欄名一致', () => {
    expect(columnLetters(0)).toBe('A');
    expect(columnLetters(25)).toBe('Z');
    expect(columnLetters(26)).toBe('AA');
    expect(columnLetters(16383)).toBe('XFD');
  });

  it('位址與欄名可互相還原', () => {
    for (const col of [0, 1, 25, 26, 27, 51, 52, 701, 16383]) {
      expect(parseAddress(`${columnLetters(col)}1`)?.col).toBe(col);
    }
  });
});

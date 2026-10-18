import '@testing-library/jest-dom/vitest';

// jsdom 沒有 ResizeObserver；網格與虛擬列表在測試環境需要最簡單的替代實作。
class ResizeObserverStub {
  observe() {}
  unobserve() {}
  disconnect() {}
}

if (!('ResizeObserver' in globalThis)) {
  (globalThis as unknown as { ResizeObserver: unknown }).ResizeObserver = ResizeObserverStub;
}

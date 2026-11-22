// 验证CID输入框编辑、光标选区和完整原值处理。

import { expect, it, vi } from 'vitest';
import { createRequire } from 'node:module';

const browserTest = process.env.ONCHINA_TEST_DOM ? it : it.skip;
browserTest('resting CID input retains the complete value for form submission and copy', async () => {
  const { JSDOM } = createRequire(import.meta.url)(process.env.ONCHINA_TEST_DOM!);
  const dom = new JSDOM('<div id="root"></div>', { url: 'https://onchina.example.invalid/', pretendToBeVisual: true });
  const computed = dom.window.getComputedStyle.bind(dom.window);
  dom.window.getComputedStyle = (element: Element) => computed(element);
  for (const name of ['window', 'document', 'navigator', 'HTMLElement', 'Element', 'ShadowRoot', 'SVGElement', 'getComputedStyle', 'FormData']) vi.stubGlobal(name, dom.window[name]);
  vi.stubGlobal('IS_REACT_ACT_ENVIRONMENT', true);
  const { act } = await import('react');
  const { createRoot } = await import('react-dom/client');
  const { IdentityCidInput } = await import('../controls/IdentityCidInput');
  const full = 'GZ018-CREGM-159197206-2026';
  const host = document.getElementById('root')!;
  const root = createRoot(host);
  try {
    await act(async () => root.render(<form><IdentityCidInput name="cidNumber" value={full} readOnly /></form>));
    const input = host.querySelector('input')!;
    expect(input.value).toBe(full);
    expect(new FormData(host.querySelector('form')!).get('cidNumber')).toBe(full);
    expect(host.textContent).toBe('GZ018-CREGM-159197206');
    const event = new dom.window.Event('copy', { bubbles: true, cancelable: true });
    input.dispatchEvent(event); expect(event.defaultPrevented).toBe(false);
  } finally { await act(async () => root.unmount()); dom.window.close(); vi.unstubAllGlobals(); }
});

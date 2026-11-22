// 验证账户余额零值和大额精度，以及余额列与地址和链状态的排列。

import { expect, it, vi } from 'vitest';
import { createRequire } from 'node:module';
import { renderToStaticMarkup } from 'react-dom/server';
import { AccountList } from '../accounts/AccountList';
import type { InstitutionAccount } from '../institutions/api';

const account = (name: string, balanceFen?: string | null): InstitutionAccount => ({
  cidNumber: 'GZ018-CREGM-159197206-2026', accountName: name,
  accountId: '0x' + '11'.repeat(32), accountKind: 'main', canClose: false,
  canDelete: false, chainStatus: 'ACTIVE_ON_CHAIN', chainTxHash: null,
  chainBlockNumber: null, createdAt: null, balanceFen,
});

it('shared account list preserves zero, decimal precision and unavailable balances', () => {
  const html = renderToStaticMarkup(<AccountList accounts={[
    account('零余额', '0'), account('普通余额', '123456789'),
    account('大额余额', '340282366920938463463374607431768211455'),
    account('余额未读取', null), account('旧响应兼容'),
  ]} loading={false} canDelete={false} />);
  expect(html).toContain('0.00 元');
  expect(html).toContain('1,234,567.89 元');
  expect(html).toContain('3,402,823,669,209,384,634,633,746,074,317,682,114.55 元');
  expect(html).toContain('已上链');
});

const browserTest = process.env.ONCHINA_TEST_DOM ? it : it.skip;
browserTest('balance column is between the complete account address and chain status', async () => {
  const { JSDOM } = createRequire(import.meta.url)(process.env.ONCHINA_TEST_DOM!);
  const dom = new JSDOM('<div id="root"></div>', { url: 'https://onchina.example.invalid/', pretendToBeVisual: true });
  const computed = dom.window.getComputedStyle.bind(dom.window);
  dom.window.getComputedStyle = (element: Element) => computed(element);
  dom.window.matchMedia = () => ({ matches: false, addListener() {}, removeListener() {}, addEventListener() {}, removeEventListener() {} });
  for (const name of ['window', 'document', 'navigator', 'HTMLElement', 'Element', 'ShadowRoot', 'SVGElement', 'getComputedStyle']) vi.stubGlobal(name, dom.window[name]);
  vi.stubGlobal('IS_REACT_ACT_ENVIRONMENT', true);
  const { act } = await import('react');
  const { createRoot } = await import('react-dom/client');
  const root = createRoot(document.getElementById('root')!);
  const rows = [account('主账户', '0'), account('费用账户', null)];
  try {
    await act(async () => root.render(<AccountList accounts={rows} loading={false} canDelete={false} />));
    expect([...document.querySelectorAll('thead th')].map(cell => cell.textContent)).toEqual([
      '序号', '账户名称', '账户地址', '账户余额', '链上状态', '创建时间',
    ]);
    const actual = [...document.querySelectorAll('tbody tr')].filter(row => !row.classList.contains('ant-table-measure-row'));
    const first = actual[0].querySelectorAll('td');
    expect(first[2].textContent!.length).toBeGreaterThan(40);
    expect(first[2].textContent).not.toContain('…');
    expect(first[3].textContent).toBe('0.00 元');
    expect(first[4].textContent).toBe('已上链');
    expect(actual[1].querySelectorAll('td')[3].textContent).toBe('-');
    expect(rows[0].balanceFen).toBe('0');
    expect(rows[0].accountId).toBe('0x' + '11'.repeat(32));
  } finally { await act(async () => root.unmount()); dom.window.close(); vi.unstubAllGlobals(); }
});

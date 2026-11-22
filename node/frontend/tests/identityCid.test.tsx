// 验证CID年份隐藏、选区复制及原始值隔离。

import { afterEach, describe, expect, it } from 'vitest';
import { act } from 'react';
import { createRoot, type Root } from 'react-dom/client';
import { formatIdentityCidText, IdentityCidText, identityCidUiText } from '../client/identityCid';

Object.assign(globalThis, { IS_REACT_ACT_ENVIRONMENT: true });

const full = 'GZ018-CREGM-159197206-2026';
const shown = 'GZ018-CREGM-159197206';
let root: Root | undefined;
afterEach(() => { if (root) act(() => root!.unmount()); root = undefined; document.body.innerHTML = ''; window.getSelection()?.removeAllRanges(); });

function mount(value: string) {
  const container = document.createElement('div'); document.body.append(container);
  root = createRoot(container); act(() => root!.render(<IdentityCidText value={value} />));
  return container;
}
function copy(range: Range) {
  const selection = window.getSelection()!; selection.removeAllRanges(); selection.addRange(range);
  const result = new Map<string, string>();
  const event = new Event('copy', { bubbles: true, cancelable: true });
  Object.defineProperty(event, 'clipboardData', { value: { setData: (kind: string, text: string) => result.set(kind, text) } });
  document.dispatchEvent(event); return result;
}

describe('CID display keeps complete identifiers outside rendered text', () => {
  it('hides only the year segment and leaves full/short values, account addresses and dates intact', () => {
    expect(formatIdentityCidText(full)).toBe(shown);
    expect(full).toBe('GZ018-CREGM-159197206-2026');
    expect(formatIdentityCidText(shown)).toBe(shown);
    expect(formatIdentityCidText('CN001-CTZN4-123456789-10000')).toBe('CN001-CTZN4-123456789');
    for (const other of ['2026-11-22', 'personal-account:0x1234', '0x' + 'ab'.repeat(32), 'GZ018-CREGM-159197206-20261234567']) expect(formatIdentityCidText(other)).toBe(other);
    expect(identityCidUiText('公民CID号 / 居民CID号 / 机构CID号 / 身份ID / 公民号')).toBe('身份CID号 / 身份CID号 / 身份CID号 / 身份CID号 / 身份CID号');
  });
  it('copies the complete CID even when only its text node is selected', () => {
    const el = mount(full); expect(el.textContent).toBe(shown);
    const node = el.querySelector('span')!.firstChild!;
    const range = document.createRange(); range.setStart(node, 0); range.setEnd(node, shown.length);
    expect(copy(range).get('text/plain')).toBe(full);
    expect(el.textContent).toBe(shown);
  });
  it('preserves mixed selection and distinct full values with the same displayed prefix', () => {
    const later = full.replace('2026', '2036');
    const el = mount(`目标：${full}；原号：${later}`);
    const range = document.createRange(); range.selectNodeContents(el);
    expect(copy(range).get('text/plain')).toBe(`目标：${full}；原号：${later}`);
    expect(el.textContent).toBe(`目标：${shown}；原号：${shown}`);
  });
  it('leaves a partial CID selection and explicit input copying alone', () => {
    const el = mount(full); const node = el.querySelector('span')!.firstChild!;
    const range = document.createRange(); range.setStart(node, 0); range.setEnd(node, 5);
    expect(copy(range).size).toBe(0);
    const input = document.createElement('input'); input.value = full; document.body.append(input);
    const event = new Event('copy', { bubbles: true, cancelable: true });
    input.dispatchEvent(event); expect(event.defaultPrevented).toBe(false); expect(input.value).toBe(full);
  });
});

import { afterEach, beforeEach, expect, it, vi } from 'vitest';
import { fireEvent, render, screen, within } from '@testing-library/react';
import { LocalDocViewer } from '../control/LocalDocViewer';
import { readFileSync } from 'node:fs';
import { resolve } from 'node:path';

beforeEach(() => {
  vi.stubGlobal('IntersectionObserver', class {
    observe() {}
    disconnect() {}
  });
});
afterEach(() => vi.unstubAllGlobals());

const markdown = `# **《公民链白皮书》**<br><span class="whitepaper-title-en">CitizenChain Whitepaper</span>

# 目录<br><span class="whitepaper-heading-en">Table of Contents</span>

- [1.总则](#1总则)

---

# 1.总则<br><span class="whitepaper-heading-en">1. General Principles</span>

## 1.1.目的<br><span class="whitepaper-heading-en">1.1. Purpose</span>

正文保持完整。

# 2.白皮书发布规则

这里是实际章节，不能因标题包含白皮书而删除。
`;

it('迁移后的真实白皮书保留配图、目录展开和5.2协议基础库跳转', () => {
  const root = resolve(process.cwd(), '../..');
  let source = readFileSync(resolve(root, 'CitizenChainWhitepaper.md'), 'utf8');
  const originals: string[] = [];
  for (const name of ['whitepaper-node-diagram.svg', 'whitepaper-reserve-architecture.svg']) {
    const svg = readFileSync(resolve(root, 'icons', name), 'utf8');
    originals.push(svg);
    // 与后端返回的百分号编码 data URL 一致，确认既有净化器保留两张原图。
    const encoded = [...Buffer.from(svg)].map(byte => '%' + byte.toString(16).padStart(2, '0').toUpperCase()).join('');
    source = source.replace('./icons/' + name, 'data:image/svg+xml,' + encoded);
  }
  const descriptor = Object.getOwnPropertyDescriptor(Element.prototype, 'scrollIntoView');
  const scroll = vi.fn();
  Object.defineProperty(Element.prototype, 'scrollIntoView', { configurable: true, value: scroll });
  const view = render(<LocalDocViewer doc={{key:'whitepaper',markdown:source}}/>);
  try {
    const article = view.container.querySelector('article')!;
    expect([...article.querySelectorAll('img')].map(img => decodeURIComponent(img.src.split(',')[1]))).toEqual(originals);
    const toc = within(view.container.querySelector('aside')!);
    fireEvent.click(toc.getByRole('button', { name: /5\.公民协议/ }));
    fireEvent.click(toc.getByRole('link', { name: /5\.2\.协议基础库/ }));
    expect(scroll).toHaveBeenCalled();
    expect(article.querySelector('[id="52协议基础库"]')?.textContent).toContain('5.2. primitives');
  } finally {
    if (descriptor) Object.defineProperty(Element.prototype, 'scrollIntoView', descriptor);
    else delete (Element.prototype as unknown as {scrollIntoView?: unknown}).scrollIntoView;
  }
});

it('白皮书只在顶部显示封面，正文从总则开始并保留实际章节', () => {
  const doc = { key: 'whitepaper' as const, markdown };
  const view = render(<LocalDocViewer doc={doc}/>);
  const article = view.container.querySelector('article')!;
  expect(view.container.querySelector('.hero h1')?.textContent).toBe('《公民链白皮书》');
  expect(article.firstElementChild?.tagName).toBe('H1');
  expect(article.firstElementChild?.textContent).toContain('1.总则');
  expect(article.textContent).not.toContain('CitizenChain Whitepaper');
  expect(article.querySelector('hr')).toBeNull();
  expect(article.querySelector('.whitepaper-title-en')).toBeNull();
  expect(within(article).getByText('正文保持完整。')).toBeTruthy();
  expect(within(article).getByRole('heading', { name: '2.白皮书发布规则' })).toBeTruthy();
  expect(doc.markdown).toBe(markdown);
});

it('目录展开和跳转后不会重新插入重复封面，滚动目标仍为正文章节', () => {
  const descriptor = Object.getOwnPropertyDescriptor(Element.prototype, 'scrollIntoView');
  const scroll = vi.fn();
  Object.defineProperty(Element.prototype, 'scrollIntoView', { configurable: true, value: scroll });
  const view = render(<LocalDocViewer doc={{key:'whitepaper',markdown}}/>);
  try {
    const toc = within(view.container.querySelector('aside')!);
    fireEvent.click(toc.getByRole('button', { name: /1\.总则/ }));
    fireEvent.click(toc.getByRole('link', { name: /1\.1\.目的/ }));
    expect(scroll).toHaveBeenCalledExactlyOnceWith({behavior:'smooth',block:'start'});
    expect((scroll.mock.contexts[0] as Element).textContent).toContain('1.1.目的');
    expect(view.container.querySelector('article')?.firstElementChild?.textContent).toContain('1.总则');
    expect(view.container.querySelector('article')?.textContent).not.toContain('CitizenChain Whitepaper');
    expect(screen.getByRole('heading', {name:'《公民链白皮书》'})).toBeTruthy();
  } finally {
    view.unmount();
    if (descriptor) Object.defineProperty(Element.prototype, 'scrollIntoView', descriptor);
    else Reflect.deleteProperty(Element.prototype, 'scrollIntoView');
  }
});

// 缩写显示CID并在复制时恢复完整原值，保留业务和签名标识。

import { Fragment, useEffect } from 'react';

export const IDENTITY_CID_LABEL = '身份CID号';
const cidPattern = () => /(?<![A-Za-z0-9-])([A-Z0-9]{5}-[A-Z0-9]{5}-[0-9]{9})-[0-9]{4,10}(?![A-Za-z0-9-])/g;

/** 只省略已提供号码的年份段，不生成、补全或校验 CID。 */
export function formatIdentityCidText(value: string): string {
  return value.replace(cidPattern(), '$1');
}

/** 仅供系统提示文案；不得用于昵称、聊天正文或其他用户内容。 */
export function identityCidUiText(value: string): string {
  return formatIdentityCidText(value).replace(
    /机构身份CID号|身份\s*ID|(?:公民|居民|机构|身份)\s*CID(?:\s*(?:编号|号码|号))?|公民号|(?:身份\s*)?CID\s*(?:编号|号码|号)|\bCID(?![A-Za-z0-9_])/g,
    IDENTITY_CID_LABEL,
  );
}

// 浏览器选择复制仍使用完整原号。只还原选择范围中完整选中的 CID，
// 不改写用户选择的其他文字，也不拦截输入框或已有的显式复制处理。
let readers = 0;
function copyFullCid(event: ClipboardEvent) {
  if (event.defaultPrevented || !event.clipboardData) return;
  const target = event.target;
  if (target instanceof Element && target.closest('input,textarea,[contenteditable="true"]')) return;
  const selection = document.getSelection();
  if (!selection || selection.isCollapsed || selection.rangeCount !== 1) return;
  const range = selection.getRangeAt(0);
  const fragment = range.cloneContents();
  const tokens = [...document.querySelectorAll<HTMLElement>('[data-identity-cid]')]
    .filter(node => {
      const text = node.firstChild;
      return text?.nodeType === Node.TEXT_NODE && range.intersectsNode(node)
        && range.comparePoint(text, 0) === 0
        && range.comparePoint(text, text.textContent!.length) === 0;
    });
  if (!tokens.length) return;
  let text = selection.toString();
  let offset = 0;
  for (const token of tokens) {
    const visible = token.textContent!;
    const index = text.indexOf(visible, offset);
    if (index < 0) continue;
    const full = token.dataset.identityCid!;
    text = text.slice(0, index) + full + text.slice(index + visible.length);
    offset = index + full.length;
  }
  for (const node of fragment.querySelectorAll<HTMLElement>('[data-identity-cid]')) {
    if (node.textContent === formatIdentityCidText(node.dataset.identityCid!)) node.textContent = node.dataset.identityCid!;
    node.removeAttribute('data-identity-cid');
  }
  event.clipboardData.setData('text/plain', text);
  const html = document.createElement('div');
  html.append(fragment);
  event.clipboardData.setData('text/html', html.innerHTML);
  event.preventDefault();
}

/** 展示文本与完整原值分离；悬停、读屏显示短号，选择复制保留原号。 */
export function IdentityCidText({ value, systemText = false }: { value: string | number | null | undefined; systemText?: boolean }) {
  useEffect(() => {
    if (readers++ === 0) document.addEventListener('copy', copyFullCid);
    return () => { if (--readers === 0) document.removeEventListener('copy', copyFullCid); };
  }, []);
  const original = value == null ? '' : String(value);
  const raw = systemText ? original.replace(
    /机构身份CID号|身份\s*ID|(?:公民|居民|机构|身份)\s*CID(?:\s*(?:编号|号码|号))?|公民号|(?:身份\s*)?CID\s*(?:编号|号码|号)|\bCID(?![A-Za-z0-9_])/g,
    IDENTITY_CID_LABEL,
  ) : original;
  const parts = [];
  let start = 0;
  for (const match of raw.matchAll(cidPattern())) {
    const index = match.index!;
    parts.push(<Fragment key={`text-${index}`}>{raw.slice(start, index)}</Fragment>);
    parts.push(<span key={`cid-${index}`} data-identity-cid={match[0]}>{match[1]}</span>);
    start = index + match[0].length;
  }
  parts.push(<Fragment key="tail">{raw.slice(start)}</Fragment>);
  return <>{parts}</>;
}

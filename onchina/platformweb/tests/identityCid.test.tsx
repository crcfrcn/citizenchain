// 验证CID缩写呈现及复制时恢复完整原值。

import { describe, expect, it } from 'vitest';
import { renderToStaticMarkup } from 'react-dom/server';
import { formatIdentityCidText, IdentityCidText, identityCidUiText } from '../codes/identityCid';

describe('identity CID UI boundary', () => {
  it('renders short identifiers while retaining complete raw values for copy', () => {
    const full = 'GZ018-CREGM-159197206-2026';
    const short = 'GZ018-CREGM-159197206';
    const html = renderToStaticMarkup(<IdentityCidText value={`身份CID号：${full}`} />);
    expect(html).toContain(`data-identity-cid="${full}"`);
    expect(html).toContain(`>${short}</span>`);
    expect(formatIdentityCidText(full)).toBe(short);
    expect(full).toBe('GZ018-CREGM-159197206-2026');
    expect(formatIdentityCidText('2026-11-22')).toBe('2026-11-22');
    expect(identityCidUiText('公民号 / 身份ID / 机构CID / 居民CID号')).toBe('身份CID号 / 身份CID号 / 身份CID号 / 身份CID号');
  });
});

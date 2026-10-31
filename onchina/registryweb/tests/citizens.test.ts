// 所属业务前端回归，公共测试辅助只有一份。
import { describe, expect, it, vi } from 'vitest';
import { readFileSync } from 'node:fs';
import { dirname, join, normalize } from 'node:path';
import { SECURITY_GRANT_HEADER, PASSKEY_ASSERTION_HEADER, passkeySubmitHeaders, securityGrantSubmitHeaders, commitAdminAction, rustConst } from '@gmb/onchina-platform/testing';

describe('实际业务认证', () => {
const auth = { accessToken: 'test-session', accountId: '0x' + '11'.repeat(32) } as never;
it('公民编辑和资料上传删除均携带一次性断言', async () => {
    const api = await import('../citizens/api');
    const { assertPasskey } = await import('@gmb/onchina-platform/authentication/passkeyClient');
    const fetcher = vi.spyOn(globalThis, 'fetch').mockResolvedValue(new Response(JSON.stringify({ code: 0, data: {} })));
    try {
      for (const operation of [
        () => api.editCitizen(auth, 'citizen', {} as never),
        () => api.uploadCitizenDocument(auth, 'citizen', new File(['file'], 'sample.bin'), '其他材料'),
        () => api.deleteCitizenDocument(auth, 'citizen', 1),
      ]) {
        vi.mocked(assertPasskey).mockClear();
        fetcher.mockClear();
        fetcher.mockResolvedValue(new Response(JSON.stringify({ code: 0, data: {} })));
        await operation();
        expect(assertPasskey).toHaveBeenCalledTimes(1);
        const headers = new Headers(fetcher.mock.calls[0][1]?.headers);
        expect(headers.get('authorization')).toBe('Bearer test-session');
        expect(headers.get(PASSKEY_ASSERTION_HEADER)).toBe('fake-passkey-assertion');
      }
    } finally { fetcher.mockRestore(); }
  });
});
describe('实际业务认证', () => {
const auth = { accessToken: 'test-session', accountId: '0x' + '11'.repeat(32) } as never;
it('公民编辑保留原URL、JSON字段及一次认证，取消时不发送写请求', async () => {
    const { editCitizen } = await import('../citizens/api');
    const { assertPasskey } = await import('@gmb/onchina-platform/authentication/passkeyClient');
    const fetcher = vi.spyOn(globalThis, 'fetch');
    const payload = { familyName: '测', givenName: '试', cityCode: '001' } as never;
    try {
      vi.mocked(assertPasskey).mockClear();
      fetcher.mockResolvedValue(new Response(JSON.stringify({ code: 0, data: {} })));
      await editCitizen(auth, 'citizen', payload);
      expect(assertPasskey).toHaveBeenCalledTimes(1);
      expect(fetcher).toHaveBeenCalledTimes(1);
      expect(fetcher.mock.calls[0][0]).toBe('/api/admin/citizens/citizen/edit');
      expect(fetcher.mock.calls[0][1]?.method).toBe('POST');
      expect(JSON.parse(String(fetcher.mock.calls[0][1]?.body))).toEqual(payload);
      expect(new Headers(fetcher.mock.calls[0][1]?.headers).get('content-type')).toBe('application/json');
      fetcher.mockClear();
      vi.mocked(assertPasskey).mockRejectedValueOnce(new Error('cancelled'));
      await expect(editCitizen(auth, 'citizen', payload)).rejects.toThrow('cancelled');
      expect(fetcher).not.toHaveBeenCalled();
    } finally { fetcher.mockRestore(); }
  });
});

// 验证私权工作台和会员价格清单及调价准备的Passkey边界。

import { describe, expect, it, vi } from "vitest";
import { readFileSync } from "node:fs";
import { dirname, join, normalize } from "node:path";
import { SECURITY_GRANT_HEADER, PASSKEY_ASSERTION_HEADER, passkeySubmitHeaders, securityGrantSubmitHeaders, commitAdminAction, rustConst } from "@gmb/onchina-platform/testing";
import { renderToStaticMarkup } from "react-dom/server";
import { Fragment, createElement } from "react";
import { PrivateInstitutionWorkspace } from "./Workspace";

// 所属业务前端回归，公共测试辅助只有一份。





describe('会员调价真实请求', () => {
const auth = { accessToken: 'test-session', accountId: '0x' + '11'.repeat(32) } as never;
it('会员调价准备等待一次Passkey，保留请求且失败不重试', async () => {
    const { assertPasskey } = await import('@gmb/onchina-platform/authentication/passkeyClient');
    const { prepareInstitutionGovernance } = await import('@gmb/onchina-platform/institutions/governance');
    const { proposePlatformPrice } = await import("./Workspace");
    const governance = { cidNumber: 'fixture-institution', proposerRoleCode: 'REPRESENTATIVE' };
    const operations = [{ path: '/api/membership/platform-prices/propose', payload: { proposerRoleCode: 'COMMITTEE_MEMBER', membershipLevel: 'freedom', newPriceFen: '100' }, run: () => proposePlatformPrice(auth, 'COMMITTEE_MEMBER', 'freedom', '100') }];
    const assertion = vi.mocked(assertPasskey);
    const fetcher = vi.spyOn(globalThis, 'fetch');
    const prepared = { requestId: 'fixture-request', signRequest: 'fixture-sign-request' };
    try {
      for (const operation of operations) {
        assertion.mockReset(); fetcher.mockReset();
        let complete!: (value: string) => void;
        assertion.mockReturnValueOnce(new Promise<string>(resolve => { complete = resolve; }));
        fetcher.mockResolvedValueOnce(new Response(JSON.stringify({ code: 0, data: prepared })));
        const pending = operation.run();
        expect(assertion).toHaveBeenCalledTimes(1);
        expect(assertion).toHaveBeenCalledWith(auth);
        expect(fetcher).not.toHaveBeenCalled();
        complete('fake-passkey-assertion');
        await expect(pending).resolves.toEqual(prepared);
        expect(fetcher).toHaveBeenCalledTimes(1);
        const [path, init] = fetcher.mock.calls[0];
        const headers = new Headers(init?.headers);
        expect(path).toBe(operation.path);
        expect(init?.method).toBe('POST');
        expect(headers.get('authorization')).toBe('Bearer test-session');
        expect(headers.get('content-type')).toBe('application/json');
        expect(headers.get(PASSKEY_ASSERTION_HEADER)).toBe('fake-passkey-assertion');
        expect(headers.has(SECURITY_GRANT_HEADER)).toBe(false);
        expect(JSON.parse(String(init?.body))).toEqual(operation.payload);

        for (const failure of [new Error('cancelled'), new Error('verification failed')]) {
          assertion.mockReset(); fetcher.mockClear();
          assertion.mockRejectedValueOnce(failure);
          await expect(operation.run()).rejects.toBe(failure);
          expect(assertion).toHaveBeenCalledTimes(1);
          expect(fetcher).not.toHaveBeenCalled();
        }
        assertion.mockReset(); fetcher.mockClear();
        assertion.mockResolvedValueOnce('fake-passkey-assertion');
        fetcher.mockResolvedValueOnce(new Response(JSON.stringify({ code: 2003, message: 'denied' }), { status: 403 }));
        await expect(operation.run()).rejects.toThrow('denied');
        expect(assertion).toHaveBeenCalledTimes(1);
        expect(fetcher).toHaveBeenCalledTimes(1);
      }
    } finally {
      assertion.mockReset(); assertion.mockResolvedValue('fake-passkey-assertion');
      fetcher.mockRestore();
    }
  });
});

// 私权工作台范围与准确实例业务清单。



vi.mock('@gmb/onchina-platform/workspace/WorkspaceShell', () => ({ WorkspaceShell: ({operations, display, records}: Record<string, never>) => createElement(Fragment, null, operations, display, records) }));
vi.mock('@gmb/onchina-platform/accounts/AccountManageSection', () => ({ AccountManageSection: () => null }));
vi.mock('@gmb/onchina-platform/institutions/InfoPanel', () => ({ OwnInstitutionInfoPanel: () => createElement('span', null, '本机构信息') }));

const auth = (kind: string, modules: string[] = []) => ({ workspace: { workspace_kind: kind, workspace_modules: modules } }) as never;
describe('私权本机构工作台', () => {
  it('缺少工作台和其他机构工作台均不渲染', () => {
    expect(renderToStaticMarkup(<PrivateInstitutionWorkspace auth={{} as never} />)).toBe('');
    for (const kind of ['registry', 'judicial', 'legislation']) expect(renderToStaticMarkup(<PrivateInstitutionWorkspace auth={auth(kind)} />)).toBe('');
  });
  it('会员价格只按后端准确实例清单出现，本机构资料保留', () => {
    const normal = renderToStaticMarkup(<PrivateInstitutionWorkspace auth={auth('private')} />);
    expect(normal).toContain('本机构信息'); expect(normal).not.toContain('管理员'); expect(normal).not.toContain('ant-spin');
    const pricing = renderToStaticMarkup(<PrivateInstitutionWorkspace auth={auth('private', ['platform_membership_price'])} />);
    expect(normal).not.toContain('会员定价');
    expect(pricing).toContain('会员定价');
    // 拆页后首屏仅挂载账户页，定价读取不能抢先发生。
    expect(pricing).not.toContain('ant-spin');
    for (const label of ['岗位管理', '任职管理', '法定代表人']) expect(normal).toContain(label);
    expect(normal).not.toContain('机构治理');
  });
});

import { describe, expect, it, vi } from 'vitest';
import { dateFromDay, dayFromDate, governanceProposers, nonAdminGovernance, representativeChanges, validateAssignments, getGovernanceState, type GovernanceState } from '../institutions/governance';
const anchor = '0x' + '11'.repeat(32), signer = '0x' + '22'.repeat(32);
const role = { roleCode: 'GOV', roleName: '治理委员', active: true, fixed: false, assignmentLocked: false, termRequired: false, minAssignments: 0, maxAssignments: null,
    permissions: [{ moduleTag: 'pri-mgmt', actionCode: 3, operation: 'PROPOSE' as const }],
    assignments: [{ accountId: anchor, termStart: 0, termEnd: 0, active: true, effective: true, sourceLabel: '机构内部治理', sourceRef: '0x01' }] };
const state: GovernanceState = { cidNumber: 'fixture-institution', blockHash: '0x01', blockNumber: 1, currentDay: 20000, moduleTag: 'pri-mgmt', legalRoleCode: 'LR',
    members: [{ accountId: anchor, signerId: signer, cidNumber: 'fixture-citizen', familyName: '测', givenName: '试' }],
    roles: [role, { ...role, roleCode: 'LR', roleName: '法定代表人', fixed: true, assignmentLocked: true, maxAssignments: 1 }],
    representative: { familyName: '测', givenName: '试', cidNumber: 'fixture-citizen', accountId: anchor } };
const auth = { setupRequired: false, accessToken: 'fixture-session', accountId: signer, institutionCidNumber: state.cidNumber, institutionCode: 'SFGY', familyName: '测', givenName: '试' };
describe('独立治理页载荷及身份边界', () => {
    it('发起岗位按当前钱包解析，任职写入仍保留名册锚点，其他机构只读', () => {
        expect(governanceProposers(auth, state).map(r => r.roleCode)).toEqual(['GOV', 'LR']);
        expect(governanceProposers({ ...auth, institutionCidNumber: 'another' } as never, state)).toEqual([]);
        expect(governanceProposers({ ...auth, accountId: anchor } as never, state)).toEqual([]);
        const input = nonAdminGovernance(state, { assignmentChanges: [{ roleCode: 'GOV', assignments: [{ accountId: anchor, termStart: 0, termEnd: 0 }] }] }, 'GOV');
        expect(input.assignmentChanges?.[0].assignments[0].accountId).toBe(anchor);
        expect(input).not.toHaveProperty('admins');
    });
    it('岗位创建、改名和删除不夹带其他功能，固定岗位及既有权限不可改写', () => {
        const create = nonAdminGovernance(state, { roleMutations: [{ mutation: 'CREATE', roleName: '新岗位', permissions: role.permissions, assignments: [] }] }, 'GOV');
        expect(create.roleMutations?.[0]).not.toHaveProperty('roleCode');
        expect(create).not.toHaveProperty('admins');
        expect(create.assignmentChanges).toBeUndefined();
        expect(create.representativeCid).toBeUndefined();
        expect(() => nonAdminGovernance(state, { roleMutations: [{ mutation: 'RENAME', roleCode: 'GOV', roleName: '' }] }, 'GOV')).toThrow('名称');
        expect(() => nonAdminGovernance(state, { roleMutations: [{ mutation: 'RENAME', roleCode: 'LR', roleName: '新名称' }] }, 'GOV')).toThrow('不可');
        expect(() => nonAdminGovernance(state, { roleMutations: [{ mutation: 'RENAME', roleCode: 'GOV', roleName: '新名称', permissions: role.permissions }] }, 'GOV')).toThrow('只允许');
        expect(() => nonAdminGovernance(state, { admins: [] } as never, 'GOV')).toThrow('名册');
        expect(() => nonAdminGovernance(state, {}, 'GOV')).toThrow('没有');
        expect(nonAdminGovernance(state, { roleMutations: [{ mutation: 'DELETE', roleCode: 'GOV' }] }, 'GOV').roleMutations?.[0].mutation).toBe('DELETE');
    });
    it('任职仅限现有成员，人数和任期规则不能绕过，FRG等专属岗位不走独立入口', () => {
        expect(() => validateAssignments(state, role, [{ accountId: 'outsider', termStart: 0, termEnd: 0 }])).toThrow('现有');
        expect(() => validateAssignments(state, role, [{ accountId: anchor, termStart: 1, termEnd: 2 }])).toThrow('任期');
        expect(() => validateAssignments(state, { ...role, termRequired: true }, [{ accountId: anchor, termStart: 2, termEnd: 1 }])).toThrow('任期');
        expect(() => validateAssignments(state, { ...role, minAssignments: 1 }, [])).toThrow('人数');
        const locked = { ...state, roles: [{ ...role, assignmentLocked: true }] };
        expect(() => nonAdminGovernance(locked, { assignmentChanges: [{ roleCode: 'GOV', assignments: [] }] }, 'GOV')).toThrow('专属');
    });
    it('法定代表人更换和解除同步LR任职，不产生管理员名单变更', () => {
        const set = representativeChanges(state, anchor);
        expect(set.representativeCid).toBe('fixture-citizen');
        expect(set.assignmentChanges?.[0]).toEqual({ roleCode: 'LR', assignments: [{ accountId: anchor, termStart: 0, termEnd: 0 }] });
        expect(nonAdminGovernance(state, set, 'GOV')).not.toHaveProperty('admins');
        const clear = representativeChanges(state, null);
        expect(clear.clearLegalRepresentative).toBe(true);
        expect(clear.assignmentChanges?.[0].assignments).toEqual([]);
        expect(() => representativeChanges(state, 'outsider')).toThrow('现有');
        expect(() => representativeChanges({ ...state, roles: [role] }, anchor)).toThrow('未配置');
        expect(() => representativeChanges({ ...state, roles: [role, { ...state.roles[1], minAssignments: 1 }] }, null)).toThrow('人数');
    });
    it('UTC日序跨时区不漂移、拒绝不存在的日期', () => {
        for (const date of ['2024-02-29', '2026-11-13', '2027-01-01'])
            expect(dateFromDay(dayFromDate(date))).toBe(date);
        expect(() => dayFromDate('2026-02-29')).toThrow('不存在');
        expect(() => dayFromDate('2026-13-01')).toThrow();
        expect(dateFromDay(0)).toBe('');
    });
    it('同会话读取合并在途请求且只有登录态，不长期缓存', async () => {
        let finish!: (r: Response) => void;
        const fetcher = vi.spyOn(globalThis, 'fetch').mockImplementation(() => new Promise(resolve => { finish = resolve; }));
        try {
            const first = getGovernanceState(auth, state.cidNumber), second = getGovernanceState(auth, state.cidNumber);
            expect(fetcher).toHaveBeenCalledTimes(1);
            const headers = new Headers(fetcher.mock.calls[0][1]?.headers);
            expect(headers.get('authorization')).toBe('Bearer fixture-session');
            expect(headers.has('x-passkey-assertion')).toBe(false);
            finish(new Response(JSON.stringify({ code: 0, data: state })));
            await Promise.all([first, second]);
            const third = getGovernanceState(auth, state.cidNumber);
            expect(fetcher).toHaveBeenCalledTimes(2);
            finish(new Response(JSON.stringify({ code: 0, data: state })));
            await third;
        }
        finally {
            fetcher.mockRestore();
        }
    });
});
describe('治理独立页面真实DOM交互', () => {
    const browserTest = process.env.ONCHINA_TEST_DOM ? it : it.skip;
    async function mount(options: {
        readonly?: boolean;
        failedRead?: boolean;
        delayedPasskey?: boolean;
        view?: 'roles' | 'assignments' | 'representative';
    } = {}) {
        vi.resetModules();
        const { createRequire } = await import('node:module');
        const { JSDOM } = createRequire(import.meta.url)(process.env.ONCHINA_TEST_DOM!);
        const dom = new JSDOM('<div id="root"></div>', { url: 'https://onchina.example.invalid/', pretendToBeVisual: true });
        // jsdom不实现伪元素布局，本组只验证业务交互，不以模拟值验收视觉尺寸。
        const computedStyle = dom.window.getComputedStyle.bind(dom.window);
        dom.window.getComputedStyle = (element: Element) => computedStyle(element);
        for (const key of ['window', 'document', 'navigator', 'sessionStorage', 'HTMLElement', 'HTMLBodyElement', 'Element', 'ShadowRoot', 'SVGElement', 'getComputedStyle'])
            vi.stubGlobal(key, dom.window[key]);
        dom.window.matchMedia = () => ({ matches: false, addListener() { }, removeListener() { }, addEventListener() { }, removeEventListener() { }, dispatchEvent() { return false; } });
        vi.stubGlobal('IS_REACT_ACT_ENVIRONMENT', true);
        let releaseKey!: (value: string) => void;
        const assertion = vi.fn(() => options.delayedPasskey ? new Promise<string>(resolve => { releaseKey = resolve; }) : Promise.resolve('fixture-assertion'));
        const signChain = vi.fn(async () => ({ accountId: signer, signature: 'fixture-signature' }));
        const submit = vi.fn(async () => ({ txHash: 'fixture-transaction' }));
        vi.doMock('../authentication/passkeyClient', () => ({ assertPasskey: assertion, PASSKEY_ASSERTION_HEADER: 'x-passkey-assertion' }));
        vi.doMock('../qr/useChainSign', () => ({ useChainSign: () => ({ signChain, chainSignModal: null }), submitChainSign: submit }));
        vi.doMock('../controls/notice', () => ({ notice: { success: vi.fn(), error: vi.fn() } }));
        let fail = options.failedRead;
        const fetcher = vi.spyOn(globalThis, 'fetch').mockImplementation(async (path) => {
            if (String(path).endsWith('/governance')) {
                if (fail) {
                    fail = false;
                    return new Response('unavailable', { status: 503 });
                }
                return new Response(JSON.stringify({ code: 0, data: state }));
            }
            if (String(path).endsWith('/prepare'))
                return new Response(JSON.stringify({ code: 0, data: { requestId: 'fixture-request', signRequest: 'fixture-request-code' } }));
            throw new Error('unexpected request');
        });
        const React = await import('react');
        const { createRoot } = await import('react-dom/client');
        const { RolesView } = await import('../institutions/RolesView');
        const { AssignmentsView } = await import('../institutions/AssignmentsView');
        const { RepresentativeView } = await import('../institutions/RepresentativeView');
        const View = options.view === 'assignments' ? AssignmentsView : options.view === 'representative' ? RepresentativeView : RolesView;
        const root = createRoot(document.getElementById('root')!);
        const flush = () => React.act(async () => { await new Promise(resolve => setTimeout(resolve, 35)); });
        await React.act(async () => { root.render(React.createElement(View, { auth: options.readonly ? { ...auth, institutionCidNumber: 'other' } : auth, cidNumber: state.cidNumber })); });
        await flush();
        const button = (text: string) => [...document.querySelectorAll('button')].find(b => (b.textContent || '').replace(/\s/g, '') === text) as HTMLButtonElement;
        const click = async (text: string) => { expect(button(text), text).toBeTruthy(); await React.act(async () => button(text).click()); await flush(); };
        const confirm = async () => {
            await click('删除');
            const selector = document.querySelector('.ant-select-selector')!;
            expect(selector).toBeTruthy();
            await React.act(async () => selector.dispatchEvent(new dom.window.MouseEvent('mousedown', { bubbles: true })));
            await flush();
            const option = document.querySelector('.ant-select-item-option[title="治理委员"]') as HTMLElement;
            expect(option).toBeTruthy();
            await React.act(async () => option.click());
            await flush();
            await click('验证并签名提交提案');
        };
        const unmount = () => React.act(async () => root.unmount());
        const changeSession = () => React.act(async () => root.render(React.createElement(View, { auth: { ...auth, accessToken: 'another-session' }, cidNumber: state.cidNumber })));
        return { changeSession, assertion, signChain, submit, fetcher, click, confirm, button, flush, unmount, releaseKey: () => releaseKey('fixture-assertion'), dispose: async () => { await unmount(); fetcher.mockRestore(); vi.doUnmock('../authentication/passkeyClient'); vi.doUnmock('../qr/useChainSign'); vi.doUnmock('../controls/notice'); dom.window.close(); vi.unstubAllGlobals(); } };
    }
    browserTest('其它机构可读岗位，但没有写入口，读取不触发密钥认证', async () => {
        const c = await mount({ readonly: true });
        try {
            expect(document.body.textContent).toContain('治理委员');
            expect(c.button('新增岗位').disabled).toBe(true);
            expect(c.button('删除')).toBeUndefined();
            expect(c.assertion).not.toHaveBeenCalled();
        }
        finally {
            await c.dispose();
        }
    }, 15000);
    browserTest('取消创建不发提案、不认证、不弹钱包', async () => {
        const c = await mount();
        try {
            await c.click('新增岗位');
            await c.click('取消');
            expect(c.assertion).not.toHaveBeenCalled();
            expect(c.signChain).not.toHaveBeenCalled();
            expect(c.fetcher.mock.calls.some(([path]) => String(path).endsWith('/prepare'))).toBe(false);
        }
        finally {
            await c.dispose();
        }
    }, 15000);
    browserTest('确认后只认证一次和签名一次，非名册载荷原样提交', async () => {
        const c = await mount();
        try {
            await c.confirm();
            expect(c.assertion).toHaveBeenCalledTimes(1);
            expect(c.signChain).toHaveBeenCalledTimes(1);
            expect(c.submit).toHaveBeenCalledTimes(1);
            const posts = c.fetcher.mock.calls.filter(([path]) => String(path).endsWith('/prepare'));
            expect(posts).toHaveLength(1);
            const input = JSON.parse(String(posts[0][1]?.body));
            expect(input).not.toHaveProperty('admins');
            expect(input.roleMutations).toEqual([{ mutation: 'DELETE', roleCode: 'GOV' }]);
        }
        finally {
            await c.dispose();
        }
    }, 15000);
    browserTest('离开页面后迟到的Passkey认证不能发送治理请求', async () => {
        const c = await mount({ delayedPasskey: true });
        try {
            await c.confirm();
            expect(c.assertion).toHaveBeenCalledTimes(1);
            await c.unmount();
            c.releaseKey();
            await c.flush();
            expect(c.signChain).not.toHaveBeenCalled();
            expect(c.submit).not.toHaveBeenCalled();
            expect(c.fetcher.mock.calls.some(([path]) => String(path).endsWith('/prepare'))).toBe(false);
        }
        finally {
            await c.dispose();
        }
    }, 15000);
    browserTest('读取失败显示错误并可重试，不伪装为空岗位', async () => {
        const c = await mount({ failedRead: true });
        try {
            expect(document.body.textContent).toContain('治理数据读取失败');
            await c.click('重试');
            expect(document.body.textContent).toContain('治理委员');
            expect(c.assertion).not.toHaveBeenCalled();
        }
        finally {
            await c.dispose();
        }
    }, 15000);
    browserTest('任职页独立展示当前任职，取消完整集合调整不认证', async () => {
        const c = await mount({ view: 'assignments' });
        try {
            expect(document.body.textContent).toContain('任职管理');
            expect(document.body.textContent).toContain('机构内部治理');
            expect(c.button('新增岗位')).toBeUndefined();
            await c.click('发起任职变更');
            expect(document.body.textContent).toContain('完整目标任职集合');
            await c.click('取消');
            expect(c.assertion).not.toHaveBeenCalled();
            expect(c.fetcher.mock.calls.every(([path]) => String(path).endsWith('/governance'))).toBe(true);
        }
        finally {
            await c.dispose();
        }
    }, 15000);
    browserTest('法定代表人独立展示，解除确认同时列出专属任职且取消不认证', async () => {
        const c = await mount({ view: 'representative' });
        try {
            expect(document.body.textContent).toContain('fixture-citizen');
            expect(c.button('新增岗位')).toBeUndefined();
            await c.click('发起解除');
            expect(document.body.textContent).toContain('确认解除法定代表人');
            expect(document.body.textContent).toContain('专属岗位任职');
            await c.click('取消');
            expect(c.assertion).not.toHaveBeenCalled();
            expect(c.signChain).not.toHaveBeenCalled();
        }
        finally {
            await c.dispose();
        }
    }, 15000);
    browserTest('切换登录会话销毁旧草稿，迟到认证不能提交', async () => {
        const c = await mount({ delayedPasskey: true });
        try {
            await c.confirm();
            expect(c.assertion).toHaveBeenCalledTimes(1);
            await c.changeSession();
            await c.flush();
            c.releaseKey();
            await c.flush();
            expect(document.body.textContent).not.toContain('确认岗位删除提案');
            expect(c.signChain).not.toHaveBeenCalled();
            expect(c.fetcher.mock.calls.filter(([path]) => String(path).endsWith('/prepare'))).toHaveLength(0);
        }
        finally {
            await c.dispose();
        }
    }, 15000);
});

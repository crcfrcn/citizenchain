// 验证机构岗位、任职和代表人治理的载荷、身份及真实DOM交互。

import { describe, expect, it, vi } from 'vitest';
import { dateFromDay, dayFromDate, governanceProposers, nonAdminGovernance, representativeChanges, validateAssignments, getGovernanceState, type GovernanceState } from '../institutions/governance';
const anchor = '0x' + '11'.repeat(32), signer = '0x' + '22'.repeat(32);
const role = { roleCode: 'GOV', roleName: '治理委员', active: true, fixed: false, assignmentLocked: false, termRequired: false, minAssignments: 0, maxAssignments: null,
    permissions: [{ moduleTag: 'pri-mgmt', actionCode: 3, operation: 'PROPOSE' as const }],
    assignments: [{ accountId: anchor, termStart: 0, termEnd: 0, active: true, effective: true, sourceLabel: '机构内部治理', sourceRef: '0x01' }] };
const state: GovernanceState = { cidNumber: 'fixture-institution', blockHash: '0x01', blockNumber: 1, currentDay: 20000, moduleTag: 'pri-mgmt', legalRoleCode: 'LR',
    members: [{ accountId: signer, authorizationAccountId: anchor, signerId: signer, cidNumber: 'fixture-citizen', familyName: '测', givenName: '试' }],
    roles: [role, { ...role, roleCode: 'LR', roleName: '法定代表人', fixed: true, assignmentLocked: true, maxAssignments: 1 }],
    representative: { familyName: '测', givenName: '试', cidNumber: 'fixture-citizen', accountId: anchor } };
const auth = { walletRevision: '0', walletNamespace: 'PrivateAdmins' as const, setupRequired: false, accessToken: 'fixture-session', accountId: signer, institutionCidNumber: state.cidNumber, institutionCode: 'SFGY', familyName: '测', givenName: '试' };
describe('独立治理页载荷及身份边界', () => {
    it('发起岗位按当前钱包解析，任职写入仍保留原岗位引用，其他机构只读', () => {
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


// 合成二维码只用于前端协议与并发边界回归；真实密码学由Runtime/冷钱包测试覆盖。
describe('机构钱包统一双签与投票更换协议', () => {
    const account = (byte: number) => '0x' + byte.toString(16).padStart(2, '0').repeat(32);
    const cid = 'LN001-NRC0G-944805165-2026';
    const encode = (bytes: number[] | Uint8Array) => Buffer.from(bytes).toString('base64url');
    const bytes = (hex: string) => [...Buffer.from(hex.slice(2), 'hex')];
    const hex = (data: number[] | Uint8Array) => '0x' + Buffer.from(data).toString('hex');
    const u64 = (value: number) => { const out = Buffer.alloc(8); out.writeBigUInt64LE(BigInt(value)); return [...out]; };
    async function fixture(options: { mode?: 'SELF' | 'GOVERNANCE'; moduleTag?: string; code?: string; failSubmit?: boolean; tamper?: 'acceptance' | 'call' | 'tail' | 'expiry'; changedAfterConsent?: boolean; deferredPasskey?: boolean; signatureBytes?: 64 | 5261 } = {}) {
        vi.resetModules(); vi.doUnmock('../qr/useChainSign');
        const values = new Map<string, string>();
        vi.stubGlobal('sessionStorage', { get length() { return values.size; }, key: (i: number) => [...values.keys()][i] ?? null,
            getItem: (key: string) => values.get(key) ?? null, setItem: (key: string, value: string) => values.set(key, value), removeItem: (key: string) => values.delete(key) });
        let finishPasskey!: () => void;
        const passkey = vi.fn(async () => { if (options.deferredPasskey) await new Promise<void>(resolve => { finishPasskey = resolve; }); return 'wallet-assertion'; });
        vi.doMock('../authentication/passkeyClient', () => ({ assertPasskey: passkey, PASSKEY_ASSERTION_HEADER: 'x-passkey-assertion' }));
        const g = await import('../institutions/governance');
        const mode = options.mode ?? 'SELF', moduleTag = options.moduleTag ?? 'pub-mgmt';
        const actor = mode === 'SELF' ? account(3) : account(5);
        const authentication = { walletRevision: '0', walletNamespace: 'PublicAdmins' as const, setupRequired: false, accessToken: 'wallet-session', accountId: actor, institutionCidNumber: cid, institutionCode: options.code ?? 'NRC', familyName: '测', givenName: '试' };
        const snapshot: import('../institutions/governance').GovernanceState = {
            cidNumber: cid, blockHash: account(9), blockNumber: 10, currentDay: 20000, moduleTag, legalRoleCode: 'LR', representative: null,
            members: [{ accountId: account(3), authorizationAccountId: account(2), signerId: account(3), walletRevision: 7, cidNumber: 'person-cid', familyName: '张', givenName: '三' },
                { accountId: account(5), authorizationAccountId: account(6), signerId: account(5), walletRevision: 0, cidNumber: '', familyName: '李', givenName: '四' }],
            roles: [{ ...role, roleCode: 'PROVINCE-GZ', permissions: [{ moduleTag, actionCode: 3, operation: 'PROPOSE' }, { moduleTag, actionCode: 3, operation: 'VOTE' }],
                assignments: [account(2), account(6)].map(accountId => ({ ...role.assignments[0], accountId })) }],
        };
        const draft = g.buildInstitutionWalletDraft(authentication, snapshot, mode, account(3), account(4), mode === 'GOVERNANCE' ? 'PROVINCE-GZ' : '');
        const calls: string[] = [];
        const proof = '0x' + 'ab'.repeat(options.signatureBytes ?? 64);
        const now = Math.floor(Date.now() / 1000), expires = now + (mode === 'SELF' ? 160 : 86400), qrExpiry = now + 100;
        const { blake2b } = await import('@noble/hashes/blake2.js');
        const material = [...Array(32).fill(1), cid.length << 2, ...Buffer.from(cid), ...bytes(account(3)), ...bytes(account(3)), ...bytes(account(4)), ...u64(7), mode === 'SELF' ? 0 : 1, ...u64(expires)];
        const requestId = 'institution-wallet-' + Buffer.from(blake2b(Uint8Array.from(material), { dkLen: 32 })).toString('hex');
        const qr = (id: string, signer: string, action: number, payload: number[], expiry: number) => JSON.stringify({ p: 'QR_V1', k: 1, i: id, e: expiry, b: { a: action, g: 0, u: encode(bytes(signer)), d: encode(payload) } });
        let failSubmit = options.failSubmit ?? false;
        let finalized = false;
        let latest = snapshot;
        const fetcher = vi.spyOn(globalThis, 'fetch').mockImplementation(async (path, init) => {
            if (String(path).endsWith('/governance')) { calls.push('read'); return new Response(JSON.stringify({ code: 0, data: latest })); }
            if (String(path).endsWith('/wallet/prepare')) {
                const input = JSON.parse(String(init?.body));
                if (!input.newWalletResponse) {
                    calls.push('accept'); expect(new Headers(init?.headers).has('x-passkey-assertion')).toBe(false);
                    const acceptance = [...material]; if (options.tamper === 'acceptance') acceptance[33 + cid.length] ^= 1;
                    return new Response(JSON.stringify({ code: 0, data: { requestId, expiresAt: expires, acceptancePayloadHex: hex(acceptance), signRequest: qr(requestId, account(4), 21, acceptance, options.tamper === 'expiry' ? now - 1 : qrExpiry) } }));
                }
                calls.push('prepare'); expect(new Headers(init?.headers).get('x-passkey-assertion')).toBe('wallet-assertion');
                expect(input).toEqual({ cidNumber: cid, adminAccount: account(3), newWallet: account(4), mode, proposerRoleCode: draft.proposerRoleCode, expiresAt: expires, newWalletResponse: consentResponse });
                let call = g.institutionWalletCall(draft, expires, proof, moduleTag);
                if (options.tamper === 'call') call = hex([...bytes(call).slice(0, -1), 0]);
                const tail = [0, 0, 0, 0, 1, 0, 0, 0, 1, 0, 0, 0, ...Array(64).fill(1), 0];
                if (options.tamper === 'tail') tail[12] = 2;
                return new Response(JSON.stringify({ code: 0, data: { requestId: 'institution-governance-wallet-test', cidNumber: cid, callDataHex: call, chainAction: (moduleTag === 'pub-mgmt' ? 30 : 31) * 256 + (mode === 'SELF' ? 10 : 8), expiresAt: now + 100,
                    signRequest: qr('institution-governance-wallet-test', actor, (moduleTag === 'pub-mgmt' ? 30 : 31) * 256 + (mode === 'SELF' ? 10 : 8), [...bytes(call), ...tail], now + 100) } }));
            }
            if (String(path).endsWith('/chain/submit')) {
                calls.push('submit'); expect(values.size).toBe(1);
                expect(JSON.parse(String(init?.body))).toEqual({ requestId: 'institution-governance-wallet-test', accountId: actor, signature: proof });
                if (failSubmit) throw new Error('response lost');
                finalized = true;
                return new Response(JSON.stringify({ code: 0, data: { purpose: 'INSTITUTION_WALLET', cidNumber: cid, txHash: account(8), blockNumber: 11 } }));
            }
            throw new Error('unexpected request ' + path);
        });
        const consentResponse = JSON.stringify({ p: 'QR_V1', k: 2, i: requestId, e: qrExpiry, b: { u: encode(bytes(account(4))), s: encode(bytes(proof).slice(options.signatureBytes === 5261 ? 1952 : 0)), ...(options.signatureBytes === 5261 ? { q: encode(bytes(proof).slice(0, 1952)) } : {}) } });
        const sign = vi.fn(async (id: string) => {
            calls.push(id === requestId ? 'new-sign' : 'old-sign');
            if (id === requestId) {
                expect(passkey).not.toHaveBeenCalled();
                if (options.changedAfterConsent) latest = { ...snapshot, members: [{ ...snapshot.members[0], walletRevision: 8 }, snapshot.members[1]] };
                return { accountId: account(4), signature: proof, response: consentResponse };
            }
            return { accountId: actor, signature: proof };
        });
        const remembered: (import('../institutions/governance').PendingInstitutionWallet | null)[] = [];
        const remember = (receipt: import('../institutions/governance').PendingInstitutionWallet | null, previous?: import('../institutions/governance').PendingInstitutionWallet) => { remembered.push(receipt); g.rememberInstitutionWallet(receipt, previous); };
        const run = (extra = {}) => g.runInstitutionWalletChange({ auth: authentication, state: snapshot, draft, sign, remember, check: () => {}, ...extra });
        return { g, authentication, snapshot, draft, calls, sign, passkey, run, values, remembered, proof, fetcher, get finalized() { return finalized; },
            releasePasskey: () => finishPasskey(), allowSubmit: () => { failSubmit = false; }, setLatest: (state: typeof snapshot) => { latest = state; },
            dispose: () => { fetcher.mockRestore(); vi.doUnmock('../authentication/passkeyClient'); vi.unstubAllGlobals(); } };
    }
    it('公私权与私权非法人逐字段锁定，接收在前、一次Passkey、原钱包签交易在后', async () => {
        for (const [mode, moduleTag, code, signatureBytes] of [['SELF', 'pub-mgmt', 'NRC', 64], ['GOVERNANCE', 'pri-mgmt', 'SFGY', 64], ['SELF', 'pri-mgmt', 'UNIN', 5261]] as const) {
            const f = await fixture({ mode, moduleTag, code, signatureBytes });
            try {
                const result = await f.run();
                expect(result.mode).toBe(mode); expect(f.finalized).toBe(true);
                expect(f.calls).toEqual(['read', 'accept', 'new-sign', 'read', 'prepare', 'old-sign', 'submit']);
                expect(f.passkey).toHaveBeenCalledTimes(1); expect(f.sign).toHaveBeenCalledTimes(2);
                expect(f.remembered[0]?.draft).toEqual(f.draft); expect(f.remembered[f.remembered.length - 1]).toBeNull(); expect(f.values.size).toBe(0);
                // 投票提案提交不修改查询中的原钱包或岗位，也不伪造执行成功。
                expect(f.snapshot.members[0].signerId).toBe(account(3)); expect(f.snapshot.roles[0].assignments).toHaveLength(2);
            } finally { f.dispose(); }
        }
    });
    it('新旧钱包及治理action编码与Rust权威夹具完全相同', async () => {
        const f = await fixture();
        try {
            const { readFileSync } = await import('node:fs');
            const golden = JSON.parse(readFileSync(new URL('../../../runtime/primitives/tests/fixtures/role_permission.json', import.meta.url), 'utf8')).institution_wallet;
            expect(f.g.institutionWalletCall(f.draft, golden.expires_at, '0x' + 'ab'.repeat(64), 'pub-mgmt')).toBe(golden.self_call_hex);
            const action = f.g.institutionWalletCall({ ...f.draft, mode: 'GOVERNANCE', proposerRoleCode: 'PROVINCE-GZ' }, golden.expires_at, '0x' + 'ab'.repeat(64), 'pri-mgmt');
            expect(action.startsWith('0x1f08')).toBe(true); expect(action).toContain(golden.governance_action_hex.slice(2));
        } finally { f.dispose(); }
    });
    it('机构隔离、本人/他人及FRG省组都只能按原有效岗位选择', async () => {
        const f = await fixture({ mode: 'GOVERNANCE', code: 'FRG' });
        try {
            expect(f.g.institutionWalletRoles(f.authentication, f.snapshot, account(3))).toHaveLength(1);
            const otherProvince = { ...f.snapshot, roles: [{ ...f.snapshot.roles[0], assignments: f.snapshot.roles[0].assignments.filter(a => a.accountId === account(6)) }] };
            expect(f.g.institutionWalletRoles(f.authentication, otherProvince, account(3))).toEqual([]);
            expect(() => f.g.buildInstitutionWalletDraft(f.authentication, otherProvince, 'GOVERNANCE', account(3), account(4), 'PROVINCE-GZ')).toThrow('提案权限');
            expect(() => f.g.buildInstitutionWalletDraft(f.authentication, f.snapshot, 'SELF', account(3), account(4))).toThrow('本人');
            expect(() => f.g.buildInstitutionWalletDraft({ ...f.authentication, institutionCidNumber: 'other' }, f.snapshot, 'GOVERNANCE', account(3), account(4), 'PROVINCE-GZ')).toThrow('本机构');
            expect(() => f.g.buildInstitutionWalletDraft(f.authentication, f.snapshot, 'GOVERNANCE', account(3), account(5), 'PROVINCE-GZ')).toThrow('已被');
            expect(() => f.g.buildInstitutionWalletDraft(f.authentication, { ...f.snapshot, members: [{ ...f.snapshot.members[0], walletRevision: Number.MAX_SAFE_INTEGER + 1 }, f.snapshot.members[1]] }, 'GOVERNANCE', account(3), account(4), 'PROVINCE-GZ')).toThrow('版本');
        } finally { f.dispose(); }
    });
    it('错误新钱包、取消、陈旧版本和篡改证明不触发Passkey或原钱包签名', async () => {
        for (const option of [{ tamper: 'acceptance' }, { tamper: 'expiry' }, { changedAfterConsent: true }] as const) {
            const f = await fixture(option);
            try { await expect(f.run()).rejects.toThrow(); expect(f.passkey).not.toHaveBeenCalled(); expect(f.calls).not.toContain('submit'); } finally { f.dispose(); }
        }
        for (const sign of [async () => { throw new Error('已取消签名'); }, async () => ({ accountId: account(9), signature: '0x' + 'ab'.repeat(64), response: 'wrong' })]) {
            const f = await fixture();
            try { await expect(f.run({ sign })).rejects.toThrow(); expect(f.passkey).not.toHaveBeenCalled(); expect(f.calls).not.toContain('submit'); } finally { f.dispose(); }
        }
    });
    it('待签交易或创世链篡改不能交给原钱包签名', async () => {
        for (const tamper of ['call', 'tail'] as const) {
            const f = await fixture({ tamper });
            try { await expect(f.run()).rejects.toThrow(); expect(f.passkey).toHaveBeenCalledTimes(1); expect(f.sign).toHaveBeenCalledTimes(1); expect(f.calls).not.toContain('submit'); } finally { f.dispose(); }
        }
    });
    it('Passkey等待期间离开或切换身份，迟到结果不能准备或提交交易', async () => {
        const f = await fixture({ deferredPasskey: true }); const controller = new AbortController();
        try {
            const running = f.run({ signal: controller.signal, check: () => controller.signal.throwIfAborted() });
            const rejected = expect(running).rejects.toThrow();
            await vi.waitFor(() => expect(f.passkey).toHaveBeenCalledTimes(1));
            controller.abort(); f.releasePasskey(); await rejected;
            expect(f.calls).not.toContain('prepare'); expect(f.calls).not.toContain('submit');
        } finally { f.dispose(); }
    });
    it('响应丢失先保留原签名，重登录恢复只提交原会话，不再prepare/Passkey/签名', async () => {
        const f = await fixture({ failSubmit: true });
        try {
            await expect(f.run()).rejects.toThrow('无法连接服务器');
            const receipts = f.g.readInstitutionWalletReceipts({ ...f.authentication, accessToken: 'relogin' });
            expect(receipts).toHaveLength(1); expect(receipts[0].signature).toBe(f.proof);
            expect(f.values.size).toBe(1); f.allowSubmit();
            const calls = f.calls.length;
            await f.run({ pending: receipts[0], state: undefined, draft: undefined });
            expect(f.calls.slice(calls)).toEqual(['submit']); expect(f.passkey).toHaveBeenCalledTimes(1); expect(f.sign).toHaveBeenCalledTimes(2);
            expect(f.values.size).toBe(0);
        } finally { f.dispose(); }
    });
    it('新钱包可恢复本机构旧签名，其他机构/第三方/篡改持久化记录不可恢复', async () => {
        const f = await fixture({ failSubmit: true });
        try {
            await expect(f.run()).rejects.toThrow();
            const nextAuth = { ...f.authentication, accountId: account(4), accessToken: 'new-wallet-login' };
            expect(f.g.readInstitutionWalletReceipts(nextAuth)).toHaveLength(1);
            expect(f.g.readInstitutionWalletReceipts({ ...nextAuth, institutionCidNumber: 'other' })).toEqual([]);
            expect(f.g.readInstitutionWalletReceipts({ ...nextAuth, accountId: account(8) })).toEqual([]);
            const [key, value] = [...f.values.entries()][0];
            f.values.set(key, JSON.stringify({ ...JSON.parse(value), actorAccount: account(8) }));
            expect(f.g.readInstitutionWalletReceipts(nextAuth)).toEqual([]);
        } finally { f.dispose(); }
    });
    it('原版本仍有效时不能清除签名重签；只有版本改变后显式重新审阅', async () => {
        const f = await fixture({ failSubmit: true });
        try {
            await expect(f.run()).rejects.toThrow(); const receipt = f.g.readInstitutionWalletReceipts(f.authentication)[0];
            await expect(f.g.reviewInstitutionWalletConflict(f.authentication, receipt)).rejects.toThrow('原版本仍有效');
            expect(f.values.size).toBe(1);
            f.setLatest({ ...f.snapshot, members: [{ ...f.snapshot.members[0], walletRevision: 8 }, f.snapshot.members[1]] });
            await expect(f.g.reviewInstitutionWalletConflict(f.authentication, receipt)).resolves.toHaveProperty('cidNumber', cid);
            expect(f.values.size).toBe(1); // 查询本身不能隐式丢弃签名。
            f.setLatest({ ...f.snapshot, currentDay: Math.ceil(receipt.proofExpiresAt / 86400) });
            await expect(f.g.reviewInstitutionWalletConflict(f.authentication, receipt)).resolves.toHaveProperty('cidNumber', cid);

        } finally { f.dispose(); }
    });

    it('新钱包恢复遇到已消费会话，只核对链上目标，不伪造原交易回执或重新签名', async () => {
        const f = await fixture({ failSubmit: true });
        try {
            await expect(f.run()).rejects.toThrow(); const pending = f.g.readInstitutionWalletReceipts(f.authentication)[0];
            f.setLatest({ ...f.snapshot, blockNumber: 11, members: [{ ...f.snapshot.members[0], accountId: account(4), signerId: account(4), walletRevision: 0 }, f.snapshot.members[1]] });
            f.fetcher.mockImplementationOnce(async () => new Response(JSON.stringify({ code: 1004, message: 'missing' }), { status: 404 }));
            const observed = await f.run({ auth: { ...f.authentication, accountId: account(4) }, state: undefined, draft: undefined, pending });
            expect(observed.observed).toBe(true); expect(observed.receipt).toBeUndefined();
            expect(f.passkey).toHaveBeenCalledTimes(1); expect(f.sign).toHaveBeenCalledTimes(2); expect(f.values.size).toBe(0);
        } finally { f.dispose(); }
    });
    it('治理会话丢失且尚未执行，不能假报生效；签名继续保留以核对原会话', async () => {
        const f = await fixture({ mode: 'GOVERNANCE', failSubmit: true });
        try {
            await expect(f.run()).rejects.toThrow(); const pending = f.g.readInstitutionWalletReceipts(f.authentication)[0];
            f.fetcher.mockImplementationOnce(async () => new Response(JSON.stringify({ code: 1004, message: 'missing' }), { status: 404 }));
            await expect(f.run({ pending, state: undefined, draft: undefined })).rejects.toThrow('missing');
            expect(f.values.size).toBe(1); expect(f.snapshot.members[0].signerId).toBe(account(3));
            expect(f.passkey).toHaveBeenCalledTimes(1); expect(f.sign).toHaveBeenCalledTimes(2);
        } finally { f.dispose(); }
    });

    async function transportFrames(text: string): Promise<string[]> {
        const { blake2b } = await import('@noble/hashes/blake2.js');
        const content=Buffer.from(text); const id=Buffer.from(blake2b(content,{dkLen:32})).toString('hex');
        const count=Math.ceil(content.length/1200);
        return Array.from({length:count},(_,i)=>`GMBQR1:${id}:${i}:${count}:${content.subarray(i*1200,(i+1)*1200).toString('base64url')}`);
    }
    it('PQ长请求分帧必须完整保留原签名字节，拒绝混帧、丢帧、重复或替换内容', async () => {
        const { verifiedQrTransportFrames } = await import('../qr/citizenQr');
        const text='完整机构钱包请求'.repeat(600), frames=await transportFrames(text);
        expect(verifiedQrTransportFrames(text,[...frames].reverse())).toEqual(frames);
        for (const broken of [frames.slice(1),[frames[0],...frames.slice(0,-1)],[frames[0].replace(/:0:/,':99:'),...frames.slice(1)], await transportFrames(text+'tamper')]) expect(()=>verifiedQrTransportFrames(text,broken)).toThrow();
        expect(verifiedQrTransportFrames('single-qr',['single-qr'])).toEqual(['single-qr']);
    });
    const walletBrowserTest = process.env.ONCHINA_TEST_DOM ? it : it.skip;
    async function walletPage(mode: 'SELF' | 'GOVERNANCE' = 'SELF') {
        const f = await fixture({ mode });
        vi.resetModules();
        const { createRequire } = await import('node:module');
        const { JSDOM } = createRequire(import.meta.url)(process.env.ONCHINA_TEST_DOM!);
        const dom = new JSDOM('<div id="root"></div>', { url: 'https://onchina.example.invalid/', pretendToBeVisual: true });
        const computed = dom.window.getComputedStyle.bind(dom.window);
        dom.window.getComputedStyle = (element: Element) => computed(element);
        dom.window.matchMedia = () => ({ matches: false, addListener() {}, removeListener() {}, addEventListener() {}, removeEventListener() {} });
        for (const key of ['window', 'document', 'navigator', 'HTMLElement', 'HTMLBodyElement', 'HTMLInputElement', 'Element', 'ShadowRoot', 'SVGElement', 'getComputedStyle']) vi.stubGlobal(key, dom.window[key]);
        vi.stubGlobal('IS_REACT_ACT_ENVIRONMENT', true);
        const React = await import('react');
        const logout = vi.fn(), success = vi.fn(), info = vi.fn();
        vi.doMock('../authentication/useAuth', () => ({ useAuth: () => ({ auth: f.authentication, logout }) }));
        vi.doMock('../controls/notice', () => ({ notice: { success, info, error: vi.fn() } }));
        vi.doMock('@gmb/scanner-react', () => ({ ScannerView: () => null, createQrAssembler: () => async (raw: string) => raw }));
        let lateScan: (() => void) | undefined;
        vi.doMock('../qr/CitizenSignatureModal', () => ({ CitizenSignatureModal: (props: { open: boolean; title: string; qrValue: string; onDetected: (raw: string) => void; onCancel: () => void }) => {
            if (!props.open) return null;
            const scan = () => { const request = JSON.parse(props.qrValue); props.onDetected(JSON.stringify({ p: 'QR_V1', k: 2, i: request.i, e: request.e, b: { u: request.b.u, s: encode(bytes(f.proof)) } })); };
            lateScan = scan;
            return React.createElement('section', { 'data-wallet-signature': true }, props.title,
                React.createElement('button', { onClick: scan }, '回扫当前签名'), React.createElement('button', { onClick: props.onCancel }, '取消当前签名'));
        } }));
        const { createRoot } = await import('react-dom/client');
        const { InstitutionWalletSelfAction } = await import('../institutions/AdminsView');
        const { InstitutionWalletGovernancePanel } = await import('../institutions/GovernanceReview');
        const root = createRoot(document.getElementById('root')!);
        let mounted = true;
        const leave = async () => { if (mounted) { mounted = false; await React.act(async () => root.unmount()); } };
        const render = () => mode === 'SELF' ? React.createElement(React.Fragment, null,
            React.createElement(InstitutionWalletSelfAction, { accountId: f.authentication.accountId, institutionCidNumber: cid }),
            React.createElement(InstitutionWalletSelfAction, { accountId: account(9), institutionCidNumber: cid }),
            React.createElement(InstitutionWalletSelfAction, { accountId: f.authentication.accountId, institutionCidNumber: 'other' }))
            : React.createElement(InstitutionWalletGovernancePanel, { auth: f.authentication, cidNumber: cid });
        const flush = () => React.act(async () => { await new Promise(resolve => setTimeout(resolve, 30)); });
        const button = (text: string) => [...document.querySelectorAll('button')].find(b => b.textContent?.replace(/\s/g, '') === text) as HTMLButtonElement | undefined;
        const click = async (text: string) => { const item = button(text); expect(item, text).toBeTruthy(); await React.act(async () => item!.click()); await flush(); };
        await React.act(async () => root.render(render())); await flush();
        const open = async () => {
            await click(mode === 'SELF' ? '更换' : '提案更换钱包');
            const input = document.querySelector('input[aria-label="新钱包账户"]') as HTMLInputElement;
            expect(input).toBeTruthy();
            await React.act(async () => {
                Object.getOwnPropertyDescriptor(dom.window.HTMLInputElement.prototype, 'value')!.set!.call(input, account(4));
                input.dispatchEvent(new dom.window.Event('input', { bubbles: true }));
            }); await flush();
            if (mode === 'GOVERNANCE') {
                const selector = document.querySelector('.ant-select-selector')!;
                await React.act(async () => selector.dispatchEvent(new dom.window.MouseEvent('mousedown', { bubbles: true }))); await flush();
                const option = document.querySelector('.ant-select-item-option') as HTMLElement;
                expect(option).toBeTruthy(); await React.act(async () => option.click()); await flush();
            }
        };
        return { ...f, logout, success, info, root, React, button, click, open, flush, leave, lateScan: () => lateScan?.(),
            disposePage: async () => { await leave(); f.dispose(); vi.doUnmock('../authentication/useAuth'); vi.doUnmock('../controls/notice'); vi.doUnmock('@gmb/scanner-react'); vi.doUnmock('../qr/CitizenSignatureModal'); dom.window.close(); } };
    }
    walletBrowserTest('真实本人按钮只属于当前机构本人，双二维码明确分阶段且完成后退出旧登录', async () => {
        const c = await walletPage();
        try {
            expect([...document.querySelectorAll('button')].filter(b => b.textContent?.replace(/\s/g, '') === '更换')).toHaveLength(1);
            expect(c.fetcher).not.toHaveBeenCalled(); await c.open();
            expect(document.body.textContent).toContain('身份CID号'); expect(document.body.textContent).not.toContain('NRC0G-944805165-2026');
            expect(c.button('确认并开始双签')?.disabled).toBe(false);
            await c.click('确认并开始双签');
            expect(document.querySelector('[data-wallet-signature]')?.textContent).toContain('新钱包接收确认'); expect(c.passkey).not.toHaveBeenCalled();
            await c.click('回扫当前签名');
            expect(document.querySelector('[data-wallet-signature]')?.textContent).toContain('本人旧钱包交易签名'); expect(c.passkey).toHaveBeenCalledTimes(1);
            await c.click('回扫当前签名');
            expect(c.calls.filter(value => value === 'submit')).toHaveLength(1); expect(c.logout).toHaveBeenCalledTimes(1);
            expect(c.info).toHaveBeenCalledWith('请使用新钱包重新登录本机构');
        } finally { await c.disposePage(); }
    }, 15000);
    walletBrowserTest('取消新钱包签名后迟到回扫，不消耗Passkey也不提交交易', async () => {
        const c = await walletPage();
        try {
            await c.open(); await c.click('确认并开始双签'); await c.click('取消当前签名');
            await c.React.act(async () => c.lateScan()); await c.flush();
            expect(c.passkey).not.toHaveBeenCalled(); expect(c.calls).not.toContain('submit'); expect(c.logout).not.toHaveBeenCalled();
        } finally { await c.disposePage(); }
    }, 15000);

    walletBrowserTest('离开页面销毁签名弹窗，旧摄像头迟到回调不能继续旧登录的操作', async () => {
        const c = await walletPage();
        try {
            await c.open(); await c.click('确认并开始双签'); await c.leave();
            await c.React.act(async () => c.lateScan()); await c.flush();
            expect(c.passkey).not.toHaveBeenCalled(); expect(c.calls).not.toContain('submit'); expect(c.logout).not.toHaveBeenCalled();
        } finally { await c.disposePage(); }
    }, 15000);
    walletBrowserTest('真实治理入口依原岗位选择发起人，提交成功仍显示原钱包并等待执行', async () => {
        const c = await walletPage('GOVERNANCE');
        try {
            await c.open(); expect(c.button('确认并签名提案')?.disabled).toBe(false);
            await c.click('确认并签名提案'); await c.click('回扫当前签名');
            expect(document.querySelector('[data-wallet-signature]')?.textContent).toContain('机构治理提案签名');
            await c.click('回扫当前签名');
            expect(c.logout).not.toHaveBeenCalled(); expect(c.success).toHaveBeenCalledWith('钱包更换提案已提交，等待原岗位投票通过并执行');
            expect(c.snapshot.members[0].signerId).toBe(account(3));
            const { encodeSs58 } = await import('../codes/ss58');
            expect(document.body.textContent).toContain(encodeSs58(account(3)));
            expect(document.body.textContent).not.toContain(encodeSs58(account(4)));
            expect(document.querySelector('input[aria-label="新钱包账户"]')).toBeNull();
        } finally { await c.disposePage(); }
    }, 15000);

    walletBrowserTest('PQ长二维码以校验过的原生分帧轮播，切换请求不展示旧帧或超大单码', async () => {
        const c=await walletPage();
        try {
            const captured: string[]=[];
            vi.doMock('antd',async importOriginal=>{
                const actual=await importOriginal<typeof import('antd')>();
                return {...actual,QRCode:({value}:{value:string})=>{captured.push(value);return c.React.createElement('output',{'data-qr-frame':value});}};
            });
            const {CitizenSignaturePanel}=await import('../qr/CitizenSignaturePanel');
            const text='signed-institution-wallet-request-'.repeat(220), frames=await transportFrames(text);
            c.fetcher.mockImplementationOnce(async(path,init)=>{
                expect(path).toBe('/api/qr/split');expect(JSON.parse(String(init?.body))).toBe(text);
                return new Response(JSON.stringify({code:0,data:frames}));
            });
            const props={framed:true,qrValue:text,onDetected:vi.fn()};
            await c.React.act(async()=>c.root.render(c.React.createElement(CitizenSignaturePanel,props)));await c.flush();
            expect(document.body.textContent).toContain(`1 / ${frames.length} 帧`);
            expect(document.querySelector('[data-qr-frame]')?.getAttribute('data-qr-frame')).toBe(frames[0]);
            await c.click('暂停轮播');await c.click('下一帧');
            expect(document.querySelector('[data-qr-frame]')?.getAttribute('data-qr-frame')).toBe(frames[1]);
            await c.React.act(async()=>c.root.render(c.React.createElement(CitizenSignaturePanel,{...props,qrValue:'short-new-request'})));await c.flush();
            expect(captured[captured.length - 1]).toBe('short-new-request');
            expect(captured.every(value=>new TextEncoder().encode(value).length<=2331)).toBe(true);
        } finally {vi.doUnmock('antd');await c.disposePage();}
    },15000);

    walletBrowserTest('原签名调用未启用机构选项时保持旧返回形状，不夹带响应原文', async () => {
        const c=await walletPage();
        try {
            const {useChainSign}=await import('../qr/useChainSign');
            let flow!: ReturnType<typeof useChainSign>;
            function ExistingConsumer(){ flow=useChainSign();return flow.chainSignModal; }
            await c.React.act(async()=>c.root.render(c.React.createElement(ExistingConsumer)));
            const request=JSON.stringify({p:'QR_V1',k:1,i:'original-sign-request-test',e:Math.floor(Date.now()/1000)+60,b:{a:1,g:0,u:encode(bytes(account(3))),d:encode([1,2,3])}});
            let signed!: Promise<import('../qr/useChainSign').ChainSignResult>;
            await c.React.act(async()=>{signed=flow.signChain('original-sign-request-test',request);});
            await c.click('回扫当前签名');
            const result=await signed;
            expect(result.accountId).toBe(account(3));expect(result.signature).toBe(c.proof);expect(result).not.toHaveProperty('response');
            expect(c.passkey).not.toHaveBeenCalled();expect(c.calls).not.toContain('submit');
        } finally {await c.disposePage();}
    },15000);

});

it('旧名单字段在认证与签名前拒绝，不自动转换为纯岗位动作', async () => {
  const {prepareInstitutionGovernance}=await import('../institutions/governance');
  const client=await import('../authentication/passkeyClient');
  const passkey=vi.spyOn(client,'assertPasskey').mockResolvedValue('not-consumed');
  const fetcher=vi.spyOn(globalThis,'fetch');
  try{
    passkey.mockClear();
    await expect(prepareInstitutionGovernance(auth,{cidNumber:state.cidNumber,proposerRoleCode:'GOV',admins:[]} as never)).rejects.toThrow('停用');
    expect(passkey).not.toHaveBeenCalled();expect(fetcher).not.toHaveBeenCalled();
  }finally{fetcher.mockRestore();passkey.mockRestore();}
});

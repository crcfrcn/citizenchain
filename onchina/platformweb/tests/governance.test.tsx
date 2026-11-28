// 验证机构岗位、任职和代表人治理的载荷、身份及真实DOM交互。

import { describe, expect, it, vi } from 'vitest';
import { dateFromDay, dayFromDate, governanceProposers, buildGovernance, representativeChanges, validateAssignments, getGovernanceState, type GovernanceState } from '../institutions/governance';
const anchor = 'CN220-CTZN2-198805200-2026', signer = '0x' + '22'.repeat(32);
const role = { roleCode: 'GOV', roleName: '治理委员', active: true, fixed: false, assignmentLocked: false, termRequired: false, seats: 2, minAssignments: 0, maxAssignments: 2,
    permissions: [{ moduleTag: 'pri-mgmt', actionCode: 3, operation: 'PROPOSE' as const }],
    assignments: [{ personCid: anchor, termStart: 0, termEnd: 0, active: true, effective: true, sourceLabel: '机构内部治理', sourceRef: '0x01' }] };
const state: GovernanceState = { cidNumber: 'fixture-institution', blockHash: '0x01', blockNumber: 1, currentDay: 20000, moduleTag: 'pri-mgmt', legalRoleCode: 'LR',maxSeats: 64,
    proposerRoles:[{roleCode:'GOV',roleName:'治理委员'}],registrationRoles:[],registrationStateHash:'0x'+'ab'.repeat(32),governanceThreshold:2,
    roles: [role, { ...role, roleCode: 'LR', roleName: '法定代表人', fixed: true, assignmentLocked: true, seats:1,maxAssignments: 1 }],
    representative: { familyName: '测', givenName: '试', personCid: anchor } };
const auth = { bindingRevision:'1', personCid:anchor,roleCode:'GOV',setupRequired: false, accessToken: 'fixture-session', accountId: signer, institutionCidNumber: state.cidNumber, institutionCode: 'SFGY', familyName: '测', givenName: '试' };
describe('独立治理页载荷及身份边界', () => {
    it('发起岗位限定本次所选，任职直接使用人员CID，其他机构只读', () => {
        expect(governanceProposers(auth, state).map(r => r.roleCode)).toEqual(['GOV']);
        expect(governanceProposers({ ...auth, institutionCidNumber: 'another' } as never, state)).toEqual([]);
        expect(governanceProposers({ ...auth, roleCode: 'OTHER' } as never, state)).toEqual([]);
        const input = buildGovernance(state, { assignmentChanges: [{ roleCode: 'GOV', assignments: [{ personCid: anchor, termStart: 0, termEnd: 0 }] }] }, 'GOV');
        expect(input.assignmentChanges?.[0].assignments[0].personCid).toBe(anchor);
        expect(input).not.toHaveProperty('admins');
    });
    it('岗位创建、改名和删除不夹带其他功能，固定岗位及既有权限不可改写', () => {
        const create = buildGovernance(state, { roleMutations: [{ mutation: 'CREATE', roleName: '新岗位', seats:2, permissions: role.permissions, assignments: [] }] }, 'GOV');
        expect(create.roleMutations?.[0]).not.toHaveProperty('roleCode');
        expect(create).not.toHaveProperty('admins');
        expect(create.assignmentChanges).toBeUndefined();
        expect(create.representativeCid).toBeUndefined();
        expect(() => buildGovernance(state, { roleMutations: [{ mutation: 'RENAME', roleCode: 'GOV', roleName: '' }] }, 'GOV')).toThrow('名称');
        expect(() => buildGovernance(state, { roleMutations: [{ mutation: 'RENAME', roleCode: 'LR', roleName: '新名称' }] }, 'GOV')).toThrow('不可');
        expect(() => buildGovernance(state, { roleMutations: [{ mutation: 'RENAME', roleCode: 'GOV', roleName: '新名称', permissions: role.permissions }] }, 'GOV')).toThrow('只允许');
        expect(() => buildGovernance(state, { admins: [] } as never, 'GOV')).toThrow('名单');
        expect(() => buildGovernance(state, {}, 'GOV')).toThrow('没有');
        expect(buildGovernance(state, { roleMutations: [{ mutation: 'DELETE', roleCode: 'GOV' }] }, 'GOV').roleMutations?.[0].mutation).toBe('DELETE');
    });
    it('任职接受有效人员CID，人数和任期规则不能绕过，FRG等专属岗位不走独立入口', () => {
        expect(() => validateAssignments(state, role, [{ personCid: 'outsider', termStart: 0, termEnd: 0 }])).toThrow('CID');
        expect(() => validateAssignments(state, role, [{ personCid: anchor, termStart: 1, termEnd: 2 }])).toThrow('任期');
        expect(() => validateAssignments(state, { ...role, termRequired: true }, [{ personCid: anchor, termStart: 2, termEnd: 1 }])).toThrow('任期');
        expect(() => validateAssignments(state, { ...role, minAssignments: 1 }, [])).toThrow('人数');
        const locked = { ...state, roles: [{ ...role, assignmentLocked: true }] };
        expect(() => buildGovernance(locked, { assignmentChanges: [{ roleCode: 'GOV', assignments: [] }] }, 'GOV')).toThrow('专属');
    });
    it('法定代表人更换和解除同步LR任职，不产生管理员名单变更', () => {
        const set = representativeChanges(state, anchor,'测','试');
        expect(set.representativeCid).toBe(anchor);
        expect(set.assignmentChanges?.[0]).toEqual({ roleCode: 'LR', assignments: [{ personCid: anchor, termStart: 0, termEnd: 0 }] });
        expect(buildGovernance(state, set, 'GOV')).not.toHaveProperty('admins');
        const clear = representativeChanges(state, null);
        expect(clear.clearLegalRepresentative).toBe(true);
        expect(clear.assignmentChanges?.[0].assignments).toEqual([]);
        expect(() => representativeChanges(state, 'outsider')).toThrow('CID');
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
        state?: GovernanceState;
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
                return new Response(JSON.stringify({ code: 0, data: options.state ?? state }));
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
        const changeRole = () => React.act(async () => root.render(React.createElement(View, { auth: {...auth,roleCode:"OTHER"},cidNumber:state.cidNumber })));
        return { changeRole, changeSession, assertion, signChain, submit, fetcher, click, confirm, button, flush, unmount, releaseKey: () => releaseKey('fixture-assertion'), dispose: async () => { await unmount(); fetcher.mockRestore(); vi.doUnmock('../authentication/passkeyClient'); vi.doUnmock('../qr/useChainSign'); vi.doUnmock('../controls/notice'); dom.window.close(); vi.unstubAllGlobals(); } };
    }
    browserTest('原生权限入口复用治理审阅，核对前不发链请求', async () => {
        const c = await mount({ state: { ...state, policyAvailable: true, roles: [...state.roles, { ...role, roleCode: 'R_POLICY', roleName: '业务岗位' }] } });
        try {
            await c.click('权限');
            expect(document.body.textContent).toContain('原生权限政策');
            await c.click('核对变更');
            expect(document.body.textContent).toContain('确认岗位原生权限政策');
            expect(c.signChain).not.toHaveBeenCalled();
            expect(c.fetcher.mock.calls.some(([path]) => String(path).endsWith('/prepare'))).toBe(false);
        } finally { await c.dispose(); }
    });
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
    browserTest('同令牌切换岗位也销毁草稿并拒绝迟到认证', async()=>{
        const c=await mount({delayedPasskey:true});
        try {await c.confirm();expect(c.assertion).toHaveBeenCalledTimes(1);await c.changeRole();await c.flush();c.releaseKey();await c.flush();expect(c.signChain).not.toHaveBeenCalled();expect(c.fetcher.mock.calls.some(([path])=>String(path).endsWith('/prepare'))).toBe(false);}
        finally {await c.dispose();}
    },15000);
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
            expect(document.querySelector('[data-identity-cid="'+anchor+'"]')).not.toBeNull();
            expect(c.button('新增岗位')).toBeUndefined();
            await c.click('发起解除');
            expect(document.body.textContent).toContain('确认解除法定代表人');
            expect(document.body.textContent).toContain('唯一LR任职');
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
describe('通用链签名二维码传输回归', () => {
    const account = (n: number) => '0x' + n.toString(16).padStart(2, '0').repeat(32);
    const bytes = (hex: string) => Uint8Array.from(Buffer.from(hex.slice(2), 'hex'));
    const encode = (value: ArrayLike<number>) => Buffer.from(value).toString('base64url');
    const browserTest = process.env.ONCHINA_TEST_DOM ? it : it.skip;
    async function signaturePage() {
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
        const proof = '0x' + 'ab'.repeat(64), calls: string[] = [];
        const fetcher = vi.spyOn(globalThis, 'fetch').mockImplementation(async path => { calls.push(String(path)); throw new Error('unexpected request'); });
        const client = await import('../authentication/passkeyClient');
        const passkey = vi.spyOn(client, 'assertPasskey').mockResolvedValue('unused');
        vi.doMock('@gmb/scanner-react', async importOriginal => ({...await importOriginal<typeof import('@gmb/scanner-react')>(), ScannerView: () => null}));
        vi.doMock('../qr/CitizenSignatureModal', () => ({ CitizenSignatureModal: (props: { open: boolean; title: string; qrValue: string; onDetected: (raw: string) => void; onCancel: () => void }) => {
            if (!props.open) return null;
            const scan = () => { const request = JSON.parse(props.qrValue); props.onDetected(JSON.stringify({ p: 'QR_V1', k: 2, i: request.i, e: request.e, b: { u: request.b.u, s: encode(bytes(proof)) } })); };
            return React.createElement('section', null, props.title, React.createElement('button', { onClick: scan }, '回扫当前签名'));
        } }));
        const { createRoot } = await import('react-dom/client');
        const root = createRoot(document.getElementById('root')!);
        const flush = () => React.act(async () => { await new Promise(resolve => setTimeout(resolve, 30)); });
        const click = async (text: string) => { const item = [...document.querySelectorAll('button')].find(b => b.textContent?.replace(/\s/g, '') === text); expect(item, text).toBeTruthy(); await React.act(async () => item!.click()); await flush(); };
        return { React, root, proof, calls, fetcher, passkey, flush, click, disposePage: async () => {
            await React.act(async () => root.unmount()); fetcher.mockRestore(); passkey.mockRestore();
            vi.doUnmock('@gmb/scanner-react'); vi.doUnmock('../qr/CitizenSignatureModal'); dom.window.close(); vi.unstubAllGlobals(); vi.resetModules();
        } };
    }
    async function transportFrames(text: string): Promise<string[]> {
        const {QrSession}=await import('@gmb/scanner-react');const encoder=await QrSession.open('encode',text);
        const first=encoder.next().text;const count=first.split('/').length===3?Number(first.split('/')[1]!.split('-')[1]):1;
        const frames=[first,...Array.from({length:count-1},()=>encoder.next().text)];encoder.close();return frames;
    }
    it('PQ长请求由公共UR恢复原文，拒绝混帧和篡改，丢帧重复不提前完成',async()=>{
        const {QrSession}=await import('@gmb/scanner-react');const text='完整链签名请求'.repeat(600),frames=await transportFrames(text);
        const receiver=await QrSession.open('decode');
        try{
            expect(receiver.next(frames[0]!).complete).toBe(false);expect(receiver.next(frames[0]!).complete).toBe(false);
            const other=await transportFrames(text+'tamper');expect(()=>receiver.next(other[0]!)).toThrow();
            expect(()=>receiver.next(frames[1]!.slice(0,-1)+'!')).toThrow();
            let complete='';for(const frame of frames.slice(1).reverse()){const value=receiver.next(frame);if(value.complete)complete=value.text;}
            expect(complete).toBe(text);
        }finally{receiver.close();}
    });
    browserTest('PQ长二维码自动轮播，切换请求不展示旧帧，且不逐帧请求后端',async()=>{
        const c=await signaturePage();const {QrSession}=await import('@gmb/scanner-react');
        const next=vi.spyOn(QrSession.prototype,'next');
        try{
            const {CitizenSignaturePanel}=await import('../qr/CitizenSignaturePanel');
            const text='signed-chain-request-'.repeat(220),props={qrValue:text,onDetected:vi.fn()};
            await c.React.act(async()=>c.root.render(c.React.createElement(CitizenSignaturePanel,props)));await c.flush();await c.flush();
            expect(document.querySelector('svg path[fill="#007A74"]')).not.toBeNull();
            expect(document.body.textContent).toContain('请持续扫码');
            const first=next.mock.results.filter(value=>value.type==='return').slice(-1)[0]!.value.text;
            expect(first.startsWith('UR:CITIZEN/')).toBe(true);
            await c.click('暂停');await c.click('下一帧');
            const second=next.mock.results.filter(value=>value.type==='return').slice(-1)[0]!.value.text;
            expect(second).not.toBe(first);
            await c.React.act(async()=>c.root.render(c.React.createElement(CitizenSignaturePanel,{...props,qrValue:'short-new-request'})));await c.flush();
            const latest=next.mock.results.filter(value=>value.type==='return').slice(-1)[0]!.value.text;
            const receiver=await QrSession.open('decode');try{expect(receiver.next(latest).text).toBe('short-new-request')}finally{receiver.close()}
            expect(c.calls).toEqual([]);
            expect(next.mock.results.filter(value=>value.type==='return').every(value=>new TextEncoder().encode(value.value.text).length<=2048)).toBe(true);
        }finally{next.mockRestore();await c.disposePage();}
    },15000);

    browserTest('原签名调用未启用机构选项时保持旧返回形状，不夹带响应原文', async () => {
        const c=await signaturePage();
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
    for (const retired of [{ admins: [] }, { walletChanges: {} }, { mode: 'SELF', newWallet: signer }]) {
      await expect(prepareInstitutionGovernance(auth,{cidNumber:state.cidNumber,proposerRoleCode:'GOV',...retired} as never)).rejects.toThrow('停用');
      expect(() => buildGovernance(state,{assignmentChanges:[{roleCode:'GOV',assignments:[]}],...retired} as never,'GOV')).toThrow();
    }
    expect(passkey).not.toHaveBeenCalled();expect(fetcher).not.toHaveBeenCalled();
  }finally{fetcher.mockRestore();passkey.mockRestore();}
});

it('人员CID年份接受规范u32边界，拒绝溢出、多余前导零和空白',()=>{
  for(const year of ['0000','2026','10000','4294967295']) {
    expect(()=>validateAssignments(state,role,[{personCid:`CN220-CTZN2-198805200-${year}`,termStart:0,termEnd:0}])).not.toThrow();
  }
  for(const year of ['202','02026','00000','4294967296','10000000000','2026 ']) {
    expect(()=>validateAssignments(state,role,[{personCid:`CN220-CTZN2-198805200-${year}`,termStart:0,termEnd:0}])).toThrow('CID');
  }
  expect(()=>validateAssignments({...state,moduleTag:'pub-mgmt'},role,[{personCid:'CN220-NATP2-198805200-10000',termStart:0,termEnd:0}])).toThrow('CID');
});

describe('原生政策和岗位属性接线', () => {
    const dynamic = { ...role, roleCode: 'R_POLICY' };
    const current: GovernanceState = { ...state, policyAvailable: true, roles: [dynamic, state.roles[1]] };
    it('配置属性与目标任职同案，停用必须显式清空，固定岗位拒绝', () => {
        const configuration = { mutation: 'CONFIGURE' as const, roleCode: dynamic.roleCode, termRequired: false, seats: 2, active: false };
        expect(() => buildGovernance(current, { roleMutations: [configuration] }, 'GOV')).toThrow('清空');
        expect(buildGovernance(current, { roleMutations: [configuration], assignmentChanges: [{ roleCode: dynamic.roleCode, assignments: [] }] }, 'GOV').roleMutations?.[0].active).toBe(false);
        expect(() => buildGovernance(current, { roleMutations: [{ ...configuration, roleCode: 'LR' }] }, 'GOV')).toThrow('固定');
        expect(() => buildGovernance({ ...current, policyAvailable: false }, { roleMutations: [{ ...configuration, active: true }] }, 'GOV')).toThrow('配置无效');
    });
    it('政策完整替换非空权限并绑定原指纹，拒绝混入人事及陈旧状态', () => {
        const policy = { permissionChanges: [{ roleCode: dynamic.roleCode, permissions: dynamic.permissions }], threshold: 3, expectedStateHash: current.registrationStateHash };
        expect(buildGovernance(current, { policy }, 'GOV').policy).toEqual(policy);
        expect(() => buildGovernance(current, { policy: { ...policy, permissionChanges: [{ roleCode: dynamic.roleCode, permissions: [] }] } }, 'GOV')).toThrow('不能为空');
        expect(() => buildGovernance(current, { policy: { ...policy, expectedStateHash: '0x' + 'cd'.repeat(32) } }, 'GOV')).toThrow('指纹');
        expect(() => buildGovernance(current, { policy, assignmentChanges: [{ roleCode: dynamic.roleCode, assignments: [] }] }, 'GOV')).toThrow('人事');
        expect(() => buildGovernance(current, { policy: { ...policy, threshold: 1 } }, 'GOV')).toThrow('阈值');
    });
});

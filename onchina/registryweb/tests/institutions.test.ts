// 验证注册局管理员登记请求和提交状态的准确传递。

import { describe, expect, it, vi } from 'vitest';
import { readFileSync } from 'node:fs';
import { dirname, join, normalize } from 'node:path';
import { SECURITY_GRANT_HEADER, PASSKEY_ASSERTION_HEADER, passkeySubmitHeaders, securityGrantSubmitHeaders, commitAdminAction, rustConst } from '@gmb/onchina-platform/testing';

const regPerson=(n:number)=>`CN220-CTZN2-${String(198805200+n).padStart(9,'0')}-2026`;
const regIdentity={personCid:regPerson(8),bindingRevision:'1',roleCode:'R_REGISTRAR',actorInstitutionCid:'ZS001-FRG07-249474503-2026'};

describe('机构本地资料与照片写入', () => {
  it('保存资料和上传照片只取一次Passkey，不请求冷签prepare或commit', async () => {
    const { updateInstitution, uploadLegalRepresentativePhoto } = await import('../private/api');
    const { assertPasskey } = await import('@gmb/onchina-platform/authentication/passkeyClient');
    const auth = { setupRequired: false, accessToken: 'fixture-session' } as never;
    const fetcher = vi.spyOn(globalThis, 'fetch').mockImplementation(async () => new Response(JSON.stringify({ code: 0, data: {} })));
    try {
      for (const run of [() => updateInstitution(auth, 'fixture', { cidFullName: 'fixture' }), () => uploadLegalRepresentativePhoto(auth, new File(['photo'], 'photo.png', { type: 'image/png' }))]) {
        fetcher.mockClear(); vi.mocked(assertPasskey).mockClear();
        await run();
        expect(assertPasskey).toHaveBeenCalledTimes(1);
        expect(fetcher).toHaveBeenCalledTimes(1);
        const [path, init] = fetcher.mock.calls[0];
        expect(String(path)).toMatch(/^\/api\/institutions\//);
        const headers = new Headers(init?.headers);
        expect(headers.get(PASSKEY_ASSERTION_HEADER)).toBe('fake-passkey-assertion');
        expect(headers.has(SECURITY_GRANT_HEADER)).toBe(false);
      }
    } finally { fetcher.mockRestore(); vi.mocked(assertPasskey).mockClear(); }
  });
});

describe('注册局正常机构浏览入口', () => {
  it('按后端能力保留业务和两个注册局入口，不再显示人员名单', async () => {
    vi.resetModules();
    const React = await import('react');
    const { renderToStaticMarkup } = await import('react-dom/server');
    vi.doMock('../citizens/CitizensView', () => ({ CitizensView: () => React.createElement('div', { 'data-page': 'citizens' }) }));
    try {
      const { RegistryWorkspace } = await import('../controls/Workspace');
      const { EMPTY_CAPABILITIES } = await import('@gmb/onchina-platform/workspace/modules');
      const capabilities = { ...EMPTY_CAPABILITIES, canViewCitizens: true, canViewCityRegistry: true, canViewFederalRegistry: true };
      const props = { auth: { accountId: 'fixture-account', institutionCidNumber: 'fixture-institution', institutionCode: 'FRG' } as never, capabilities, cidMeta: null, setCidMeta: () => {} };
      const ready = renderToStaticMarkup(React.createElement(RegistryWorkspace, props));
      expect(ready).toContain('data-page="citizens"');
      expect(ready).toContain('>市注册局</button>'); expect(ready).toContain('>联邦注册局</button>');
      expect(ready).not.toContain('管理员');
      const denied = renderToStaticMarkup(React.createElement(RegistryWorkspace, { ...props, capabilities: EMPTY_CAPABILITIES }));
      expect(denied).toContain('当前身份没有可访问的工作台入口'); expect(denied).not.toContain('data-page');
    } finally { vi.doUnmock('../citizens/CitizensView'); vi.resetModules(); }
  });
});

describe('机构操作记录读取', () => {
  const browserTest = process.env.ONCHINA_TEST_DOM ? it : it.skip;
  browserTest('按targetCid查询，失败显示重试而非零条记录，重试后显示真实记录', async () => {
    vi.resetModules();
    const { createRequire } = await import('node:module');
    const { isAbsolute } = await import('node:path');
    if (!isAbsolute(process.env.ONCHINA_TEST_DOM!)) throw new Error('DOM测试运行库必须是显式绝对路径');
    const { JSDOM } = createRequire(import.meta.url)(process.env.ONCHINA_TEST_DOM!);
    const dom = new JSDOM('<div id="root"></div>', { url: 'https://onchina.example.invalid/' });
    dom.window.matchMedia = () => ({ matches: false, addListener() {}, removeListener() {}, addEventListener() {}, removeEventListener() {} });
    for (const name of ['window', 'document', 'navigator', 'HTMLElement', 'Element', 'ShadowRoot', 'SVGElement']) vi.stubGlobal(name, dom.window[name]);
    vi.stubGlobal('getComputedStyle', (element: Element) => dom.window.getComputedStyle(element));
    vi.stubGlobal('IS_REACT_ACT_ENVIRONMENT', true);
    vi.doMock('@gmb/onchina-platform/institutions/institutionLabels', () => ({ useInstitutionCodeLabels: () => ({}) }));
    const React = await import('react');
    const { createRoot } = await import('react-dom/client');
    const root = createRoot(document.getElementById('root')!);
    const fetcher = vi.spyOn(globalThis, 'fetch')
      .mockResolvedValueOnce(new Response('Failed to deserialize query string', { status: 400 }))
      .mockResolvedValueOnce(new Response(JSON.stringify({ code: 0, data: [{ seq: 1, action: 'CITIZEN_CREATE', actorAccountId: '', detail: {}, createdAt: '2026-01-01T00:00:00Z' }] })));
    try {
      const { OperationRecords } = await import('../government/OperationRecords');
      await React.act(async () => root.render(React.createElement(OperationRecords, { auth: { accessToken: 'fixture-session' } as never, cidNumber: 'fixture & cid' })));
      expect(document.body.textContent).toContain('请求参数错误');
      expect(document.body.textContent).not.toContain('操作记录(0)');
      const retry = [...document.querySelectorAll('button')].find(button => button.textContent?.replace(/\s/g, '') === '重试')!;
      await React.act(async () => retry.click());
      expect(document.body.textContent).toContain('操作记录(1)');
      expect(document.body.textContent).toContain('注册局录入公民');
      expect(fetcher).toHaveBeenCalledTimes(2);
      for (const [path, init] of fetcher.mock.calls) {
        const url = new URL(String(path), 'https://example.invalid');
        expect([...url.searchParams]).toEqual([['targetCid', 'fixture & cid'], ['limit', '1000']]);
        expect(new Headers(init?.headers).get('authorization')).toBe('Bearer fixture-session');
      }
    } finally {
      await React.act(async () => root.unmount());
      fetcher.mockRestore(); dom.window.close();
      vi.doUnmock('@gmb/onchina-platform/institutions/institutionLabels');
      vi.unstubAllGlobals(); vi.resetModules();
    }
  });
});

describe('注册局登记真实请求', () => {
const auth = { ...regIdentity, accessToken: 'test-session', accountId: '0x' + '11'.repeat(32) } as never;
it('注册局登记准备等待一次Passkey，保留请求且失败不重试', async () => {
    const { assertPasskey } = await import('@gmb/onchina-platform/authentication/passkeyClient');
    const { prepareInstitutionGovernance } = await import('@gmb/onchina-platform/institutions/governance');
    const { prepareRegisterInstitutionRoles } = await import('../controls/registration');
    const governance = { cidNumber: 'fixture-institution', proposerRoleCode: 'REPRESENTATIVE' };
    const registration = { expectedStateHash: '0x' + 'ab'.repeat(32), cidNumber: 'fixture-institution', actorRoleCode: 'REGISTRY-ROLE', assignmentChanges: [{ roleCode: 'REGISTRY-ROLE', assignments: [{ personCid: regPerson(1), termStart: 0, termEnd: 0 }] }] };
    const operations = [{ path: '/api/admin/institution/roles/register/prepare', payload: registration, run: () => prepareRegisterInstitutionRoles(auth, registration) }];
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
describe('正常岗位配置请求参数衔接', () => {
  it('正常配置保留原URL和Bearer，透传岗位码与任职载荷', async () => {
    const { prepareRegisterInstitutionRoles } = await import('../controls/registration');
    const { assertPasskey } = await import('@gmb/onchina-platform/authentication/passkeyClient');
    const auth = { ...regIdentity, accessToken: 'test-session' } as never;
    const input = { expectedStateHash: '0x' + 'ab'.repeat(32), cidNumber: 'target-fixture-cid', actorRoleCode: 'REGISTRY-ROLE', assignmentChanges: [{ roleCode: 'REGISTRY-ROLE', assignments: [{ personCid: regPerson(1), termStart: 0, termEnd: 0 }] }] };
    const fetcher = vi.spyOn(globalThis, 'fetch').mockResolvedValue(new Response(JSON.stringify({ code: 0, data: {} })));
    try {
      vi.mocked(assertPasskey).mockClear();
      await prepareRegisterInstitutionRoles(auth, input);
      expect(assertPasskey).toHaveBeenCalledTimes(1);
      expect(fetcher).toHaveBeenCalledTimes(1);
      const [path, init] = fetcher.mock.calls[0];
      expect(path).toBe('/api/admin/institution/roles/register/prepare');
      expect(init?.method).toBe('POST');
      expect(new Headers(init?.headers).get('authorization')).toBe('Bearer test-session');
      expect(new Headers(init?.headers).get(PASSKEY_ASSERTION_HEADER)).toBe('fake-passkey-assertion');
      expect(JSON.parse(String(init?.body))).toEqual(input);
    } finally { fetcher.mockRestore(); }
  });
});

// 正常机构浏览与详情拆页共用岗位、任职、法代页面。
describe('机构详情非管理员治理入口', () => {
  it('公权和私权详情保留正常治理页面，删除钱包维护入口', async () => {
    vi.resetModules();
    const React = await import('react');
    const { renderToStaticMarkup } = await import('react-dom/server');
    const detail = { institution: { cidNumber: 'fixture-institution', institutionCode: 'SFGY', category: 'PUBLIC_INSTITUTION', status: 'ACTIVE', cidFullName: '测试机构', createdAt: '2026-01-01T00:00:00Z' }, accounts: [] };
    vi.doMock('@gmb/onchina-platform/codes/cache', () => ({ institutionDetailCacheKey: () => 'fixture-key', readCachedInstitutionDetail: () => detail, writeCachedInstitutionDetail: vi.fn() }));
    vi.doMock('@gmb/onchina-platform/institutions/institutionLabels', () => ({ useInstitutionCodeLabels: () => ({}) }));
    vi.doMock('@gmb/onchina-platform/qr/useChainSign', () => ({ useChainSign: () => ({ signChain: vi.fn(), chainSignModal: null }), submitChainSign: vi.fn() }));
    try {
      const { GovDetailPage } = await import('../government/GovDetailPage');
      const { PrivateDetailLayout } = await import('../private/PrivateDetailLayout');
      const auth = { ...regIdentity, accessToken: 'fixture-session', accountId: 'fixture-account', institutionCidNumber: detail.institution.cidNumber, institutionCode: 'SFGY' } as never;
      const official = renderToStaticMarkup(React.createElement(GovDetailPage, { auth, cidNumber: detail.institution.cidNumber, canWrite: true, initialActiveKey: 'governance' }));
      const privatePage = renderToStaticMarkup(React.createElement(PrivateDetailLayout, { auth, detail: { ...detail, institution: { ...detail.institution, category: 'PRIVATE_INSTITUTION' } } as never, canWrite: true, loading: false, onReload: () => {} }));
      for (const html of [official, privatePage]) {
        for (const label of ['岗位管理', '任职管理', '法定代表人', '机构治理', '账户列表']) expect(html).toContain(label);
        expect(html).not.toContain('岗位操作');
        expect(html).not.toContain('岗位权限集合');
        expect(html).not.toContain('提案发起岗位码');
        expect(html).not.toContain('岗位码（改名/删除）');
      }
      expect(official).not.toContain('管理员列表');
      expect(official).not.toContain('治理管理员集合');
      expect(official).toContain('岗位与任职请使用对应治理页面');
      expect(official).not.toContain('注册局直接登记管理员');
      expect(official).not.toContain('管理员集合');
      expect(official).toContain('首次治理配置');
      const readOnly = renderToStaticMarkup(React.createElement(GovDetailPage, { auth, cidNumber: detail.institution.cidNumber, canWrite: false }));
      for (const label of ['岗位管理', '任职管理', '法定代表人']) expect(readOnly).toContain(label);
      expect(readOnly).not.toContain('>机构治理<');
    } finally {
      vi.doUnmock('@gmb/onchina-platform/codes/cache');
      vi.doUnmock('@gmb/onchina-platform/institutions/institutionLabels');
      vi.doUnmock('@gmb/onchina-platform/qr/useChainSign');
      vi.resetModules();
    }
  });
});


// 第4步使用原真实HTTP包装器、Passkey和提交入口；无需新建JSON夹具或DOM配置。
import type { GovernanceState, InstitutionRegistrationInput } from '@gmb/onchina-platform/institutions/governance';
const regAccount = (n: number) => '0x' + n.toString(16).padStart(2, '0').repeat(32);
const regCid = 'NX013-CREGN-937676656-2026';
function registrationState(): GovernanceState {
  const target = (n: number) => ({ personCid: regPerson(n), termStart: 0, termEnd: 0, active: true, effective: true, sourceLabel: '机构内部治理', sourceRef: '0xff0080' });
  return { cidNumber: regCid, blockHash: '0x' + '10'.repeat(32), blockNumber: 10, currentDay: 20000, moduleTag: 'pub-mgmt', legalRoleCode: 'LR', representative: null,maxSeats:64,proposerRoles:[{roleCode:"R_GOV",roleName:"机构治理岗"}],
    registrationStateHash: '0x' + 'ab'.repeat(32), governanceThreshold: 2, registrationRoles: [{ roleCode: 'R_REGISTRAR', roleName: '登记办理岗' }],
    roles: [
      { seats:1,roleCode: 'LR', roleName: '法定代表人', active: true, fixed: true, assignmentLocked: true, termRequired: false, minAssignments: 0, maxAssignments: 1, permissions: [], assignments: [] },
      { seats:2,roleCode: 'R_GOV', roleName: '机构治理岗', active: true, fixed: false, assignmentLocked: false, termRequired: false, minAssignments: 0, maxAssignments: 2,
        permissions: ['PROPOSE', 'VOTE'].map(operation => ({ moduleTag: 'pub-mgmt', actionCode: 3, operation: operation as 'PROPOSE' | 'VOTE' })), assignments: [target(1), target(2)] },
      { seats:2,roleCode: 'R_BUSINESS', roleName: '公民身份办理岗', active: true, fixed: false, assignmentLocked: false, termRequired: false, minAssignments: 0, maxAssignments: 2,
        permissions: [{ moduleTag: 'cit-id', actionCode: 0, operation: 'PROPOSE' }], assignments: [target(3)] },
    ] };
}
function roleRegistration(state: GovernanceState): InstitutionRegistrationInput {
  return { cidNumber: state.cidNumber, expectedStateHash: state.registrationStateHash!, actorRoleCode: 'R_REGISTRAR',
    assignmentChanges: [{ roleCode: 'R_BUSINESS', assignments: [{ personCid: regPerson(2), termStart: 0, termEnd: 0 }] }] };
}
function registrationAfter(before: GovernanceState): GovernanceState {
  const state = structuredClone(before); state.blockNumber++; state.registrationStateHash = '0x' + 'cd'.repeat(32);
  state.roles[2].assignments = [{ personCid: regPerson(2), termStart: 0, termEnd: 0, active: true, effective: true, sourceLabel: '注册局', sourceRef: '0x' + '12'.repeat(32) }];
  return state;
}

describe('独立首次治理配置',()=>{
  it('显式配置岗位和阈值而不修改任何钱包；已有配置不能重置',async()=>{
    const {buildInstitutionConfiguration}=await import('@gmb/onchina-platform/institutions/governance');
    const state={...registrationState(),governanceThreshold:null,roles:[registrationState().roles[0]]};
    const initialRoles=[1,2].map(n=>({mutation:'CREATE' as const,seats:1,roleName:`新治理岗${n}`,permissions:[{moduleTag:'pub-mgmt',actionCode:3,operation:'VOTE' as const},...(n===1?[{moduleTag:'pub-mgmt',actionCode:3,operation:'PROPOSE' as const}]:[])],assignments:[{personCid:regPerson(n),termStart:0,termEnd:0}]}));
    const config=buildInstitutionConfiguration(state,{initialRoles,initialThreshold:2},'R_REGISTRAR');
    expect(config).not.toHaveProperty('admins');
    expect(config).not.toHaveProperty('walletChanges');expect(config.initialThreshold).toBe(2);
    expect(config.roleMutations?.every(r=>r.seats===1)).toBe(true);
    expect(()=>buildInstitutionConfiguration(state,{initialRoles,initialThreshold:1},'R_REGISTRAR')).toThrow('阈值');
    expect(()=>buildInstitutionConfiguration(registrationState(),{initialRoles,initialThreshold:2},'R_REGISTRAR')).toThrow('重置');
  });
});

describe('登记冷签提交与原会话恢复', () => {
  const auth = { ...regIdentity, accessToken: 'registration-session', accountId: regAccount(8), institutionCidNumber: 'ZS001-FRG07-249474503-2026' } as never;
  it('真实请求只取一次Passkey和钱包签名；响应丢失后原会话恢复，不重签不重建', async () => {
    vi.resetModules();
    const { runInstitutionRegistration } = await import('../controls/registration');
    const { assertPasskey } = await import('@gmb/onchina-platform/authentication/passkeyClient');
    const before = registrationState(), after = registrationAfter(before);
    const input = roleRegistration(before);
    let receipt: import('../controls/registration').PendingInstitutionRegistration | null = null;
    const signature = '0x' + '55'.repeat(64), requestId = 'institution-governance-11111111-1111-1111-1111-111111111111';
    const sign = vi.fn(async () => ({ accountId: regAccount(8), signature }));
    let submissions = 0;
    const bodies: unknown[] = [];
    const fetcher = vi.spyOn(globalThis, 'fetch').mockImplementation(async (path, init) => {
      if (String(path).endsWith('/governance')) return new Response(JSON.stringify({ code: 0, data: submissions ? after : before }));
      if (String(path).endsWith('/prepare')) { bodies.push(JSON.parse(String(init?.body))); return new Response(JSON.stringify({ code: 0, data: { requestId, cidNumber: regCid, signRequest: 'fixture-qr', expiresAt: Math.floor(Date.now() / 1000) + 60 } })); }
      if (String(path).endsWith('/submit')) { submissions++; bodies.push(JSON.parse(String(init?.body))); return new Response(JSON.stringify({ code: 2004, message: submissions === 1 ? '已提交，结果待确认' : '冷签会话不存在' }), { status: submissions === 1 ? 502 : 404 }); }
      throw new Error('unexpected request');
    });
    try {
      vi.mocked(assertPasskey).mockClear();
      const run = () => runInstitutionRegistration({ auth, cidNumber: regCid, input: receipt ? null : input, pending: receipt, sign, remember: r => { receipt = r; }, check: () => {} });
      await expect(run()).rejects.toThrow('待确认');
      expect(receipt).not.toBeNull();
      expect(sign).toHaveBeenCalledTimes(1); expect(assertPasskey).toHaveBeenCalledTimes(1);
      const result = await run();
      expect(result.recovered).toBe(true); expect(receipt).toBeNull();
      expect(sign).toHaveBeenCalledTimes(1); expect(assertPasskey).toHaveBeenCalledTimes(1);
      expect(bodies).toEqual([input, { requestId, accountId: regAccount(8), signature }, { requestId, accountId: regAccount(8), signature }]);
    } finally { fetcher.mockRestore(); vi.resetModules(); }
  });
  it('指纹冲突在认证前拒绝，身份切换后的签名不发送提交', async () => {
    vi.resetModules();
    const { runInstitutionRegistration } = await import('../controls/registration');
    const { assertPasskey } = await import('@gmb/onchina-platform/authentication/passkeyClient');
    const before = registrationState(), input = roleRegistration(before);
    let stale = true, changed = false;
    const fetcher = vi.spyOn(globalThis, 'fetch').mockImplementation(async path => new Response(JSON.stringify({ code: 0, data: String(path).endsWith('/prepare')
      ? { requestId: 'institution-governance-context', cidNumber: regCid, expiresAt: Math.floor(Date.now() / 1000) + 60, signRequest: 'qr' }
      : stale ? { ...before, registrationStateHash: '0x' + 'ff'.repeat(32) } : before })));
    const sign = vi.fn(async () => { changed = true; return { accountId: regAccount(8), signature: '0x' + '55'.repeat(64) }; });
    const remember = vi.fn();
    try {
      vi.mocked(assertPasskey).mockClear();
      const run = () => runInstitutionRegistration({ auth, cidNumber: regCid, input, pending: null, sign, remember, check: () => { if (changed) throw new Error('页面目标已变化'); } });
      await expect(run()).rejects.toThrow('状态已变化');
      expect(assertPasskey).not.toHaveBeenCalled(); expect(sign).not.toHaveBeenCalled();
      stale = false; fetcher.mockClear();
      await expect(run()).rejects.toThrow('页面目标已变化');
      expect(remember).not.toHaveBeenCalled();
      expect(fetcher.mock.calls.every(([path]) => !String(path).endsWith('/submit'))).toBe(true);
    } finally { fetcher.mockRestore(); vi.resetModules(); }
  });
  it('提交成功仍回读真实岗位；保留已签名目标，不以空元数据或HTTP成功代替链上结果', async () => {
    vi.resetModules();
    const { runInstitutionRegistration } = await import('../controls/registration');
    const before = registrationState(), after = registrationAfter(before), input = roleRegistration(before);
    const signature = '0x' + '55'.repeat(64);
    const pending = {mode:'registration' as const,...regIdentity, requestId: 'institution-governance-success', cidNumber: regCid, accountId: regAccount(8), signature, expected: input, reviewedBlockNumber: before.blockNumber };
    const sign = vi.fn(), remember = vi.fn();
    let correct = false, missingTarget = false;
    const fetcher = vi.spyOn(globalThis, 'fetch').mockImplementation(async path => new Response(JSON.stringify({ code: 0, data: String(path).endsWith('/submit')
      ? { cidNumber: missingTarget ? '' : regCid, purpose: 'INSTITUTION_REGISTER_ROLES', txHash: '0x' + '44'.repeat(32), blockNumber: 11 } : correct ? after : before })));
    try {
      const run = () => runInstitutionRegistration({ auth, cidNumber: regCid, input: null, pending, sign, remember, check: () => {} });
      await expect(run()).rejects.toThrow('尚未核对'); expect(remember).not.toHaveBeenCalled();
      correct = true; missingTarget = true; await expect(run()).rejects.toThrow('尚未确认'); expect(remember).not.toHaveBeenCalled();
      missingTarget = false; expect((await run()).state.roles[2].assignments).toMatchObject([{ personCid: regPerson(2) }]); expect(remember).toHaveBeenCalledWith(null);
      expect(sign).not.toHaveBeenCalled();
    } finally { fetcher.mockRestore(); vi.resetModules(); }
  });
  it('缺少审阅状态的岗位配置在Passkey前失败', async () => {
    const { prepareRegisterInstitutionRoles } = await import('../controls/registration');
    const { assertPasskey } = await import('@gmb/onchina-platform/authentication/passkeyClient');
    const fetcher = vi.spyOn(globalThis, 'fetch');
    try {
      vi.mocked(assertPasskey).mockClear();
      await expect(prepareRegisterInstitutionRoles(auth, { cidNumber: regCid, assignmentChanges: roleRegistration(registrationState()).assignmentChanges } as never)).rejects.toThrow('核对');
      expect(assertPasskey).not.toHaveBeenCalled(); expect(fetcher).not.toHaveBeenCalled();
    } finally { fetcher.mockRestore(); }
  });
});


describe('登记恢复记录与最终拒绝', () => {
  it('钱包维护载荷和已签回执在认证或提交前拒绝，浏览器旧记录不被恢复', async () => {
    const { prepareRegisterInstitutionRoles, runInstitutionRegistration, readRegistrationReceipt } = await import('../controls/registration');
    const { assertPasskey } = await import('@gmb/onchina-platform/authentication/passkeyClient');
    const auth = { ...regIdentity, accessToken: 'retired-wallet-input', accountId: regAccount(8), institutionCidNumber: regCid } as never;
    const sign = vi.fn(), remember = vi.fn();
    const fetcher = vi.spyOn(globalThis, 'fetch');
    let stored = '';
    vi.stubGlobal('sessionStorage', { getItem: () => stored, setItem: vi.fn(), removeItem: vi.fn() });
    try {
      vi.mocked(assertPasskey).mockClear();
      for (const retired of [{ admins: [] }, { walletChanges: { added: [], removed: [], replaced: [] } }, { mode: 'SELF', newWallet: regAccount(7) }]) {
        const expected = { ...roleRegistration(registrationState()), ...retired };
        const pending = {mode:'registration' as const,...regIdentity, requestId: 'institution-governance-retired-wallet', cidNumber: regCid, accountId: regAccount(8), signature: '0x' + '55'.repeat(64), reviewedBlockNumber: 10, expected };
        stored = JSON.stringify(pending);
        expect(readRegistrationReceipt(auth, regCid)).toBeNull();
        await expect(prepareRegisterInstitutionRoles(auth, expected as never)).rejects.toThrow('不接收');
        await expect(runInstitutionRegistration({ auth, cidNumber: regCid, input: null, pending: pending as never, sign, remember, check: () => {} })).rejects.toThrow('不接收');
      }
      expect(fetcher).not.toHaveBeenCalled(); expect(assertPasskey).not.toHaveBeenCalled();
      expect(sign).not.toHaveBeenCalled(); expect(remember).not.toHaveBeenCalled();
      expect(sessionStorage.removeItem).not.toHaveBeenCalled();
    } finally { fetcher.mockRestore(); vi.unstubAllGlobals(); }
  });

  it('停用名单输入和原签名在认证或广播前拒绝', async () => {
    vi.resetModules();
    const { prepareRegisterInstitutionRoles, runInstitutionRegistration, saveRegistrationReceipt, readRegistrationReceipt } = await import('../controls/registration');
    const { assertPasskey } = await import('@gmb/onchina-platform/authentication/passkeyClient');
    const auth = {accessToken:'retired-input',accountId:regAccount(8),institutionCidNumber:regCid} as never;
    const expected = {cidNumber:regCid,expectedStateHash:'0x'+'ab'.repeat(32),admins:[{accountId:regAccount(2)}]};
    const pending = {requestId:'institution-governance-retired',cidNumber:regCid,accountId:regAccount(8),signature:'0x'+'55'.repeat(64),expected,reviewedBlockNumber:10};
    const fetcher=vi.spyOn(globalThis,'fetch').mockResolvedValue(new Response(JSON.stringify({code:2004,message:'旧名单动作停用'}),{status:409}));
    const sign=vi.fn(),remember=vi.fn();
    try {
      vi.mocked(assertPasskey).mockClear();
      await expect(prepareRegisterInstitutionRoles(auth,expected as never)).rejects.toThrow('不接收');
      expect(assertPasskey).not.toHaveBeenCalled();expect(fetcher).not.toHaveBeenCalled();
      await expect(runInstitutionRegistration({auth,cidNumber:regCid,input:null,pending:pending as never,sign,remember,check:()=>{}})).rejects.toThrow('不接收');
      expect(fetcher).not.toHaveBeenCalled();
      expect(sign).not.toHaveBeenCalled();expect(remember).not.toHaveBeenCalled();
    } finally {fetcher.mockRestore();vi.resetModules();}
  });

  it('正常岗位配置回执按签名账户、办理机构和目标隔离', async () => {
    vi.resetModules();
    const { saveRegistrationReceipt, readRegistrationReceipt } = await import('../controls/registration');
    const storage = new Map<string, string>();
    vi.stubGlobal('sessionStorage', { getItem: (key: string) => storage.get(key) ?? null, setItem: (key: string, value: string) => storage.set(key, value), removeItem: (key: string) => storage.delete(key) });
    const auth = { ...regIdentity, accountId: regAccount(8), institutionCidNumber: 'ZS001-FRG07-249474503-2026' } as never;
    const receipt = {mode:'registration' as const,...regIdentity, requestId: 'institution-governance-stored', cidNumber: regCid, accountId: regAccount(8), signature: '0x' + '55'.repeat(64), reviewedBlockNumber: 10,
      expected: roleRegistration(registrationState()) };
    try {
      saveRegistrationReceipt(auth, regCid, receipt);
      expect(readRegistrationReceipt(auth, regCid)).toEqual(receipt);
      const pure={...receipt,expected:roleRegistration(registrationState())};
      saveRegistrationReceipt(auth,regCid,pure);
      expect(readRegistrationReceipt(auth,regCid)).toEqual(pure);
      expect(readRegistrationReceipt({ ...auth as object, personCid: regPerson(7) } as never, regCid)).toBeNull();
      expect(readRegistrationReceipt(auth, 'different-target')).toBeNull();
      saveRegistrationReceipt(auth, regCid, null); expect(readRegistrationReceipt(auth, regCid)).toBeNull();
    } finally { vi.unstubAllGlobals(); vi.resetModules(); }
  });
  it('链端明确拒绝后清理原签名；结果不确定或回读不符时保持恢复记录', async () => {
    vi.resetModules();
    const { runInstitutionRegistration } = await import('../controls/registration');
    const before = registrationState();
    const pending = {mode:'registration' as const,...regIdentity, requestId: 'institution-governance-rejected', cidNumber: regCid, accountId: regAccount(8), signature: '0x' + '55'.repeat(64), reviewedBlockNumber: 10,
      expected: roleRegistration(before) };
    const remember = vi.fn(), sign = vi.fn();
    const auth = { ...regIdentity, accessToken: 'fixture-session', accountId: regAccount(8),institutionCidNumber:regIdentity.actorInstitutionCid } as never;
    let rejected = false;
    const fetcher = vi.spyOn(globalThis, 'fetch').mockImplementation(async path => String(path).endsWith('/submit')
      ? new Response(JSON.stringify({ code: 2004, message: rejected ? '链端拒绝' : '会话已消费' }), { status: rejected ? 422 : 409 })
      : new Response(JSON.stringify({ code: 0, data: before })));
    try {
      const run = () => runInstitutionRegistration({ auth, cidNumber: regCid, input: null, pending, sign, remember, check: () => {} });
      await expect(run()).rejects.toThrow('已消费'); expect(remember).not.toHaveBeenCalled();
      rejected = true; await expect(run()).rejects.toThrow('链端拒绝'); expect(remember).toHaveBeenCalledWith(null);
      expect(sign).not.toHaveBeenCalled();
    } finally { fetcher.mockRestore(); vi.resetModules(); }
  });
});


describe('第5步通用页面与原投票语义', () => {
  it('公权私权详情共用中文岗位表单，旧岗位码与文本任职入口消失', async () => {
    const { buildInstitutionRoleGovernance } = await import('@gmb/onchina-platform/institutions/governance');
    const state = registrationState();
    const auth = { ...regIdentity,roleCode:'R_GOV', accountId: regAccount(9), institutionCidNumber: state.cidNumber } as never;
    const input = buildInstitutionRoleGovernance(auth, state, { assignments: [{roleCode:'R_BUSINESS',assignments:[{personCid:regPerson(2),termStart:0,termEnd:0}]}] });
    expect(input.actorRoleCode).toBe('R_GOV');
    expect(input).not.toHaveProperty('admins');
    expect(() => buildInstitutionRoleGovernance(auth, state, { addedAdmins: [] } as never)).toThrow('不接收');
    expect(() => buildInstitutionRoleGovernance({ ...auth as object, institutionCidNumber: 'other' } as never, state, { addedAdmins: [] } as never)).toThrow('本机构');
    expect(() => buildInstitutionRoleGovernance(auth, { ...state, moduleTag: 'pri-mgmt' }, { addedAdmins: [] } as never)).toThrow('不接收');
  });
  it('治理使用原call 8接口，不把提案上链误认作名册已生效', async () => {
    vi.resetModules();
    const { runInstitutionRegistration } = await import('../controls/registration');
    const { buildInstitutionRoleGovernance } = await import('@gmb/onchina-platform/institutions/governance');
    const state = registrationState();
    const auth = { ...regIdentity,roleCode:'R_GOV', accessToken: 'self-governance', accountId: regAccount(9), institutionCidNumber: state.cidNumber } as never;
    const input = buildInstitutionRoleGovernance(auth, state, { assignments: [{roleCode:'R_BUSINESS',assignments:[{personCid:regPerson(2),termStart:0,termEnd:0}]}] });
    let submitted = false;
    const bodies: { path: string; body: unknown }[] = [];
    const fetcher = vi.spyOn(globalThis, 'fetch').mockImplementation(async (path, init) => {
      if (String(path).endsWith('/governance')) return new Response(JSON.stringify({ code: 0, data: { ...state, blockNumber: submitted ? 11 : 10 } }));
      bodies.push({ path: String(path), body: JSON.parse(String(init?.body)) });
      if (String(path).endsWith('/prepare')) return new Response(JSON.stringify({ code: 0, data: { cidNumber: regCid, requestId: 'institution-governance-proposal', signRequest: 'qr', expiresAt: Math.floor(Date.now()/1000)+60 } }));
      submitted = true;
      return new Response(JSON.stringify({ code: 0, data: { purpose: 'INSTITUTION_GOVERNANCE', cidNumber: regCid, txHash: '0x' + '77'.repeat(32), blockNumber: 11 } }));
    });
    const sign = vi.fn(async () => ({ accountId: regAccount(9), signature: '0x' + '55'.repeat(64) }));
    try {
      const result = await runInstitutionRegistration({ auth, cidNumber: regCid, input, pending: null, sign, remember: () => {}, check: () => {}, mode: 'governance' });
      expect(result.state.roles[2].assignments[0].personCid).toBe(regPerson(3)); // 仅提案上链，名单尚未变化。
      expect(bodies[0].path).toBe('/api/admin/institution/governance/prepare');
      expect(bodies[0].body).toMatchObject({ cidNumber: regCid, proposerRoleCode: 'R_GOV', assignmentChanges: input.assignmentChanges });
      expect(bodies[0].body).not.toHaveProperty('admins');
      expect(bodies[0].body).not.toHaveProperty('actorRoleCode');
      expect(bodies[0].body).not.toHaveProperty('expectedStateHash');
      expect(sign).toHaveBeenCalledTimes(1);
    } finally { fetcher.mockRestore(); vi.resetModules(); }
  });
});


(process.env.ONCHINA_TEST_DOM ? it : it.skip)('首次岗位配置保留岗位和阈值表单，不再提供管理员钱包表单',async()=>{
  vi.resetModules();
  const {createRequire}=await import('node:module');
  const {JSDOM}=createRequire(import.meta.url)(process.env.ONCHINA_TEST_DOM!);
  const dom=new JSDOM('<div id="wallet-root"></div>',{url:'https://onchina.example.invalid/',pretendToBeVisual:true});
  const computed=dom.window.getComputedStyle.bind(dom.window);dom.window.getComputedStyle=(element:Element)=>computed(element);
  dom.window.matchMedia=()=>({matches:false,addListener(){},removeListener(){},addEventListener(){},removeEventListener(){}});
  for(const key of ['window','document','navigator','HTMLElement','Element','ShadowRoot','SVGElement','getComputedStyle'])vi.stubGlobal(key,dom.window[key]);
  vi.stubGlobal('IS_REACT_ACT_ENVIRONMENT',true);
  vi.doMock('@gmb/onchina-platform/qr/useChainSign',()=>({useChainSign:()=>({signChain:vi.fn(),chainSignModal:null}),submitChainSign:vi.fn()}));
  const React=await import('react');const {createRoot}=await import('react-dom/client');
  const {InstitutionConfigurationModal}=await import('../controls/Configuration');
  const state={...registrationState(),governanceThreshold:null};
  const fetcher=vi.spyOn(globalThis,'fetch').mockResolvedValue(new Response(JSON.stringify({code:0,data:state})));
  const root=createRoot(document.getElementById('wallet-root')!);
  const props={auth:{accessToken:'wallet-form',accountId:regAccount(8),institutionCidNumber:'ZS001-FRG07-249474503-2026'} as never,cidNumber:regCid,open:true,onClose:vi.fn(),onSubmitted:vi.fn()};
  try{
    await React.act(async()=>{root.render(React.createElement(InstitutionConfigurationModal,props));await new Promise(r=>setTimeout(r,30));});
    expect(document.querySelector('[aria-label="管理员账户1"]')).toBeNull();
    expect(document.querySelector('[aria-label="新管理员钱包"]')).toBeNull();
    expect(document.querySelector('[aria-label="初始治理阈值"]')).not.toBeNull();
    expect(document.body.textContent).toContain('添加动态岗位');
    expect(document.body.textContent).not.toContain('新增机构管理员');
    expect(fetcher.mock.calls.every(([path])=>String(path).endsWith('/governance'))).toBe(true);
  }finally{await React.act(async()=>root.unmount());fetcher.mockRestore();dom.window.close();vi.unstubAllGlobals();vi.doUnmock('@gmb/onchina-platform/qr/useChainSign');}
},15000);


describe('注册局机构浏览与读取失败', () => {
  const browserTest = process.env.ONCHINA_TEST_DOM ? it : it.skip;
  async function browser() {
    vi.resetModules();
    const { createRequire } = await import('node:module');
    const { JSDOM } = createRequire(import.meta.url)(process.env.ONCHINA_TEST_DOM!);
    const dom = new JSDOM('<div id="registry-root"></div>', { url: 'https://onchina.example.invalid/', pretendToBeVisual: true });
    const computed = dom.window.getComputedStyle.bind(dom.window); dom.window.getComputedStyle = (element: Element) => computed(element);
    dom.window.matchMedia = () => ({ matches: false, addListener() {}, removeListener() {}, addEventListener() {}, removeEventListener() {} });
    for (const name of ['window', 'document', 'navigator', 'HTMLElement', 'Element', 'ShadowRoot', 'SVGElement', 'getComputedStyle']) vi.stubGlobal(name, dom.window[name]);
    vi.stubGlobal('IS_REACT_ACT_ENVIRONMENT', true);
    const React = await import('react'); const { createRoot } = await import('react-dom/client');
    const root = createRoot(document.getElementById('registry-root')!);
    vi.doMock('../government/GovDetailPage', () => ({ GovDetailPage: ({ cidNumber, onBack }: { cidNumber: string; onBack?: () => void }) => React.createElement('article', { 'data-cid': cidNumber }, '机构详情', onBack && React.createElement('button', { onClick: onBack }, '返回目录')) }));
    const { RegistryView } = await import('../government/Registry');
    const auth = { ...regIdentity, accountId: regAccount(8), accessToken: 'registry-read', institutionCidNumber: 'ZS001-FRG07-249474503-2026', institutionCode: 'FRG', adminLevel: 'NATIONAL', scopeProvinceName: '贵州省' } as never;
    const flush = () => React.act(async () => { await new Promise(resolve => setTimeout(resolve, 30)); });
    const click = async (label: string) => { await React.act(async () => [...document.querySelectorAll('button')].find(button => button.textContent?.replace(/\s/g, '') === label)!.click()); await flush(); };
    return { React, root, RegistryView, auth, flush, click, dispose: async () => { await React.act(async () => root.unmount()); vi.doUnmock('../government/GovDetailPage'); dom.window.close(); vi.unstubAllGlobals(); vi.resetModules(); } };
  }
  browserTest('目录失败明确重试，按游标分页，准确CID进入详情，不读人员名单', async () => {
    const c = await browser(); let failed = true;
    const requests: string[] = [];
    const fetcher = vi.spyOn(globalThis, 'fetch').mockImplementation(async path => {
      requests.push(String(path)); const url = new URL(String(path), 'https://onchina.example.invalid');
      expect(url.pathname).toBe('/api/institutions/gov'); expect(url.searchParams.get('institutionCode')).toBe('CREG');
      if (failed) return new Response('unavailable', { status: 503 });
      const second = url.searchParams.get('cursor') === 'page-2';
      return new Response(JSON.stringify({ code: 0, data: { items: [{ cidNumber: second ? 'second-registry' : regCid, institutionCode: 'CREG', cidFullName: second ? '第二注册局' : '第一注册局', provinceName: '贵州省', cityName: '贵阳市' }], page_size: 20, has_more: !second, next_cursor: second ? null : 'page-2' } }));
    });
    try {
      await c.React.act(async () => c.root.render(c.React.createElement(c.RegistryView, { auth: c.auth, mode: 'city-registry' }))); await c.flush();
      expect(document.body.textContent).toContain('市注册局读取失败'); expect(document.body.textContent).not.toContain('暂无数据');
      failed = false; await c.click('重试'); expect(document.body.textContent).toContain('第一注册局');
      await c.click('下一页'); expect(document.body.textContent).toContain('第二注册局');
      expect(([...document.querySelectorAll('button')].find(button => button.textContent?.replace(/\s/g, '') === '下一页') as HTMLButtonElement).disabled).toBe(true);
      await c.React.act(async () => (document.querySelector('tbody tr[data-row-key]') as HTMLElement).click()); await c.flush();
      expect(document.querySelector('[data-cid]')?.getAttribute('data-cid')).toBe('second-registry');
      expect(document.body.textContent).not.toContain('管理员'); expect(requests).toHaveLength(3);
    } finally { fetcher.mockRestore(); await c.dispose(); }
  });
  browserTest('切换为市注册局会话丢弃旧目录响应，仅进入本机构详情', async () => {
    const c = await browser(); let resolve!: (response: Response) => void;
    const fetcher = vi.spyOn(globalThis, 'fetch').mockImplementation(() => new Promise<Response>(done => { resolve = done; }));
    try {
      await c.React.act(async () => c.root.render(c.React.createElement(c.RegistryView, { auth: c.auth, mode: 'city-registry' }))); await c.flush();
      const own = { ...c.auth as object, accessToken: 'city-session', institutionCode: 'CREG', institutionCidNumber: regCid, adminLevel: 'CITY', scopeCityName: '贵阳市' } as never;
      await c.React.act(async () => c.root.render(c.React.createElement(c.RegistryView, { auth: own, mode: 'city-registry' })));
      await c.React.act(async () => resolve(new Response(JSON.stringify({ code: 0, data: { items: [{ cidNumber: 'late-foreign', institutionCode: 'CREG' }], has_more: false } })))); await c.flush();
      expect(document.querySelector('[data-cid]')?.getAttribute('data-cid')).toBe(regCid); expect(document.body.textContent).not.toContain('late-foreign'); expect(fetcher).toHaveBeenCalledTimes(1);
    } finally { fetcher.mockRestore(); await c.dispose(); }
  });
  browserTest('联邦资料读取失败可重试，正常详情不附带管理员分区', async () => {
    const c = await browser(); let failed = true;
    const fetcher = vi.spyOn(globalThis, 'fetch').mockImplementation(async path => {
      expect(path).toBe('/api/institutions/federal-registry');
      return failed ? new Response('unavailable', { status: 503 }) : new Response(JSON.stringify({ code: 0, data: { institution: { cidNumber: 'federal-cid' }, accounts: [] } }));
    });
    try {
      await c.React.act(async () => c.root.render(c.React.createElement(c.RegistryView, { auth: c.auth, mode: 'federal-registry' }))); await c.flush();
      expect(document.body.textContent).toContain('联邦注册局读取失败'); failed = false; await c.click('重试');
      expect(document.querySelector('[data-cid]')?.getAttribute('data-cid')).toBe('federal-cid'); expect(document.body.textContent).not.toContain('管理员'); expect(fetcher).toHaveBeenCalledTimes(2);
    } finally { fetcher.mockRestore(); await c.dispose(); }
  });
});

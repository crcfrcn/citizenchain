// 验证注册局管理员登记请求和提交状态的准确传递。

import { describe, expect, it, vi } from 'vitest';
import { readFileSync } from 'node:fs';
import { dirname, join, normalize } from 'node:path';
import { SECURITY_GRANT_HEADER, PASSKEY_ASSERTION_HEADER, passkeySubmitHeaders, securityGrantSubmitHeaders, commitAdminAction, rustConst } from '@gmb/onchina-platform/testing';

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

describe('注册局首帧和既有双Tab', () => {
  it('未知状态不显示入口；未设置密钥直接进所属入口；已设置仍保留两个Tab', async () => {
    vi.resetModules();
    const React = await import('react');
    const { renderToStaticMarkup } = await import('react-dom/server');
    vi.doMock('../citizens/CitizensView', () => ({ CitizensView: () => React.createElement('div', { 'data-page': 'citizens' }) }));
    vi.doMock('../admins/ProvinceDetailView', () => ({ RegistryAdminsView: ({ mode }: { mode: string }) => React.createElement('div', { 'data-page': mode }) }));
    try {
      const { RegistryWorkspace } = await import('../controls/Workspace');
      const { EMPTY_CAPABILITIES } = await import('@gmb/onchina-platform/workspace/modules');
      const capabilities = { ...EMPTY_CAPABILITIES, canViewCitizens: true, canViewCityRegistry: true, canViewFederalRegistry: true };
      const props = { auth: { accountId: 'fixture-account', institutionCidNumber: 'fixture-institution', institutionCode: 'FRG' } as never,
        capabilities, cidMeta: null, setCidMeta: () => {} };
      const pending = renderToStaticMarkup(React.createElement(RegistryWorkspace, { ...props, passkeyRegistered: null }));
      expect(pending).not.toContain('data-page');
      expect(pending).not.toContain('<button');
      const locked = renderToStaticMarkup(React.createElement(RegistryWorkspace, { ...props, passkeyRegistered: false }));
      expect(locked).toContain('data-page="federal-registry"');
      expect(locked).not.toContain('data-page="citizens"');
      expect(locked).not.toContain('>市注册局</button>');
      const city = renderToStaticMarkup(React.createElement(RegistryWorkspace, { ...props, auth: { ...props.auth as object, institutionCode: 'CREG' } as never, passkeyRegistered: false }));
      expect(city).toContain('data-page="city-registry"');
      const ready = renderToStaticMarkup(React.createElement(RegistryWorkspace, { ...props, passkeyRegistered: true }));
      expect(ready).toContain('>市注册局</button>');
      expect(ready).toContain('>联邦注册局</button>');
      const denied = renderToStaticMarkup(React.createElement(RegistryWorkspace, { ...props, capabilities: EMPTY_CAPABILITIES, passkeyRegistered: true }));
      expect(denied).toContain('当前身份没有可访问的工作台入口');
      expect(denied).not.toContain('data-page');
    } finally {
      vi.doUnmock('../citizens/CitizensView'); vi.doUnmock('../admins/ProvinceDetailView'); vi.resetModules();
    }
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
const auth = { accessToken: 'test-session', accountId: '0x' + '11'.repeat(32) } as never;
it('注册局登记准备等待一次Passkey，保留请求且失败不重试', async () => {
    const { assertPasskey } = await import('@gmb/onchina-platform/authentication/passkeyClient');
    const { prepareInstitutionGovernance } = await import('@gmb/onchina-platform/institutions/governance');
    const { prepareRegisterInstitutionAdmins } = await import('../admins/cityRegistryAdminsApi');
    const governance = { cidNumber: 'fixture-institution', proposerRoleCode: 'REPRESENTATIVE' };
    const registration = { expectedStateHash: '0x' + 'ab'.repeat(32), cidNumber: 'fixture-institution', actorRoleCode: 'REGISTRY-ROLE', admins: [{ accountId: 'fixture-account', familyName: '测', givenName: '试' }], walletChanges: { added: [{ accountId: 'fixture-account', familyName: '测', givenName: '试' }], removed: [], replaced: [] } };
    const operations = [{ path: '/api/admin/institution/admins/register/prepare', payload: registration, run: () => prepareRegisterInstitutionAdmins(auth, registration) }];
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
describe('登记管理员原有请求参数衔接', () => {
  it('原URL、Bearer与管理员集合保持，透传操作注册局岗位码', async () => {
    const { prepareRegisterInstitutionAdmins } = await import('../admins/cityRegistryAdminsApi');
    const { assertPasskey } = await import('@gmb/onchina-platform/authentication/passkeyClient');
    const auth = { accessToken: 'test-session' } as never;
    const input = { expectedStateHash: '0x' + 'ab'.repeat(32), cidNumber: 'target-fixture-cid', admins: [{ accountId: 'fixture-account', familyName: '测', givenName: '试' }], actorRoleCode: 'REGISTRY-ROLE', walletChanges: { added: [{ accountId: 'fixture-account', familyName: '测', givenName: '试' }], removed: [], replaced: [] } };
    const fetcher = vi.spyOn(globalThis, 'fetch').mockResolvedValue(new Response(JSON.stringify({ code: 0, data: {} })));
    try {
      vi.mocked(assertPasskey).mockClear();
      await prepareRegisterInstitutionAdmins(auth, input);
      expect(assertPasskey).toHaveBeenCalledTimes(1);
      expect(fetcher).toHaveBeenCalledTimes(1);
      const [path, init] = fetcher.mock.calls[0];
      expect(path).toBe('/api/admin/institution/admins/register/prepare');
      expect(init?.method).toBe('POST');
      expect(new Headers(init?.headers).get('authorization')).toBe('Bearer test-session');
      expect(new Headers(init?.headers).get(PASSKEY_ASSERTION_HEADER)).toBe('fake-passkey-assertion');
      expect(JSON.parse(String(init?.body))).toEqual(input);
    } finally { fetcher.mockRestore(); }
  });
});

// 原注册局双Tab与管理员列表断言在上方保留；本轮只验证机构详情拆页挂载。
describe('机构详情非管理员治理入口', () => {
  it('公权和私权详情保留独立治理页面和专用钱包更换入口', async () => {
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
      const auth = { accessToken: 'fixture-session', accountId: 'fixture-account', institutionCidNumber: detail.institution.cidNumber, institutionCode: 'SFGY' } as never;
      const official = renderToStaticMarkup(React.createElement(GovDetailPage, { auth, cidNumber: detail.institution.cidNumber, canWrite: true, adminListSection: React.createElement('span', null, '原管理员名单'), initialActiveKey: 'governance' }));
      const privatePage = renderToStaticMarkup(React.createElement(PrivateDetailLayout, { auth, detail: { ...detail, institution: { ...detail.institution, category: 'PRIVATE_INSTITUTION' } } as never, canWrite: true, loading: false, onReload: () => {} }));
      for (const html of [official, privatePage]) {
        for (const label of ['岗位管理', '任职管理', '法定代表人', '机构治理', '账户列表']) expect(html).toContain(label);
        expect(html).not.toContain('岗位操作');
        expect(html).not.toContain('岗位权限集合');
        expect(html).not.toContain('提案发起岗位码');
        expect(html).not.toContain('岗位码（改名/删除）');
      }
      expect(official).toContain('管理员列表');
      expect(official).not.toContain('治理管理员集合');
      expect(official).toContain('岗位与任职请使用对应治理页面');
      expect(official).toContain('注册局直接登记管理员');
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
  const target = (n: number) => ({ accountId: regAccount(n), termStart: 0, termEnd: 0, active: true, effective: true, sourceLabel: '机构内部治理', sourceRef: '0xff0080' });
  return { cidNumber: regCid, blockHash: '0x' + '10'.repeat(32), blockNumber: 10, currentDay: 20000, moduleTag: 'pub-mgmt', legalRoleCode: 'LR', representative: null,
    registrationStateHash: '0x' + 'ab'.repeat(32), governanceThreshold: 2, registrationRoles: [{ roleCode: 'R_REGISTRAR', roleName: '登记办理岗' }],
    members: [1, 2, 3].map(n => ({ accountId: n === 1 ? regAccount(9) : regAccount(n), authorizationAccountId: regAccount(n), signerId: n === 1 ? regAccount(9) : regAccount(n), cidNumber: 'citizen-' + n, walletRevision: 0, familyName: '原', givenName: String(n) })),
    roles: [
      { roleCode: 'LR', roleName: '法定代表人', active: true, fixed: true, assignmentLocked: true, termRequired: false, minAssignments: 0, maxAssignments: 1, permissions: [], assignments: [] },
      { roleCode: 'R_GOV', roleName: '机构治理岗', active: true, fixed: false, assignmentLocked: false, termRequired: false, minAssignments: 0, maxAssignments: null,
        permissions: ['PROPOSE', 'VOTE'].map(operation => ({ moduleTag: 'pub-mgmt', actionCode: 3, operation: operation as 'PROPOSE' | 'VOTE' })), assignments: [target(1), target(2)] },
      { roleCode: 'R_BUSINESS', roleName: '公民身份办理岗', active: true, fixed: false, assignmentLocked: false, termRequired: false, minAssignments: 0, maxAssignments: null,
        permissions: [{ moduleTag: 'cit-id', actionCode: 0, operation: 'PROPOSE' }], assignments: [target(3)] },
    ] };
}
const regAdded = { accountId: regAccount(4), familyName: '新', givenName: '四' };
const regAssignments = [{ roleCode: 'R_BUSINESS', assignments: [{ accountId: regAccount(4), termStart: 0, termEnd: 0 }] }];
function registrationAfter(before: GovernanceState): GovernanceState {
  const state = structuredClone(before); state.blockNumber++; state.registrationStateHash = '0x' + 'cd'.repeat(32);
  state.members.push({ ...regAdded, cidNumber: 'new-citizen', signerId: regAccount(4), authorizationAccountId: regAccount(4) });
  state.roles[2].assignments.push({ ...regAssignments[0].assignments[0], active: true, effective: true, sourceLabel: '注册局', sourceRef: '0x' + '12'.repeat(32) });
  return state;
}

describe('统一链上名册登记构造', () => {
  it('新增钱包不夹带岗位和阈值，允许尚无治理配置的机构首次登记', async () => {
    const {buildInstitutionRegistration}=await import('@gmb/onchina-platform/institutions/governance');
    const before=registrationState(),original=structuredClone(before);
    const input=buildInstitutionRegistration(before,{addedAdmins:[regAdded]});
    expect(input.walletChanges).toEqual({added:[regAdded],removed:[],replaced:[]});
    expect(input).not.toHaveProperty('assignmentChanges');expect(input).not.toHaveProperty('initialThreshold');expect(input).not.toHaveProperty('roleMutations');
    const empty={...before,members:[],governanceThreshold:null};
    expect(buildInstitutionRegistration(empty,{addedAdmins:[regAdded]}).admins).toHaveLength(1);
    expect(()=>buildInstitutionRegistration(before,{addedAdmins:[regAdded],assignments:regAssignments})).toThrow('夹带');
    expect(before).toEqual(original);
  });
  it('删除钱包保留所有岗位人员，允许删除最后一个；更换显式冻结原账户和版本', async () => {
    const {buildInstitutionRegistration}=await import('@gmb/onchina-platform/institutions/governance');
    const before=registrationState(),original=structuredClone(before);
    const removed=buildInstitutionRegistration(before,{removeAccountId:regAccount(3)});
    expect(removed.walletChanges?.removed).toEqual([{accountId:regAccount(3),expectedRevision:0}]);
    expect(removed).not.toHaveProperty('assignmentChanges');
    expect(buildInstitutionRegistration({...before,members:[before.members[0]]},{removeAccountId:regAccount(9)}).admins).toEqual([]);
    const replaced=buildInstitutionRegistration(before,{replacement:{accountId:regAccount(9),newWallet:regAccount(7)}});
    expect(replaced.walletChanges?.replaced).toEqual([{adminAccount:regAccount(9),expectedWallet:regAccount(9),newWallet:regAccount(7),expectedRevision:0}]);
    expect(()=>buildInstitutionRegistration(before,{replacement:{accountId:regAccount(9),newWallet:regAccount(2)}})).toThrow('已在名册');
    expect(()=>buildInstitutionRegistration(before,{removeAccountId:regAccount(7)})).toThrow('目标');
    expect(before).toEqual(original);
  });
  it('岗位唯一自动确定，多岗位必须选择有效候选；失效选择不自动换岗', async () => {
    const { registrationActorRole, buildInstitutionRegistration } = await import('@gmb/onchina-platform/institutions/governance');
    const state = registrationState();
    expect(registrationActorRole(state)).toBe('R_REGISTRAR');
    const multiple = { ...state, registrationRoles: [...state.registrationRoles!, { roleCode: 'R_REVIEWER', roleName: '登记复核岗' }] };
    expect(() => registrationActorRole(multiple)).toThrow('名称');
    expect(registrationActorRole(multiple, 'R_REVIEWER')).toBe('R_REVIEWER');
    expect(() => registrationActorRole(state, 'R_REVIEWER')).toThrow('有效');
    expect(() => buildInstitutionRegistration({ ...state, registrationStateHash: undefined }, { addedAdmins: [regAdded] })).toThrow('未就绪');
    expect(() => buildInstitutionRegistration(state, { addedAdmins: [regAdded], assignments: regAssignments, initialThreshold: 2 })).toThrow('夹带');
  });
  it('链上结果必须包含完整目标名册、任职和明确的初始化配置', async () => {
    const { buildInstitutionRegistration, institutionRegistrationMatches } = await import('@gmb/onchina-platform/institutions/governance');
    const before = registrationState(), after = registrationAfter(before);
    const input = buildInstitutionRegistration(before, { addedAdmins: [regAdded] });
    expect(institutionRegistrationMatches(after, input)).toBe(true);
    expect(institutionRegistrationMatches(before, input)).toBe(false);
    expect(institutionRegistrationMatches({ ...after, cidNumber: 'other-target' }, input)).toBe(false);
    const wrongTerm = structuredClone(after); wrongTerm.roles[2].assignments[1].termEnd = 1;
    expect(institutionRegistrationMatches(wrongTerm, input)).toBe(true); // 钱包回执不能伪称修改了独立任职。
    const missing = structuredClone(after); missing.members.shift();
    expect(institutionRegistrationMatches(missing, input)).toBe(false);
  });
});

describe('独立首次治理配置',()=>{
  it('显式配置岗位和阈值而不修改任何钱包；已有配置不能重置',async()=>{
    const {buildInstitutionConfiguration}=await import('@gmb/onchina-platform/institutions/governance');
    const state={...registrationState(),governanceThreshold:null,roles:[registrationState().roles[0]]};
    const initialRoles=[1,2].map(n=>({mutation:'CREATE' as const,roleName:`新治理岗${n}`,permissions:[{moduleTag:'pub-mgmt',actionCode:3,operation:'VOTE' as const},...(n===1?[{moduleTag:'pub-mgmt',actionCode:3,operation:'PROPOSE' as const}]:[])],assignments:[{accountId:regAccount(n),termStart:0,termEnd:0}]}));
    const config=buildInstitutionConfiguration(state,{initialRoles,initialThreshold:2});
    expect(config.admins).toBeUndefined();
    expect(config.walletChanges).toBeUndefined();expect(config.initialThreshold).toBe(2);
    expect(()=>buildInstitutionConfiguration({...state,members:[]},{initialRoles,initialThreshold:2})).toThrow('先登记');
    expect(()=>buildInstitutionConfiguration(registrationState(),{initialRoles,initialThreshold:2})).toThrow('重置');
  });
});

describe('登记冷签提交与原会话恢复', () => {
  const auth = { accessToken: 'registration-session', accountId: regAccount(8), institutionCidNumber: 'ZS001-FRG07-249474503-2026' } as never;
  it('真实请求只取一次Passkey和钱包签名；响应丢失后原会话恢复，不重签不重建', async () => {
    vi.resetModules();
    const { runInstitutionRegistration } = await import('../admins/cityRegistryAdminsApi');
    const { buildInstitutionRegistration } = await import('@gmb/onchina-platform/institutions/governance');
    const { assertPasskey } = await import('@gmb/onchina-platform/authentication/passkeyClient');
    const before = registrationState(), after = registrationAfter(before);
    const input = buildInstitutionRegistration(before, { addedAdmins: [regAdded] });
    let receipt: import('../admins/cityRegistryAdminsApi').PendingInstitutionRegistration | null = null;
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
    const { runInstitutionRegistration } = await import('../admins/cityRegistryAdminsApi');
    const { buildInstitutionRegistration } = await import('@gmb/onchina-platform/institutions/governance');
    const { assertPasskey } = await import('@gmb/onchina-platform/authentication/passkeyClient');
    const before = registrationState(), input = buildInstitutionRegistration(before, { addedAdmins: [regAdded] });
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
  it('提交成功仍回读真实名册；保留已签名目标，不以空元数据或HTTP成功代替链上结果', async () => {
    vi.resetModules();
    const { runInstitutionRegistration } = await import('../admins/cityRegistryAdminsApi');
    const { buildInstitutionRegistration } = await import('@gmb/onchina-platform/institutions/governance');
    const before = registrationState(), after = registrationAfter(before), input = buildInstitutionRegistration(before, { addedAdmins: [regAdded] });
    const signature = '0x' + '55'.repeat(64);
    const pending = { requestId: 'institution-governance-success', cidNumber: regCid, accountId: regAccount(8), signature, expected: input, reviewedBlockNumber: before.blockNumber };
    const sign = vi.fn(), remember = vi.fn();
    let correct = false, missingTarget = false;
    const fetcher = vi.spyOn(globalThis, 'fetch').mockImplementation(async path => new Response(JSON.stringify({ code: 0, data: String(path).endsWith('/submit')
      ? { cidNumber: missingTarget ? '' : regCid, purpose: 'INSTITUTION_REGISTER_ADMINS', txHash: '0x' + '44'.repeat(32), blockNumber: 11 } : correct ? after : before })));
    try {
      const run = () => runInstitutionRegistration({ auth, cidNumber: regCid, input: null, pending, sign, remember, check: () => {} });
      await expect(run()).rejects.toThrow('尚未核对'); expect(remember).not.toHaveBeenCalled();
      correct = true; missingTarget = true; await expect(run()).rejects.toThrow('尚未确认'); expect(remember).not.toHaveBeenCalled();
      missingTarget = false; expect((await run()).state.members).toHaveLength(4); expect(remember).toHaveBeenCalledWith(null);
      expect(sign).not.toHaveBeenCalled();
    } finally { fetcher.mockRestore(); vi.resetModules(); }
  });
  it('缺少审阅指纹的旧调用在Passkey前失败；名单错误不会伪装为空列表', async () => {
    vi.resetModules();
    const { prepareRegisterInstitutionAdmins, listCityRegistryAdmins } = await import('../admins/cityRegistryAdminsApi');
    const { assertPasskey } = await import('@gmb/onchina-platform/authentication/passkeyClient');
    const fetcher = vi.spyOn(globalThis, 'fetch').mockResolvedValue(new Response(JSON.stringify({ code: 0, data: { cidNumber: 'other-target', rows: [] } })));
    try {
      vi.mocked(assertPasskey).mockClear();
      await expect(prepareRegisterInstitutionAdmins(auth, { cidNumber: regCid, walletChanges: { added: [], removed: [], replaced: [] } } as never)).rejects.toThrow('核对');
      expect(assertPasskey).not.toHaveBeenCalled(); expect(fetcher).not.toHaveBeenCalled();
      await expect(listCityRegistryAdmins(auth, regCid)).rejects.toThrow('目标不匹配');
      expect(String(fetcher.mock.calls[0][0])).toContain(encodeURIComponent(regCid));
    } finally { fetcher.mockRestore(); vi.resetModules(); }
  });
});


describe('登记恢复记录与最终拒绝', () => {
  it('停用名单输入在认证前拒绝，旧签名冲突不按现状推断成功', async () => {
    vi.resetModules();
    const { prepareRegisterInstitutionAdmins, runInstitutionRegistration, saveRegistrationReceipt, readRegistrationReceipt } = await import('../admins/cityRegistryAdminsApi');
    const { assertPasskey } = await import('@gmb/onchina-platform/authentication/passkeyClient');
    const auth = {accessToken:'retired-input',accountId:regAccount(8),institutionCidNumber:regCid} as never;
    const expected = {cidNumber:regCid,expectedStateHash:'0x'+'ab'.repeat(32),admins:[{accountId:regAccount(2)}]};
    const pending = {requestId:'institution-governance-retired',cidNumber:regCid,accountId:regAccount(8),signature:'0x'+'55'.repeat(64),expected,reviewedBlockNumber:10};
    const fetcher=vi.spyOn(globalThis,'fetch').mockResolvedValue(new Response(JSON.stringify({code:2004,message:'旧名单动作停用'}),{status:409}));
    const sign=vi.fn(),remember=vi.fn();
    try {
      vi.mocked(assertPasskey).mockClear();
      await expect(prepareRegisterInstitutionAdmins(auth,expected)).rejects.toThrow('不接收');
      expect(assertPasskey).not.toHaveBeenCalled();expect(fetcher).not.toHaveBeenCalled();
      await expect(runInstitutionRegistration({auth,cidNumber:regCid,input:null,pending,sign,remember,check:()=>{}})).rejects.toThrow('停用');
      expect(fetcher).toHaveBeenCalledTimes(1);expect(String(fetcher.mock.calls[0][0])).toContain('/submit');
      expect(sign).not.toHaveBeenCalled();expect(remember).not.toHaveBeenCalled();
    } finally {fetcher.mockRestore();vi.resetModules();}
  });

  it('原签名恢复记录按管理员、办理机构和页面目标隔离', async () => {
    vi.resetModules();
    const { saveRegistrationReceipt, readRegistrationReceipt } = await import('../admins/cityRegistryAdminsApi');
    const { buildInstitutionRegistration } = await import('@gmb/onchina-platform/institutions/governance');
    const storage = new Map<string, string>();
    vi.stubGlobal('sessionStorage', { getItem: (key: string) => storage.get(key) ?? null, setItem: (key: string, value: string) => storage.set(key, value), removeItem: (key: string) => storage.delete(key) });
    const auth = { accountId: regAccount(8), institutionCidNumber: 'ZS001-FRG07-249474503-2026' } as never;
    const receipt = { requestId: 'institution-governance-stored', cidNumber: regCid, accountId: regAccount(8), signature: '0x' + '55'.repeat(64), reviewedBlockNumber: 10,
      expected: buildInstitutionRegistration(registrationState(), { addedAdmins: [regAdded] }) };
    try {
      saveRegistrationReceipt(auth, regCid, receipt);
      expect(readRegistrationReceipt(auth, regCid)).toEqual(receipt);
      const pure={...receipt,expected:{cidNumber:regCid,expectedStateHash:'0x'+'ab'.repeat(32),assignmentChanges:[]}};
      saveRegistrationReceipt(auth,regCid,pure);
      expect(readRegistrationReceipt(auth,regCid)).toEqual(pure);
      expect(readRegistrationReceipt({ ...auth as object, accountId: regAccount(7) } as never, regCid)).toBeNull();
      expect(readRegistrationReceipt(auth, 'different-target')).toBeNull();
      saveRegistrationReceipt(auth, regCid, null); expect(readRegistrationReceipt(auth, regCid)).toBeNull();
    } finally { vi.unstubAllGlobals(); vi.resetModules(); }
  });
  it('链端明确拒绝后清理原签名；结果不确定或回读不符时保持恢复记录', async () => {
    vi.resetModules();
    const { runInstitutionRegistration } = await import('../admins/cityRegistryAdminsApi');
    const { buildInstitutionRegistration } = await import('@gmb/onchina-platform/institutions/governance');
    const before = registrationState();
    const pending = { requestId: 'institution-governance-rejected', cidNumber: regCid, accountId: regAccount(8), signature: '0x' + '55'.repeat(64), reviewedBlockNumber: 10,
      expected: buildInstitutionRegistration(before, { addedAdmins: [regAdded] }) };
    const remember = vi.fn(), sign = vi.fn();
    const auth = { accessToken: 'fixture-session', accountId: regAccount(8) } as never;
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
    const { buildInstitutionAdminGovernance } = await import('@gmb/onchina-platform/institutions/governance');
    const state = registrationState();
    const auth = { accountId: regAccount(9), institutionCidNumber: state.cidNumber } as never;
    const input = buildInstitutionAdminGovernance(auth, state, { assignments: [{roleCode:'R_BUSINESS',assignments:[{accountId:regAccount(2),termStart:0,termEnd:0}]}] });
    expect(input.actorRoleCode).toBe('R_GOV');
    expect(input).not.toHaveProperty('admins');
    expect(() => buildInstitutionAdminGovernance(auth, state, { addedAdmins: [regAdded] })).toThrow('专用');
    expect(() => buildInstitutionAdminGovernance({ ...auth as object, institutionCidNumber: 'other' } as never, state, { addedAdmins: [regAdded] })).toThrow('本机构');
    expect(() => buildInstitutionAdminGovernance(auth, { ...state, moduleTag: 'pri-mgmt' }, { addedAdmins: [regAdded] })).toThrow('专用');
  });
  it('治理使用原call 8接口，不把提案上链误认作名册已生效', async () => {
    vi.resetModules();
    const { runInstitutionRegistration } = await import('../admins/cityRegistryAdminsApi');
    const { buildInstitutionAdminGovernance } = await import('@gmb/onchina-platform/institutions/governance');
    const state = registrationState();
    const auth = { accessToken: 'self-governance', accountId: regAccount(9), institutionCidNumber: state.cidNumber } as never;
    const input = buildInstitutionAdminGovernance(auth, state, { assignments: [{roleCode:'R_BUSINESS',assignments:[{accountId:regAccount(2),termStart:0,termEnd:0}]}] });
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
      expect(result.state.members).toHaveLength(3); // 仅提案上链，名单尚未变化。
      expect(bodies[0].path).toBe('/api/admin/institution/governance/prepare');
      expect(bodies[0].body).toMatchObject({ cidNumber: regCid, proposerRoleCode: 'R_GOV', assignmentChanges: input.assignmentChanges });
      expect(bodies[0].body).not.toHaveProperty('admins');
      expect(bodies[0].body).not.toHaveProperty('actorRoleCode');
      expect(bodies[0].body).not.toHaveProperty('expectedStateHash');
      expect(sign).toHaveBeenCalledTimes(1);
    } finally { fetcher.mockRestore(); vi.resetModules(); }
  });
});


describe('注册局管理员列与操作', () => {
  const browserTest = process.env.ONCHINA_TEST_DOM ? it : it.skip;
  browserTest('联邦和市注册局岗位姓名分列、展示当前钱包并保留原操作权限', async () => {
    vi.resetModules();
    const { createRequire } = await import('node:module');
    const { JSDOM } = createRequire(import.meta.url)(process.env.ONCHINA_TEST_DOM!);
    const dom = new JSDOM('<div id="root"></div>', { url: 'https://onchina.example.invalid/', pretendToBeVisual: true });
    const computed = dom.window.getComputedStyle.bind(dom.window);
    dom.window.getComputedStyle = (element: Element) => computed(element);
    dom.window.matchMedia = () => ({ matches: false, addListener() {}, removeListener() {}, addEventListener() {}, removeEventListener() {} });
    for (const name of ['window', 'document', 'navigator', 'HTMLElement', 'Element', 'ShadowRoot', 'SVGElement', 'getComputedStyle']) vi.stubGlobal(name, dom.window[name]);
    vi.stubGlobal('IS_REACT_ACT_ENVIRONMENT', true);
    const account = (byte: string) => '0x' + byte.repeat(32);
    const cityCid = 'GZ018-CREGM-159197206-2026';
    let auth = { accessToken: 'registry-table-session', accountId: account('11'), institutionCode: 'FRG', institutionCidNumber: 'ZS001-FRG07-249474503-2026', adminLevel: 'NATIONAL', scopeProvinceName: '贵州省', scopeCityName: null as string | null };
    const register = vi.fn(async () => {});
    const remove = vi.fn();
    vi.doMock('@gmb/onchina-platform/authentication/useAuth', () => ({ useAuth: () => ({ auth }) }));
    vi.doMock('@gmb/onchina-platform/authentication/usePasskey', () => ({ usePasskeyRegistration: () => ({ registered: false, busy: false, register }) }));
    const React = await import('react');
    vi.doMock('../government/GovDetailPage', () => ({ GovDetailPage: ({ adminListSection }: { adminListSection: import('react').ReactNode }) => React.createElement('div', null, adminListSection) }));
    const { createRoot } = await import('react-dom/client');
    const { FederalRegistryAdminSubTab } = await import('../admins/FederalRegistryAdminSubTab');
    const { CityRegistryView } = await import('../admins/ProvinceDetailView');
    const { decodeSs58 } = await import('@gmb/onchina-platform/codes/ss58');
    const host = document.getElementById('root')!;
    const root = createRoot(host);
    const common = { termRequired: false, termStart: 0, termEnd: 0, assignmentSource: 0, assignmentSourceLabel: '创世', assignmentSourceRef: '', builtIn: true, createdAt: '2026-01-01T00:00:00Z' };
    const federalRows = [
      { ...common, id: 1, provinceName: '贵州省', accountId: account('11'), familyName: '张', givenName: '三', roleCode: 'PROVINCE_COMMISSIONER_GZ', roleName: '贵州省专员', balanceFen: '1200' },
      { ...common, id: 2, provinceName: '广东省', accountId: account('44'), familyName: '', givenName: '', roleCode: 'PROVINCE_COMMISSIONER_GD', roleName: '广东省专员', balanceFen: '0' },
    ];
    const cityRows = federalRows.map((row, i) => ({ ...row, id: String(row.id), institutionCidNumber: cityCid, rosterAccountId: account(i === 0 ? '22' : '33'), citizenCidNumber: 'fixture-person', institutionCode: 'CREG', cityName: '贵阳市', roleName: i === 0 ? '登记岗' : '复核岗' }));
    const state = { selectedFederalRegistry: federalRows[0], selectedCity: '贵阳市', setSelectedCity: vi.fn(), cityRegistryAdmins: cityRows, cityRegistryAdminsLoading: false, cityRegistryAdminsError: null, refreshCityRegistryAdmins: vi.fn(async () => cityRows), cityRegistryAdminListPage: 1, setCityRegistryListPage: vi.fn(), cityRegistryAdminCities: [], cityRegistryAdminCitiesLoading: false, setAddCityRegistryOpen: vi.fn(), onDeleteCityRegistry: remove, cityRegistryCid: cityCid, cityRegistryLoading: false, cityRegistryError: null, reloadCityRegistry: vi.fn() };
    try {
      await React.act(async () => root.render(React.createElement(FederalRegistryAdminSubTab, { selectedFederalRegistry: federalRows[0], federalRegistryAdmins: federalRows, federalRegistryAdminsLoading: false, institutionCidNumber: auth.institutionCidNumber })));
      expect([...host.querySelectorAll('thead th')].map(cell => cell.textContent)).toEqual(['序号', '省份', '岗位', '姓名', '钱包账户', '余额', '操作']);
      let actual = [...host.querySelectorAll('tbody tr[data-row-key]')];
      let first = actual[0].querySelectorAll('td');
      expect(first[2].textContent).toBe('贵州省专员'); expect(first[3].textContent).toBe('张三');
      expect(decodeSs58(first[4].textContent!)).toBe(federalRows[0].accountId);
      expect(first[5].textContent).toBe('12.00 元'); expect(first[6].textContent?.replace(/\s/g, '')).toBe('密钥更换');
      expect(actual[1].querySelectorAll('td')[2].textContent).toBe('广东省专员');
      expect(actual[1].querySelectorAll('td')[3].textContent).toBe('-');
      expect(actual[1].querySelectorAll('td')[6].textContent).toBe('');
      await React.act(async () => first[6].querySelector('button')!.click());
      expect(register).toHaveBeenCalledTimes(1);
      expect(document.body.textContent).not.toContain('管理员岗位任职');
      await React.act(async () => root.render(React.createElement(CityRegistryView, { state: state as never })));
      expect([...host.querySelectorAll('thead th')].map(cell => cell.textContent)).toEqual(['序号', '岗位', '姓名', '钱包账户', '余额', '操作']);
      actual = [...host.querySelectorAll('tbody tr[data-row-key]')]; first = actual[0].querySelectorAll('td');
      expect(first[1].textContent).toBe('登记岗'); expect(first[2].textContent).toBe('张三');
      expect(decodeSs58(first[3].textContent!)).toBe(cityRows[0].accountId);
      expect(decodeSs58(first[3].textContent!)).not.toBe(cityRows[0].rosterAccountId);
      expect(first[5].textContent?.replace(/\s/g, '')).toBe('删除密钥');
      expect(actual[1].querySelectorAll('td')[5].textContent?.replace(/\s/g, '')).toBe('删除');
      await React.act(async () => first[5].querySelector('button')!.click());
      expect(remove).toHaveBeenCalledWith(cityRows[0]);
      auth = { ...auth, institutionCode: 'CREG', institutionCidNumber: cityCid, adminLevel: 'CITY', scopeCityName: '贵阳市' };
      await React.act(async () => root.render(React.createElement(CityRegistryView, { state: state as never })));
      actual = [...host.querySelectorAll('tbody tr[data-row-key]')];
      expect(actual[0].querySelectorAll('td')[5].textContent?.replace(/\s/g, '')).toBe('更换密钥');
      expect(actual[1].querySelectorAll('td')[5].textContent).toBe('');
      expect([...host.querySelectorAll('button')].some(button => button.textContent?.replace(/\s/g, '') === '删除')).toBe(false);
      expect(cityRows[0].accountId).toBe(account('11'));
    } finally {
      await React.act(async () => root.unmount());
      vi.doUnmock('@gmb/onchina-platform/authentication/useAuth'); vi.doUnmock('@gmb/onchina-platform/authentication/usePasskey'); vi.doUnmock('../government/GovDetailPage');
      dom.window.close(); vi.unstubAllGlobals(); vi.resetModules();
    }
  });
});


(process.env.ONCHINA_TEST_DOM ? it : it.skip)('钱包登记与首次治理配置具有独立表单，不自动要求岗位或阈值',async()=>{
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
  const {InstitutionAdminChangeModal}=await import('../admins/AddCityRegistryAdminModal');
  const state={...registrationState(),governanceThreshold:null};
  const fetcher=vi.spyOn(globalThis,'fetch').mockResolvedValue(new Response(JSON.stringify({code:0,data:state})));
  const root=createRoot(document.getElementById('wallet-root')!);
  const props={auth:{accessToken:'wallet-form',accountId:regAccount(8),institutionCidNumber:'ZS001-FRG07-249474503-2026'} as never,cidNumber:regCid,open:true,onClose:vi.fn(),onSubmitted:vi.fn()};
  try{
    await React.act(async()=>{root.render(React.createElement(InstitutionAdminChangeModal,props));await new Promise(r=>setTimeout(r,30));});
    expect(document.querySelector('[aria-label="管理员账户1"]')).not.toBeNull();
    expect(document.querySelector('[aria-label="初始治理阈值"]')).toBeNull();
    expect(document.body.textContent).not.toContain('添加动态岗位');
    fetcher.mockResolvedValue(new Response(JSON.stringify({code:0,data:state})));
    await React.act(async()=>{root.render(React.createElement(InstitutionAdminChangeModal,{...props,configuration:true}));await new Promise(r=>setTimeout(r,30));});
    expect(document.querySelector('[aria-label="管理员账户1"]')).toBeNull();
    expect(document.querySelector('[aria-label="初始治理阈值"]')).not.toBeNull();
    expect(document.body.textContent).toContain('添加动态岗位');
    expect(fetcher.mock.calls.every(([path])=>String(path).endsWith('/governance'))).toBe(true);
  }finally{await React.act(async()=>root.unmount());fetcher.mockRestore();dom.window.close();vi.unstubAllGlobals();vi.doUnmock('@gmb/onchina-platform/qr/useChainSign');}
},15000);

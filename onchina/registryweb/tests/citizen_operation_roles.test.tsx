import '@gmb/onchina-platform/testing';
import { createElement } from 'react';
import { renderToStaticMarkup } from 'react-dom/server';
import { describe, it, expect, vi } from 'vitest';
import { OperationRoleSelect, operationRoleValue } from '../citizens/OperationRoleSelect';
import type { CitizenOperationRole } from '../citizens/api';

const roles: CitizenOperationRole[] = [
  { roleCode: 'INTERNAL_ROLE_A', roleName: '公民身份登记员' },
  { roleCode: 'INTERNAL_ROLE_B', roleName: '登记负责人' },
];
function control(options: CitizenOperationRole[], value?: string) {
  return { roles: options, roleCode: operationRoleValue(options, value), loading: false, error: undefined,
    ready: !!operationRoleValue(options, value), onChange: vi.fn(), retry: vi.fn() };
}

describe('公民业务自动办理岗位', () => {
  it('唯一候选自动使用，页面仅显示中文岗位名称，不需要输入或选择', () => {
    const selected = control(roles.slice(0, 1));
    expect(selected.roleCode).toBe('INTERNAL_ROLE_A');
    expect(selected.ready).toBe(true);
    const markup = renderToStaticMarkup(createElement(OperationRoleSelect, { control: selected }));
    expect(markup).toContain('公民身份登记员');
    expect(markup).not.toContain('INTERNAL_ROLE_A');
    expect(markup).not.toContain('<input');
  });
  it('多个候选必须选择，选择后显示中文名称，不能接受候选外的岗位', () => {
    expect(control(roles).ready).toBe(false);
    expect(control(roles, 'FOREIGN_ROLE').ready).toBe(false);
    const selected = control(roles, 'INTERNAL_ROLE_B');
    expect(selected.ready).toBe(true);
    const markup = renderToStaticMarkup(createElement(OperationRoleSelect, { control: selected }));
    expect(markup).toContain('登记负责人');
    expect(markup).not.toContain('INTERNAL_ROLE_');
    expect(markup).toContain('role="combobox"');
    expect(markup.toLowerCase()).toContain('readonly');
  });
  it('没有岗位、加载中、查询失败分别显示对应状态，失败可重试', () => {
    const empty = control([]);
    expect(empty.ready).toBe(false);
    expect(renderToStaticMarkup(createElement(OperationRoleSelect, { control: empty }))).toContain('无此业务办理权限');
    expect(renderToStaticMarkup(createElement(OperationRoleSelect, { control: { ...empty, loading: true } }))).toContain('正在加载');
    expect(renderToStaticMarkup(createElement(OperationRoleSelect, { control: { ...empty, error: '办理岗位加载失败' } }))).toMatch(/重\s*试/);
  });
  it('每个操作单独查询，CID正确编码，读取不消费Passkey', async () => {
    const { fetchCitizenOperationRoles } = await import('../citizens/api');
    const { assertPasskey } = await import('@gmb/onchina-platform/authentication/passkeyClient');
    const fetcher = vi.spyOn(globalThis, 'fetch').mockImplementation(async () => new Response(JSON.stringify({ code: 0, data: { roles, finalizedBlockHash: 'block', finalizedBlockNumber: 1 } })));
    try {
      vi.mocked(assertPasskey).mockClear();
      for (const operation of ['register_voting_identity', 'upgrade_candidate_identity', 'rebind_wallet', 'revoke_identity'] as const) {
        const result = await fetchCitizenOperationRoles({ accessToken: 'session' } as never, 'cid /&', operation);
        expect(result.roles).toEqual(roles);
        const [url, init] = fetcher.mock.calls[fetcher.mock.calls.length - 1];
        expect(url).toBe(`/api/admin/citizens/cid%20%2F%26/operation-roles?operation=${operation}`);
        expect(new Headers(init?.headers).get('authorization')).toBe('Bearer session');
        expect(init?.body).toBeUndefined();
      }
      expect(assertPasskey).not.toHaveBeenCalled();
    } finally { fetcher.mockRestore(); }
  });
});

// 与本仓已有浏览器回归共用显式交付的 JSDOM，不安装或更改工程依赖。
const domTest = process.env.ONCHINA_TEST_DOM ? it : it.skip;
domTest('切换公民/业务后丢弃旧请求，唯一岗位自动确定，多岗位选择中文名称，失败重试', async () => {
  const { createRequire } = await import('node:module');
  const { JSDOM } = createRequire(import.meta.url)(process.env.ONCHINA_TEST_DOM!);
  const dom = new JSDOM('<div id="root"></div>', { url: 'https://onchina.example.invalid/' });
  dom.window.matchMedia = () => ({ matches: false, addListener() {}, removeListener() {}, addEventListener() {}, removeEventListener() {} });
  for (const name of ['window', 'document', 'navigator', 'HTMLElement', 'Element', 'ShadowRoot', 'SVGElement']) vi.stubGlobal(name, dom.window[name]);
  vi.stubGlobal('getComputedStyle', (element: Element) => dom.window.getComputedStyle(element));
  vi.stubGlobal('IS_REACT_ACT_ENVIRONMENT', true);
  const React = await import('react');
  const { createRoot } = await import('react-dom/client');
  const { useCitizenOperationRoles } = await import('../citizens/OperationRoleSelect');
  const auth = { accessToken: 'session', accountId: 'admin', institutionCidNumber: 'registry' } as never;
  type Control = ReturnType<typeof useCitizenOperationRoles>;
  let current: Control | undefined;
  function Probe({ cid, operation }: { cid: string; operation: 'rebind_wallet' | 'revoke_identity' }) {
    current = useCitizenOperationRoles(auth, cid, operation, true, 1);
    return createElement('div', null,
      createElement(OperationRoleSelect, { control: current }),
      createElement('button', { id: 'action', disabled: !current.ready }, '办理'),
    );
  }
  const pending: Array<(response: Response) => void> = [];
  const fetcher = vi.spyOn(globalThis, 'fetch').mockImplementation(() => new Promise<Response>((resolve) => pending.push(resolve)));
  const response = (items: CitizenOperationRole[]) => new Response(JSON.stringify({ code: 0, data: { roles: items, finalizedBlockHash: 'block', finalizedBlockNumber: 1 } }));
  const root = createRoot(document.getElementById('root')!);
  try {
    await React.act(async () => root.render(createElement(Probe, { cid: 'first', operation: 'rebind_wallet' })));
    expect((document.getElementById('action') as HTMLButtonElement).disabled).toBe(true);
    await React.act(async () => root.render(createElement(Probe, { cid: 'second', operation: 'revoke_identity' })));
    await React.act(async () => { pending[0](response(roles.slice(0, 1))); });
    expect(current?.ready).toBe(false);
    await React.act(async () => { pending[1](response(roles)); });
    expect(current?.ready).toBe(false);
    await React.act(async () => { current?.onChange('INTERNAL_ROLE_B'); });
    expect(current?.ready).toBe(true);
    expect(document.body.textContent).toContain('登记负责人');
    expect(document.body.textContent).not.toContain('INTERNAL_ROLE_');
    expect((document.querySelector('input') as HTMLInputElement).readOnly).toBe(true);
    await React.act(async () => root.render(createElement(Probe, { cid: 'third', operation: 'rebind_wallet' })));
    expect(current?.roleCode).toBeUndefined();
    await React.act(async () => { pending[2](new Response('unavailable', { status: 502 })); });
    expect(current?.ready).toBe(false);
    expect(document.body.textContent).toContain('办理岗位加载失败');
    await React.act(async () => { current?.retry(); });
    expect(current?.loading).toBe(true);
    await React.act(async () => { pending[3](response(roles.slice(0, 1))); });
    expect(current?.ready).toBe(true);
    expect(current?.roleCode).toBe('INTERNAL_ROLE_A');
    expect(document.querySelector('input')).toBeNull();
    expect((document.getElementById('action') as HTMLButtonElement).disabled).toBe(false);
  } finally {
    await React.act(async () => root.unmount());
    fetcher.mockRestore(); vi.unstubAllGlobals(); dom.window.close();
  }
});

it('上链只在创建管理员冷签会话时验证一次Passkey，换绑和吊销在prepare验证', async () => {
  const api = await import('../citizens/api');
  const { assertPasskey, PASSKEY_ASSERTION_HEADER } = await import('@gmb/onchina-platform/authentication/passkeyClient');
  const auth = { accessToken: 'session' } as never;
  const fetcher = vi.spyOn(globalThis, 'fetch').mockImplementation(async () => new Response(JSON.stringify({ code: 0, data: {} })));
  try {
    vi.mocked(assertPasskey).mockClear();
    await api.prepareCitizenOnchainSignature(auth, 'cid', 'wallet', undefined, 'voting');
    expect(assertPasskey).not.toHaveBeenCalled();
    expect(JSON.parse(String(fetcher.mock.calls[0][1]?.body))).toEqual({ accountId: 'wallet', identityLevel: 'voting' });
    await api.completeCitizenOnchainSignature(auth, 'cid', 'wallet', 'FROZEN_ROLE', 'voting', 'signature');
    expect(assertPasskey).toHaveBeenCalledTimes(1);
    expect(new Headers(fetcher.mock.calls[1][1]?.headers).get(PASSKEY_ASSERTION_HEADER)).toBe('fake-passkey-assertion');
    expect(JSON.parse(String(fetcher.mock.calls[1][1]?.body)).actorRoleCode).toBe('FROZEN_ROLE');
    for (const prepare of [
      () => api.prepareCitizenRebind(auth, 'cid', undefined),
      () => api.prepareCitizenRevoke(auth, 'cid', 'INTERNAL_ROLE_A'),
    ]) {
      vi.mocked(assertPasskey).mockClear(); fetcher.mockClear();
      await prepare();
      expect(assertPasskey).toHaveBeenCalledTimes(1);
      expect(new Headers(fetcher.mock.calls[0][1]?.headers).get(PASSKEY_ASSERTION_HEADER)).toBe('fake-passkey-assertion');
    }
  } finally { fetcher.mockRestore(); }
});

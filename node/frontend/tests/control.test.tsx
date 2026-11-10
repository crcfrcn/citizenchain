import {beforeEach, expect, it, vi} from 'vitest';
import {homeNodeApi} from '../control/api';
const mocks=vi.hoisted(()=>({invoke:vi.fn()}));
vi.mock('../client/tauri',()=>({invoke:mocks.invoke,sanitizeError:(error:unknown)=>String(error)}));
const api=homeNodeApi as unknown as Record<string,(...args:unknown[])=>Promise<unknown>>;
beforeEach(()=>{mocks.invoke.mockReset();});
it("getNodeStatus保留唯一IPC合同且返回真实回执", async () => { const receipt = { receipt: '合成回执' }; mocks.invoke.mockResolvedValueOnce(receipt); await expect(api["getNodeStatus"](...[])).resolves.toBe(receipt); expect(mocks.invoke).toHaveBeenCalledExactlyOnceWith("get_node_status"); });
it("startNode保留唯一IPC合同且返回真实回执", async () => { const receipt = { receipt: '合成回执' }; mocks.invoke.mockResolvedValueOnce(receipt); await expect(api["startNode"](...[])).resolves.toBe(receipt); expect(mocks.invoke).toHaveBeenCalledExactlyOnceWith("start_node"); });
it("stopNode保留唯一IPC合同且返回真实回执", async () => { const receipt = { receipt: '合成回执' }; mocks.invoke.mockResolvedValueOnce(receipt); await expect(api["stopNode"](...[])).resolves.toBe(receipt); expect(mocks.invoke).toHaveBeenCalledExactlyOnceWith("stop_node"); });
it("getTotalIssuance保留唯一IPC合同且返回真实回执", async () => { const receipt = { receipt: '合成回执' }; mocks.invoke.mockResolvedValueOnce(receipt); await expect(api["getTotalIssuance"](...[])).resolves.toBe(receipt); expect(mocks.invoke).toHaveBeenCalledExactlyOnceWith("get_total_issuance"); });
it('IPC失败保留拒绝状态且不自动重放', async()=>{const failure=new Error('合成失败');mocks.invoke.mockRejectedValueOnce(failure);await expect(api["getNodeStatus"](...[])).rejects.toBe(failure);expect(mocks.invoke).toHaveBeenCalledOnce();});

import {shouldShowDesktopUpdateDot} from '../control/updateIndicator';
it.each(['checking','unavailable','error'] as const)('状态%s不显示新版标记', status => { expect(shouldShowDesktopUpdateDot({status})).toBe(false); });
it('已发现新版持续显示标记', () => { expect(shouldShowDesktopUpdateDot({status:'available'})).toBe(true); });

import {fireEvent, render, screen, waitFor, within} from '@testing-library/react';
import {OtherTabsSection} from '../control/OtherTabsSection';
vi.mock('../control/LocalDocViewer',()=>({LocalDocViewer:({doc}:{doc:{markdown:string}})=><article>{doc.markdown}</article>}));
vi.mock('../guard/RuntimeConstitutionViewer',()=>({RuntimeConstitutionViewer:()=>null}));
it('白皮书请求中可切换、失败可重试、成功后重新进入复用正文', async()=>{
  let reject!:(error:Error)=>void;
  mocks.invoke.mockImplementationOnce(()=>new Promise((_resolve,fail)=>{reject=fail;}));
  const first=render(<OtherTabsSection activeKey="whitepaper"/>);
  expect(screen.getByText('白皮书加载中...')).toBeTruthy();
  first.unmount();
  const second=render(<OtherTabsSection activeKey="whitepaper"/>);
  expect(mocks.invoke).toHaveBeenCalledOnce();
  reject(new Error('合成读取失败'));
  await waitFor(()=>expect(screen.getByText('Error: 合成读取失败')).toBeTruthy());
  mocks.invoke.mockResolvedValueOnce({markdown:'# 已加载正文'});
  fireEvent.click(screen.getByRole('button',{name:'重试'}));
  await waitFor(()=>expect(screen.getByText('# 已加载正文')).toBeTruthy());
  expect(mocks.invoke).toHaveBeenCalledTimes(2);
  second.unmount();
  const third=render(<OtherTabsSection activeKey="whitepaper"/>);
  expect(screen.getByText('# 已加载正文')).toBeTruthy();
  expect(mocks.invoke).toHaveBeenCalledTimes(2);
  third.unmount();
});


import {SettingsSection} from '../control/SettingsSection';
vi.mock('../control/NodeModeSection',()=>({NodeModeSection:()=>null}));
import {OnChinaPlatformSection} from '../control/OnChinaPlatformSection';
vi.mock('../control/RewardAccountSection',()=>({RewardAccountSection:()=>null}));
it.each([
  ['available','安装更新',false,0], ['downloading','下载中',true,45],
  ['ready','安装更新',false,100], ['installing','安装中',true,100],
  ['error','重试下载',false,0],
] as const)('设置更新状态%s显示正确按钮与实际进度',async(status,label,disabled,downloaded)=>{
  mocks.invoke.mockImplementation(async(command:string)=>command==='get_chain_status'
    ? {nodeVersion:'1.0.0',specVersion:2,transactionVersion:1} : null);
  const onUpdateDesktop=vi.fn().mockResolvedValue(undefined);
  const view=render(<SettingsSection desktopUpdateInfo={{status,currentVersion:'1.0.0',latestVersion:'1.0.1',
    downloaded,total:100,error:status==='error'?'下载中断':null}} onUpdateDesktop={onUpdateDesktop}/>);
  const button=await screen.findByRole('button',{name:label});
  expect((button as HTMLButtonElement).disabled).toBe(disabled);
  if(['downloading','ready','installing'].includes(status)){
    expect((screen.getByRole('progressbar',{name:'新版下载进度'}) as HTMLProgressElement).value).toBe(downloaded);
    expect(screen.getByText(`${downloaded}%`)).toBeTruthy();
  }
  fireEvent.click(button);
  expect(onUpdateDesktop).toHaveBeenCalledTimes(disabled?0:1);
  view.unmount();
});

it.each([[0, '0'], [null, '未取得链上版本']] as const)(
  '设置页显示节点程序、公民协议和交易签名版本 %s',
  async (transactionVersion, expected) => {
    mocks.invoke.mockImplementation(async (command: string) => command === 'get_chain_status'
      ? { nodeVersion: '1.0.3', specVersion: 7, transactionVersion }
      : null);
    render(<SettingsSection desktopUpdateInfo={{ status: 'unavailable', currentVersion: '1.0.0',
      latestVersion: null, error: null }} onUpdateDesktop={vi.fn()}/>);
    const label = await screen.findByText('交易签名版本');
    const row = label.closest('.settings-version-row');
    expect(row?.querySelector('.settings-version-value')?.textContent).toBe(expected);
    expect(row?.previousElementSibling?.textContent).toContain('公民协议版本');
    expect(row?.previousElementSibling?.textContent).toContain('Runtime 7');
    const nodeVersionRow = screen.getByText('节点程序版本').closest('.settings-version-row');
    expect(nodeVersionRow?.querySelector('.settings-version-value')?.textContent).toBe('v1.0.3');
  },
);

it('设置页显示引导节点和最终性节点的标题、私钥占位和空输入提示', async () => {
  mocks.invoke.mockImplementation(async (command: string) => {
    if (command === 'has_any_activated_admin') return true;
    if (command === 'get_bootnode_key') return { nodeKey: null, peerId: null, cidShortName: null };
    if (command === 'get_grandpa_key') return { key: null, cidShortName: null };
    return null;
  });
  render(<SettingsSection desktopUpdateInfo={{ status: 'unavailable', currentVersion: '1.0.0',
    latestVersion: null, error: null }} onUpdateDesktop={vi.fn()}/>);
  expect(await screen.findByRole('heading', { name: /^引导节点/ })).toBeTruthy();
  expect(screen.getByRole('heading', { name: /^最终性节点/ })).toBeTruthy();
  expect(screen.getByPlaceholderText('请输入引导节点私钥（Ed25519 十六进制）')).toBeTruthy();
  expect(screen.getByPlaceholderText('请输入最终性节点私钥')).toBeTruthy();
  fireEvent.click(screen.getByRole('button', { name: '绑定身份' }));
  expect(screen.getByText('请输入引导节点私钥')).toBeTruthy();
  fireEvent.click(screen.getByRole('button', { name: '上传私钥' }));
  expect(screen.getByText('请输入最终性节点私钥')).toBeTruthy();
});

it('链上中国主机在设备密码验证后提交完整多行 PEM 并更新设置状态', async () => {
  mocks.invoke.mockImplementation(async (command: string) => {
    if (command === 'has_any_activated_admin') return true;
    if (command === 'get_bootnode_key') return { peerId: 'test-peer-id' };
    if (command === 'get_grandpa_key') return { cidShortName: null };
    if (command === 'get_onchina_host_key_status') return false;
    if (command === 'set_onchina_host_key') return undefined;
    return null;
  });
  render(<SettingsSection desktopUpdateInfo={{ status: 'unavailable', currentVersion: '1.0.0',
    latestVersion: null, error: null }} onUpdateDesktop={vi.fn()}/>);
  const heading = await screen.findByRole('heading', { name: '链上中国主机' });
  const hostRow = within(heading.closest<HTMLElement>('.settings-credential-row')!);
  await hostRow.findByText('未设置');
  const field = hostRow.getByPlaceholderText<HTMLInputElement>('请输入链上中国主机HTTPS密钥');
  const pem = '-----BEGIN PRIVATE KEY-----\nfixture\n-----END PRIVATE KEY-----';
  fireEvent.paste(field, { clipboardData: { getData: () => pem } });
  fireEvent.click(hostRow.getByRole('button', { name: '上传链上中国主机私钥' }));
  expect(mocks.invoke.mock.calls.some(([command]) => command === 'set_onchina_host_key')).toBe(false);
  fireEvent.change(screen.getByPlaceholderText('请输入设备开机密码'), { target: { value: 'fixture-password' } });
  fireEvent.click(screen.getByRole('button', { name: '确认' }));
  await hostRow.findByText('已设置');
  expect(mocks.invoke).toHaveBeenCalledWith('set_onchina_host_key', { key: pem, unlockPassword: 'fixture-password' });
  expect(field.value).toBe('');
});
it.each(['downloading','ready','installing'] as const)('状态%s保留设置新版标记',status=>{
  expect(shouldShowDesktopUpdateDot({status})).toBe(true);
});

it('下载失败仍保留已知新版标记',()=>expect(shouldShowDesktopUpdateDot({status:'error',latestVersion:'1.0.1'})).toBe(true));

it('链上中国未取得产品说明时显示入口加载中，不显示替代地址',async()=>{
  mocks.invoke.mockImplementation(() => new Promise(() => {}));
  const view=render(<OnChinaPlatformSection/>);
  expect(screen.getByText('入口加载中')).toBeTruthy();
  expect((screen.getByRole('button',{name:'启动链上中国'}) as HTMLButtonElement).disabled).toBe(true);
  view.unmount();
});
it('链上中国只显示宿主交付的入口和具体启动诊断',async()=>{
  mocks.invoke.mockResolvedValue({running:false,status:'error',statusLabel:'启动失败',url:'https://product.example:1988/',detail:'主机尚未配置固定地址',modules:[]});
  const view=render(<OnChinaPlatformSection/>);
  expect(await screen.findByText('https://product.example:1988')).toBeTruthy();
  const status = screen.getByRole('status');
  expect(status.getAttribute('aria-label')).toBe('启动失败');
  expect(status.getAttribute('data-status')).toBe('error');
  expect(status.classList.contains('error')).toBe(false);
  expect(status.nextElementSibling).toBe(screen.getByText('链上中国平台'));
  expect(screen.getByText('主机尚未配置固定地址')).toBeTruthy();
  view.unmount();
});

it('链上中国只允许打开已就绪模块，并只传模块 ID', async () => {
  mocks.invoke.mockImplementation(async (command: string) => {
    if (command === 'get_onchina_platform') {
      return {running:true,status:'enabled',statusLabel:'已开启',url:'https://product.example:1988/',modules:['registry']};
    }
    if (command === 'open_onchina_module') throw new Error('浏览器不可用');
    throw new Error(command);
  });
  const view = render(<OnChinaPlatformSection />);
  await screen.findByRole('status', {name:'已开启'});
  fireEvent.click(screen.getByRole('button', {name:'关闭链上中国'}));
  expect(screen.getByText('确认关闭链上中国平台？')).toBeTruthy();
  fireEvent.click(screen.getByRole('button', {name:'取消'}));
  const available = screen.getByRole<HTMLButtonElement>('button', {name:/^注册局/});
  const unavailable = screen.getByRole<HTMLButtonElement>('button', {name:/^立法/});
  expect(available.disabled).toBe(false);
  expect(unavailable.disabled).toBe(true);
  fireEvent.click(unavailable);
  expect(mocks.invoke).toHaveBeenCalledTimes(1);
  fireEvent.click(available);
  expect((await screen.findByRole('alert')).textContent).toContain('浏览器不可用');
  expect(mocks.invoke).toHaveBeenLastCalledWith('open_onchina_module', {moduleId:'registry'});
  view.unmount();
});

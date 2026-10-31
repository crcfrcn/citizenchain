import {beforeEach, expect, it, vi} from 'vitest';
import {homeNodeApi} from '../control/api';
const mocks=vi.hoisted(()=>({invoke:vi.fn()}));
vi.mock('../client/tauri',()=>({invoke:mocks.invoke,sanitizeError:(error:unknown)=>String(error)}));
const api=homeNodeApi as unknown as Record<string,(...args:unknown[])=>Promise<unknown>>;
beforeEach(()=>mocks.invoke.mockReset());
it("getNodeStatus保留唯一IPC合同且返回真实回执", async () => { const receipt = { receipt: '合成回执' }; mocks.invoke.mockResolvedValueOnce(receipt); await expect(api["getNodeStatus"](...[])).resolves.toBe(receipt); expect(mocks.invoke).toHaveBeenCalledExactlyOnceWith("get_node_status"); });
it("startNode保留唯一IPC合同且返回真实回执", async () => { const receipt = { receipt: '合成回执' }; mocks.invoke.mockResolvedValueOnce(receipt); await expect(api["startNode"](...[])).resolves.toBe(receipt); expect(mocks.invoke).toHaveBeenCalledExactlyOnceWith("start_node"); });
it("stopNode保留唯一IPC合同且返回真实回执", async () => { const receipt = { receipt: '合成回执' }; mocks.invoke.mockResolvedValueOnce(receipt); await expect(api["stopNode"](...[])).resolves.toBe(receipt); expect(mocks.invoke).toHaveBeenCalledExactlyOnceWith("stop_node"); });
it("getTotalIssuance保留唯一IPC合同且返回真实回执", async () => { const receipt = { receipt: '合成回执' }; mocks.invoke.mockResolvedValueOnce(receipt); await expect(api["getTotalIssuance"](...[])).resolves.toBe(receipt); expect(mocks.invoke).toHaveBeenCalledExactlyOnceWith("get_total_issuance"); });
it('IPC失败保留拒绝状态且不自动重放', async()=>{const failure=new Error('合成失败');mocks.invoke.mockRejectedValueOnce(failure);await expect(api["getNodeStatus"](...[])).rejects.toBe(failure);expect(mocks.invoke).toHaveBeenCalledOnce();});

import {shouldShowDesktopUpdateDot} from '../control/updateIndicator';
it.each(['checking','unavailable','error'] as const)('状态%s不显示新版标记', status => { expect(shouldShowDesktopUpdateDot({status})).toBe(false); });
it('已发现新版持续显示标记', () => { expect(shouldShowDesktopUpdateDot({status:'available'})).toBe(true); });

import {fireEvent, render, screen, waitFor} from '@testing-library/react';
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
vi.mock('../control/OnChinaPlatformSection',()=>({OnChinaPlatformSection:()=>null}));
vi.mock('../control/RewardAccountSection',()=>({RewardAccountSection:()=>null}));
it.each([
  ['available','安装更新',false,0], ['downloading','下载中',true,45],
  ['ready','安装更新',false,100], ['installing','安装中',true,100],
  ['error','重试下载',false,0],
] as const)('设置更新状态%s显示正确按钮与实际进度',async(status,label,disabled,downloaded)=>{
  mocks.invoke.mockImplementation(async(command:string)=>command==='get_chain_status'
    ? {nodeVersion:'1.0.0',specVersion:2} : null);
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
it.each(['downloading','ready','installing'] as const)('状态%s保留设置新版标记',status=>{
  expect(shouldShowDesktopUpdateDot({status})).toBe(true);
});

it('下载失败仍保留已知新版标记',()=>expect(shouldShowDesktopUpdateDot({status:'error',latestVersion:'1.0.1'})).toBe(true));

it('链上中国未取得产品说明时显示入口加载中，不显示替代地址',async()=>{
  const {OnChinaPlatformSection}=await vi.importActual<typeof import('../control/OnChinaPlatformSection')>('../control/OnChinaPlatformSection');
  const view=render(<OnChinaPlatformSection platform={null} onUpdated={vi.fn()}/>);
  expect(screen.getByText('入口加载中')).toBeTruthy();
  expect((screen.getByRole('button',{name:'启动'}) as HTMLButtonElement).disabled).toBe(true);
  view.unmount();
});
it('链上中国只显示宿主交付的入口和具体启动诊断',async()=>{
  const {OnChinaPlatformSection}=await vi.importActual<typeof import('../control/OnChinaPlatformSection')>('../control/OnChinaPlatformSection');
  const view=render(<OnChinaPlatformSection platform={{running:false,status:'error',statusLabel:'启动失败',url:'https://product.example:1988/',detail:'主机尚未配置固定地址'}} onUpdated={vi.fn()}/>);
  expect(screen.getByText('https://product.example:1988')).toBeTruthy();
  const status = screen.getByRole('status');
  expect(status.textContent?.trim()).toBe('启动失败');
  expect(status.getAttribute('data-status')).toBe('error');
  expect(status.classList.contains('error')).toBe(false);
  expect(status.parentElement).toBe(screen.getByText('链上中国平台').parentElement);
  expect(screen.getByText('主机尚未配置固定地址')).toBeTruthy();
  view.unmount();
});

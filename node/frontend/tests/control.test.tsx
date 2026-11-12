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

import {act, fireEvent, render, screen, waitFor, within} from '@testing-library/react';
import {IssuanceSection} from '../control/IssuanceSection';
import {HomeNodeSection} from '../control/HomeNodeSection';
import {OtherTabsSection} from '../control/OtherTabsSection';
import type {NodeStatus} from '../control/types';

const runningNode: NodeStatus = {running:true,state:'running',pid:1,lastError:null};
const stoppedNode: NodeStatus = {running:false,state:'stopped',pid:null,lastError:null};
const homeReceipts: Record<string, unknown> = {
  get_chain_status: {blockHeight:10,finalizedHeight:9,syncing:false,specVersion:1,transactionVersion:1,nodeVersion:'1.0.3'},
  get_node_identity: {peerId:null,role:null,genesisHash:null},
  get_total_issuance: {totalIssuance:'100.00'},
  get_total_stake: {totalStake:'20.00'},
  get_total_frozen: {totalFrozen:'75.25'},
  get_local_miner_ss58_address: null,
};
function deferred<T>() {
  let resolve!: (value:T) => void;
  let reject!: (error:Error) => void;
  const promise = new Promise<T>((yes,no) => {resolve=yes;reject=no;});
  return {promise,resolve,reject};
}
function mockHome(status: () => Promise<NodeStatus>, overrides: Record<string, () => Promise<unknown>> = {}) {
  mocks.invoke.mockImplementation((command: string) => {
    if (command === 'get_node_status') return status();
    return overrides[command]?.() ?? Promise.resolve(homeReceipts[command] ?? null);
  });
}

it('冻结总额通过独立 IPC 查询，保留完整精度的金额字符串', async () => {
  const receipt = { totalFrozen: '3,402,823,669,209,384,634,633,746,074,317,682,114.55' };
  mocks.invoke.mockResolvedValueOnce(receipt);
  await expect(homeNodeApi.getTotalFrozen()).resolves.toBe(receipt);
  expect(mocks.invoke).toHaveBeenCalledExactlyOnceWith('get_total_frozen');
});

it.each([
  [null, '-'], ['0.00', '0.00元（公民币）'], ['1,234.56', '1,234.56元（公民币）'],
])('冻结金额 %s 显示在永久质押卡片下方', (amount, expected) => {
  const view = render(<IssuanceSection issuance={{totalIssuance:'10.00'}} stake={{totalStake:'5.00'}}
    frozen={{totalFrozen:amount}}/>);
  const cards = [...view.container.querySelectorAll('.metric-card')];
  expect(cards.map(card => card.querySelector('.metric-label > span')?.textContent))
    .toEqual(['全链发行总额', '永久质押金额', '临时冻结金额']);
  expect(cards[2].querySelector('.metric-value')?.textContent).toBe(expected);
  expect(within(cards[2] as HTMLElement).getByRole('button', {name:'全链冻结金额说明'}).textContent).toBe('说明');
});

it('冻结金额说明显示三条原文及达标表格，保留原说明并可点击外部关闭', () => {
  const prototype = HTMLDialogElement.prototype;
  const show = Object.getOwnPropertyDescriptor(prototype, 'showModal');
  const close = Object.getOwnPropertyDescriptor(prototype, 'close');
  Object.defineProperties(prototype, {
    showModal: {configurable:true, value(this:HTMLDialogElement) {this.open=true;}},
    close: {configurable:true, value(this:HTMLDialogElement) {this.open=false;}},
  });
  const view = render(<IssuanceSection issuance={{totalIssuance:'10.00'}} stake={{totalStake:'5.00'}}
    frozen={{totalFrozen:'0.00'}}/>);
  try {
    fireEvent.click(screen.getByRole('button', {name:'全链冻结金额说明'}));
    const dialog = screen.getByRole('dialog', {name:'全链冻结金额说明'});
    const items = within(dialog).getAllByRole('listitem');
    expect(items).toHaveLength(3);
    expect(items[0].textContent).toBe('所有市注册局的「主账户」达标前只进不出，达标前冻结市注册局主账户。');
    expect(items[1].textContent).toContain('市注册局达标规则如下：');
    expect(items[2].textContent).toBe('市注册局的注册公民人数达标后，主账户冻结资金自动解冻。');
    const table = within(dialog).getByRole('table', {name:'市注册局达标规则'});
    expect(within(table).getAllByRole('columnheader').map(cell=>cell.textContent))
      .toEqual(['全国达标顺序','该市注册局累计注册公民数']);
    expect(within(table).getAllByRole('cell').map(cell=>cell.textContent))
      .toEqual(['第 1～100 个','≥ 1,001 人','第 101～1,000 个','≥ 10,001 人','第 1,001 个起','≥ 100,001 人']);
    fireEvent.click(table);
    expect(screen.getByRole('dialog', {name:'全链冻结金额说明'})).toBe(dialog);
    fireEvent.click(dialog);
    expect(screen.queryByRole('dialog')).toBeNull();
    fireEvent.click(screen.getByRole('button', {name:'全链发行总额说明'}));
    expect(screen.getByRole('dialog', {name:'全链发行总额说明'})).toBeTruthy();
    expect(screen.getByText('新增发行统一由公民协议「即公民链的Runtime」的发行模组执行。')).toBeTruthy();
  } finally {
    view.unmount();
    if (show) Object.defineProperty(prototype, 'showModal', show);
    else Reflect.deleteProperty(prototype, 'showModal');
    if (close) Object.defineProperty(prototype, 'close', close);
    else Reflect.deleteProperty(prototype, 'close');
  }
});

it('首页首次不显示假状态，节点回执和后续状态刷新不等待慢数据', async () => {
  vi.useFakeTimers();
  const initial = deferred<NodeStatus>();
  const frozen = deferred<{totalFrozen:string}>();
  let reads = 0;
  const ready = vi.fn();
  mockHome(() => ++reads === 1 ? initial.promise : Promise.resolve(stoppedNode), {
    get_total_frozen: () => frozen.promise,
  });
  const view = render(<HomeNodeSection onNodeReadyChange={ready}/>);
  try {
    expect(screen.queryByText(/^状态：(已停止|启动中|运行中|关闭中)$/)).toBeNull();
    expect(screen.queryByText(/查询中|加载中/)).toBeNull();
    expect(screen.getByRole<HTMLButtonElement>('button',{name:'启动节点'}).disabled).toBe(true);
    await act(async () => initial.resolve(runningNode));
    expect(screen.getByText('状态：运行中')).toBeTruthy();
    expect(ready).toHaveBeenLastCalledWith(true);
    await act(async () => {await vi.advanceTimersByTimeAsync(3000);});
    expect(screen.getByText('状态：已停止')).toBeTruthy();
    expect(ready).toHaveBeenLastCalledWith(false);
    await act(async () => frozen.resolve({totalFrozen:'75.25'}));
    expect(screen.getByText('临时冻结金额').closest('.metric-card')?.querySelector('.metric-value')?.textContent).toBe('-');
  } finally {
    view.unmount();
    vi.useRealTimers();
  }
});

it('运行中切回首页立即保留状态，读取失败也不闪现已停止，真实停止仍能更新', async () => {
  mockHome(() => Promise.resolve(runningNode));
  let view!: ReturnType<typeof render>;
  await act(async () => {view=render(<HomeNodeSection onNodeReadyChange={vi.fn()}/>);});
  expect(screen.getByText('状态：运行中')).toBeTruthy();
  view.unmount();
  const pending = deferred<NodeStatus>();
  mockHome(() => pending.promise);
  view=render(<HomeNodeSection onNodeReadyChange={vi.fn()}/>);
  expect(screen.getByText('状态：运行中')).toBeTruthy();
  expect(screen.queryByText('状态：已停止')).toBeNull();
  await act(async () => pending.reject(new Error('fixture status unavailable')));
  expect(screen.getByText('状态：运行中')).toBeTruthy();
  expect(screen.getByText(/fixture status unavailable/)).toBeTruthy();
  view.unmount();
  mockHome(() => Promise.resolve(stoppedNode));
  await act(async () => {view=render(<HomeNodeSection onNodeReadyChange={vi.fn()}/>);});
  expect(screen.getByText('状态：已停止')).toBeTruthy();
  view.unmount();
  mockHome(() => Promise.reject(new Error('fixture stopped status unavailable')));
  await act(async () => {view=render(<HomeNodeSection onNodeReadyChange={vi.fn()}/>);});
  expect(screen.getByText('状态：已停止')).toBeTruthy();
  expect(screen.getByText(/fixture stopped status unavailable/)).toBeTruthy();
  view.unmount();
});

it('离开页面后的旧回执不能覆盖重新进入时确认的运行状态', async () => {
  const old = deferred<NodeStatus>();
  mockHome(() => old.promise);
  const first=render(<HomeNodeSection onNodeReadyChange={vi.fn()}/>);
  first.unmount();
  mockHome(() => Promise.resolve(runningNode));
  let second!: ReturnType<typeof render>;
  await act(async () => {second=render(<HomeNodeSection onNodeReadyChange={vi.fn()}/>);});
  expect(screen.getByText('状态：运行中')).toBeTruthy();
  await act(async () => old.resolve(stoppedNode));
  expect(screen.getByText('状态：运行中')).toBeTruthy();
  second.unmount();
  mockHome(() => new Promise(() => {}));
  const third=render(<HomeNodeSection onNodeReadyChange={vi.fn()}/>);
  expect(screen.getByText('状态：运行中')).toBeTruthy();
  third.unmount();
});

it('启停保持原确认流程，迟到查询不覆盖启停回执，慢数据不延长启动状态', async () => {
  vi.useFakeTimers();
  const old = deferred<NodeStatus>();
  const stop = deferred<NodeStatus>();
  const start = deferred<NodeStatus>();
  const frozen = deferred<{totalFrozen:string}>();
  let backend = runningNode;
  let reads = 0;
  let slow = false;
  mockHome(() => ++reads === 2 ? old.promise : Promise.resolve(backend), {
    stop_node: () => stop.promise,
    start_node: () => start.promise,
    get_total_frozen: () => slow ? frozen.promise : Promise.resolve(homeReceipts.get_total_frozen),
  });
  let view!: ReturnType<typeof render>;
  try {
    await act(async () => {view=render(<HomeNodeSection onNodeReadyChange={vi.fn()}/>);});
    await act(async () => {await vi.advanceTimersByTimeAsync(3000);});
    fireEvent.click(screen.getByRole('button',{name:'关闭节点'}));
    expect(screen.getByRole('dialog')).toBeTruthy();
    expect(mocks.invoke.mock.calls.some(([name]) => name==='stop_node')).toBe(false);
    fireEvent.click(screen.getByRole('button',{name:'确认关闭'}));
    expect(screen.getByText('状态：关闭中')).toBeTruthy();
    backend=stoppedNode;
    await act(async () => stop.resolve(backend));
    expect(screen.getByText('状态：已停止')).toBeTruthy();
    await act(async () => old.resolve(runningNode));
    expect(screen.getByText('状态：已停止')).toBeTruthy();
    fireEvent.click(screen.getByRole('button',{name:'启动节点'}));
    fireEvent.click(screen.getByRole('button',{name:'确认启动'}));
    expect(screen.getByText('状态：启动中')).toBeTruthy();
    backend=runningNode;
    slow=true;
    await act(async () => start.resolve(backend));
    expect(screen.getByText('状态：运行中')).toBeTruthy();
    expect(screen.getByRole<HTMLButtonElement>('button',{name:'关闭节点'}).disabled).toBe(false);
    await act(async () => frozen.resolve({totalFrozen:'75.25'}));
    expect(mocks.invoke.mock.calls.filter(([name]) => name==='stop_node')).toHaveLength(1);
    expect(mocks.invoke.mock.calls.filter(([name]) => name==='start_node')).toHaveLength(1);
  } finally {
    view?.unmount();
    vi.useRealTimers();
  }
});

it('上次启动的慢刷新结束后，不会提前取消随后关闭中的状态', async () => {
  const start=deferred<NodeStatus>();
  const stop=deferred<NodeStatus>();
  const frozen=deferred<{totalFrozen:string}>();
  let backend=stoppedNode;
  let slow=false;
  mockHome(() => Promise.resolve(backend), {
    start_node: () => start.promise,
    stop_node: () => stop.promise,
    get_total_frozen: () => slow ? frozen.promise : Promise.resolve(homeReceipts.get_total_frozen),
  });
  let view!: ReturnType<typeof render>;
  await act(async () => {view=render(<HomeNodeSection onNodeReadyChange={vi.fn()}/>);});
  fireEvent.click(screen.getByRole('button',{name:'启动节点'}));
  fireEvent.click(screen.getByRole('button',{name:'确认启动'}));
  backend=runningNode;
  slow=true;
  await act(async () => start.resolve(backend));
  expect(screen.getByText('状态：运行中')).toBeTruthy();
  fireEvent.click(screen.getByRole('button',{name:'关闭节点'}));
  fireEvent.click(screen.getByRole('button',{name:'确认关闭'}));
  await act(async () => frozen.resolve({totalFrozen:'75.25'}));
  expect(screen.getByText('状态：关闭中')).toBeTruthy();
  expect(screen.getByRole<HTMLButtonElement>('button',{name:'关闭中...'}).disabled).toBe(true);
  backend=stoppedNode;
  await act(async () => stop.resolve(backend));
  expect(screen.getByText('状态：已停止')).toBeTruthy();
  view.unmount();
});

it.each([
  ['stopped',false,'已停止'], ['running',true,'运行中'],
  ['starting',false,'启动中'], ['initializing',false,'启动中'], ['restarting',false,'启动中'],
  ['stopping',false,'关闭中'], ['failed',false,'已停止'], ['exited',false,'已停止'], ['lock_held',false,'已停止'],
] as const)('首页状态%s仅显示约定四种文案：%s/%s', async (state,running,label) => {
  mockHome(() => Promise.resolve({running,state,pid:running?1:null,lastError:null}));
  let view!: ReturnType<typeof render>;
  await act(async () => {view=render(<HomeNodeSection onNodeReadyChange={vi.fn()}/>);});
  expect(view.container.querySelector('.status-line')?.textContent).toBe(`状态：${label}`);
  view.unmount();
});

it('首页定时更新冻结金额，失败或节点停止时清空，重新运行后恢复查询', async () => {
  vi.useFakeTimers();
  let running = true;
  let failFrozen = false;
  let amount = '75.25';
  mocks.invoke.mockImplementation(async (command: string) => {
    switch (command) {
      case 'get_node_status': return {running,state:running?'running':'stopped',pid:running?1:null,lastError:null};
      case 'get_chain_status': return {blockHeight:10,finalizedHeight:9,syncing:false,specVersion:1,transactionVersion:1,nodeVersion:'1.0.3'};
      case 'get_node_identity': return {peerId:null,role:null,genesisHash:null};
      case 'get_total_issuance': return {totalIssuance:'100.00'};
      case 'get_total_stake': return {totalStake:'20.00'};
      case 'get_total_frozen':
        if (failFrozen) throw new Error('查询失败');
        return {totalFrozen:amount};
      default: return null;
    }
  });
  let view: ReturnType<typeof render> | undefined;
  const frozenValue = () => screen.getByText('临时冻结金额').closest('.metric-card')?.querySelector('.metric-value')?.textContent;
  try {
    await act(async () => {view = render(<HomeNodeSection onNodeReadyChange={vi.fn()}/>);});
    expect(frozenValue()).toBe('75.25元（公民币）');
    failFrozen = true;
    await act(async () => {await vi.advanceTimersByTimeAsync(3000);});
    expect(frozenValue()).toBe('-');
    failFrozen = false;
    amount = '0.00';
    await act(async () => {await vi.advanceTimersByTimeAsync(3000);});
    expect(frozenValue()).toBe('0.00元（公民币）');
    running = false;
    await act(async () => {await vi.advanceTimersByTimeAsync(3000);});
    expect(frozenValue()).toBe('-');
    running = true;
    amount = '12.34';
    await act(async () => {await vi.advanceTimersByTimeAsync(3000);});
    expect(frozenValue()).toBe('12.34元（公民币）');
  } finally {
    view?.unmount();
    vi.useRealTimers();
  }
});
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
import {NodeKeySection} from '../control/NodeKeySection';
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

it('主机状态在重新进入及查询失败时保留上次结果，后续查询仍可刷新', async () => {
  const props = { nodeKey: {nodeKey: null, peerId: null, cidShortName: null}, onUpdated: vi.fn(), onApplied: vi.fn() };
  const hostRow = () => within(screen.getByRole('heading', {name: '链上中国主机'}).closest<HTMLElement>('.settings-credential-row')!);
  mocks.invoke.mockImplementation(async (command: string) => command === 'get_onchina_host_key_status' ? true : {cidShortName: null});
  const first = render(<NodeKeySection {...props}/>);
  await hostRow().findByText('已设置');
  first.unmount();
  let rejectStatus!: (error: Error) => void;
  mocks.invoke.mockImplementation((command: string) => command === 'get_onchina_host_key_status'
    ? new Promise<boolean>((_, reject) => {rejectStatus = reject;}) : Promise.resolve({cidShortName: null}));
  const second = render(<NodeKeySection {...props}/>);
  expect(hostRow().getByText('已设置')).toBeTruthy();
  expect(hostRow().queryByText('—')).toBeNull();
  await act(async () => rejectStatus(new Error('fixture status unavailable')));
  expect(hostRow().getByText('已设置')).toBeTruthy();
  expect(screen.getByText(/fixture status unavailable/)).toBeTruthy();
  second.unmount();
  mocks.invoke.mockImplementation(async (command: string) => command === 'get_onchina_host_key_status' ? false : {cidShortName: null});
  const third = render(<NodeKeySection {...props}/>);
  await hostRow().findByText('未设置');
  third.unmount();
});

it('主机密钥上传成功后不会被较早发出的状态查询覆盖', async () => {
  let resolveStatus!: (configured: boolean) => void;
  mocks.invoke.mockImplementation((command: string) => command === 'get_onchina_host_key_status'
    ? new Promise<boolean>((resolve) => {resolveStatus = resolve;}) : Promise.resolve({cidShortName: null}));
  const view = render(<NodeKeySection nodeKey={{nodeKey: null, peerId: null, cidShortName: null}} onUpdated={vi.fn()} onApplied={vi.fn()}/>);
  const row = within(screen.getByRole('heading', {name: '链上中国主机'}).closest<HTMLElement>('.settings-credential-row')!);
  fireEvent.change(row.getByPlaceholderText('请输入链上中国主机HTTPS密钥'), {target: {value: 'fixture-private-key'}});
  fireEvent.click(row.getByRole('button', {name: '上传链上中国主机私钥'}));
  fireEvent.change(screen.getByPlaceholderText('请输入设备开机密码'), {target: {value: 'fixture-password'}});
  fireEvent.click(screen.getByRole('button', {name: '确认'}));
  await waitFor(() => expect(screen.queryByText('设备密码验证')).toBeNull());
  expect(row.getByText('已设置')).toBeTruthy();
  await act(async () => resolveStatus(false));
  expect(row.getByText('已设置')).toBeTruthy();
  view.unmount();
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

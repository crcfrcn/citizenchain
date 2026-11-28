// 验证钱包名称与删除确认、地址保留及转账费用和冷签边界。

import { parseYuanToFen, transferFeeFen, quoteTransfers } from "../onchain/fee";
import {readFileSync} from 'node:fs';
import {resolve} from 'node:path';
import {MetricHelp} from '../desktop/MetricHelp';
import { TransferForm } from "../onchain/TransferForm";
import { TransferSigningFlow } from "../onchain/TransferSigningFlow";
import {signatureSecondsRemaining} from '../client/citizenQr';
import { TransactionPanel, TransactionHistorySection } from "../onchain/TransactionPanel";
import { WalletManagerModal } from "../onchain/WalletManagerModal";
import { RewardAccountSection } from "../control/RewardAccountSection";
import { parseAddressQr } from "../client/parseAddressQr";
import { accountIdToSs58 } from "../client/ss58";
import type { Wallet } from "../onchain/types";
// @vitest-environment jsdom
// 使用本产品 scanner 原锁的组件测试工具；执行真实组件与 IPC 参数，不发送生产交易。
import { act, cleanup, fireEvent, render, screen, waitFor } from '@testing-library/react';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import accountCode from '../../../crates/protocol/tests/account_id_code.json';
import userContact from '../../../crates/protocol/tests/user_contact.json';
import userTransfer from '../../../crates/protocol/tests/user_transfer.json';

const mocks = vi.hoisted(() => ({
  invoke: vi.fn(),
  scan: undefined as undefined | ((value: string) => Promise<void>),
}));
vi.mock('../client/tauri', () => ({
  invoke: mocks.invoke,
  sanitizeError: (error: unknown) => error instanceof Error ? error.message : String(error),
}));
vi.mock("../desktop/signing", () => ({
  CitizenSignaturePanel: ({ onScan }: { onScan: (value: string) => Promise<void> }) => {
    mocks.scan = onScan;
    return <p>合成扫码界面</p>;
  },
}));
vi.mock("../desktop/scanner", () => ({ AddressScanModal: ({onResult}:{onResult:(value:{ss58Address:string})=>void}) => <button onClick={()=>onResult({ss58Address:destination})}>合成地址扫码</button> }));

it('矿工收款账户展示截断地址，提交时保留完整以太坊地址', async () => {
  const accountId = '0x' + '11'.repeat(32);
  const currentAddress = accountIdToSs58(accountId);
  const ethereumAddress = '0x5aAeb6053F3E94C9b9A09f33669435E7Ef1BeAed';
  mocks.invoke.mockResolvedValue({ accountId });
  const onUpdated = vi.fn();
  render(<RewardAccountSection rewardAccount={{ accountId }} onUpdated={onUpdated} />);

  expect(screen.getByText('已绑定')).toBeTruthy();
  expect(screen.getByText(`${currentAddress.slice(0, 6)}........${currentAddress.slice(-10)}`).title).toBe(currentAddress);
  fireEvent.change(screen.getByPlaceholderText('请输入公民链或以太坊地址'), { target: { value: ethereumAddress } });
  fireEvent.click(screen.getByRole('button', { name: '变更地址' }));
  fireEvent.change(screen.getByPlaceholderText('请输入设备开机密码'), { target: { value: 'fixture-password' } });
  fireEvent.click(screen.getAllByRole('button', { name: '变更地址' })[1]);
  await waitFor(() => expect(onUpdated).toHaveBeenCalledWith({ accountId }));
  expect(mocks.invoke).toHaveBeenCalledWith('set_reward_account', {
    ss58Address: ethereumAddress, unlockPassword: 'fixture-password',
  });
});

const wallet: Wallet = {
  name: '合成冷钱包', signMode: 'cold',
  accountId: `0x${'11'.repeat(32)}`,
};
const destination = accountIdToSs58(`0x${'22'.repeat(32)}`);
const request = {
  get requestJson() { return JSON.stringify({i:'client-test',e:Math.floor(Date.now()/1000)+300}); }, requestId: 'client-test', expectedPayloadHash: '合成载荷摘要',
  callDataHex: '0x0400', signNonce: 7, signBlockNumber: 0, feeFen: '111', scanFrom: 0,
};
const txHash = `0x${'33'.repeat(32)}`;

function deferred<T>() {
  let resolve!: (value: T) => void;
  let reject!: (reason: Error) => void;
  const promise = new Promise<T>((yes, no) => { resolve = yes; reject = no; });
  return { promise, resolve, reject };
}

function flow(signMode: Wallet['signMode'] = 'cold') {
  const onClose = vi.fn(), onSuccess = vi.fn();
  render(<TransferSigningFlow wallet={{ ...wallet, signMode }} mode="transfer" transfers={[{toAddress:destination,amountFen:"10500",remark:"合成备注"}]} onClose={onClose} onSuccess={onSuccess} />);
  return { onClose, onSuccess };
}

it('单笔确认摘要只保留收款地址、备注与一份转账金额，地址值左对齐',()=>{
  const style=document.createElement('style');style.textContent=readFileSync(resolve(process.cwd(),'desktop/global.css'),'utf8');document.head.append(style);
  try{
    flow();
    expect(screen.queryByText('第1笔')).toBeNull();
    expect(screen.queryByText(/^本金合计/)).toBeNull();
    expect(screen.queryByText('金额：105.00 元')).toBeNull();
    expect(screen.queryByText('仅成功转账项收费，按原费率逐笔分账。')).toBeNull();
    expect(screen.getByText('转账金额').nextElementSibling?.textContent).toBe('105.00 元');
    for(const label of ['付款地址','收款地址'])expect(getComputedStyle(screen.getByText(label).nextElementSibling!).textAlign).toBe('left');
    expect(screen.getByText('收款地址').nextElementSibling?.textContent).toBe(destination);
    expect(screen.getByText('冷签（公民钱包离线签名）')).toBeTruthy();
  }finally{style.remove();}
});

it('倒计时读取原请求的300秒期限，跨越时间和过期均不另起计时',()=>{
 vi.useFakeTimers();
 try{const raw=JSON.stringify({e:Math.floor(Date.now()/1000)+300});expect(signatureSecondsRemaining(raw)).toBe(300);vi.advanceTimersByTime(91000);expect(signatureSecondsRemaining(raw)).toBe(209);vi.advanceTimersByTime(210000);expect(signatureSecondsRemaining(raw)).toBe(0);for(const bad of ['{}','null','[]','{"e":0}','{"e":"300"}'])expect(()=>signatureSecondsRemaining(bad)).toThrow('有效期');}
 finally{vi.useRealTimers();}
});

// 新增只读准备与能力查询共用合成边界，不替代任何签名或提交断言。
function readonlyFixture(command:string,args?:unknown):unknown {
  if(command==='get_transfer_policy')return {executionFee:true,batchEnabled:true};
  if(command==='get_transfer_result')return {state:'pending',cursor:0};
  if(command==='get_transaction_history')return {records:[],cursor:null};
  if(command==='prepare_transfer'){
    const transfers=(args as {transfers:{toAddress:string;amountFen:string;remark:string}[]}).transfers;
    return {transfers,feeFen:quoteTransfers(transfers).fee.toString()};
  }
  return undefined;
}

beforeEach(() => {
  mocks.invoke.mockReset();
  mocks.scan = undefined;
  mocks.invoke.mockImplementation(async (command: string, args?:{transfers?:unknown}) => {
      const readonly=readonlyFixture(command,args);if(readonly!==undefined)return readonly;
    switch (command) {
      case 'get_wallets': return { wallets: [wallet], activeAccountId: wallet.accountId };
      case 'prepare_transfer': return {transfers:args?.transfers,feeFen:quoteTransfers(args?.transfers as {toAddress:string;amountFen:string;remark:string}[]).fee.toString()};
      case 'get_transfer_policy': return {executionFee:true,batchEnabled:true};
      case 'get_transfer_result': return {state:'pending',cursor:0};
      case 'get_wallet_balance': return '100000';
      case 'build_cold_transfer_request': return request;
      case 'submit_cold_transfer':
      case 'submit_hot_transfer': return { txHash, scanFrom:0 };
      default: throw new Error(`未声明的合成调用: ${command}`);
    }
  });
});
afterEach(() => { cleanup(); vi.useRealTimers(); });

it('钱包名称最多10字，组合输入确认后截断，非BMP字符同样按一个字计数', () => {
  render(<WalletManagerModal wallets={[]} activeAccountId={null}
    onClose={vi.fn()} onUpdate={vi.fn()} onSelect={vi.fn()} />);
  const input = screen.getByPlaceholderText('请输入钱包名称') as HTMLInputElement;
  fireEvent.change(input, {target:{value:'一二三四五六七八九十一'}});
  expect(input.value).toBe('一二三四五六七八九十');
  fireEvent.input(input, {target:{value:'中文输入法组合测试中一二'},isComposing:true});
  expect(input.value).toBe('中文输入法组合测试中一二');
  fireEvent.compositionEnd(input);
  expect(input.value).toBe('中文输入法组合测试中');
  fireEvent.change(input, {target:{value:'😀'.repeat(11)}});
  expect(input.value).toBe('😀'.repeat(10));
  expect(screen.queryByText(/最多.*10|10.*字/)).toBeNull();
});

describe('钱包圆点删除与选择区域', () => {
  const hot: Wallet = { name: '矿工热钱包', signMode: 'hot', accountId: `0x${'55'.repeat(32)}` };
  const other: Wallet = { ...wallet, name: '另一冷钱包', accountId: `0x${'66'.repeat(32)}` };

  function manager() {
    const onUpdate = vi.fn(), onSelect = vi.fn(), onClose = vi.fn();
    const view = render(<WalletManagerModal wallets={[hot, wallet, other]} activeAccountId={hot.accountId}
      onClose={onClose} onUpdate={onUpdate} onSelect={onSelect} />);
    return { ...view, onUpdate, onSelect, onClose };
  }

  it('蓝点两次点击才删除，处理中重复点击不重复提交，也不选择钱包', async () => {
    const removal = deferred<{ wallets: Wallet[]; activeAccountId: string }>();
    mocks.invoke.mockImplementation(async (command: string, args?:{transfers?:unknown}) => {
      const readonly=readonlyFixture(command,args);if(readonly!==undefined)return readonly;
      if (command === 'get_wallet_balance') return '100000';
      if (command === 'remove_wallet') return removal.promise;
      throw new Error(`不应调用: ${command}`);
    });
    const view = manager();
    const dot = screen.getByRole('button', { name: `删除${wallet.name}` });
    fireEvent.click(dot);
    expect(dot.classList.contains('confirm')).toBe(true);
    expect(mocks.invoke.mock.calls.filter(([c]) => c === 'remove_wallet')).toHaveLength(0);
    fireEvent.click(dot);
    fireEvent.click(dot);
    expect(mocks.invoke.mock.calls.filter(([c]) => c === 'remove_wallet')).toEqual([
      ['remove_wallet', { accountId: wallet.accountId }],
    ]);
    const store = { wallets: [hot, other], activeAccountId: hot.accountId };
    await act(async () => removal.resolve(store));
    expect(view.onUpdate).toHaveBeenCalledExactlyOnceWith(store);
    expect(view.onSelect).not.toHaveBeenCalled();
    expect(view.onClose).not.toHaveBeenCalled();
  });

  it('改点另一个蓝点重新确认；删除失败显示错误并可重试', async () => {
    mocks.invoke.mockImplementation(async (command: string, args?:{transfers?:unknown}) => {
      const readonly=readonlyFixture(command,args);if(readonly!==undefined)return readonly;
      if (command === 'get_wallet_balance') return '0';
      throw new Error('合成删除失败');
    });
    const view = manager();
    const first = screen.getByRole('button', { name: `删除${wallet.name}` });
    const second = screen.getByRole('button', { name: `删除${other.name}` });
    fireEvent.click(first);
    fireEvent.click(second);
    expect(first.classList.contains('confirm')).toBe(false);
    expect(second.classList.contains('confirm')).toBe(true);
    expect(mocks.invoke.mock.calls.some(([c]) => c === 'remove_wallet')).toBe(false);
    fireEvent.click(second);
    await screen.findByText('合成删除失败');
    expect(view.onUpdate).not.toHaveBeenCalled();
    expect(screen.getByText(other.name)).toBeTruthy();
    fireEvent.click(second);
    await waitFor(() => expect(mocks.invoke.mock.calls.filter(([c]) => c === 'remove_wallet')).toHaveLength(2));
    expect(mocks.invoke.mock.calls.filter(([c]) => c === 'remove_wallet').every(([, args]) => args.accountId === other.accountId)).toBe(true);
  });

  it.each(['.wallet-row-name', '.wallet-row-address', '.wallet-row-balance'])(
    '名称左侧和红点不选择或删除，点击%s仍选择钱包', async (selector) => {
      const store = { wallets: [hot, wallet, other], activeAccountId: wallet.accountId };
      mocks.invoke.mockImplementation(async (command: string, args?:{transfers?:unknown}) => {
      const readonly=readonlyFixture(command,args);if(readonly!==undefined)return readonly;
        if (command === 'get_wallet_balance') return '100000';
        if (command === 'set_active_wallet') return store;
        throw new Error(`不应调用: ${command}`);
      });
      const view = manager();
      fireEvent.click(screen.getByRole('radio', { name: `${hot.name}（热钱包）` }));
      const row = screen.getByText(wallet.name).closest('.wallet-row')!;
      fireEvent.click(row);
      expect(mocks.invoke.mock.calls.some(([c]) => ['set_active_wallet', 'remove_wallet'].includes(c))).toBe(false);
      expect(view.container.querySelectorAll('.wallet-radio.hot')).toHaveLength(1);
      expect(screen.queryByRole('button', { name: `删除${hot.name}` })).toBeNull();
      fireEvent.click(row.querySelector(selector)!);
      await waitFor(() => expect(view.onClose).toHaveBeenCalledOnce());
      expect(mocks.invoke).toHaveBeenCalledWith('set_active_wallet', { accountId: wallet.accountId });
      expect(view.onSelect).toHaveBeenCalledWith(wallet, expect.any(Object));
    },
  );
});

describe('现有四档金额和费用', () => {
  it.each(['hot', 'cold'] as const)('%s确认弹窗包含备注附加费', (signMode) => {
    flow(signMode);
    expect(screen.getByText(signMode==='hot'?'热签（矿工钱包签名）':'冷签（公民钱包离线签名）')).toBeTruthy();
    const row = screen.getByText('预估手续费').closest('.transfer-signing-row');
    expect(row?.textContent).toContain('1.11 元');
  });
  it.each(['a', '中', ' ', '\n', 'a'.repeat(99)])('普通转账备注非空统一加一次FLAT_FEE，清空备注恢复基础费：%j', (remark) => {
    expect(transferFeeFen('10500',remark)).toBe(111n);
    expect(()=>transferFeeFen('0',remark)).toThrow();
    const view = render(<TransferForm activeWallet={wallet} balance="1000000" onSubmit={vi.fn()} />);
    fireEvent.change(screen.getByPlaceholderText('0.00'), { target: { value: '105' } });
    const input = view.container.querySelector('#transfer-form-remark')!;
    fireEvent.change(input, { target: { value: remark } });
    expect(view.container.querySelector('.transfer-form-summary-line')?.textContent).toContain('1.11');
    fireEvent.change(input, { target: { value: '' } });
    expect(view.container.querySelector('.transfer-form-summary-line')?.textContent).toContain('0.11');
  });
  it.each([destination, '0x6D66a0cB155051055F3FB3Cc73d9230D3Fb4D5B5'])(
    '收款地址%s原样交给后端解析，不在前端改写大小写', (address) => {
      const submit = vi.fn();
      render(<TransferForm activeWallet={wallet} balance="1000000" onSubmit={submit} />);
      fireEvent.change(screen.getByPlaceholderText('请输入公民链或以太坊地址'), { target: { value: ` ${address} ` } });
      fireEvent.change(screen.getByPlaceholderText('0.00'), { target: { value: '2' } });
      fireEvent.click(screen.getByRole('button', { name: '签名交易' }));
      expect(submit).toHaveBeenCalledExactlyOnceWith({mode:'transfer',transfers:[{toAddress:address,amountFen:'200',remark:''}]});
    },
  );
  it.each([[1, 0.10], [104.99, 0.10], [105, 0.11], [1000, 1]])(
    '%s GMB按既有规则预估为%s GMB，传入签名流程的金额保持一致', (amount, fee) => {
      expect(Number(transferFeeFen(parseYuanToFen(String(amount))))/100).toBe(fee);
      const submit = vi.fn();
      const view = render(<TransferForm activeWallet={wallet} balance="1000000" onSubmit={submit} />);
      expect(view.container.querySelector('.transfer-form-balance-label')?.textContent).toBe(wallet.name);
      fireEvent.change(screen.getByPlaceholderText('请输入公民链或以太坊地址'), { target: { value: destination } });
      fireEvent.change(screen.getByPlaceholderText('0.00'), { target: { value: String(amount) } });
      expect(view.container.querySelector('.transfer-form-summary-line')?.textContent)
        .toContain(fee.toFixed(2));
      fireEvent.click(screen.getByRole('button', { name: '签名交易' }));
      expect(submit).toHaveBeenCalledExactlyOnceWith({mode:'transfer',transfers:[{toAddress:destination,amountFen:parseYuanToFen(String(amount)),remark:''}]});
    },
  );

  it('零金额和超长UTF-8备注不发起签名', () => {
    const submit = vi.fn();
    render(<TransferForm activeWallet={wallet} balance="1000000" onSubmit={submit} />);
    fireEvent.change(screen.getByPlaceholderText('请输入公民链或以太坊地址'), { target: { value: destination } });
    fireEvent.click(screen.getByRole('button', { name: '签名交易' }));
    fireEvent.change(screen.getByPlaceholderText('0.00'), { target: { value: '105' } });
    fireEvent.change(screen.getByPlaceholderText('输入备注收1.00元备注手续费。'), { target: { value: '中'.repeat(34) } });
    fireEvent.click(screen.getByRole('button', { name: '签名交易' }));
    expect(submit).not.toHaveBeenCalled();
  });
});

describe('钱包余额查询状态', () => {
  it('当前钱包与钱包管理原样显示后端总余额，不扣减保留金或保留资金', async () => {
    mocks.invoke.mockImplementation(async (command: string, args?:{transfers?:unknown}) => {
      const readonly=readonlyFixture(command,args);if(readonly!==undefined)return readonly;
      if (command === 'get_wallets') return { wallets: [wallet], activeAccountId: wallet.accountId };
      if (command === 'get_wallet_balance') return '300311';
      throw new Error(`非预期调用：${command}`);
    });
    const view = render(<TransactionPanel nodeReady />);
    await waitFor(() => expect(view.container.querySelector('.transfer-form-balance-value')?.textContent).toBe('3,003.11 元'));
    cleanup();
    const list = render(<WalletManagerModal wallets={[wallet]} activeAccountId={wallet.accountId}
      onSelect={vi.fn()} onClose={vi.fn()} onUpdate={vi.fn()} />);
    await waitFor(() => expect(list.container.querySelector('.wallet-row-balance')?.textContent).toBe('3,003.11 元'));
  });

  it('未另选钱包时显示后端默认的矿工热钱包名称和对应余额', async () => {
    const miner: Wallet = { name: '矿工热钱包', signMode: 'hot', accountId: `0x${'55'.repeat(32)}` };
    mocks.invoke.mockImplementation(async (command: string, args?:{transfers?:unknown}) => {
      const readonly=readonlyFixture(command,args);if(readonly!==undefined)return readonly;
      if (command === 'get_wallets') return { wallets: [miner, wallet], activeAccountId: miner.accountId };
      if (command === 'get_wallet_balance') return '56789';
      throw new Error(`非预期调用：${command}`);
    });
    const view = render(<TransactionPanel nodeReady />);
    await waitFor(() => expect(view.container.querySelector('.transfer-form-balance-value')?.textContent).toBe('567.89 元'));
    expect(view.container.querySelector('.transfer-form-balance-label')?.textContent).toBe(miner.name);
    expect(mocks.invoke).toHaveBeenCalledWith('get_wallet_balance', { accountId: miner.accountId });
  });

  it('节点启动期间不查余额，就绪后自动查询', async () => {
    mocks.invoke.mockImplementation(async (command: string, args?:{transfers?:unknown}) => {
      const readonly=readonlyFixture(command,args);if(readonly!==undefined)return readonly;
      if (command === 'get_wallets') return { wallets: [wallet], activeAccountId: wallet.accountId };
      if (command === 'get_wallet_balance') return '12345';
      throw new Error(`非预期调用：${command}`);
    });
    const view = render(<TransactionPanel nodeReady={false} />);
    await waitFor(() => expect(screen.queryByText('请先在钱包管理中添加钱包')).toBeNull());
    expect(view.container.querySelector('.transfer-form-balance-value')?.textContent).toBe('-');
    expect(mocks.invoke.mock.calls.map(([command]) => command)).toEqual(['get_wallets']);
    expect(screen.queryByRole('alert')).toBeNull();
    expect(screen.queryByRole('button', { name: '重试查询' })).toBeNull();

    view.rerender(<TransactionPanel nodeReady />);
    await screen.findByText('123.45 元');
    expect(mocks.invoke).toHaveBeenCalledWith('get_wallet_balance', { accountId: wallet.accountId });
  });

  it('链上无账户记录时显示零余额，钱包列表也不再显示加载中', async () => {
    mocks.invoke.mockImplementation(async (command: string, args?:{transfers?:unknown}) => {
      const readonly=readonlyFixture(command,args);if(readonly!==undefined)return readonly;
      if (command === 'get_wallets') return { wallets: [wallet], activeAccountId: wallet.accountId };
      if (command === 'get_wallet_balance') return null;
      throw new Error(`非预期调用：${command}`);
    });
    const view = render(<TransactionPanel nodeReady />);
    await waitFor(() => expect(view.container.querySelector('.transfer-form-balance-value')?.textContent).toBe('0.00 元'));
    expect(screen.queryByText('查询中...')).toBeNull();
    fireEvent.click(screen.getByRole('button', { name: '钱包管理' }));
    await waitFor(() => expect(view.container.querySelector('.wallet-row-balance')?.textContent).toBe('0.00 元'));
    expect(screen.queryByText('查询中...')).toBeNull();
  });

  it('RPC错误显示原因，手动重试期间显示加载，成功后恢复余额', async () => {
    const pending = deferred<string>();
    let requests = 0;
    mocks.invoke.mockImplementation(async (command: string, args?:{transfers?:unknown}) => {
      const readonly=readonlyFixture(command,args);if(readonly!==undefined)return readonly;
      if (command === 'get_wallets') return { wallets: [wallet], activeAccountId: wallet.accountId };
      if (command === 'get_wallet_balance') {
        if (++requests === 1) throw new Error('合成RPC连接失败');
        return pending.promise;
      }
      throw new Error(`非预期调用：${command}`);
    });
    render(<TransactionPanel nodeReady />);
    await screen.findByRole('alert');
    expect(screen.getByRole('alert').textContent).toContain('合成RPC连接失败');
    expect(screen.queryByText('查询中...')).toBeNull();
    fireEvent.click(screen.getByRole('button', { name: '重试查询' }));
    await screen.findByText('查询中...');
    await act(async () => pending.resolve('12345'));
    await screen.findByText('123.45 元');
    expect(screen.queryByRole('alert')).toBeNull();
  });

  it('钱包列表明确显示查询错误，不将RPC失败当成零余额', async () => {
    mocks.invoke.mockImplementation(async (command: string, args?:{transfers?:unknown}) => {
      const readonly=readonlyFixture(command,args);if(readonly!==undefined)return readonly;
      if (command === 'get_wallets') return { wallets: [wallet], activeAccountId: wallet.accountId };
      if (command === 'get_wallet_balance') throw new Error('合成RPC超时');
      throw new Error(`非预期调用：${command}`);
    });
    const view = render(<TransactionPanel nodeReady />);
    await screen.findByRole('alert');
    fireEvent.click(screen.getByRole('button', { name: '钱包管理' }));
    await waitFor(() => expect(view.container.querySelector('.wallet-row-balance')?.textContent).toBe('查询失败'));
    expect(view.container.querySelector('.wallet-row-balance')?.getAttribute('title')).toBe('合成RPC超时');
  });

  it('切换钱包后，旧账户迟到的查询结果不能覆盖当前余额', async () => {
    const oldRequest = deferred<string>();
    const otherWallet: Wallet = { ...wallet, name: '另一钱包', accountId: `0x${'44'.repeat(32)}` };
    const wallets = [wallet, otherWallet];
    let oldRequests = 0;
    mocks.invoke.mockImplementation(async (command: string, args?: { accountId: string }) => {
      const readonly=readonlyFixture(command,args);if(readonly!==undefined)return readonly;
      if (command === 'get_wallets') return { wallets, activeAccountId: wallet.accountId };
      if (command === 'set_active_wallet') return { wallets, activeAccountId: otherWallet.accountId };
      if (command === 'get_wallet_balance') {
        if (args?.accountId === wallet.accountId && ++oldRequests === 1) return oldRequest.promise;
        return args?.accountId === wallet.accountId ? '10000' : '20000';
      }
      throw new Error(`非预期调用：${command}`);
    });
    const view = render(<TransactionPanel nodeReady />);
    await waitFor(() => expect(oldRequests).toBe(1));
    expect(view.container.querySelector('.transfer-form-balance-label')?.textContent).toBe(wallet.name);
    fireEvent.click(screen.getByRole('button', { name: '钱包管理' }));
    await screen.findByText('另一钱包');
    fireEvent.click(screen.getByText('另一钱包'));
    await waitFor(() => expect(view.container.querySelector('.transfer-form-balance-value')?.textContent).toBe('200.00 元'));
    expect(view.container.querySelector('.transfer-form-balance-label')?.textContent).toBe(otherWallet.name);
    await act(async () => oldRequest.resolve('999999'));
    expect(view.container.querySelector('.transfer-form-balance-value')?.textContent).toBe('200.00 元');
    expect(view.container.querySelector('.transfer-form-balance-label')?.textContent).toBe(otherWallet.name);
  });
});

describe('现有确认、扫码与提交', () => {
  it('取消确认不构建请求、不签名、不提交', () => {
    const callbacks = flow();
    fireEvent.click(screen.getByRole('button', { name: '取消' }));
    expect(callbacks.onClose).toHaveBeenCalledOnce();
    expect(callbacks.onSuccess).not.toHaveBeenCalled();
    expect(mocks.invoke).not.toHaveBeenCalled();
  });

  it('冷签请求尚未返回时连续点击只构建一次', async () => {
    const pending = deferred<typeof request>();
    mocks.invoke.mockReturnValue(pending.promise);
    flow();
    const button = screen.getByRole('button', { name: '确认转账' });
    act(() => { button.click(); button.click(); });
    expect(mocks.invoke).toHaveBeenCalledExactlyOnceWith('build_cold_transfer_request', {
      accountId: wallet.accountId, mode:'transfer', transfers:[{toAddress:destination,amountFen:'10500',remark:'合成备注'}],
    });
    await act(async () => pending.resolve(request));
    expect(screen.getByText('合成扫码界面')).toBeTruthy();
  });

  it('热签异步提交在同一事件批次内也只能发起一次', async () => {
    const pending = deferred<{ txHash: string; scanFrom: number }>();
    mocks.invoke.mockReturnValue(pending.promise);
    const callbacks = flow('hot');
    fireEvent.change(screen.getByPlaceholderText('请输入设备开机密码'), { target: { value: '合成测试口令' } });
    const button = screen.getByRole('button', { name: '确认并提交' });
    act(() => { button.click(); button.click(); });
    expect(mocks.invoke).toHaveBeenCalledTimes(1);
    await act(async () => pending.resolve({ txHash, scanFrom:0 }));
    expect(screen.getByText('转账已提交')).toBeTruthy();
    expect(callbacks.onSuccess).toHaveBeenCalledExactlyOnceWith({txHash,scanFrom:0});
    fireEvent.click(screen.getByRole('button', { name: '完成' }));
    expect(callbacks.onSuccess).toHaveBeenCalledTimes(1);
  });

  it('扫码重复回调只提交一次且完整保留同一签名会话字段', async () => {
    const callbacks = flow();
    fireEvent.click(screen.getByRole('button', { name: '确认转账' }));
    await screen.findByText('合成扫码界面');
    const pending = deferred<{ txHash: string; scanFrom: number }>();
    mocks.invoke.mockClear().mockReturnValue(pending.promise);
    const scan = mocks.scan!;
    act(() => { void scan('合成响应'); void scan('合成响应'); });
    expect(mocks.invoke).toHaveBeenCalledExactlyOnceWith('submit_cold_transfer', {
      accountId: wallet.accountId, requestId: request.requestId,
      expectedPayloadHash: request.expectedPayloadHash, callDataHex: request.callDataHex,
      signNonce: request.signNonce, signBlockNumber: request.signBlockNumber,
      responseJson: '合成响应',
    });
    await act(async () => pending.resolve({ txHash, scanFrom:0 }));
    expect(screen.getByText('转账已提交')).toBeTruthy();
    // 已排队的摄像头回调即使在提交完成后到达，也不能再次提交同一会话。
    await act(async () => scan('合成响应'));
    expect(mocks.invoke).toHaveBeenCalledTimes(1);
    expect(callbacks.onSuccess).toHaveBeenCalledExactlyOnceWith({txHash,scanFrom:0});
  });

  it('请求失败可重试，失败不会产生成功回调', async () => {
    mocks.invoke.mockRejectedValueOnce(new Error('合成余额不足'));
    const callbacks = flow();
    fireEvent.click(screen.getByRole('button', { name: '确认转账' }));
    await screen.findByText('合成余额不足');
    expect(callbacks.onSuccess).not.toHaveBeenCalled();
    fireEvent.click(screen.getByRole('button', { name: '重试' }));
    fireEvent.click(screen.getByRole('button', { name: '确认转账' }));
    await screen.findByText('合成扫码界面');
    expect(mocks.invoke).toHaveBeenCalledTimes(2);
  });

  it('扫码提交失败后重新生成请求，不直接重放已消费会话', async () => {
    const callbacks = flow();
    fireEvent.click(screen.getByRole('button', { name: '确认转账' }));
    await screen.findByText('合成扫码界面');
    mocks.invoke.mockRejectedValueOnce(new Error('合成会话已到期'));
    await act(async () => mocks.scan!('合成响应'));
    expect(screen.getByText('合成会话已到期')).toBeTruthy();
    expect(callbacks.onSuccess).not.toHaveBeenCalled();
    fireEvent.click(screen.getByRole('button', { name: '重试' }));
    fireEvent.click(screen.getByRole('button', { name: '确认转账' }));
    await screen.findByText('合成扫码界面');
    expect(mocks.invoke.mock.calls.map(([command]) => command))
      .toEqual(['build_cold_transfer_request', 'submit_cold_transfer', 'build_cold_transfer_request']);
  });

  it('二维码到期只显示错误，不自动发起提交', async () => {
    vi.useFakeTimers();
    flow();
    await act(async () => fireEvent.click(screen.getByRole('button', { name: '确认转账' })));
    expect(screen.getByText('合成扫码界面')).toBeTruthy();
    for (let second = 0; second <= 300; second++) {
      await act(async () => vi.advanceTimersByTime(1000));
    }
    expect(screen.getByText('签名请求已过期，请重新操作')).toBeTruthy();
    expect(mocks.invoke).toHaveBeenCalledTimes(1);
  });

  it('交易池返回哈希后首页明确显示已提交', async () => {
    const view = render(<TransactionPanel nodeReady />);
    await screen.findByText('1,000.00 元');
    fireEvent.change(screen.getByPlaceholderText('请输入公民链或以太坊地址'), { target: { value: destination } });
    fireEvent.change(screen.getByPlaceholderText('0.00'), { target: { value: '105' } });
    fireEvent.click(screen.getByRole('button', { name: '签名交易' }));
    fireEvent.click(await screen.findByRole('button', { name: '确认转账' }));
    await screen.findByText('合成扫码界面');
    await act(async () => mocks.scan!('合成响应'));
    fireEvent.click(screen.getByRole('button', { name: '完成' }));
    await waitFor(() => expect(view.container.querySelector('.transfer-notice')?.textContent)
      .toContain('转账已提交'));
    expect(view.container.textContent).not.toContain('转账成功');
  });
});

// 用真实父子组件验证表单清空时点，仅替换IPC和设备签名边界。
describe('提交成功后的表单输入', () => {
  async function draft(signMode: Wallet['signMode'] = 'cold') {
    mocks.invoke.mockResolvedValueOnce({ wallets: [{ ...wallet, signMode }], activeAccountId: wallet.accountId });
    const view = render(<TransactionPanel nodeReady />);
    await screen.findByText('1,000.00 元');
    const address = screen.getByPlaceholderText('请输入公民链或以太坊地址') as HTMLInputElement;
    const amount = screen.getByPlaceholderText('0.00') as HTMLInputElement;
    const remark = screen.getByPlaceholderText('输入备注收1.00元备注手续费。') as HTMLTextAreaElement;
    fireEvent.change(address, { target: { value: destination } });
    fireEvent.change(amount, { target: { value: '105' } });
    fireEvent.change(remark, { target: { value: '合成备注' } });
    fireEvent.click(screen.getByRole('button', { name: '签名交易' }));
    await screen.findByRole('button',{name:signMode==='hot'?'确认并提交':'确认转账'});
    return { view, address, amount, remark };
  }

  async function submit(signMode: Wallet['signMode']) {
    if (signMode === 'hot') {
      fireEvent.change(screen.getByPlaceholderText('请输入设备开机密码'), { target: { value: '合成测试口令' } });
      fireEvent.click(screen.getByRole('button', { name: '确认并提交' }));
    } else {
      fireEvent.click(screen.getByRole('button', { name: '确认转账' }));
      await screen.findByText('合成扫码界面');
      act(() => { void mocks.scan!('合成响应'); });
    }
  }

  for (const signMode of ['hot', 'cold'] as const) {
    for (const close of ['完成', '关闭'] as const) {
      it(`${signMode}成功返回哈希立即清空金额备注、保留地址，${close}后可以继续填写`, async () => {
        const c = await draft(signMode);
        const pending = deferred<{ txHash: string; scanFrom: number }>();
        const original = mocks.invoke.getMockImplementation()!;
        mocks.invoke.mockImplementation((command: string, ...args: unknown[]) =>
          command === `submit_${signMode}_transfer` ? pending.promise : original(command, ...args));
        await submit(signMode);
        await screen.findByText('提交中...');
        expect(c.amount.value).toBe('105'); expect(c.remark.value).toBe('合成备注');
        await act(async () => pending.resolve({ txHash, scanFrom:0 }));
        await waitFor(() => expect(c.amount.value).toBe(''));
        expect(c.address.value).toBe(destination); expect(c.remark.value).toBe('');
        expect(c.view.container.querySelector('.transfer-form-remark-counter')?.textContent).toBe('0/99 字节');
        for (const line of c.view.container.querySelectorAll('.transfer-form-summary-line')) expect(line.textContent).toContain('0.00 元');
        // 成功弹窗尚未关闭，表单已经清空；完成/关闭不应再次触发成功。
        expect(screen.getByRole('button', { name: '完成' })).toBeTruthy();
        if (close === '完成') fireEvent.click(screen.getByRole('button', { name: '完成' }));
        else fireEvent.click(c.view.container.querySelector('.transfer-signing-close')!);
        const sign = screen.getByRole('button', { name: '签名交易' }) as HTMLButtonElement;
        expect(sign.disabled).toBe(true);
        fireEvent.change(c.amount, { target: { value: '2' } });
        fireEvent.change(c.remark, { target: { value: '下一笔' } });
        expect(sign.disabled).toBe(false);
        expect(c.address.value).toBe(destination); expect(c.amount.value).toBe('2'); expect(c.remark.value).toBe('下一笔');
        expect(c.view.container.querySelector('.transfer-notice')?.textContent).toContain('转账已提交');
      });
    }
  }

  it.each(['确认前', '扫码中'])('%s取消不清空任何输入', async (phase) => {
    const c = await draft();
    if (phase === '扫码中') {
      fireEvent.click(screen.getByRole('button', { name: '确认转账' }));
      await screen.findByText('合成扫码界面');
      fireEvent.click(c.view.container.querySelector('.transfer-signing-close')!);
    } else fireEvent.click(screen.getByRole('button', { name: '取消' }));
    expect(c.address.value).toBe(destination); expect(c.amount.value).toBe('105'); expect(c.remark.value).toBe('合成备注');
    expect(c.view.container.querySelector('.transfer-notice')).toBeNull();
  });

  it.each(['request', 'cold', 'hot'] as const)('%s失败时不清空输入', async (phase) => {
    const mode = phase === 'hot' ? 'hot' : 'cold';
    const c = await draft(mode);
    const original = mocks.invoke.getMockImplementation()!;
    const failing = phase === 'request' ? 'build_cold_transfer_request' : `submit_${mode}_transfer`;
    mocks.invoke.mockImplementation((command: string, ...args: unknown[]) =>
      command === failing ? Promise.reject(new Error('合成转账失败')) : original(command, ...args));
    if (phase === 'request') fireEvent.click(screen.getByRole('button', { name: '确认转账' }));
    else await submit(mode);
    await screen.findByText('合成转账失败');
    fireEvent.click(c.view.container.querySelector('.transfer-signing-close')!);
    expect(c.address.value).toBe(destination); expect(c.amount.value).toBe('105'); expect(c.remark.value).toBe('合成备注');
    expect(c.view.container.querySelector('.transfer-notice')).toBeNull();
  });

  it('二维码过期保留金额备注和地址', async () => {
    const c = await draft();
    // 必须在二维码倒计时首次创建前接管时钟，避免残留真实定时器。
    vi.useFakeTimers();
    await act(async () => fireEvent.click(screen.getByRole('button', { name: '确认转账' })));
    expect(screen.getByText('合成扫码界面')).toBeTruthy();
    for (let second = 0; second <= 300; second++) await act(async () => vi.advanceTimersByTime(1000));
    expect(screen.getByText('签名请求已过期，请重新操作')).toBeTruthy();
    fireEvent.click(c.view.container.querySelector('.transfer-signing-close')!);
    expect(c.address.value).toBe(destination); expect(c.amount.value).toBe('105'); expect(c.remark.value).toBe('合成备注');
    expect(c.view.container.querySelector('.transfer-notice')).toBeNull();
  });
});

describe('账户二维码沿用唯一协议夹具', () => {
  it.each([accountCode, userContact])('k=$k解析为同一公民链SS58账户', (fixture) => {
    expect(parseAddressQr(JSON.stringify(fixture)).ss58Address).toBe(accountIdToSs58(fixture.b.n));
  });
  it('账户输入框拒绝收款码、损坏账户码和普通文本', () => {
    expect(() => parseAddressQr(JSON.stringify(userTransfer))).toThrow();
    expect(() => parseAddressQr('{"p":"QR_V1","k":5,"b":{"n":"0x00"}}')).toThrow();
    expect(() => parseAddressQr('普通文本')).toThrow();
  });
});

describe('批量转账分组与两种官方模式', () => {
    it('遮罩不关闭、默认红点不可删、蓝点两次点击删除', () => {
        render(<TransferForm activeWallet={wallet} balance="1000000" onSubmit={vi.fn()}/>);
        fireEvent.click(screen.getByRole('button', { name: '批量转账' }));
        const dialog = screen.getByRole('dialog');
        fireEvent.click(dialog.parentElement!);
        expect(screen.getByRole('dialog')).toBeTruthy();
        expect(screen.getByLabelText('默认组不可删除').tagName).toBe('SPAN');
        fireEvent.click(screen.getByRole('button', { name: '添加' }));
        fireEvent.click(screen.getByRole('button', { name: '删除第2组' }));
        expect(screen.getByLabelText('第2组金额')).toBeTruthy();
        fireEvent.click(screen.getByRole('button', { name: '确认删除第2组' }));
        expect(screen.queryByLabelText('第2组金额')).toBeNull();
        fireEvent.click(screen.getByRole('button', { name: '关闭批量转账' }));
        expect(screen.queryByRole('dialog')).toBeNull();
    });
    it('一个金额对应多行地址，多个组按顺序展开，一批只选一种模式', () => {
        const submit = vi.fn();
        render(<TransferForm activeWallet={wallet} balance="1000000" onSubmit={submit}/>);
        fireEvent.click(screen.getByRole('button', { name: '批量转账' }));
        const a = destination, b = accountIdToSs58(`0x${'33'.repeat(32)}`);
        fireEvent.change(screen.getByLabelText('第1组金额'), { target: { value: '1.05' } });
        fireEvent.change(screen.getByLabelText('第1组地址'), { target: { value: `${a}\n${b}` } });
        expect((screen.getByLabelText('第1组地址') as HTMLTextAreaElement).value.endsWith('\n')).toBe(true);
        fireEvent.click(screen.getByRole('button', { name: '添加' }));
        fireEvent.change(screen.getByLabelText('第2组金额'), { target: { value: '105' } });
        fireEvent.change(screen.getByLabelText('第2组地址'), { target: { value: a } });
        fireEvent.click(screen.getByLabelText('逐笔成功'));
        const buttons = screen.getAllByRole('button', { name: '签名交易' });
        fireEvent.click(buttons[buttons.length - 1]);
        expect(submit).toHaveBeenCalledExactlyOnceWith({ mode: 'force_batch', transfers: [{ toAddress: a, amountFen: '105', remark: '' }, { toAddress: b, amountFen: '105', remark: '' }, { toAddress: a, amountFen: '10500', remark: '' }] });
        expect(screen.queryByLabelText('遇错停止')).toBeNull();
    });
    it('精确整数分及汇总溢出不会静默截断', () => {
        expect(parseYuanToFen('104.99')).toBe('10499');
        expect(() => parseYuanToFen('1.005')).toThrow();
        expect(() => quoteTransfers([{ toAddress: destination, amountFen: ((1n << 128n) - 1n).toString(), remark: '' }])).toThrow();
    });
});
it('批量扫码逐次追加当前组，首行无空白且已有地址不覆盖', () => {
    render(<TransferForm activeWallet={wallet} balance="1000000" onSubmit={vi.fn()}/>);
    fireEvent.click(screen.getByRole('button', { name: '批量转账' }));
    for (let i = 0; i < 2; i++) {
        fireEvent.click(screen.getByTitle('扫描第1组地址'));
        fireEvent.click(screen.getByRole('button', { name: '合成地址扫码' }));
    }
    expect((screen.getByLabelText('第1组地址') as HTMLTextAreaElement).value).toBe(`${destination}\n${destination}\n`);
});

it.each([{state:'partial',states:['success','failed','success'],success:2},{state:'success',states:['success','success','success'],success:3},{state:'rolled_back',states:['rolled_back','rolled_back','rolled_back'],success:0},{state:'failed',states:['failed','failed','failed'],success:0},{state:'failed',states:['failed','not_executed','not_executed'],success:0}])('批量$state只显示短暂真实统计，不保留哈希和逐笔长回执',async({state,states,success})=>{
  const original=mocks.invoke.getMockImplementation()!;
  mocks.invoke.mockImplementation((command:string,...args:unknown[])=>command==='get_transfer_result'
    ?Promise.resolve({state,cursor:1,feeFen:'21',items:states.map(state=>({state,feeFen:'0'}))})
    :original(command,...args));
  const view=render(<TransactionPanel nodeReady/>);
  await screen.findByText('1,000.00 元');
  const batch=screen.getByRole('button',{name:'批量转账'});
  await waitFor(()=>expect((batch as HTMLButtonElement).disabled).toBe(false));
  fireEvent.click(batch);
  const second=accountIdToSs58(`0x${'33'.repeat(32)}`);
  fireEvent.change(screen.getByLabelText('第1组金额'),{target:{value:'1'}});
  fireEvent.change(screen.getByLabelText('第1组地址'),{target:{value:`${destination}\n${second}`}});
  fireEvent.click(screen.getByRole('button',{name:'添加'}));
  fireEvent.change(screen.getByLabelText('第2组金额'),{target:{value:'105'}});
  fireEvent.change(screen.getByLabelText('第2组地址'),{target:{value:destination}});
  fireEvent.click(screen.getByLabelText('逐笔成功'));
  fireEvent.click(screen.getAllByRole('button',{name:'签名交易'}).slice(-1)[0]);
  const confirm=await screen.findByRole('button',{name:'确认转账'});
  vi.useFakeTimers();
  await act(async()=>fireEvent.click(confirm));
  await act(async()=>mocks.scan!('合成响应'));
  expect(screen.queryByRole('dialog')).toBeNull();
  await act(async()=>vi.advanceTimersByTimeAsync(3000));
  expect(view.container.querySelector('.transfer-notice')?.textContent).toBe(`批量转账3笔，成功${success}笔，失败${3-success}笔。`);
  expect(view.container.querySelector('.transfer-result')).toBeNull();expect(view.container.textContent).not.toContain(txHash);expect(view.container.textContent).not.toContain('实际手续费合计');
  await act(async()=>vi.advanceTimersByTimeAsync(2999));expect(view.container.querySelector('.transfer-notice')).toBeTruthy();
  await act(async()=>vi.advanceTimersByTimeAsync(1));expect(view.container.querySelector('.transfer-notice')).toBeNull();
  expect(mocks.invoke.mock.calls.filter(([c])=>c==='get_transfer_result')).toHaveLength(1);
  expect(mocks.invoke.mock.calls.filter(([c])=>c==='submit_cold_transfer')).toHaveLength(1);
});


it('批量转账与同页说明按钮实际字号一致，不继承表单父级的大字体',()=>{
 const style=document.createElement('style');style.textContent=readFileSync(resolve(process.cwd(),'desktop/global.css'),'utf8');document.head.append(style);
 const view=render(<><div className="transaction-panel-heading"><MetricHelp title="转账说明" items={['合成说明']}/></div><div style={{fontSize:'24px'}}><TransferForm activeWallet={wallet} balance="1000000" onSubmit={vi.fn()}/></div></>);
 try{
  const batch=getComputedStyle(screen.getByRole('button',{name:'批量转账'}));const help=getComputedStyle(screen.getByRole('button',{name:'转账说明'}));
  expect(batch.fontSize).toBe('12px');expect(batch.fontSize).toBe(help.fontSize);expect(batch.padding).toBe(help.padding);expect(batch.color).toBe(help.color);expect(batch.backgroundColor).toBe(help.backgroundColor);
 }finally{view.unmount();style.remove();}
});


it('批量默认组与新增组单行等高，只有地址行数向下增长且超过8行仍增长',()=>{
 const style=document.createElement('style');style.textContent=readFileSync(resolve(process.cwd(),'desktop/global.css'),'utf8');document.head.append(style);
 const view=render(<TransferForm activeWallet={wallet} balance="1000000" onSubmit={vi.fn()}/>);
 try{
  fireEvent.click(screen.getByRole('button',{name:'批量转账'}));fireEvent.click(screen.getByRole('button',{name:'添加'}));
  for(const index of [1,2]){
   const amount=screen.getByLabelText(`第${index}组金额`) as HTMLInputElement,address=screen.getByLabelText(`第${index}组地址`) as HTMLTextAreaElement;
   const scan=screen.getByTitle(`扫描第${index}组地址`),row=amount.parentElement!;
   expect(address.rows).toBe(1);expect(getComputedStyle(amount).height).toBe(getComputedStyle(scan).height);
   expect(getComputedStyle(amount).paddingTop).toBe('5px');expect(getComputedStyle(address).paddingTop).toBe('5px');
   expect(getComputedStyle(address).resize).toBe('none');expect(getComputedStyle(row).alignItems).toBe('start');
   fireEvent.change(address,{target:{value:'第一行\n第二行'}});expect(address.rows).toBe(2);
   fireEvent.change(address,{target:{value:Array.from({length:12},(_,i)=>`地址${i}`).join('\n')}});expect(address.rows).toBe(12);
   expect(getComputedStyle(amount).height).toBe(getComputedStyle(scan).height);
   fireEvent.change(address,{target:{value:''}});expect(address.rows).toBe(1);
  }
 }finally{view.unmount();style.remove();}
});

it('批量底栏左侧交易模式右侧签名、两选项顺序及圈大小正确，汇总只占一行',()=>{
 const style=document.createElement('style');style.textContent=readFileSync(resolve(process.cwd(),'desktop/global.css'),'utf8');document.head.append(style);
 const view=render(<TransferForm activeWallet={wallet} balance="1000000" onSubmit={vi.fn()}/>);
 try{
  fireEvent.click(screen.getByRole('button',{name:'批量转账'}));
  const footer=view.container.querySelector('.batch-transfer-footer')!,mode=footer.querySelector('.batch-transfer-mode')!,sign=footer.querySelector('.transfer-form-submit')!;
  expect(Array.from(footer.children)).toEqual([mode,sign]);expect(mode.querySelector('.batch-transfer-mode-title')?.textContent).toBe('交易模式');
  expect(getComputedStyle(footer).marginTop).toBe('20px');
  expect(getComputedStyle(view.container.querySelector('.batch-transfer-modal > .wallet-manager-body')!).paddingTop).toBe('4px');
  expect(mode.tagName).toBe('DIV');expect(mode.getAttribute('role')).toBe('group');
  expect(getComputedStyle(mode).fontSize).toBe('14px');expect(getComputedStyle(mode).alignItems).toBe('center');
  expect(getComputedStyle(mode.querySelector('.batch-transfer-options')!).justifyContent).toBe('flex-start');
  expect(getComputedStyle(view.container.querySelector('.batch-transfer-summary')!).fontSize).toBe('13px');
  expect(Array.from(mode.querySelectorAll('label')).map(node=>node.textContent)).toEqual(['整批成功','逐笔成功']);
  for(const label of mode.querySelectorAll('label'))expect(getComputedStyle(label).flexDirection).toBe('row');
  const options=mode.querySelectorAll<HTMLInputElement>('input[type="radio"]');expect(options[0].checked).toBe(true);expect(options[1].checked).toBe(false);
  for(const radio of options){expect(getComputedStyle(radio).width).toBe('19.5px');expect(getComputedStyle(radio).height).toBe('19.5px');}
  expect(view.container.querySelector('.batch-transfer-summary')?.textContent).toBe('转账数量：0 笔｜转账金额：0.00 元｜预计手续费：0.00 元｜合计：0.00 元');
  expect(getComputedStyle(view.container.querySelector('.batch-transfer-summary')!).whiteSpace).toBe('nowrap');
  expect(screen.queryByText(/任一笔失败，整批本金和手续费回滚/)).toBeNull();expect(view.container.querySelector('.batch-transfer-hint')).toBeNull();
  fireEvent.click(screen.getByLabelText('逐笔成功'));expect(options[0].checked).toBe(false);expect(options[1].checked).toBe(true);
  fireEvent.change(screen.getByLabelText('第1组金额'),{target:{value:'1'}});fireEvent.change(screen.getByLabelText('第1组地址'),{target:{value:destination}});
  expect(view.container.querySelector('.batch-transfer-summary')?.textContent).toBe('转账数量：1 笔｜转账金额：1.00 元｜预计手续费：0.10 元｜合计：1.10 元');
 }finally{view.unmount();style.remove();}
});


let historyEvent=0;
function historyRecord(overrides:Record<string,unknown>={}){
 return {blockNumber:9,timestamp:1782950400000,txHash:'0xhistory',eventIndex:++historyEvent,accountId:wallet.accountId,direction:'sent',mode:'batch_all',transferCount:4,amountFen:'10500',feeFen:'40',state:'success',...overrides};
}
it('同一笔钱包互转分别显示各钱包的支出和收入，不再合并内部转账',async()=>{
 const second={...wallet,name:'合成第二钱包',accountId:'0x'+'22'.repeat(32)};const original=mocks.invoke.getMockImplementation()!;
 mocks.invoke.mockImplementation((command:string,...args:unknown[])=>command==='get_wallets'?Promise.resolve({wallets:[wallet,second],activeAccountId:wallet.accountId}):command==='get_transaction_history'?Promise.resolve({records:[historyRecord({accountId:wallet.accountId,direction:'sent'}),historyRecord({accountId:second.accountId,direction:'received',feeFen:'0'})],cursor:null}):original(command,...args));
 const view=render(<TransactionHistorySection/>);const region=await screen.findByRole('region',{name:'交易记录'});
 await screen.findByText('合成第二钱包');expect(region.querySelectorAll('tbody tr')).toHaveLength(2);
 expect(screen.getByText('-105.00 元')).toBeTruthy();expect(screen.getByText('+105.00 元')).toBeTruthy();
 expect(region.textContent).not.toContain('内部转账');expect(region.textContent).not.toContain('合成冷钱包、合成第二钱包');
 expect(screen.getByRole('button',{name:'加载更多'}).nextElementSibling?.textContent).toBe('刷新');
 expect(screen.getByText('第1页 / 全部1页')).toBeTruthy();expect((screen.getByRole('button',{name:'下一页'}) as HTMLButtonElement).disabled).toBe(true);
 expect(mocks.invoke).toHaveBeenCalledWith('get_transaction_history',{cursor:null});
 expect(screen.queryByRole('dialog')).toBeNull();expect(screen.queryByRole('button',{name:'关闭交易记录'})).toBeNull();view.unmount();
});
it('历史记录跳过无相关交易的区块页，加载更多读取更早记录，不建立定时轮询',async()=>{
 let reads=0;const original=mocks.invoke.getMockImplementation()!;
 mocks.invoke.mockImplementation((command:string,...args:unknown[])=>{
  if(command==='get_transaction_history'){
   ++reads;return Promise.resolve(reads===1?{records:[],cursor:{blockNumber:100,eventIndex:null}}:reads===2?{records:[historyRecord()],cursor:{blockNumber:90,eventIndex:null}}:{records:[historyRecord({blockNumber:8,txHash:'older',direction:'received',amountFen:'200',feeFen:'0'})],cursor:null});
  }return original(command,...args);
 });
 const view=render(<TransactionHistorySection/>);
 await screen.findByText('-105.00 元');expect(reads).toBe(2);expect(mocks.invoke).toHaveBeenCalledWith('get_transaction_history',{cursor:{blockNumber:100,eventIndex:null}});
 fireEvent.click(screen.getByRole('button',{name:'加载更多'}));await screen.findByText('+2.00 元');expect(reads).toBe(3);
 expect(screen.getByRole('region',{name:'交易记录'}).querySelectorAll('tbody tr')).toHaveLength(2);expect((screen.getByRole('button',{name:'加载更多'}) as HTMLButtonElement).disabled).toBe(true);
 vi.useFakeTimers();await act(async()=>vi.advanceTimersByTimeAsync(20000));expect(reads).toBe(3);view.unmount();
});
it('历史记录按可用高度分页，翻页不查链，刷新回首页且底栏始终保留',async()=>{
 const original=mocks.invoke.getMockImplementation()!;let resize:()=>void=()=>{};
 const height=vi.spyOn(HTMLElement.prototype,'clientHeight','get').mockReturnValue(241);
 vi.stubGlobal('ResizeObserver',class{constructor(callback:()=>void){resize=callback;}observe(){}disconnect(){}});
 const style=document.createElement('style');style.textContent=readFileSync(resolve(process.cwd(),'desktop/global.css'),'utf8');document.head.append(style);
 mocks.invoke.mockImplementation((command:string,...args:unknown[])=>command==='get_transaction_history'?Promise.resolve({records:Array.from({length:25},(_,index)=>historyRecord({amountFen:String((index+1)*100),direction:'received',txHash:null})),cursor:null}):original(command,...args));
 const view=render(<div className="app"><div className="content"><TransactionHistorySection/></div></div>);
 try{
  await screen.findByText('+1.00 元');expect(screen.getByText('第1页 / 全部5页')).toBeTruthy();
  const region=screen.getByRole('region',{name:'交易记录'}),rows=()=>region.querySelectorAll('tbody tr');expect(rows()).toHaveLength(5);
  const reads=mocks.invoke.mock.calls.filter(([c])=>c==='get_transaction_history').length;
  fireEvent.click(screen.getByRole('button',{name:'下一页'}));expect(screen.getByText('+6.00 元')).toBeTruthy();expect(screen.queryByText('+1.00 元')).toBeNull();
  fireEvent.click(screen.getByRole('button',{name:'下一页'}));expect(screen.getByText('+11.00 元')).toBeTruthy();
  fireEvent.click(screen.getByRole('button',{name:'上一页'}));expect(screen.getByText('+6.00 元')).toBeTruthy();
  fireEvent.click(screen.getByRole('button',{name:'首页'}));expect(screen.getByText('+1.00 元')).toBeTruthy();
  height.mockReturnValue(441);act(()=>resize());expect(rows()).toHaveLength(10);expect(screen.getByText('第1页 / 全部3页')).toBeTruthy();
  fireEvent.click(screen.getByRole('button',{name:'下一页'}));fireEvent.click(screen.getByRole('button',{name:'下一页'}));expect(rows()).toHaveLength(5);expect(screen.getByText('+25.00 元')).toBeTruthy();expect((screen.getByRole('button',{name:'下一页'}) as HTMLButtonElement).disabled).toBe(true);
  expect(mocks.invoke.mock.calls.filter(([c])=>c==='get_transaction_history')).toHaveLength(reads);
  const more=screen.getByRole('button',{name:'加载更多'}),refresh=screen.getByRole('button',{name:'刷新'}),moreStyle=getComputedStyle(more),refreshStyle=getComputedStyle(refresh);
  expect(more.nextElementSibling).toBe(refresh);expect(moreStyle.width).toBe('80px');expect(moreStyle.width).toBe(refreshStyle.width);expect(moreStyle.padding).toBe(refreshStyle.padding);expect(moreStyle.fontSize).toBe(refreshStyle.fontSize);expect(moreStyle.borderWidth).toBe(refreshStyle.borderWidth);
  expect(more.classList.contains('transaction-history-more')).toBe(true);expect(getComputedStyle(document.documentElement).getPropertyValue('--action-blue').trim()).toBe('#2563eb');
  expect(getComputedStyle(region.querySelector('.transaction-history-body')!).overflowY).toBe('hidden');
  expect(getComputedStyle(screen.getByRole('navigation',{name:'交易记录分页'})).justifyContent).toBe('flex-end');
  expect(getComputedStyle(region).overflow).toBe('hidden');
  fireEvent.click(refresh);await screen.findByText('+1.00 元');expect(screen.getByText('第1页 / 全部3页')).toBeTruthy();
 }finally{view.unmount();style.remove();height.mockRestore();vi.unstubAllGlobals();}
});
it('历史读取失败保留错误可刷新，不把失败当作空记录；关闭后迟到结果不继续查链',async()=>{
 let mode:'error'|'empty'|'pending'='error';const pending=deferred<unknown>();const original=mocks.invoke.getMockImplementation()!;
 mocks.invoke.mockImplementation((command:string,...args:unknown[])=>command==='get_transaction_history'?mode==='error'?Promise.reject(Error('合成历史读取失败')):mode==='empty'?Promise.resolve({records:[],cursor:null}):pending.promise:original(command,...args));
 const view=render(<TransactionHistorySection/>);
 await screen.findByText('合成历史读取失败');expect(screen.queryByText('暂无交易记录')).toBeNull();
 mode='empty';fireEvent.click(screen.getByRole('button',{name:'刷新'}));await screen.findByText('暂无交易记录');
 mode='pending';fireEvent.click(screen.getByRole('button',{name:'刷新'}));const count=mocks.invoke.mock.calls.filter(([c])=>c==='get_transaction_history').length;
 view.unmount();await act(async()=>pending.resolve({records:[],cursor:{blockNumber:1,eventIndex:null}}));
 expect(mocks.invoke.mock.calls.filter(([c])=>c==='get_transaction_history')).toHaveLength(count);expect(screen.queryByRole('dialog',{name:'交易记录'})).toBeNull();view.unmount();
});
it('节点未运行时显示原查询错误，不显示伪空记录',async()=>{
 const original=mocks.invoke.getMockImplementation()!;
 mocks.invoke.mockImplementation((command:string,...args:unknown[])=>command==='get_transaction_history'?Promise.reject(Error('节点未运行')):original(command,...args));
 const view=render(<TransactionHistorySection/>);await screen.findByText('节点未运行');expect(screen.queryByText('暂无交易记录')).toBeNull();view.unmount();
});

it('批量结果子项不完整时不编造成功数，保留原游标再核验且不重新付款',async()=>{
 const original=mocks.invoke.getMockImplementation()!;let reads=0;
 mocks.invoke.mockImplementation((command:string,...args:unknown[])=>command==='get_transfer_result'?Promise.resolve(++reads===1?{state:'success',cursor:1,items:[]}:{state:'success',cursor:1,items:[{state:'success',feeFen:'10'}]}):original(command,...args));
 const view=render(<TransactionPanel nodeReady/>);await screen.findByText('1,000.00 元');
 const batch=screen.getByRole('button',{name:'批量转账'});await waitFor(()=>expect((batch as HTMLButtonElement).disabled).toBe(false));fireEvent.click(batch);
 fireEvent.change(screen.getByLabelText('第1组金额'),{target:{value:'1'}});fireEvent.change(screen.getByLabelText('第1组地址'),{target:{value:destination}});
 fireEvent.click(screen.getAllByRole('button',{name:'签名交易'}).slice(-1)[0]);const confirm=await screen.findByRole('button',{name:'确认转账'});
 vi.useFakeTimers();await act(async()=>fireEvent.click(confirm));await act(async()=>mocks.scan!('合成响应'));
 await act(async()=>vi.advanceTimersByTimeAsync(3000));expect(view.container.querySelector('.transfer-notice')).toBeNull();expect(screen.getByText(/结果子项不完整/)).toBeTruthy();
 await act(async()=>vi.advanceTimersByTimeAsync(3000));expect(view.container.querySelector('.transfer-notice')?.textContent).toBe('批量转账1笔，成功1笔，失败0笔。');
 const checks=mocks.invoke.mock.calls.filter(([c])=>c==='get_transfer_result');expect(checks).toHaveLength(2);expect(checks.every(([,args])=>args.cursor===0)).toBe(true);
 expect(mocks.invoke.mock.calls.filter(([c])=>c==='submit_cold_transfer')).toHaveLength(1);view.unmount();
});

it('历史游标不前进时停止读取并报错，不循环查询或显示伪空记录',async()=>{
 const original=mocks.invoke.getMockImplementation()!;
 mocks.invoke.mockImplementation((command:string,...args:unknown[])=>command==='get_transaction_history'?Promise.resolve({records:[],cursor:{blockNumber:1,eventIndex:null}}):original(command,...args));
 const view=render(<TransactionHistorySection/>);
 await screen.findByText('交易记录游标未前进');expect(mocks.invoke.mock.calls.filter(([c])=>c==='get_transaction_history')).toHaveLength(2);expect(screen.queryByText('暂无交易记录')).toBeNull();view.unmount();
});


it('批量模式容器采用普通横排分组，锁定时两选项仍同时禁用',()=>{
 const view=render(<TransferForm activeWallet={wallet} balance="1000000" onSubmit={vi.fn()}/>);
 fireEvent.click(screen.getByRole('button',{name:'批量转账'}));
 const group=screen.getByRole('group',{name:'交易模式'});expect(group.tagName).toBe('DIV');expect(group.querySelector('legend')).toBeNull();
 fireEvent.click(screen.getByLabelText('逐笔成功'));expect((screen.getByLabelText('逐笔成功') as HTMLInputElement).checked).toBe(true);
 view.rerender(<TransferForm activeWallet={wallet} balance="1000000" onSubmit={vi.fn()} disabled/>);
 for(const input of group.querySelectorAll<HTMLInputElement>('input'))expect(input.disabled).toBe(true);
 expect(group.getAttribute('aria-disabled')).toBe('true');view.unmount();
});

it('首页只保留钱包管理，不再包含历史弹窗入口',async()=>{
 const view=render(<TransactionPanel nodeReady={false}/>);await screen.findByRole('button',{name:'钱包管理'});
 expect(screen.queryByRole('button',{name:'交易记录'})).toBeNull();view.unmount();
});
it('历史刷新重新获取当前钱包名称，收入支出带符号，失败回滚仍显示状态',async()=>{
 const original=mocks.invoke.getMockImplementation()!;let changed=false;
 mocks.invoke.mockImplementation((command:string,...args:unknown[])=>command==='get_wallets'?Promise.resolve({wallets:[{...wallet,name:changed?'改名钱包':wallet.name}],activeAccountId:wallet.accountId}):command==='get_transaction_history'?Promise.resolve({records:[historyRecord({amountFen:'100',direction:'received'}),historyRecord({txHash:'failed',amountFen:'200',state:'failed'}),historyRecord({txHash:'rolled',amountFen:'300',state:'rolled_back'})],cursor:null}):original(command,...args));
 const view=render(<TransactionHistorySection/>);await screen.findByText('+1.00 元');expect(screen.getByText('-2.00 元')).toBeTruthy();expect(screen.getByText('-3.00 元')).toBeTruthy();expect(screen.getByText('失败')).toBeTruthy();expect(screen.getByText('已回滚')).toBeTruthy();
 const before=mocks.invoke.mock.calls.filter(([c])=>c==='get_wallets').length;changed=true;fireEvent.click(screen.getByRole('button',{name:'刷新'}));await screen.findAllByText('改名钱包');expect(mocks.invoke.mock.calls.filter(([c])=>c==='get_wallets')).toHaveLength(before+1);view.unmount();
});

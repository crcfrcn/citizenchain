import { calculateTransferFeeYuan } from "../onchain/fee";
import { TransferForm } from "../onchain/TransferForm";
import { TransferSigningFlow } from "../onchain/TransferSigningFlow";
import { TransactionPanel } from "../onchain/TransactionPanel";
import { parseAddressQr } from "../client/parseAddressQr";
import { accountIdToSs58 } from "../client/ss58";
import type { Wallet } from "../onchain/types";
// @vitest-environment jsdom
// 使用本产品 scanner 原锁的组件测试工具；执行真实组件与 IPC 参数，不发送生产交易。
import { act, cleanup, fireEvent, render, screen, waitFor } from '@testing-library/react';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import accountCode from '../../../crates/protocol/fixtures/account_id_code.json';
import userContact from '../../../crates/protocol/fixtures/user_contact.json';
import userTransfer from '../../../crates/protocol/fixtures/user_transfer.json';

const mocks = vi.hoisted(() => ({
  invoke: vi.fn(),
  scan: undefined as undefined | ((value: string) => Promise<void>),
}));
vi.mock('../client/tauri', () => ({
  invoke: mocks.invoke,
  sanitizeError: (error: unknown) => error instanceof Error ? error.message : String(error),
}));
vi.mock('../desktop/CitizenSignaturePanel', () => ({
  CitizenSignaturePanel: ({ onScan }: { onScan: (value: string) => Promise<void> }) => {
    mocks.scan = onScan;
    return <p>合成扫码界面</p>;
  },
}));
vi.mock('../desktop/AddressScanModal', () => ({ AddressScanModal: () => null }));

const wallet: Wallet = {
  name: '合成冷钱包', signMode: 'cold',
  accountId: `0x${'11'.repeat(32)}`,
};
const destination = accountIdToSs58(`0x${'22'.repeat(32)}`);
const request = {
  requestJson: '合成签名请求', requestId: 'client-test', expectedPayloadHash: '合成载荷摘要',
  callDataHex: '0x0400', signNonce: 7, signBlockNumber: 0, feeYuan: 0.11,
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
  render(<TransferSigningFlow wallet={{ ...wallet, signMode }} toAddress={destination}
    amountYuan={105} remark="合成备注" onClose={onClose} onSuccess={onSuccess} />);
  return { onClose, onSuccess };
}

beforeEach(() => {
  mocks.invoke.mockReset();
  mocks.scan = undefined;
  mocks.invoke.mockImplementation(async (command: string) => {
    switch (command) {
      case 'get_wallets': return { wallets: [wallet], activeAccountId: wallet.accountId };
      case 'get_wallet_balance': return '100000';
      case 'build_cold_transfer_request': return request;
      case 'submit_cold_transfer':
      case 'submit_hot_transfer': return { txHash };
      default: throw new Error(`未声明的合成调用: ${command}`);
    }
  });
});
afterEach(() => { cleanup(); vi.useRealTimers(); });

describe('现有四档金额和费用', () => {
  it.each([[1, 0.10], [104.99, 0.10], [105, 0.11], [1000, 1]])(
    '%s GMB按既有规则预估为%s GMB，传入签名流程的金额保持一致', (amount, fee) => {
      expect(calculateTransferFeeYuan(amount)).toBe(fee);
      const submit = vi.fn();
      const view = render(<TransferForm activeWallet={wallet} balance="1000000" onSubmit={submit} />);
      expect(view.container.querySelector('.transfer-form-balance-label')?.textContent).toBe('钱包余额');
      fireEvent.change(screen.getByPlaceholderText('请输入收款账户'), { target: { value: destination } });
      fireEvent.change(screen.getByPlaceholderText('0.00'), { target: { value: String(amount) } });
      expect(view.container.querySelector('.transfer-form-summary-line')?.textContent)
        .toContain(fee.toFixed(2));
      fireEvent.click(screen.getByRole('button', { name: '签名交易' }));
      expect(submit).toHaveBeenCalledExactlyOnceWith(destination, amount, '');
    },
  );

  it('零金额和超长UTF-8备注不发起签名', () => {
    const submit = vi.fn();
    render(<TransferForm activeWallet={wallet} balance="1000000" onSubmit={submit} />);
    fireEvent.change(screen.getByPlaceholderText('请输入收款账户'), { target: { value: destination } });
    fireEvent.click(screen.getByRole('button', { name: '签名交易' }));
    fireEvent.change(screen.getByPlaceholderText('0.00'), { target: { value: '105' } });
    fireEvent.change(screen.getByPlaceholderText('选填，最多 99 字节'), { target: { value: '中'.repeat(34) } });
    fireEvent.click(screen.getByRole('button', { name: '签名交易' }));
    expect(submit).not.toHaveBeenCalled();
  });
});

describe('钱包余额查询状态', () => {
  it('链上无账户记录时显示零余额，钱包列表也不再显示加载中', async () => {
    mocks.invoke.mockImplementation(async (command: string) => {
      if (command === 'get_wallets') return { wallets: [wallet], activeAccountId: wallet.accountId };
      if (command === 'get_wallet_balance') return null;
      throw new Error(`非预期调用：${command}`);
    });
    const view = render(<TransactionPanel />);
    await waitFor(() => expect(view.container.querySelector('.transfer-form-balance-value')?.textContent).toBe('0.00 元'));
    expect(screen.queryByText('查询中...')).toBeNull();
    fireEvent.click(screen.getByRole('button', { name: '钱包管理' }));
    await waitFor(() => expect(view.container.querySelector('.wallet-row-balance')?.textContent).toBe('0.00 元'));
    expect(screen.queryByText('查询中...')).toBeNull();
  });

  it('RPC错误显示原因，手动重试期间显示加载，成功后恢复余额', async () => {
    const pending = deferred<string>();
    let requests = 0;
    mocks.invoke.mockImplementation(async (command: string) => {
      if (command === 'get_wallets') return { wallets: [wallet], activeAccountId: wallet.accountId };
      if (command === 'get_wallet_balance') {
        if (++requests === 1) throw new Error('合成RPC连接失败');
        return pending.promise;
      }
      throw new Error(`非预期调用：${command}`);
    });
    render(<TransactionPanel />);
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
    mocks.invoke.mockImplementation(async (command: string) => {
      if (command === 'get_wallets') return { wallets: [wallet], activeAccountId: wallet.accountId };
      if (command === 'get_wallet_balance') throw new Error('合成RPC超时');
      throw new Error(`非预期调用：${command}`);
    });
    const view = render(<TransactionPanel />);
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
      if (command === 'get_wallets') return { wallets, activeAccountId: wallet.accountId };
      if (command === 'set_active_wallet') return { wallets, activeAccountId: otherWallet.accountId };
      if (command === 'get_wallet_balance') {
        if (args?.accountId === wallet.accountId && ++oldRequests === 1) return oldRequest.promise;
        return args?.accountId === wallet.accountId ? '10000' : '20000';
      }
      throw new Error(`非预期调用：${command}`);
    });
    const view = render(<TransactionPanel />);
    await waitFor(() => expect(oldRequests).toBe(1));
    fireEvent.click(screen.getByRole('button', { name: '钱包管理' }));
    await screen.findByText('另一钱包 · Cold');
    fireEvent.click(screen.getByText('另一钱包 · Cold'));
    await waitFor(() => expect(view.container.querySelector('.transfer-form-balance-value')?.textContent).toBe('200.00 元'));
    await act(async () => oldRequest.resolve('999999'));
    expect(view.container.querySelector('.transfer-form-balance-value')?.textContent).toBe('200.00 元');
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
      accountId: wallet.accountId, toSs58Address: destination, amountYuan: 105, remark: '合成备注',
    });
    await act(async () => pending.resolve(request));
    expect(screen.getByText('合成扫码界面')).toBeTruthy();
  });

  it('热签异步提交在同一事件批次内也只能发起一次', async () => {
    const pending = deferred<{ txHash: string }>();
    mocks.invoke.mockReturnValue(pending.promise);
    flow('hot');
    fireEvent.change(screen.getByPlaceholderText('请输入设备开机密码'), { target: { value: '合成测试口令' } });
    const button = screen.getByRole('button', { name: '确认并提交' });
    act(() => { button.click(); button.click(); });
    expect(mocks.invoke).toHaveBeenCalledTimes(1);
    await act(async () => pending.resolve({ txHash }));
    expect(screen.getByText('转账已提交')).toBeTruthy();
  });

  it('扫码重复回调只提交一次且完整保留同一签名会话字段', async () => {
    flow();
    fireEvent.click(screen.getByRole('button', { name: '确认转账' }));
    await screen.findByText('合成扫码界面');
    const pending = deferred<{ txHash: string }>();
    mocks.invoke.mockClear().mockReturnValue(pending.promise);
    const scan = mocks.scan!;
    act(() => { void scan('合成响应'); void scan('合成响应'); });
    expect(mocks.invoke).toHaveBeenCalledExactlyOnceWith('submit_cold_transfer', {
      accountId: wallet.accountId, requestId: request.requestId,
      expectedPayloadHash: request.expectedPayloadHash, callDataHex: request.callDataHex,
      signNonce: request.signNonce, signBlockNumber: request.signBlockNumber,
      responseJson: '合成响应',
    });
    await act(async () => pending.resolve({ txHash }));
    expect(screen.getByText('转账已提交')).toBeTruthy();
    // 已排队的摄像头回调即使在提交完成后到达，也不能再次提交同一会话。
    await act(async () => scan('合成响应'));
    expect(mocks.invoke).toHaveBeenCalledTimes(1);
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
    for (let second = 0; second <= 90; second++) {
      await act(async () => vi.advanceTimersByTime(1000));
    }
    expect(screen.getByText('签名请求已过期，请重新操作')).toBeTruthy();
    expect(mocks.invoke).toHaveBeenCalledTimes(1);
  });

  it('交易池返回哈希后首页明确显示已提交', async () => {
    const view = render(<TransactionPanel />);
    await screen.findByText('1,000.00 元');
    fireEvent.change(screen.getByPlaceholderText('请输入收款账户'), { target: { value: destination } });
    fireEvent.change(screen.getByPlaceholderText('0.00'), { target: { value: '105' } });
    fireEvent.click(screen.getByRole('button', { name: '签名交易' }));
    fireEvent.click(screen.getByRole('button', { name: '确认转账' }));
    await screen.findByText('合成扫码界面');
    await act(async () => mocks.scan!('合成响应'));
    fireEvent.click(screen.getByRole('button', { name: '完成' }));
    await waitFor(() => expect(view.container.querySelector('.transfer-success-msg')?.textContent)
      .toContain('转账已提交'));
    expect(view.container.textContent).not.toContain('转账成功');
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

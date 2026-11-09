import {beforeEach, expect, it, vi} from 'vitest';
import {adminsChangeApi} from '../institution/admins';
const mocks=vi.hoisted(()=>({invoke:vi.fn(),listen:vi.fn()}));
vi.mock('../client/tauri',()=>({invoke:mocks.invoke,sanitizeError:String}));
vi.mock('@tauri-apps/api/event',()=>({listen:mocks.listen}));
vi.mock('@tauri-apps/plugin-dialog', () => ({ open: vi.fn(async () => '/tmp/fixture-runtime.wasm') }));
const api=adminsChangeApi as unknown as Record<string,(...args:unknown[])=>Promise<unknown>>;
beforeEach(()=>{
  mocks.invoke.mockReset();
  mocks.listen.mockReset().mockResolvedValue(() => {});
});
it("buildActivateAdminRequest保留唯一IPC合同且返回真实回执", async () => { const receipt = { receipt: '合成回执' }; mocks.invoke.mockResolvedValueOnce(receipt); await expect(api["buildActivateAdminRequest"](...["public","44",{"cidNumber":"44","institutionCode":"NRC"}])).resolves.toBe(receipt); expect(mocks.invoke).toHaveBeenCalledExactlyOnceWith("build_activate_admin_request", {"signerPublicKey":"public","cidNumber":"44","expectedInstitutionCode":"NRC"}); });
it("getActivatedAdmins保留唯一IPC合同且返回真实回执", async () => { const receipt = { receipt: '合成回执' }; mocks.invoke.mockResolvedValueOnce(receipt); await expect(api["getActivatedAdmins"](...["44"])).resolves.toBe(receipt); expect(mocks.invoke).toHaveBeenCalledExactlyOnceWith("get_activated_admins", {"cidNumber":"44","expectedInstitutionCode":null}); });
it("getInstitutionAdminsState保留唯一IPC合同且返回真实回执", async () => { const receipt = { receipt: '合成回执' }; mocks.invoke.mockResolvedValueOnce(receipt); await expect(api["getInstitutionAdminsState"](...[{"cidNumber":"44","institutionCode":"NRC"}])).resolves.toBe(receipt); expect(mocks.invoke).toHaveBeenCalledExactlyOnceWith("get_institution_admins_state", {"cidNumber":"44","expectedInstitutionCode":"NRC"}); });
it('IPC失败保留拒绝状态且不自动重放', async()=>{const failure=new Error('合成失败');mocks.invoke.mockRejectedValueOnce(failure);await expect(api["buildActivateAdminRequest"](...["public","44",{"cidNumber":"44","institutionCode":"NRC"}])).rejects.toBe(failure);expect(mocks.invoke).toHaveBeenCalledOnce();});

import { act, fireEvent, render, screen, waitFor, within } from '@testing-library/react';
import { InstitutionDetailPage } from '../institution/InstitutionDetailPage';
import { accountIdToSs58 } from '../client/ss58';

vi.mock('../desktop/signing', () => ({
  CitizenSignaturePanel: ({ onScan }: { onScan: (result: string) => void }) => (
    <button type="button" onClick={() => onScan('模拟签名响应')}>扫描签名响应</button>
  ),
}));

const accountId = (byte: string) => '0x' + byte.repeat(32);
const heAccountId = accountId('05');

it.each([
  {orgType:0, expected:['主账户','费用账户','安全基金账户','两和基金账户']},
  {orgType:1, expected:['主账户','费用账户']},
  {orgType:2, expected:['主账户','永久质押账户','费用账户']},
])('机构类型 $orgType 使用共用账户表并保留返回规则', async ({orgType, expected}) => {
  const cidNumber = 'TEST-CID';
  mocks.invoke.mockImplementation(async (command: string) => {
    switch (command) {
      case 'get_institution_detail':
        return {
          cidNumber, orgType, cidFullName:'测试机构', admins:[], internalThreshold:2, jointVoteWeight:1,
          mainAccountId:accountId('01'), balanceFen:'100',
          nrcAccountId:orgType === 0 ? accountId('02') : null, nrcFeeFen:'200',
          fundAccountId:orgType === 0 ? accountId('03') : null, fundBalanceFen:'300',
          heAccountId:orgType === 0 ? heAccountId : null, heBalanceFen:null,
          cbAccountId:orgType === 1 ? accountId('02') : null, cbFeeFen:'200',
          feeAccountId:orgType === 2 ? accountId('02') : null, feeBalanceFen:'200',
          stakeAccountId:orgType === 2 ? accountId('04') : null, stakingBalanceFen:'400',
        };
      case 'get_activated_admins': return [];
      case 'get_institution_proposal_page': return {items:[], hasMore:false};
    }
  });

  const onBack = vi.fn();
  const view = render(<InstitutionDetailPage cidNumber={cidNumber} onBack={onBack} hideBackButton={orgType === 0} />);
  const table = await screen.findByRole('table');
  expect(within(table).getAllByRole('rowheader').map(row => row.textContent)).toEqual(expected);
  if (orgType === 0) {
    expect(screen.queryByRole('button', {name:'← 返回机构列表'})).toBeNull();
    const heRow = within(table).getByRole('row', {name:/^两和基金账户/});
    expect(heRow.textContent).toContain(accountIdToSs58(heAccountId));
    expect(within(heRow).getByText('—')).toBeTruthy();
    const receive = mocks.listen.mock.calls.find(([name]) => name === 'governance-balance-updated')?.[1];
    expect(receive).toBeDefined();
    await act(async () => receive({payload:{cidNumber, heBalanceFen:'19581850196600'}}));
    expect(within(heRow).getByText('195,818,501,966.00')).toBeTruthy();
  } else {
    fireEvent.click(screen.getByRole('button', {name:'← 返回机构列表'}));
    expect(onBack).toHaveBeenCalledOnce();
  }
  view.unmount();
});

const mainAccountId = accountId('01');
const feeAccountId = accountId('02');
const safetyFundAccountId = accountId('03');
const signerAccountId = accountId('06');
const receiver = accountIdToSs58(accountId('07'));
const transferCid = 'TEST-TRANSFER-CID';
const powParams = {
  paramsVersion: 1, algorithmVersion: 1, targetBlockTimeMs: 1000,
  adjustmentInterval: 20, maxAdjustUpFactor: 2, maxAdjustDownDivisor: 2,
};

function mockTransferInstitution(orgType: number, canProposeSweep: boolean) {
  mocks.invoke.mockImplementation(async (command: string) => {
    switch (command) {
      case 'get_institution_detail':
        return {
          cidNumber: transferCid, orgType, cidFullName: '测试转账机构',
          mainAccountId, balanceFen: '999999', internalThreshold: 6, jointVoteWeight: 1,
          nrcAccountId: orgType === 0 ? feeAccountId : null,
          cbAccountId: orgType === 1 ? feeAccountId : null,
          feeAccountId: orgType === 2 ? feeAccountId : null,
          fundAccountId: orgType === 0 ? safetyFundAccountId : null,
          heAccountId: orgType === 0 ? heAccountId : null,
          admins: [{
            accountId: signerAccountId,
            assignments: [{
              roleCode: 'COMMITTEE_MEMBER',
              permissions: canProposeSweep ? [{ moduleTag: 'multisig', actionCode: 2, operation: 0 }] : [],
            }],
          }],
        };
      case 'get_activated_admins': return [{ accountId: signerAccountId }];
      case 'get_institution_proposal_page': return { items: [], hasMore: false };
      case 'get_grandpa_key_change_status': return { pending: false };
      case 'get_pow_difficulty_params': return powParams;
      case 'build_grandpa_key_change_request':
      case 'build_propose_upgrade_request':
        return { requestId: 'governance-request', requestJson: '{}', expectedPayloadHash: 'hash',
          signNonce: 1, signBlockNumber: 2, newPublicKey: accountId('08') };
      case 'submit_grandpa_key_change':
      case 'submit_propose_upgrade':
        return { txHash: '0x1234' };
    }
    if (command.startsWith('build_multisig_')) {
      return { requestId: 'request', requestJson: '{}', expectedPayloadHash: 'hash', signNonce: 1, signBlockNumber: 2 };
    }
    if (command.startsWith('submit_multisig_')) return { txHash: '0x1234' };
  });
}

function installDialogMock() {
  const prototype = HTMLDialogElement.prototype;
  const show = Object.getOwnPropertyDescriptor(prototype, 'showModal');
  const close = Object.getOwnPropertyDescriptor(prototype, 'close');
  Object.defineProperties(prototype, {
    showModal: { configurable: true, value(this: HTMLDialogElement) { this.open = true; } },
    close: { configurable: true, value(this: HTMLDialogElement) { this.open = false; } },
  });
  return () => {
    if (show) Object.defineProperty(prototype, 'showModal', show);
    else Reflect.deleteProperty(prototype, 'showModal');
    if (close) Object.defineProperty(prototype, 'close', close);
    else Reflect.deleteProperty(prototype, 'close');
  };
}

it.each([
  { button: '转账', title: '发起转账提案', orgType: 2, source: mainAccountId, call: 'transfer' },
  { button: '两和基金转账', title: '两和基金转账提案', orgType: 0, source: heAccountId, call: 'transfer' },
  { button: '安全基金转账', title: '安全基金转账提案', orgType: 0, source: safetyFundAccountId, call: 'safety_fund' },
  { button: '手续费划转', title: '手续费划转提案', orgType: 1, source: feeAccountId, call: 'sweep' },
])('$button 在原机构详情上方弹出，并使用对应账户与签名调用', async ({ button, title, orgType, source, call }) => {
  const restoreDialog = installDialogMock();
  mockTransferInstitution(orgType, true);
  const view = render(<InstitutionDetailPage cidNumber={transferCid} onBack={vi.fn()} />);
  try {
    const trigger = await screen.findByRole('button', { name: button });
    fireEvent.click(trigger);
    const dialog = screen.getByRole('dialog', { name: title });
    const panel = within(dialog);
    expect(screen.getByRole('heading', { name: '提案列表' })).toBeTruthy();
    expect((panel.getByLabelText(/^转出地址/) as HTMLInputElement).value).toBe(accountIdToSs58(source));
    const footer = dialog.querySelector('footer')!;
    expect(within(footer).getAllByRole('button').map(control => control.textContent)).toEqual(['取消', '签名']);
    fireEvent.click(dialog);
    expect(screen.getByRole('dialog', { name: title })).toBe(dialog);
    fireEvent.click(panel.getByRole('button', { name: '取消' }));
    expect(screen.queryByRole('dialog', { name: title })).toBeNull();
    await waitFor(() => expect(document.activeElement).toBe(trigger));
    expect(mocks.invoke.mock.calls.some(([name]) => name.startsWith('build_multisig_'))).toBe(false);

    fireEvent.click(trigger);
    const signingDialog = screen.getByRole('dialog', { name: title });
    const signingPanel = within(signingDialog);
    const role = orgType === 2 ? 'DIRECTOR' : 'COMMITTEE_MEMBER';
    expect(signingPanel.queryByLabelText('提案发起岗位码')).toBeNull();
    fireEvent.change(signingPanel.getByLabelText(/金额/), { target: { value: '10.00' } });
    if (call === 'sweep') {
      const destination = signingPanel.getByLabelText('收款地址（本机构主账户）') as HTMLInputElement;
      expect(destination.readOnly).toBe(true);
      expect(destination.value).toBe(accountIdToSs58(mainAccountId));
      expect(signingPanel.getByText(/至少保留 1\.11 元.*80%/)).toBeTruthy();
    } else {
      fireEvent.change(signingPanel.getByLabelText('收款地址（SS58）'), { target: { value: receiver } });
    }
    fireEvent.click(signingPanel.getByRole('button', { name: '签名' }));
    const scanner = await signingPanel.findByRole('button', { name: '扫描签名响应' });
    expect(screen.getByRole('dialog', { name: title })).toBe(signingDialog);
    const requestArgs = {
      actorCidNumber: transferCid, proposerRoleCode: role, institutionAccountId: source, amountYuan: 10,
      ...(call === 'sweep' ? {} : { beneficiarySs58Address: receiver, remark: '' }),
    };
    expect(mocks.invoke).toHaveBeenCalledWith(`build_multisig_${call}_request`, { signerPublicKey: signerAccountId, ...requestArgs });
    fireEvent.click(scanner);
    fireEvent.click(await signingPanel.findByRole('button', { name: '完成' }));
    expect(mocks.invoke).toHaveBeenCalledWith(`submit_multisig_${call}`, {
      ...requestArgs, expectedPublicKey: signerAccountId, requestId: 'request', expectedPayloadHash: 'hash',
      signNonce: 1, signBlockNumber: 2, responseJson: '模拟签名响应',
    });
    expect(screen.queryByRole('dialog', { name: title })).toBeNull();
    expect(mocks.invoke.mock.calls.filter(([name]) => name === 'get_institution_proposal_page')).toHaveLength(2);
  } finally {
    view.unmount();
    restoreDialog();
  }
});

it.each([
  { title: '验证密钥', orgType: 1, build: 'build_grandpa_key_change_request', submit: 'submit_grandpa_key_change' },
  { title: '协议升级', orgType: 0, build: 'build_propose_upgrade_request', submit: 'submit_propose_upgrade' },
])('$title 保留机构详情并在同一弹窗完成签名和刷新', async ({ title, orgType, build, submit }) => {
  const restoreDialog = installDialogMock();
  mockTransferInstitution(orgType, true);
  const view = render(<InstitutionDetailPage cidNumber={transferCid} onBack={vi.fn()} />);
  try {
    const trigger = await screen.findByRole('button', { name: title });
    const overview = screen.getByRole('heading', { name: '提案列表' });
    fireEvent.click(trigger);
    const initialDialog = screen.getByRole('dialog', { name: title });
    fireEvent.click(initialDialog);
    expect(screen.getByRole('dialog', { name: title })).toBe(initialDialog);
    fireEvent.click(within(initialDialog).getByRole('button', { name: '取消' }));
    expect(screen.queryByRole('dialog', { name: title })).toBeNull();
    await waitFor(() => expect(document.activeElement).toBe(trigger));
    expect(mocks.invoke.mock.calls.some(([command]) => command === build)).toBe(false);

    fireEvent.click(trigger);
    const dialog = screen.getByRole('dialog', { name: title });
    const panel = within(dialog);
    if (title === '验证密钥') {
      fireEvent.change(panel.getByLabelText('本机解锁密码'), { target: { value: 'fixture-password' } });
    } else {
      fireEvent.change(panel.getByLabelText('升级理由'), { target: { value: '测试升级理由' } });
      fireEvent.click(panel.getByRole('button', { name: '选择文件' }));
      await panel.findByText('fixture-runtime.wasm');
    }
    fireEvent.click(panel.getByRole('button', { name: '签名' }));
    fireEvent.click(await panel.findByRole('button', { name: '扫描签名响应' }));
    expect(screen.getByRole('dialog', { name: title })).toBe(dialog);
    expect(mocks.invoke).toHaveBeenCalledWith(build, expect.objectContaining({
      actorCidNumber: transferCid, signerPublicKey: signerAccountId,
    }));
    expect(mocks.invoke).toHaveBeenCalledWith(submit, expect.objectContaining({
      requestId: 'governance-request', expectedPublicKey: signerAccountId, responseJson: '模拟签名响应',
    }));
    fireEvent.click(await panel.findByRole('button', { name: '完成' }));
    expect(screen.queryByRole('dialog', { name: title })).toBeNull();
    expect(screen.getByRole('heading', { name: '提案列表' })).toBe(overview);
    expect(mocks.invoke.mock.calls.filter(([command]) => command === 'get_institution_proposal_page')).toHaveLength(2);
  } finally {
    view.unmount();
    restoreDialog();
  }
});

it('省储委会在链上没有划转发起权时不可打开手续费划转', async () => {
  mockTransferInstitution(1, false);
  const view = render(<InstitutionDetailPage cidNumber={transferCid} onBack={vi.fn()} />);
  const sweep = await screen.findByRole('button', { name: '手续费划转' }) as HTMLButtonElement;
  expect(sweep.disabled).toBe(true);
  fireEvent.click(sweep);
  expect(screen.queryByRole('dialog')).toBeNull();
  view.unmount();
});

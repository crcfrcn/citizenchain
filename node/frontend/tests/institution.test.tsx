// 验证机构管理员与升级提案交互、IPC参数及失败重试状态。

import {beforeEach, expect, it, vi} from 'vitest';
import {adminsChangeApi} from '../institution/admins';
import {TransferProposalDialog} from '../multisig/TransferProposalDialog';
import {open as pickUpgradeFile} from '@tauri-apps/plugin-dialog';
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
import { InstitutionAssignmentCard } from '../institution/InstitutionAssignmentCard';

// 共用卡片的两种模式覆盖真实姓名、空姓名和空白边界，保留原字段及操作回归。
it.each([false, true].flatMap((compactVote) => [
  { familyName: '合成', givenName: '人员', expectedName: '合成人员' },
  { familyName: '', givenName: '', expectedName: '未填写' },
  { familyName: '  ', givenName: '\t ', expectedName: '未填写' },
  { familyName: ' 合成', givenName: '人员 ', expectedName: '合成人员' },
].map((person) => ({ compactVote, ...person }))))(
  '管理员卡片模式=$compactVote、姓名=$expectedName统一岗位与姓名，保留序号与操作',
  ({compactVote, familyName, givenName, expectedName}) => {
  const click = vi.fn();
  const {container, unmount} = render(<InstitutionAssignmentCard compactVote={compactVote} index={3}
    balanceFen="12345" action={<button onClick={click}>原操作</button>}
    admin={{accountId:accountId('01'),familyName,givenName,assignments:[{
      roleCode:'MEMBER',roleName:'委员',termRequired:true,termStart:1,termEnd:2,
      assignmentSource:0,assignmentSourceLabel:'创世',assignmentSourceRef:'TEST',
    }]}} />);
  const top = container.querySelector('.institution-assignment-top')!;
  expect(top.textContent).toBe(`3委员${expectedName}原操作`);
  expect(top.querySelector('.institution-assignment-name')?.textContent).toBe(expectedName);
  expect(top.querySelector('.admin-card-index')?.textContent).toBe('3');
  expect(top.querySelector('.institution-assignment-dot')?.getAttribute('aria-hidden')).toBe('true');
  const body = container.querySelector('.institution-assignment-account')!;
  expect([...body.querySelectorAll('.institution-assignment-label')].map(node=>node.textContent))
    .toEqual(['来源:','任期:','账户余额:','管理员账户:']);
  expect(body.textContent).toContain('1970/01/02-1970/01/03');
  expect(body.textContent).toContain('创世（依据：TEST）');
  expect(body.textContent).toContain(accountIdToSs58(accountId('01')));
  fireEvent.click(within(container).getByRole('button',{name:'原操作'}));
  expect(click).toHaveBeenCalledOnce();
  unmount();
});

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
  {orgType:2, expected:['主账户','费用账户','永久质押账户']},
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
const ethereumReceiver = '0x5aAeb6053F3E94C9b9A09f33669435E7Ef1BeAed';
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

it.each([0, 1, 2])('机构类型 %s 的账户说明逐字匹配，按机构区分专用账户并支持点外部关闭', async (orgType) => {
  const restoreDialog = installDialogMock();
  mockTransferInstitution(orgType, true);
  const view = render(<InstitutionDetailPage cidNumber={transferCid} onBack={vi.fn()} />);
  try {
    const accounts = await screen.findByRole('region', {name:'账户列表'});
    const trigger = within(accounts).getByRole('button', {name:'机构账户说明'});
    expect(trigger.textContent).toBe('说明');
    expect(trigger.previousElementSibling?.textContent).toBe('账户列表');
    const requestsBeforeHelp = mocks.invoke.mock.calls.length;
    fireEvent.click(trigger);
    const help = screen.getByRole('dialog', {name:'机构账户说明'});
    expect(within(help).getByRole('heading', {name:'机构账户说明'})).toBeTruthy();
    expect(within(help).getAllByRole('listitem').map(item => item.textContent)).toEqual([
      '所有机构均有1个主账户和1个费用账户，主账户和费用账户不可注销，所有机构特指链上注册的机构。',
      '主账户用于该机构持有资金，费用账户用于支付该机构交易产生的手续费等费用，费用账户只能向主账户转账。',
      '机构可注册更多账户，机构账户由账户名称+身份CID号经哈希运算后产生，故一个身份CID号+不同账户名称可产生无限多的机构账户。',
      '机构账户发起转账交易的，必须由该机构的管理员发起转账提案，经该机构管理员内部投票通过后执行。',
      ...(orgType === 0 ? [
        '国家储委会拥有安全基金专用账户，安全基金用于公民链的技术安全，赔付因公民链技术原因造成的用户资金损失。',
        '国家储委会拥有两和基金专用账户，两和基金用于历史和解与和平建国，修复因内战、运动、迫害等历史问题造成的民族创伤。',
      ] : orgType === 2 ? [
        '省储行拥有永久质押账户，永久质押账户用于永久质押省储行创立发行的公民币，永久质押账户只能收款，不能支出。',
      ] : []),
    ]);
    fireEvent.click(help.querySelector('.metric-help-content')!);
    expect(screen.getByRole('dialog', {name:'机构账户说明'})).toBe(help);
    fireEvent.click(help);
    expect(screen.queryByRole('dialog', {name:'机构账户说明'})).toBeNull();
    expect(within(accounts).getByRole('table')).toBeTruthy();
    expect(mocks.invoke.mock.calls.length).toBe(requestsBeforeHelp);
  } finally {
    view.unmount();
    restoreDialog();
  }
});

const transferCases = [
  { button: '转账', title: '发起转账提案', orgType: 2, source: mainAccountId, call: 'transfer' },
  { button: '两和基金转账', title: '两和基金转账提案', orgType: 0, source: heAccountId, call: 'transfer' },
  { button: '安全基金转账', title: '安全基金转账提案', orgType: 0, source: safetyFundAccountId, call: 'safety_fund' },
  { button: '手续费划转', title: '手续费划转提案', orgType: 1, source: feeAccountId, call: 'sweep' },
].flatMap((testCase) => {
  const addresses: { addressKind: string; recipient: string | null }[] = testCase.call === 'sweep'
    ? [{ addressKind: '固定主账户', recipient: null }]
    : [
      { addressKind: 'SS58', recipient: receiver },
      { addressKind: '以太坊', recipient: ethereumReceiver },
    ];
  return addresses.map((address) => ({ ...testCase, ...address }));
});

it.each([
  ['发起转账提案', '转账提案说明', '转账本金由主账户支付，金额手续费由本机构费用账户支付。'],
  ['手续费划转提案', '手续费划转提案说明', '单次划转本金不得超过可用余额的80%；可用余额按账户余额扣除链上最低保留余额计算。'],
  ['安全基金转账提案', '安全基金转账提案说明', '转账本金由安全基金账户支付，金额手续费由国家储委会费用账户支付。'],
  ['两和基金转账提案', '两和基金转账提案说明', '转账本金由两和基金账户支付，金额手续费由国家储委会费用账户支付。'],
  ['验证密钥', '验证密钥说明', '正常更换由有权限的委员发起，同时验证旧、新密钥的签名证明，无须内部投票。'],
  ['协议升级', '协议升级提案说明', '用于更新全链公民协议，即公民链Runtime运行时协议。'],
  ['决议发行提案', '决议发行提案说明', '收款账户须与链上允许的省储委会账户集合一致，各项分配金额合计必须等于发行总额。'],
  ['决议销毁提案', '决议销毁提案说明', '销毁金额从指定的本机构账户中扣除并减少全链发行总量，销毁后不可恢复。'],
])('%s 共用标题右侧说明，关闭说明保留原表单且不发起签名', (title, helpTitle, content) => {
  const restoreDialog = installDialogMock();
  const cancel = vi.fn();
  const sign = vi.fn();
  const view = render(
    <TransferProposalDialog title={title} institutionName="测试机构" step="form"
      generating={false} signDisabled={false} onCancel={cancel} onSign={sign} onComplete={vi.fn()}>
      <input aria-label="保留输入" defaultValue="尚未提交的内容" />
    </TransferProposalDialog>,
  );
  try {
    const parent = screen.getByRole('dialog', {name:title});
    const trigger = within(parent).getByRole('button', {name:helpTitle});
    expect(trigger.textContent).toBe('说明');
    expect(trigger.closest('.institution-proposal-title')?.querySelector('h2')?.textContent).toBe(title);
    fireEvent.click(trigger);
    const help = screen.getByRole('dialog', {name:helpTitle});
    expect(within(help).getByRole('heading', {name:helpTitle})).toBeTruthy();
    expect(within(help).getByText(content)).toBeTruthy();
    expect(within(help).getAllByRole('listitem').length).toBeGreaterThanOrEqual(4);
    expect(help.parentElement).toBe(document.body);
    fireEvent.click(help.querySelector('.metric-help-content')!);
    expect(screen.getByRole('dialog', {name:helpTitle})).toBe(help);
    fireEvent.click(help);
    expect(screen.queryByRole('dialog', {name:helpTitle})).toBeNull();
    expect(screen.getByRole('dialog', {name:title})).toBe(parent);
    expect((within(parent).getByLabelText('保留输入') as HTMLInputElement).value).toBe('尚未提交的内容');
    fireEvent.click(trigger);
    fireEvent(screen.getByRole('dialog', {name:helpTitle}), new Event('cancel', {cancelable:true}));
    expect(screen.queryByRole('dialog', {name:helpTitle})).toBeNull();
    expect(screen.getByRole('dialog', {name:title})).toBe(parent);
    expect(cancel).not.toHaveBeenCalled();
    expect(sign).not.toHaveBeenCalled();
    expect(mocks.invoke).not.toHaveBeenCalled();
  } finally {
    view.unmount();
    restoreDialog();
  }
});

it('协议升级提案说明逐字采用确认文案', () => {
  const restoreDialog = installDialogMock();
  const view = render(<TransferProposalDialog title="协议升级" institutionName="测试机构" step="form"
    generating={false} signDisabled={false} onCancel={vi.fn()} onSign={vi.fn()} onComplete={vi.fn()}>
    <p>表单</p>
  </TransferProposalDialog>);
  try {
    fireEvent.click(screen.getByRole('button', {name:'协议升级提案说明'}));
    const help = screen.getByRole('dialog', {name:'协议升级提案说明'});
    expect(within(help).getAllByRole('listitem').map(item => item.textContent)).toEqual([
      '用于更新全链公民协议，即公民链Runtime运行时协议。',
      '由具有相应权限的国家储委会或省储委会管理员发起，提交升级理由、Runtime WASM文件等参数。',
      '提案进入全体治理机构的联合投票，符合转入条件时进入联合公投。',
      '投票通过后由链上执行升级，配套参数按链上规定生效；执行是否成功以链上结果为准。',
      '新版公民协议须遵守节点程序的宪法守卫和节点守卫，否则节点将拒绝新版公民协议。',
    ]);
  } finally {
    view.unmount();
    restoreDialog();
  }
});

it.each(transferCases)('$button 在原机构详情上方弹出，使用 $addressKind 收款地址签名和提交', async ({ button, title, orgType, source, call, recipient }) => {
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
    if (recipient === null) {
      const destination = signingPanel.getByLabelText('收款地址（本机构主账户）') as HTMLInputElement;
      expect(destination.readOnly).toBe(true);
      expect(destination.value).toBe(accountIdToSs58(mainAccountId));
      expect(signingPanel.getByText(/至少保留 1\.11 元.*80%/)).toBeTruthy();
    } else {
      fireEvent.change(signingPanel.getByLabelText('收款地址'), { target: { value: recipient } });
    }
    fireEvent.click(signingPanel.getByRole('button', { name: '签名' }));
    const scanner = await signingPanel.findByRole('button', { name: '扫描签名响应' });
    expect(screen.getByRole('dialog', { name: title })).toBe(signingDialog);
    const requestArgs = {
      actorCidNumber: transferCid, proposerRoleCode: role, institutionAccountId: source, amountYuan: 10,
      ...(recipient === null ? {} : { beneficiarySs58Address: recipient, remark: '' }),
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

it.each([false, true])('协议升级高级设置默认收起，编辑再收起=%s 时构建和提交保留完整PoW参数', async (editParameters) => {
  const restoreDialog = installDialogMock();
  mockTransferInstitution(0, true);
  const view = render(<InstitutionDetailPage cidNumber={transferCid} onBack={vi.fn()} />);
  try {
    fireEvent.click(await screen.findByRole('button', {name:'协议升级'}));
    const dialog = screen.getByRole('dialog', {name:'协议升级'});
    const panel = within(dialog);
    const advanced = await panel.findByRole('button', {name:'高级设置'});
    expect(panel.getByText('设置 PoW 参数')).toBeTruthy();
    expect(panel.queryByText('PoW 参数（与 runtime code 一起进入联合投票）')).toBeNull();
    expect(advanced.getAttribute('aria-expanded')).toBe('false');
    expect(panel.queryAllByRole('spinbutton')).toHaveLength(0);
    const expectedParams = editParameters
      ? {...powParams, paramsVersion:2, targetBlockTimeMs:2000}
      : powParams;
    if (editParameters) {
      fireEvent.click(advanced);
      expect(advanced.getAttribute('aria-expanded')).toBe('true');
      expect(panel.getAllByRole('spinbutton')).toHaveLength(6);
      fireEvent.change(panel.getByRole('spinbutton', {name:'参数版本'}), {target:{value:'2'}});
      fireEvent.change(panel.getByRole('spinbutton', {name:'平均目标时间（毫秒）'}), {target:{value:'2000'}});
      fireEvent.click(advanced);
      expect(panel.queryAllByRole('spinbutton')).toHaveLength(0);
      expect(advanced.getAttribute('aria-expanded')).toBe('false');
      fireEvent.click(advanced);
      expect(panel.getByRole('spinbutton', {name:'参数版本'})).toHaveProperty('value', '2');
      expect(panel.getByRole('spinbutton', {name:'平均目标时间（毫秒）'})).toHaveProperty('value', '2000');
      fireEvent.click(advanced);
    }
    expect(mocks.invoke.mock.calls.filter(([command]) => command === 'get_pow_difficulty_params')).toHaveLength(1);
    fireEvent.change(panel.getByLabelText('升级理由'), {target:{value:'折叠参数回归'}});
    fireEvent.click(panel.getByRole('button', {name:'选择文件'}));
    await panel.findByText('fixture-runtime.wasm');
    fireEvent.click(panel.getByRole('button', {name:'签名'}));
    fireEvent.click(await panel.findByRole('button', {name:'扫描签名响应'}));
    await panel.findByRole('button', {name:'完成'});
    expect(panel.getByText('升级提案交易已提交')).toBeTruthy();
    expect(panel.getByText('等待链上确认，最终确认后可在提案列表查看。')).toBeTruthy();
    expect(panel.queryByText('提案已进入联合投票阶段。')).toBeNull();
    for (const command of ['build_propose_upgrade_request', 'submit_propose_upgrade']) {
      expect(mocks.invoke).toHaveBeenCalledWith(command, expect.objectContaining({
        powParams:expectedParams, actorCidNumber:transferCid,
      }));
    }
    expect(panel.queryAllByRole('spinbutton')).toHaveLength(0);
  } finally {
    view.unmount();
    restoreDialog();
  }
});

it('协议升级理由和路径的首尾空白在构建与提交时保持相同规范化', async () => {
  const restoreDialog = installDialogMock();
  mockTransferInstitution(0, true);
  vi.mocked(pickUpgradeFile).mockResolvedValueOnce('  /tmp/fixture-runtime.wasm  ');
  const view = render(<InstitutionDetailPage cidNumber={transferCid} onBack={vi.fn()} />);
  try {
    fireEvent.click(await screen.findByRole('button', {name:'协议升级'}));
    const panel = within(screen.getByRole('dialog', {name:'协议升级'}));
    await panel.findByRole('button', {name:'高级设置'});
    fireEvent.change(panel.getByLabelText('升级理由'), {target:{value:' \n 升级说明 \t'}});
    fireEvent.click(panel.getByRole('button', {name:'选择文件'}));
    await panel.findByText('fixture-runtime.wasm');
    fireEvent.click(panel.getByRole('button', {name:'签名'}));
    fireEvent.click(await panel.findByRole('button', {name:'扫描签名响应'}));
    await panel.findByRole('button', {name:'完成'});
    for (const command of ['build_propose_upgrade_request', 'submit_propose_upgrade']) {
      expect(mocks.invoke).toHaveBeenCalledWith(command, expect.objectContaining({
        reason:'升级说明', wasmPath:'/tmp/fixture-runtime.wasm', powParams,
      }));
    }
  } finally {
    view.unmount();
    restoreDialog();
  }
});

it.each([
  {reason:'中'.repeat(341) + 'a', bytes:1024, allowed:true},
  {reason:'中'.repeat(342), bytes:1026, allowed:false},
])('协议升级理由按UTF8的 $bytes 字节边界在扫码前校验', async ({reason, bytes, allowed}) => {
  const restoreDialog = installDialogMock();
  mockTransferInstitution(0, true);
  const view = render(<InstitutionDetailPage cidNumber={transferCid} onBack={vi.fn()} />);
  try {
    fireEvent.click(await screen.findByRole('button', {name:'协议升级'}));
    const panel = within(screen.getByRole('dialog', {name:'协议升级'}));
    await panel.findByRole('button', {name:'高级设置'});
    fireEvent.change(panel.getByLabelText('升级理由'), {target:{value:reason}});
    fireEvent.click(panel.getByRole('button', {name:'选择文件'}));
    await panel.findByText('fixture-runtime.wasm');
    fireEvent.click(panel.getByRole('button', {name:'签名'}));
    if (allowed) {
      await panel.findByRole('button', {name:'扫描签名响应'});
      expect(mocks.invoke).toHaveBeenCalledWith('build_propose_upgrade_request', expect.objectContaining({reason}));
    } else {
      expect((await panel.findByRole('alert')).textContent).toContain(`当前 ${bytes} 字节`);
      expect(mocks.invoke.mock.calls.some(([command]) => command === 'build_propose_upgrade_request')).toBe(false);
      expect(panel.queryByRole('button', {name:'扫描签名响应'})).toBeNull();
    }
  } finally {
    view.unmount();
    restoreDialog();
  }
});

it.each(['0', '1.5', '4294967296'])('协议升级在扫码前拒绝无效PoW整数 %s', async (value) => {
  const restoreDialog = installDialogMock();
  mockTransferInstitution(0, true);
  const view = render(<InstitutionDetailPage cidNumber={transferCid} onBack={vi.fn()} />);
  try {
    fireEvent.click(await screen.findByRole('button', {name:'协议升级'}));
    const panel = within(screen.getByRole('dialog', {name:'协议升级'}));
    fireEvent.click(await panel.findByRole('button', {name:'高级设置'}));
    fireEvent.change(panel.getByRole('spinbutton', {name:'参数版本'}), {target:{value}});
    fireEvent.change(panel.getByLabelText('升级理由'), {target:{value:'参数校验'}});
    fireEvent.click(panel.getByRole('button', {name:'选择文件'}));
    await panel.findByText('fixture-runtime.wasm');
    fireEvent.click(panel.getByRole('button', {name:'签名'}));
    expect((await panel.findByRole('alert')).textContent).toContain('PoW 参数');
    expect(mocks.invoke.mock.calls.some(([command]) => command === 'build_propose_upgrade_request')).toBe(false);
  } finally {
    view.unmount();
    restoreDialog();
  }
});

it('协议升级读取参数失败后可就地重试，保留理由、文件与管理员', async () => {
  const restoreDialog = installDialogMock();
  mockTransferInstitution(0, true);
  const original = mocks.invoke.getMockImplementation()!;
  let reads = 0;
  mocks.invoke.mockImplementation(async (command: string, ...args: unknown[]) => {
    if (command === 'get_pow_difficulty_params' && reads++ === 0) throw new Error('参数暂时读取失败');
    return original(command, ...args);
  });
  const view = render(<InstitutionDetailPage cidNumber={transferCid} onBack={vi.fn()} />);
  try {
    fireEvent.click(await screen.findByRole('button', {name:'协议升级'}));
    const dialog = screen.getByRole('dialog', {name:'协议升级'});
    const panel = within(dialog);
    const retry = await panel.findByRole('button', {name:'重试读取'});
    fireEvent.change(panel.getByLabelText('升级理由'), {target:{value:'保留的升级理由'}});
    fireEvent.click(panel.getByRole('button', {name:'选择文件'}));
    await panel.findByText('fixture-runtime.wasm');
    expect(panel.getByRole('button', {name:'签名'})).toHaveProperty('disabled', true);
    fireEvent.click(retry);
    await panel.findByRole('button', {name:'高级设置'});
    expect(panel.getByLabelText('升级理由')).toHaveProperty('value', '保留的升级理由');
    expect(panel.getByText('fixture-runtime.wasm')).toBeTruthy();
    expect(panel.getByRole('combobox', {name:'发起管理员'})).toHaveProperty('value', signerAccountId);
    expect(panel.queryByRole('button', {name:'重试读取'})).toBeNull();
    expect(reads).toBe(2);
    fireEvent.click(panel.getByRole('button', {name:'签名'}));
    await panel.findByRole('button', {name:'扫描签名响应'});
    expect(mocks.invoke).toHaveBeenCalledWith('build_propose_upgrade_request', expect.objectContaining({
      reason:'保留的升级理由', powParams,
    }));
  } finally {
    view.unmount();
    restoreDialog();
  }
});

it('协议升级的节点预检拒绝在二维码生成前展示，重试保留原输入', async () => {
  const restoreDialog = installDialogMock();
  mockTransferInstitution(0, true);
  const original = mocks.invoke.getMockImplementation()!;
  mocks.invoke.mockImplementation(async (command: string, ...args: unknown[]) => {
    if (command === 'build_propose_upgrade_request') throw new Error('当前 Runtime 不支持该 PoW 算法版本');
    return original(command, ...args);
  });
  const view = render(<InstitutionDetailPage cidNumber={transferCid} onBack={vi.fn()} />);
  try {
    fireEvent.click(await screen.findByRole('button', {name:'协议升级'}));
    const panel = within(screen.getByRole('dialog', {name:'协议升级'}));
    fireEvent.click(await panel.findByRole('button', {name:'高级设置'}));
    fireEvent.change(panel.getByRole('spinbutton', {name:'算法版本'}), {target:{value:'2'}});
    fireEvent.change(panel.getByLabelText('升级理由'), {target:{value:'保留待修正参数'}});
    fireEvent.click(panel.getByRole('button', {name:'选择文件'}));
    await panel.findByText('fixture-runtime.wasm');
    fireEvent.click(panel.getByRole('button', {name:'签名'}));
    await panel.findByText(/当前 Runtime 不支持该 PoW 算法版本/);
    expect(panel.queryByRole('button', {name:'扫描签名响应'})).toBeNull();
    expect(mocks.invoke.mock.calls.some(([command]) => command === 'submit_propose_upgrade')).toBe(false);
    fireEvent.click(panel.getByRole('button', {name:'重试'}));
    expect(panel.getByLabelText('升级理由')).toHaveProperty('value', '保留待修正参数');
    expect(panel.getByRole('spinbutton', {name:'算法版本'})).toHaveProperty('value', '2');
    expect(panel.getByText('fixture-runtime.wasm')).toBeTruthy();
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

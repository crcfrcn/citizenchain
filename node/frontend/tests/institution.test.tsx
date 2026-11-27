// 验证机构岗位与升级提案交互、IPC参数及失败重试状态。

import {beforeEach, expect, it, vi} from 'vitest';
import {readFileSync} from 'node:fs';
import {resolve} from 'node:path';
import {rolesApi} from '../institution/roles';
import {TransferProposalDialog} from '../multisig/TransferProposalDialog';
import {open as pickUpgradeFile} from '@tauri-apps/plugin-dialog';
const mocks=vi.hoisted(()=>({invoke:vi.fn(),listen:vi.fn(),activationScan:undefined as ((s:string)=>void)|undefined,activationScanError:undefined as ((s:string)=>void)|undefined}));
vi.mock('../client/tauri',()=>({invoke:mocks.invoke,sanitizeError:String}));
vi.mock('@tauri-apps/api/event',()=>({listen:mocks.listen}));
vi.mock('@tauri-apps/plugin-dialog', () => ({ open: vi.fn(async () => '/tmp/fixture-runtime.wasm') }));
const api=rolesApi as unknown as Record<string,(...args:unknown[])=>Promise<unknown>>;
beforeEach(()=>{
  mocks.invoke.mockReset();
  mocks.listen.mockReset().mockResolvedValue(() => {});
  mocks.activationScan=undefined;mocks.activationScanError=undefined;
});
it("buildRoleAccessRequest保留唯一IPC合同且返回真实回执", async () => { const receipt = { receipt: '合成回执' }; mocks.invoke.mockResolvedValueOnce(receipt); await expect(api["buildRoleAccessRequest"](...["public","44","person","ROLE",{"cidNumber":"44","institutionCode":"NRC"}])).resolves.toBe(receipt); expect(mocks.invoke).toHaveBeenCalledExactlyOnceWith("build_role_access_request", {"signerPublicKey":"public","cidNumber":"44","personCid":"person","roleCode":"ROLE","expectedInstitutionCode":"NRC"}); });
it("getRoleAccess保留唯一IPC合同且返回真实回执", async () => { const receipt = { receipt: '合成回执' }; mocks.invoke.mockResolvedValueOnce(receipt); await expect(api["getRoleAccess"](...["44"])).resolves.toBe(receipt); expect(mocks.invoke).toHaveBeenCalledExactlyOnceWith("get_role_access", {"cidNumber":"44","expectedInstitutionCode":null}); });
it("getInstitutionRoles保留唯一IPC合同且返回真实回执", async () => { const receipt = { receipt: '合成回执' }; mocks.invoke.mockResolvedValueOnce(receipt); await expect(api["getInstitutionRoles"](...[{"cidNumber":"44","institutionCode":"NRC"}])).resolves.toBe(receipt); expect(mocks.invoke).toHaveBeenCalledExactlyOnceWith("get_institution_roles", {"cidNumber":"44","expectedInstitutionCode":"NRC"}); });
it('IPC失败保留拒绝状态且不自动重放', async()=>{const failure=new Error('合成失败');mocks.invoke.mockRejectedValueOnce(failure);await expect(api["buildRoleAccessRequest"](...["public","44","person","ROLE",{"cidNumber":"44","institutionCode":"NRC"}])).rejects.toBe(failure);expect(mocks.invoke).toHaveBeenCalledOnce();});

import { act, fireEvent, render, screen, waitFor, within } from '@testing-library/react';
import { InstitutionDetailPage } from '../institution/InstitutionDetailPage';
import { accountIdToSs58 } from '../client/ss58';
import {AssignmentsPage} from '../institution/AssignmentsPage';
import {NrcSection} from '../institution/NrcSection';
import {PrcSection} from '../institution/PrcSection';
import {PrbSection} from '../institution/PrbSection';
import { InstitutionAssignmentCard } from '../institution/InstitutionAssignmentCard';

// 2da4e79：本轮重构前省级目录的原始布局金标。
const originalDirectoryStyles = `
.institution-grid {
  display: grid;
  grid-template-columns: repeat(auto-fill, minmax(220px, 1fr));
  gap: 8px;
}

/* 两个省级机构列表填满面板，四周沿用面板的 24px 内边距。 */
.app:has(> .content > .institution-list-view),
.app:has(> .content > .institution-list-view) > .content,
.institution-list-view {
  display: flex;
  flex-direction: column;
}

.app:has(> .content > .institution-list-view) > .content,
.institution-list-view,
.institution-list-view > .institution-grid {
  flex: 1;
}

.institution-list-view {
  padding: 0;
}

.institution-list-view > .institution-grid {
  grid-auto-rows: minmax(min-content, 1fr);
}

.institution-list-view .institution-card {
  display: flex;
  flex-direction: column;
  justify-content: center;
}

.institution-card {
  border: 1px solid var(--border);
  border-radius: 8px;
  padding: 8px 12px;
  background: var(--bg-card);
  cursor: pointer;
  transition: border-color 0.15s, box-shadow 0.15s;
}

.institution-card:hover {
  border-color: var(--primary);
  box-shadow: 0 1px 4px rgba(6, 182, 212, 0.2);
}

.institution-card-name {
  font-size: 14px;
  font-weight: 600;
  color: var(--text-primary);
}

.institution-card-meta {
  margin-top: 4px;
}

.institution-card-type {
  font-size: 12px;
  color: var(--text-muted);
}

`;
it('省级目录与原版完整样式一致，不继承其他卡片的居中和间距', () => {
 const actual=readFileSync(resolve(process.cwd(),'desktop/global.css'),'utf8');
 const start=actual.indexOf('.institution-grid {');
 expect(actual.slice(start,actual.indexOf('/* 机构详情页 */',start))).toBe(originalDirectoryStyles.slice(1));
 const capture=(css:string)=>{
  const style=document.createElement('style');style.textContent=css;document.head.append(style);
  const host=document.createElement('div');
  host.innerHTML='<div class="app"><main class="content"><div class="governance-section institution-list-view"><div class="institution-grid"><div class="institution-card"><div class="institution-card-name">省级机构</div><div class="institution-card-meta"><span class="institution-card-type">省储委会</span></div></div></div></div></main></div><div class="institution-row-grid">其他账户</div>';
  document.body.append(host);
  const read=(selector:string,properties:string[])=>{const value=getComputedStyle(host.querySelector(selector)!);return properties.map(property=>value.getPropertyValue(property));};
  try{return {
   view:read('.institution-list-view',['display','flex-direction','flex-grow','padding']),
   grid:read('.institution-grid',['display','grid-template-columns','grid-auto-rows','gap','flex-grow']),
   card:read('.institution-card',['display','flex-direction','align-items','justify-content','row-gap','column-gap','padding','border-radius']),
   name:read('.institution-card-name',['font-size','font-weight']),
   meta:read('.institution-card-meta',['margin-top']),
   type:read('.institution-card-type',['font-size']),
   other:read('.institution-row-grid',['grid-template-columns']),
  };}finally{host.remove();style.remove();}
 };
 const actualLayout=capture(actual),originalLayout=capture(originalDirectoryStyles);
 expect(actualLayout).toEqual({...originalLayout,other:['1fr 1fr']});
});

// 卡片以一个人员岗位任职为单位，姓名、序号、真实绑定、席位及操作均保留。
it.each([{familyName:'合成',givenName:'人员',name:'合成人员'},{familyName:'',givenName:'',name:'姓名未填写'},{familyName:'  ',givenName:'\t',name:'姓名未填写'}])('岗位任职卡片显示$name并保留操作',({familyName,givenName,name})=>{
 const click=vi.fn();const {container,unmount}=render(<InstitutionAssignmentCard index={3} balanceFen="12345" action={<button onClick={click}>原操作</button>} assignment={{personCid:'fixture-person',accountId:accountId('01'),bindingRevision:'1',familyName,givenName,roleCode:'MEMBER',roleName:'委员',seats:19,termRequired:true,termStart:1,termEnd:2,assignmentSource:0,assignmentSourceLabel:'创世',assignmentSourceRef:'0x0100',permissions:[],active:true,effective:true}}/>);
 expect(container.textContent).toContain(name);expect(container.textContent).toContain('3. 委员');expect(container.textContent).toContain('fixture-person');expect(container.textContent).toContain('席位容量：19');expect(container.textContent).toContain('1970-01-02 至 1970-01-03');expect(container.textContent).toContain(accountIdToSs58(accountId('01')));expect(Array.from(container.querySelectorAll('p')).slice(-3).map(node=>node.textContent)).toEqual(['账户余额：123.45 元','身份CID号：fixture-person',`绑定账户：${accountIdToSs58(accountId('01'))}`]);fireEvent.click(within(container).getByRole('button',{name:'原操作'}));expect(click).toHaveBeenCalledOnce();unmount();
});

vi.mock('../desktop/signing', () => ({
  CitizenSignatureModal:({open,title,onScan,onScanError,onCancel,countdownSeconds,status='ready',statusTitle,error}:{open:boolean;title:string;onScan:(s:string)=>void;onScanError:(s:string)=>void;onCancel:()=>void;countdownSeconds?:number;status?:string;statusTitle?:string;error?:string|null})=>{if(open&&status==='ready'){mocks.activationScan=onScan;mocks.activationScanError=onScanError;}return open?<section role="dialog" aria-label={title}><span>有效时间: {countdownSeconds} 秒</span>{status==='ready'?<button onClick={()=>onScan('模拟签名响应')}>扫描激活响应</button>:<p>{statusTitle}{error}</p>}<button onClick={onCancel}>关闭激活</button></section>:null;},
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
          cidNumber, orgType, cidFullName:'测试机构', roles:[], internalThreshold:2, jointVoteWeight:1,
          mainAccountId:accountId('01'), balanceFen:'100',
          nrcAccountId:orgType === 0 ? accountId('02') : null, nrcFeeFen:'200',
          fundAccountId:orgType === 0 ? accountId('03') : null, fundBalanceFen:'300',
          heAccountId:orgType === 0 ? heAccountId : null, heBalanceFen:null,
          cbAccountId:orgType === 1 ? accountId('02') : null, cbFeeFen:'200',
          feeAccountId:orgType === 2 ? accountId('02') : null, feeBalanceFen:'200',
          stakeAccountId:orgType === 2 ? accountId('04') : null, stakingBalanceFen:'400',
        };
      case 'get_role_access': return [];
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
          roles:[{roleCode:orgType===2?'DIRECTOR':'COMMITTEE_MEMBER',roleName:'委员',seats:19,active:true,termRequired:false,permissions:[{moduleTag:'multisig',actionCode:0,operation:0},{moduleTag:'multisig',actionCode:1,operation:0},{moduleTag:'gra-key',actionCode:1,operation:0},{moduleTag:'rt-upg',actionCode:0,operation:0},...(canProposeSweep?[{moduleTag:'multisig',actionCode:2,operation:0}]:[])],assignments:[{personCid:'fixture-person',accountId:signerAccountId,bindingRevision:'1',familyName:'合成',givenName:'人员',termStart:0,termEnd:0,effective:true,active:true,assignmentSource:0,assignmentSourceLabel:'创世',assignmentSourceRef:'0x00'}]}],
        };
      case 'get_role_access':return[{cidNumber:transferCid,personCid:'fixture-person',roleCode:orgType===2?'DIRECTOR':'COMMITTEE_MEMBER',accountId:signerAccountId,bindingRevision:'1',activatedAtMs:1000}];
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
      '机构账户发起转账交易的，必须由该机构有权岗位人员发起转账提案，按冻结岗位票据内部投票通过后执行。',
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
    expect(panel.getByRole('combobox', {name:'发起岗位人员'})).toHaveProperty('value', signerAccountId);
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

function activationRequest(seconds=300){return {requestId:'role-request',requestJson:JSON.stringify({e:Math.floor(Date.now()/1000)+seconds}),expectedPayloadHash:'hash',payloadHex:'0x00'};}
// 同一人兼任委员／董事和法代，页面仍只能激活指定的委员／董事岗位。
function assignmentState(orgType=0){
 const person={personCid:'same-person',accountId:signerAccountId,bindingRevision:'1',familyName:'合成',givenName:'人员',termStart:0,termEnd:0,assignmentSource:0,assignmentSourceLabel:'创世',assignmentSourceRef:'0x00',active:true,effective:true};
 return {cidNumber:'role-test',roles:[
  {roleCode:orgType===2?'DIRECTOR':'COMMITTEE_MEMBER',roleName:orgType===2?'董事':'委员',seats:19,active:true,termRequired:false,permissions:[],assignments:[{...person}]},
  {roleCode:'LR',roleName:'法定代表人',seats:1,active:true,termRequired:false,permissions:[],assignments:[{...person}]},
 ]};
}
function mockAssignmentCalls(implementation:(command:string,args:{cidNumber:string})=>unknown){
 mocks.invoke.mockImplementation(async(c,args)=>c==='get_account_balances'?{}:implementation(c,args));
}
it('同人多岗位的激活固定所选人员岗位，取消不消费证明',async()=>{
 mockAssignmentCalls(async c=>c==='get_institution_roles'?assignmentState():c==='get_role_access'?[]:c==='build_role_access_request'?activationRequest():null);
 const view=render(<AssignmentsPage orgType={0} cidNumber="role-test" accountRef={{cidNumber:'role-test'}} onBack={()=>{}}/>);
 fireEvent.click((await screen.findAllByRole('button',{name:'激活'}))[0]);await screen.findByRole('dialog',{name:'激活 · 委员'});
 expect(mocks.invoke).toHaveBeenCalledWith('build_role_access_request',expect.objectContaining({cidNumber:'role-test',personCid:'same-person',roleCode:'COMMITTEE_MEMBER',signerPublicKey:signerAccountId}));
 fireEvent.click(screen.getByRole('button',{name:'关闭激活'}));expect(mocks.invoke.mock.calls.some(([c])=>c==='verify_role_access')).toBe(false);view.unmount();
});
it('离页后迟到的激活请求不能重新打开签名或确认激活',async()=>{
 let finish!:(v:unknown)=>void;mockAssignmentCalls(async c=>c==='get_institution_roles'?assignmentState():c==='get_role_access'?[]:new Promise(resolve=>{finish=resolve;}));
 const view=render(<AssignmentsPage orgType={0} cidNumber="role-test" accountRef={{cidNumber:'role-test'}} onBack={()=>{}}/>);fireEvent.click((await screen.findAllByRole('button',{name:'激活'}))[0]);await waitFor(()=>expect(finish).toBeDefined());view.unmount();await act(async()=>finish({...activationRequest(),requestId:'late'}));expect(screen.queryByRole('dialog')).toBeNull();expect(mocks.invoke.mock.calls.some(([c])=>c==='verify_role_access')).toBe(false);
});
it('岗位读取失败显示错误并可重读，不当作空岗位',async()=>{
 let failed=true;mockAssignmentCalls(async c=>{if(c==='get_role_access')return[];if(failed)throw Error('合成链读失败');return assignmentState();});
 const view=render(<AssignmentsPage orgType={0} cidNumber="role-test" accountRef={{cidNumber:'role-test'}} onBack={()=>{}}/>);expect((await screen.findByRole('alert')).textContent).toContain('合成链读失败');expect(screen.queryByText('岗位空缺')).toBeNull();failed=false;fireEvent.click(screen.getByRole('button',{name:'重读'}));expect(await screen.findAllByRole('button',{name:'激活'})).toHaveLength(1);view.unmount();
});

it('切换机构后解除旧请求等待，迟到响应不能覆盖新机构',async()=>{
 let finish!:(v:unknown)=>void;
 mockAssignmentCalls(async(c,args)=>c==='get_institution_roles'?{...assignmentState(),cidNumber:args.cidNumber}:c==='get_role_access'?[]:new Promise(resolve=>{finish=resolve;}));
 const view=render(<AssignmentsPage orgType={0} cidNumber="role-test" accountRef={{cidNumber:'role-test'}} onBack={()=>{}}/>);
 try{
  fireEvent.click((await screen.findAllByRole('button',{name:'激活'}))[0]);
  await waitFor(()=>expect(finish).toBeDefined());
  view.rerender(<AssignmentsPage orgType={0} cidNumber="new-role-test" accountRef={{cidNumber:'new-role-test'}} onBack={()=>{}}/>);
  await waitFor(()=>expect((screen.getAllByRole('button',{name:'激活'})[0] as HTMLButtonElement).disabled).toBe(false));
  await act(async()=>finish({...activationRequest(),requestId:'late'}));
  expect(screen.queryByRole('dialog')).toBeNull();
 }finally{view.unmount();}
});


it('激活需验证成功才更新所选卡片；验证中防重复、不可关闭，重新进入恢复记录',async()=>{
 let finish!:(v:unknown)=>void;
 let saved:unknown[]=[];
 mockAssignmentCalls(async c=>c==='get_institution_roles'?assignmentState():c==='get_role_access'?saved:c==='build_role_access_request'?activationRequest():new Promise(resolve=>{finish=resolve;}));
 const props={orgType:0,cidNumber:'role-test',accountRef:{cidNumber:'role-test'},onBack:()=>{}};
 const view=render(<AssignmentsPage {...props}/>);
 fireEvent.click((await screen.findAllByRole('button',{name:'激活'}))[0]);
 const dialog=await screen.findByRole('dialog',{name:'激活 · 委员'});
 const scan=within(dialog).getByRole('button',{name:'扫描激活响应'});
 const lateScan=mocks.activationScan!,lateError=mocks.activationScanError!;
 act(()=>{fireEvent.click(scan);fireEvent.click(scan);});
 await screen.findByText('正在验证激活签名');
 expect(screen.queryByText('已激活')).toBeNull();
 expect(mocks.invoke.mock.calls.filter(([c])=>c==='verify_role_access')).toHaveLength(1);
 expect(mocks.invoke).toHaveBeenCalledWith('verify_role_access',{requestId:'role-request',signerPublicKey:signerAccountId,expectedPayloadHash:'hash',payloadHex:'0x00',responseJson:'模拟签名响应'});
 fireEvent.click(within(dialog).getByRole('button',{name:'关闭激活'}));
 expect(screen.getByRole('dialog')).toBe(dialog);
 saved=[{cidNumber:'role-test',personCid:'same-person',roleCode:'COMMITTEE_MEMBER',accountId:signerAccountId,bindingRevision:'1',activatedAtMs:1}];
 await act(async()=>finish(saved[0]));
 await screen.findByText('激活成功');
 act(()=>{lateScan('迟到的重复响应');lateError('迟到的扫码错误');});
 expect(screen.getByText('激活成功')).toBeTruthy();
 expect(mocks.invoke.mock.calls.filter(([c])=>c==='verify_role_access')).toHaveLength(1);
 expect(screen.getAllByText('已激活')).toHaveLength(1);
 fireEvent.click(within(dialog).getByRole('button',{name:'关闭激活'}));
 expect(screen.queryByRole('dialog')).toBeNull();
 view.unmount();
 const reopened=render(<AssignmentsPage {...props}/>);
 await screen.findByText('已激活');
 expect(screen.queryAllByRole('button',{name:'激活'})).toHaveLength(0);
 reopened.unmount();
});

it('激活验证失败保留错误、不激活，关闭后生成全新请求而不重扫已消费的证明',async()=>{
 mockAssignmentCalls(async c=>{if(c==='get_institution_roles')return assignmentState();if(c==='get_role_access')return[];if(c==='build_role_access_request')return activationRequest();throw Error('签名期间人员绑定或岗位任职已变化');});
 const view=render(<AssignmentsPage orgType={0} cidNumber="role-test" accountRef={{cidNumber:'role-test'}} onBack={()=>{}}/>);
 fireEvent.click((await screen.findAllByRole('button',{name:'激活'}))[0]);
 fireEvent.click(await screen.findByRole('button',{name:'扫描激活响应'}));
 await screen.findByText(/激活失败.*签名期间人员绑定或岗位任职已变化/);
 expect(screen.queryByText('已激活')).toBeNull();
 expect(screen.queryByRole('button',{name:'扫描激活响应'})).toBeNull();
 fireEvent.click(screen.getByRole('button',{name:'关闭激活'}));
 fireEvent.click(screen.getAllByRole('button',{name:'激活'})[0]);
 await screen.findByRole('button',{name:'扫描激活响应'});
 expect(mocks.invoke.mock.calls.filter(([c])=>c==='build_role_access_request')).toHaveLength(2);
 expect(mocks.invoke.mock.calls.filter(([c])=>c==='verify_role_access')).toHaveLength(1);
 view.unmount();
});

it('激活按签名请求实际到期倒计时，到期停止扫码，成功提示按原流程自动关闭',async()=>{
 mockAssignmentCalls(async c=>c==='get_institution_roles'?assignmentState():c==='get_role_access'?[]:c==='build_role_access_request'?activationRequest():{cidNumber:'role-test',personCid:'same-person',roleCode:'COMMITTEE_MEMBER',accountId:signerAccountId,bindingRevision:'1',activatedAtMs:1});
 const view=render(<AssignmentsPage orgType={0} cidNumber="role-test" accountRef={{cidNumber:'role-test'}} onBack={()=>{}}/>);
 try{
  await screen.findAllByRole('button',{name:'激活'});
  vi.useFakeTimers();vi.setSystemTime(new Date('2026-11-27T02:00:00Z'));
  await act(async()=>fireEvent.click(screen.getAllByRole('button',{name:'激活'})[0]));
  expect(screen.getByText('有效时间: 300 秒')).toBeTruthy();
  await act(async()=>vi.advanceTimersByTimeAsync(299000));
  expect(screen.getByText('有效时间: 1 秒')).toBeTruthy();
  await act(async()=>vi.advanceTimersByTimeAsync(1000));
  expect(screen.getByText('激活失败签名请求已过期，请重新激活')).toBeTruthy();
  expect(screen.queryByRole('button',{name:'扫描激活响应'})).toBeNull();
  expect(mocks.invoke.mock.calls.some(([c])=>c==='verify_role_access')).toBe(false);
  fireEvent.click(screen.getByRole('button',{name:'关闭激活'}));
  await act(async()=>fireEvent.click(screen.getAllByRole('button',{name:'激活'})[0]));
  await act(async()=>fireEvent.click(screen.getByRole('button',{name:'扫描激活响应'})));
  expect(screen.getByText('激活成功')).toBeTruthy();
  await act(async()=>vi.advanceTimersByTimeAsync(1999));
  expect(screen.getByRole('dialog')).toBeTruthy();
  await act(async()=>vi.advanceTimersByTimeAsync(1));
  expect(screen.queryByRole('dialog')).toBeNull();
  expect(screen.getAllByText('已激活')).toHaveLength(1);
 }finally{view.unmount();vi.useRealTimers();}
});

it('无效激活请求不展示二维码，禁用岗位或无当前绑定不得激活，旧绑定记录不显示已激活',async()=>{
 mockAssignmentCalls(async c=>c==='get_institution_roles'?assignmentState():c==='get_role_access'?[{cidNumber:'role-test',personCid:'same-person',roleCode:'COMMITTEE_MEMBER',accountId:signerAccountId,bindingRevision:'0'}]:{...activationRequest(),requestJson:'{}'});
 const props={orgType:0,cidNumber:'role-test',accountRef:{cidNumber:'role-test'},onBack:()=>{}};
 const view=render(<AssignmentsPage {...props}/>);
 fireEvent.click((await screen.findAllByRole('button',{name:'激活'}))[0]);
 expect(screen.queryByText('已激活')).toBeNull();
 expect((await screen.findByRole('alert')).textContent).toContain('激活签名请求无效或已过期');
 expect(screen.queryByRole('dialog')).toBeNull();view.unmount();
 const inactive=assignmentState();inactive.roles[0].active=false;
 mockAssignmentCalls(async c=>c==='get_institution_roles'?inactive:[]);
 const disabled=render(<AssignmentsPage {...props}/>);
 const button=(await screen.findAllByRole('button',{name:'激活'}))[0] as HTMLButtonElement;
 expect(button.disabled).toBe(true);const requests=mocks.invoke.mock.calls.filter(([c])=>c==='build_role_access_request').length;
 fireEvent.click(button);expect(mocks.invoke.mock.calls.filter(([c])=>c==='build_role_access_request')).toHaveLength(requests);disabled.unmount();
 const unbound=assignmentState();Object.assign(unbound.roles[0].assignments[0],{accountId:null,bindingRevision:null});
 mockAssignmentCalls(async c=>c==='get_institution_roles'?unbound:[]);
 const next=render(<AssignmentsPage {...props}/>);
 expect(((await screen.findAllByRole('button',{name:'激活'}))[0] as HTMLButtonElement).disabled).toBe(true);
 next.unmount();
});


function mockRosterInterface(orgType:number,state=assignmentState(orgType)){
 const cidNumber=orgType===0?'LN001-NRC0G-944805165-2026':'role-test';
 const item={cidNumber,cidShortName:'合成目录机构',orgTypeLabel:orgType===2?'省储行':'省储委会'};
 mocks.invoke.mockImplementation(async(c,args)=>{
  if(c==='get_governance_overview')return {nationalCouncils:[],provincialCouncils:[item],provincialBanks:[item]};
  if(c==='get_institution_detail')return {cidNumber:args.cidNumber,orgType,cidFullName:'合成机构',roles:state.roles,mainAccountId:signerAccountId,balanceFen:'0',internalThreshold:2,jointVoteWeight:1};
  if(c==='get_institution_roles')return {...state,cidNumber:args.cidNumber};
  if(c==='get_role_access')return [{cidNumber:args.cidNumber,personCid:'same-person',roleCode:'LR',accountId:state.roles[1]?.assignments[0]?.accountId,bindingRevision:'1',activatedAtMs:1}];
  if(c==='get_account_balances')return {[signerAccountId]:'12345'};
  if(c==='get_institution_proposal_page')return {items:[],hasMore:false};
  return null;
 });
 return cidNumber;
}

it.each([{orgType:0,Section:NrcSection,label:'委员'},{orgType:1,Section:PrcSection,label:'委员'},{orgType:2,Section:PrbSection,label:'董事'}])('三个tab类型$orgType从总容量进入指定岗位、隐藏法代，返回保持原导航',async({orgType,Section,label})=>{
 const state=assignmentState(orgType);state.roles[1].assignments[0].givenName='只限法代';state.roles[1].assignments[0].accountId=accountId('09');
 const cid=mockRosterInterface(orgType,state);
 const view=render(<Section/>);
 if(orgType!==0)fireEvent.click(await screen.findByText('合成目录机构'));
 const entry=await screen.findByRole('button',{name:`${label}（19席）`});
 expect(screen.queryByText(/已激活.*席/)).toBeNull();
 fireEvent.click(entry);
 await screen.findByRole('heading',{name:`${label}（19席）`});
 expect(await screen.findAllByRole('button',{name:'激活'})).toHaveLength(1);
 expect(screen.queryByText('法定代表人')).toBeNull();expect(screen.queryByText('合成只限法代')).toBeNull();
 expect(screen.queryByText('已激活')).toBeNull();
 expect(view.container.querySelectorAll('.institution-assignment-card')).toHaveLength(19);
 await screen.findByText('123.45 元');
 expect(mocks.invoke).toHaveBeenCalledWith('get_account_balances',{accountIds:[signerAccountId]});
 expect(mocks.invoke).toHaveBeenCalledWith('get_institution_roles',expect.objectContaining({cidNumber:cid}));
 fireEvent.click(screen.getByRole('button',{name:'← 返回机构详情'}));
 await screen.findByRole('button',{name:`${label}（19席）`});view.unmount();
});

it.each([0,1,2])('类型%s总席位与实际人数独立：空缺仍显示容量，不伪造已任职人数',async orgType=>{
 const state=assignmentState(orgType);state.roles[0].seats=37;state.roles[0].assignments=[];
 const cid=mockRosterInterface(orgType,state);const name=orgType===2?'董事':'委员';
 const view=render(<InstitutionDetailPage cidNumber={cid} onBack={()=>{}}/>);
 const entry=await screen.findByRole('button',{name:`${name}（37席）`});expect((entry as HTMLButtonElement).disabled).toBe(false);view.unmount();
 const page=render(<AssignmentsPage orgType={orgType} cidNumber={cid} accountRef={{cidNumber:cid}} onBack={()=>{}}/>);
 await screen.findByRole('heading',{name:`${name}（37席）`});expect(screen.queryByText('岗位空缺')).toBeNull();expect(screen.queryByText(/暂无数据/)).toBeNull();
 expect(page.container.querySelector('.admin-list-summary')).toBeNull();expect(page.container.querySelectorAll('.institution-assignment-card')).toHaveLength(37);for(const value of page.container.querySelectorAll('.institution-assignment-value'))expect(value.textContent).toBe('');
 expect(mocks.invoke.mock.calls.some(([c])=>c==='get_account_balances')).toBe(false);page.unmount();
});

it.each(['missing','zero','fraction'] as const)('岗位容量异常%s明确报错，入口及列表不把错误伪造为0席',async invalid=>{
 const state=assignmentState();if(invalid==='missing')state.roles.shift();else state.roles[0].seats=invalid==='zero'?0:1.5;
 const cid=mockRosterInterface(1,state);
 const view=render(<InstitutionDetailPage cidNumber={cid} onBack={()=>{}}/>);
 const entry=await screen.findByRole('button',{name:'委员（—席）'});expect((entry as HTMLButtonElement).disabled).toBe(true);
 expect((await screen.findByRole('alert')).textContent).toContain('委员席位数据不可用');view.unmount();
 const page=render(<AssignmentsPage orgType={1} cidNumber={cid} accountRef={{cidNumber:cid}} onBack={()=>{}}/>);
 expect((await screen.findByRole('alert')).textContent).toContain('委员席位数据不可用');
 expect(screen.queryByText('岗位空缺')).toBeNull();expect(screen.queryByRole('button',{name:'激活'})).toBeNull();page.unmount();
});

it('旧卡片保留方形序号、右侧操作、来源任期同排、余额与账户，字段来自现有绑定',async()=>{
 const state=assignmentState();Object.assign(state.roles[0],{termRequired:true});Object.assign(state.roles[0].assignments[0],{termStart:1,termEnd:2});
 mockRosterInterface(0,state);const cid='role-test';
 const view=render(<AssignmentsPage orgType={0} cidNumber={cid} accountRef={{cidNumber:cid}} onBack={()=>{}}/>);
 await screen.findByText('123.45 元');
 const card=view.container.querySelector('.institution-admin-list-grid > .metric-card.institution-assignment-card')!;
 expect(card.querySelector('.institution-assignment-top > .admin-card-index')?.textContent).toBe('1');
 expect(card.querySelector('.institution-assignment-person')?.textContent).toBe('委员合成人员');
 expect(card.querySelector('.institution-assignment-top-action > .activate-button')?.textContent).toBe('激活');
 expect(Array.from(card.querySelectorAll('.institution-assignment-account > .institution-assignment-list-row .institution-assignment-label')).map(x=>x.textContent)).toEqual(['来源:','任期:']);
 expect(card.textContent).toContain('来源:创世');expect(card.textContent).not.toContain('依据');expect(card.textContent).not.toContain('0x00');expect(card.textContent).toContain('1970/01/02-1970/01/03');
 expect(card.textContent).toContain('账户余额:123.45 元');expect(card.textContent).toContain(`绑定账户:${accountIdToSs58(signerAccountId)}`);
 expect(Array.from(card.querySelectorAll('.institution-assignment-account > .institution-assignment-field .institution-assignment-label')).map(node=>node.textContent)).toEqual(['账户余额:','身份CID号:','绑定账户:']);
 expect(card.textContent).toContain('身份CID号:same-person');expect(card.textContent).not.toContain('委员账户');
 expect(card.textContent).not.toContain('席位容量');expect(card.textContent).not.toContain('法定代表人');view.unmount();
});

it('余额读取失败显示未知且可重读，不阻塞激活；真实零余额显示0.00',async()=>{
 let failed=true;
 mocks.invoke.mockImplementation(async(c)=>{
  if(c==='get_institution_roles')return assignmentState();if(c==='get_role_access')return[];
  if(c==='get_account_balances'){if(failed)throw Error('合成余额失败');return {[signerAccountId]:'0'};}
  return activationRequest();
 });
 const view=render(<AssignmentsPage orgType={0} cidNumber="role-test" accountRef={{cidNumber:'role-test'}} onBack={()=>{}}/>);
 await screen.findByText(/余额读取失败/);
 const balance=view.container.querySelector('.institution-assignment-account > .institution-assignment-field .institution-assignment-value');
 expect(balance?.textContent).toBe('—');expect((screen.getByRole('button',{name:'激活'}) as HTMLButtonElement).disabled).toBe(false);
 expect(screen.queryByText('0.00 元')).toBeNull();
 failed=false;fireEvent.click(screen.getByRole('button',{name:'重读余额'}));await screen.findByText('0.00 元');
 expect(screen.queryByText(/余额读取失败/)).toBeNull();
 fireEvent.click(screen.getByRole('button',{name:'激活'}));await screen.findByRole('dialog',{name:'激活 · 委员'});view.unmount();
});

it('切换机构后旧余额的迟到结果不能覆盖新页面',async()=>{
 let finish!:(value:unknown)=>void;let read=0;
 mocks.invoke.mockImplementation(async(c,args)=>{
  if(c==='get_institution_roles')return {...assignmentState(),cidNumber:args.cidNumber};if(c==='get_role_access')return[];
  if(c==='get_account_balances'){if(++read===1)return new Promise(resolve=>{finish=resolve;});return {[signerAccountId]:'200'};}
 });
 const view=render(<AssignmentsPage orgType={0} cidNumber="role-test" accountRef={{cidNumber:'role-test'}} onBack={()=>{}}/>);
 await waitFor(()=>expect(finish).toBeDefined());
 view.rerender(<AssignmentsPage orgType={1} cidNumber="next" accountRef={{cidNumber:'next'}} onBack={()=>{}}/>);
 await screen.findByText('2.00 元');await act(async()=>finish({[signerAccountId]:'99999'}));
 expect(screen.queryByText('999.99 元')).toBeNull();expect(screen.getByText('2.00 元')).toBeTruthy();view.unmount();
});

it('旧卡片关键样式对照2da4e79金标，新增样式仅限三个储委会储行任职页',()=>{
 const css=readFileSync(resolve(process.cwd(),'institution/styles.css'),'utf8');
 const marker='/* 三个储委会／储行任职页：按2da4e79恢复原卡片布局，仅作用本页。 */';
 const scoped=css.slice(css.indexOf(marker)+marker.length).split('/* 投票详情恢复原任职卡片；')[0];
 const style=document.createElement('style');style.textContent=scoped;document.head.append(style);
 try{
  const rules=Array.from(style.sheet!.cssRules) as CSSStyleRule[];
  expect(rules.every(rule=>rule.selectorText.split(',').every(selector=>selector.trim().startsWith('.institution-roster ')))).toBe(true);
  // 以下属性取自重构前历史版本，锁定布局、序号、操作及字段行，避免重画卡片。
  const golden:Record<string,Record<string,string>>={
   '.institution-roster .institution-admin-list-grid':{display:'grid','grid-template-columns':'repeat(3, minmax(300px, 1fr))',gap:'8px','overflow-x':'auto'},
   '.institution-roster .institution-assignment-card .admin-card-index':{display:'inline-flex','min-width':'28px',height:'28px','border-radius':'8px','font-size':'12px','font-weight':'700'},
   '.institution-roster .institution-assignment-top-action':{display:'inline-flex','min-height':'28px','justify-content':'flex-end','margin-left':'auto'},
   '.institution-roster .institution-assignment-list-row':{display:'grid','grid-template-columns':'repeat(2, minmax(0, 1fr))',gap:'12px'},
   '.institution-roster .institution-assignment-field':{display:'grid','grid-template-columns':'max-content minmax(0, 1fr)',gap:'6px'},
   '.institution-roster .activated-tag':{color:'#4ade80','font-size':'11px','border-radius':'4px'},
   '.institution-roster .activate-button':{color:'#60a5fa','font-size':'11px',padding:'2px 8px'},
  };
  for(const [selector,properties] of Object.entries(golden)){
   const rule=rules.find(rule=>rule.selectorText===selector);expect(rule,selector).toBeDefined();
   for(const [property,value] of Object.entries(properties))expect(rule!.style.getPropertyValue(property),`${selector} ${property}`).toBe(value);
  }
 }finally{style.remove();}
});


it.each([{orgType:1,Section:PrcSection},{orgType:2,Section:PrbSection}])('省级类型$orgType没有任职仍保留原机构及席位卡片，进入后不显示空缺提示或任职数据',async({orgType,Section})=>{
 const state=assignmentState(orgType);state.roles[0].assignments=[];mockRosterInterface(orgType,state);
 const view=render(<Section/>);const item=await screen.findByText('合成目录机构');
 expect(item.closest('.institution-card')).toBeTruthy();fireEvent.click(item);
 const name=orgType===2?'董事':'委员';fireEvent.click(await screen.findByRole('button',{name:`${name}（19席）`}));
 await screen.findByRole('heading',{name:`${name}（19席）`});
 expect(view.container.querySelectorAll('.institution-assignment-card')).toHaveLength(19);expect(view.container.querySelector('.admin-list-summary')).toBeNull();for(const value of view.container.querySelectorAll('.institution-assignment-value'))expect(value.textContent).toBe('');
 expect(screen.queryByText('岗位空缺')).toBeNull();expect(screen.queryByText(/暂无数据/)).toBeNull();
 fireEvent.click(screen.getByRole('button',{name:'← 返回机构详情'}));await screen.findByRole('button',{name:`${name}（19席）`});
 fireEvent.click(screen.getByRole('button',{name:'← 返回机构列表'}));expect((await screen.findByText('合成目录机构')).closest('.institution-card')).toBeTruthy();view.unmount();
});


it.each([0,1,2])('类型%s卡片在余额后显示任职公民身份CID号，并统一显示绑定账户',async orgType=>{
 const citizenCid='GZ001-CTZN0-000000001-2026'; // 合成公民CID，区别于机构CID。
 const state=assignmentState(orgType);state.roles[0].assignments[0].personCid=citizenCid;mockRosterInterface(orgType,state);
 const page=render(<AssignmentsPage orgType={orgType} cidNumber="role-test" accountRef={{cidNumber:'role-test'}} onBack={()=>{}}/>);
 await screen.findByText('123.45 元');
 const fields=Array.from(page.container.querySelector('.institution-assignment-card')!.querySelectorAll('.institution-assignment-account > .institution-assignment-field'));
 expect(fields.map(node=>node.querySelector('.institution-assignment-label')?.textContent)).toEqual(['账户余额:','身份CID号:','绑定账户:']);
 expect(fields[1].querySelector('[data-identity-cid]')?.getAttribute('data-identity-cid')).toBe(citizenCid);
 expect(fields[1].textContent).toContain('GZ001-CTZN0-000000001');expect(fields[2].textContent).toContain(accountIdToSs58(signerAccountId));
 expect(page.container.textContent).not.toContain('委员账户');expect(page.container.textContent).not.toContain('董事账户');page.unmount();
});

it('共用岗位卡片无余额和绑定时仍显示正确的个人CID，保持字段顺序',()=>{
 const person=assignmentState().roles[0].assignments[0];
 const page=render(<InstitutionAssignmentCard assignment={{...person,accountId:null,bindingRevision:null,roleCode:'COMMITTEE_MEMBER',roleName:'委员',termRequired:false,seats:19,permissions:[]}}/>);
 expect(Array.from(page.container.querySelectorAll('p')).slice(-3).map(node=>node.textContent)).toEqual(['账户余额：—','身份CID号：same-person','绑定账户：无有效绑定']);page.unmount();
});


it.each([1,2])('省级类型%s空席卡片保留身份CID号及绑定账户结构，有任职只填本人的CID',async orgType=>{
 const state=assignmentState(orgType);state.roles[0].seats=9;state.roles[0].assignments=[];mockRosterInterface(orgType,state);
 const props={orgType,cidNumber:'role-test',accountRef:{cidNumber:'role-test'},onBack:()=>{}};
 const view=render(<AssignmentsPage {...props}/>);await screen.findByRole('heading',{name:`${orgType===2?'董事':'委员'}（9席）`});
 const cards=view.container.querySelectorAll('.institution-assignment-card');expect(cards).toHaveLength(9);
 for(const card of cards){expect(Array.from(card.querySelectorAll('.institution-assignment-account > .institution-assignment-field .institution-assignment-label')).map(node=>node.textContent)).toEqual(['账户余额:','身份CID号:','绑定账户:']);expect(card.querySelectorAll('[data-identity-cid]')).toHaveLength(0);expect(card.querySelector('button')).toBeNull();}
 expect(screen.queryByText('岗位空缺')).toBeNull();expect(screen.queryByText(/暂无数据/)).toBeNull();view.unmount();
});

// 验证机构提案分页、投票状态和签名提交IPC及失败不重放。

import {beforeEach, expect, it, vi} from 'vitest';
import { fireEvent, render, screen, within, waitFor, act } from '@testing-library/react';
import { accountIdToSs58 } from '../client/ss58';
import {proposalApi} from '../proposal/api';
import { VoteSigningFlow } from '../proposal/VoteSigningFlow';
import { ProposalDetailPage } from '../proposal/ProposalDetailPage';
const mocks=vi.hoisted(()=>({invoke:vi.fn(),response:'',scan:null as null | ((s:string)=>void)}));
vi.mock('../client/tauri',()=>({invoke:mocks.invoke,sanitizeError:String}));
vi.mock('../desktop/signing', () => ({ CitizenSignaturePanel: ({ onScan }: { onScan: (s:string)=>void }) => {
  mocks.scan = onScan;
  return <button onClick={() => onScan(mocks.response)}>扫描原响应</button>;
} }));
const api=proposalApi;
beforeEach(() => {
  mocks.invoke.mockReset(); window.localStorage.clear(); mocks.scan = null;
});
it("getInstitutionProposalPage保留唯一IPC合同且返回真实回执", async () => { const receipt = { receipt: '合成回执' }; mocks.invoke.mockResolvedValueOnce(receipt); await expect(api.getInstitutionProposalPage("44","7",20)).resolves.toBe(receipt); expect(mocks.invoke).toHaveBeenCalledExactlyOnceWith("get_institution_proposal_page", {"cidNumber":"44","startId":"7","count":20}); });
it("buildJointVoteRequest保留唯一IPC合同且返回真实回执", async () => { const receipt = { receipt: '合成回执' }; mocks.invoke.mockResolvedValueOnce(receipt); await expect(api.buildJointVoteRequest("7","public","44","role",true)).resolves.toBe(receipt); expect(mocks.invoke).toHaveBeenCalledExactlyOnceWith("build_joint_vote_request", {"proposalId":"7","signerPublicKey":"public","cidNumber":"44","voterRoleCode":"role","approve":true}); });
it("checkVoteStatus保留唯一IPC合同且返回真实回执", async () => { const receipt = { receipt: '合成回执' }; mocks.invoke.mockResolvedValueOnce(receipt); await expect(api.checkVoteStatus("7","public")).resolves.toBe(receipt); expect(mocks.invoke).toHaveBeenCalledExactlyOnceWith("check_vote_status", {"proposalId":"7","signerPublicKey":"public","cidNumber":null,"voterRoleCode":null}); });
it("submitVote保留唯一IPC合同且返回真实回执", async () => { const receipt = { receipt: '合成回执' }; mocks.invoke.mockResolvedValueOnce(receipt); await expect(api.submitVote("request","public","digest","0x0102",3,9,"response")).resolves.toBe(receipt); expect(mocks.invoke).toHaveBeenCalledExactlyOnceWith("submit_vote", {"requestId":"request","expectedPublicKey":"public","expectedPayloadHash":"digest","callDataHex":"0x0102","signNonce":3,"signBlockNumber":9,"responseJson":"response","frozenTicket":null}); });
it('IPC失败保留拒绝状态且不自动重放', async()=>{const failure=new Error('合成失败');mocks.invoke.mockRejectedValueOnce(failure);await expect(api.getInstitutionProposalPage("44","7",20)).rejects.toBe(failure);expect(mocks.invoke).toHaveBeenCalledOnce();});

it.each([0, 1])('提案类型 %i 只允许链上确认未投票的人员岗位发起投票', async (kind) => {
  const cidNumber = 'TEST-CID';
  const roleCode = 'COMMITTEE_MEMBER';
  const accounts = ['01', '02', '03', '04'].map((byte) => '0x' + byte.repeat(32));
  const [approved, rejected, unvoted, failed] = accounts;
  const votes = new Map<string, boolean | null>([[approved, true], [rejected, false], [unvoted, null]]);
  mocks.invoke.mockImplementation(async (command: string, args: { signerPublicKey: string }) => {
    switch (command) {
      case 'get_proposal_detail':
        return {
          meta: { proposalId: '7', kind, stage: kind, status: 0, subjectCidNumbers: [cidNumber] },
          voterRoleSubjects: [{ cidNumber, roleCode }],
          frozenVoteView: { genesisHash: '0xchain', blockHash: '0xfinal', blockNumber: '5', endBlock: '100', kind, stage: kind, status: 0, actorCidNumber: cidNumber, scopeError: null,
            roles: [{ cidNumber, roleCode, error: null, voters: accounts.map(accountId => ({ personCid:'person-'+accountId,accountId, bindingRevision: '1', vote: votes.get(accountId) ?? null, voteError: accountId === failed ? '合成RPC失败' : null })) }] },
        };
      case 'get_proposal_display': return null;
      case 'get_institution_detail': return { roles: [] };
      case 'check_vote_status': {
        const vote = votes.get(args.signerPublicKey);
        if (vote === undefined) throw new Error('合成 RPC 失败');
        return {
          internalVote: kind === 0 ? vote : null,
          jointVote: kind === 1 ? vote : null,
        };
      }
      default: throw new Error(`意外调用 ${command}`);
    }
  });

  render(<ProposalDetailPage proposalId={"7"} cidNumber={cidNumber}
    roleSigners={accounts.map((accountId) => ({personCid:'person-'+accountId,roleCode,bindingRevision:'1',permissions:[], accountId, roleName: '' }))}
    onBack={() => {}} />);
  await screen.findByRole('heading', { name: '原提案冻结资格（4 个当前绑定 / 4 张岗位票）' });
  expect(screen.getAllByText('姓名及岗位名称：历史资料未记录')).toHaveLength(4);
  expect(mocks.invoke.mock.calls.some(([command]) => command === 'get_institution_detail' || command === 'get_role_access')).toBe(false);

  const card = (accountId: string) =>
    within(screen.getByText(accountIdToSs58(accountId)).closest<HTMLElement>('.institution-assignment-card')!);
  expect(card(approved).getByText('赞成').classList.contains('vote-yes-tag')).toBe(true);
  expect(card(rejected).getByText('反对').classList.contains('vote-no-tag')).toBe(true);
  expect(card(failed).getByText(/投票状态未知/)).toBeTruthy();
  expect(card(unvoted).getByRole('button', { name: '投票' })).toBeTruthy();
  expect(screen.getAllByRole('button', { name: '投票' })).toHaveLength(1);
});

// 四类业务详情共用元数据标题，顶部只保留返回与原状态。
it.each(['transferDetail', 'safetyFundDetail', 'sweepDetail', 'runtimeUpgradeDetail'])(
  '%s 将提案信息放入详情卡片且保留返回与状态', async (detailKey) => {
    const accountId = '0x' + '01'.repeat(32);
    const info = {
      meta: { proposalId: '7', kind: 0, stage: 0, status: 1, subjectCidNumbers: [] },
      cidFullName: '测试机构全称', voterRoleSubjects: [],
      transferDetail: null, safetyFundDetail: null, sweepDetail: null, runtimeUpgradeDetail: null,
      [detailKey]: {
        fundingAccountId: accountId, institutionAccountId: accountId,
        beneficiaryAccountId: accountId, proposerAccountId: accountId,
        amountFen: '100', remark: '测试备注', reason: '测试原因', codeHashHex: 'abcd',
      },
    };
    mocks.invoke.mockImplementation(async (command: string) => {
      if (command === 'get_proposal_detail') return info;
      if (command === 'get_proposal_display') return null;
      throw new Error(`意外调用 ${command}`);
    });
    const onBack = vi.fn();
    render(<ProposalDetailPage proposalId="7" roleSigners={[]} onBack={onBack} />);
    const title = await screen.findByRole('heading', { name: /内部投票/ });
    const heading = title.closest<HTMLElement>('.proposal-detail-heading')!;
    expect(within(heading).getByText('测试机构全称')).toBeTruthy();
    expect(heading.parentElement!.classList.contains(
      detailKey === 'runtimeUpgradeDetail' ? 'institution-info-section' : 'proposal-transfer-card-heading',
    )).toBe(true);
    expect(title.querySelector('.proposal-detail-number')!.textContent).toBeTruthy();
    expect(screen.getAllByText('测试机构全称')).toHaveLength(1);
    expect(screen.queryByRole('heading', { name: /详情$/ })).toBeNull();
    const back = screen.getByRole('button', { name: '← 返回' });
    const toolbar = back.closest<HTMLElement>('.proposal-detail-toolbar')!;
    expect(within(toolbar).getByText('已通过').getAttribute('data-status')).toBe('1');
    expect(toolbar.querySelector('.proposal-detail-heading')).toBeNull();
    if (detailKey !== 'runtimeUpgradeDetail') {
      expect(screen.getByText('转账金额')).toBeTruthy();
      expect(screen.getByText('转出账户')).toBeTruthy();
    } else {
      expect(screen.getByText('测试原因')).toBeTruthy();
    }
    fireEvent.click(back);
    expect(onBack).toHaveBeenCalledOnce();
  },
);

const oldWallet = '0x' + '11'.repeat(32);
const frozenBinding = { genesisHash: 'chain-A', proposalId: '7', cidNumber: 'original-CID', roleCode: 'OLD_ROLE', personCid:'fixture-person',accountId: oldWallet, bindingRevision: '9007199254740993' };
const fixtureB64 = (value: number, length: number) => btoa(String.fromCharCode(...Array(length).fill(value))).replace(/\+/g, '-').replace(/\//g, '_').replace(/=+$/, '');
const oldWalletQr = fixtureB64(0x11,32);
const originalRequest = { requestId: 'original-request', requestJson: JSON.stringify({p:'QR_V1',k:1,i:'original-request',e:123,b:{a:20,g:1,u:oldWalletQr,d:'AQ'}}), callDataHex: '1400original-call', expectedPayloadHash: 'original-hash', signNonce: 8, signBlockNumber: 0 };
// PQ公钥与账户ID刻意不同：冻结资格始终取32字节账户ID。
const originalResponse = JSON.stringify({p:'QR_V1',k:2,i:'original-request',e:123,b:{u:oldWalletQr,s:fixtureB64(0x55,3309),q:fixtureB64(0x66,1952)}});

function renderFrozenFlow(onSuccess=vi.fn(),onClose=vi.fn()) {
  return { ...render(<VoteSigningFlow proposalId="7" proposalKind={0} cidNumber={frozenBinding.cidNumber}
    roleSigners={[{...frozenBinding,roleName:'原岗位',permissions:[]}]} frozenTicket={frozenBinding} onClose={onClose} onSuccess={onSuccess} />), onSuccess, onClose };
}
it('响应丢失保留原人员绑定版本、nonce、调用及签名，恢复不重新准备且不伪造hash', async () => {
  let attempts=0;
  mocks.invoke.mockImplementation(async (command:string) => {
    if(command==='build_vote_request') return originalRequest;
    if(command==='submit_vote') { if(++attempts===1) throw new Error('广播响应丢失'); return {txHash:null,observed:true}; }
    throw new Error(command);
  });
  const {onSuccess}=renderFrozenFlow();
  fireEvent.click(screen.getByRole('button',{name:'反对'}));
  fireEvent.click(screen.getByRole('button',{name:'生成签名请求'}));
  mocks.response=originalResponse;
  fireEvent.click(await screen.findByRole('button',{name:'扫描原响应'}));
  fireEvent.click(await screen.findByRole('button',{name:'恢复原会话'}));
  await waitFor(()=>expect(onSuccess).toHaveBeenCalledWith(null,oldWallet,'OLD_ROLE',true));
  const submits=mocks.invoke.mock.calls.filter(([c])=>c==='submit_vote');
  expect(submits).toHaveLength(2); expect(submits[1]).toEqual(submits[0]);
  expect(submits[0][1]).toMatchObject({expectedPublicKey:oldWallet,signNonce:8,callDataHex:originalRequest.callDataHex,responseJson:originalResponse,frozenTicket:frozenBinding});
  expect(mocks.invoke.mock.calls.filter(([c])=>c==='build_vote_request')).toHaveLength(1);
  expect(window.localStorage.length).toBe(0);
});
it('新钱包响应不能替原冻结钱包投票', async () => {
  mocks.invoke.mockResolvedValue(originalRequest);renderFrozenFlow();
  fireEvent.click(screen.getByRole('button',{name:'赞成'}));fireEvent.click(screen.getByRole('button',{name:'生成签名请求'}));
  mocks.response=originalResponse.replace(oldWalletQr,fixtureB64(0x33,32));
  fireEvent.click(await screen.findByRole('button',{name:'扫描原响应'}));
  await screen.findByText(/响应不属于原钱包或原签名请求/);
  expect(mocks.invoke.mock.calls.some(([c])=>c==='submit_vote')).toBe(false);
  expect(window.localStorage.length).toBe(0);
});
it('关闭后迟到的扫码响应不得提交', async () => {
  mocks.invoke.mockResolvedValue(originalRequest);const view=renderFrozenFlow();
  fireEvent.click(screen.getByRole('button',{name:'赞成'}));fireEvent.click(screen.getByRole('button',{name:'生成签名请求'}));
  await screen.findByRole('button',{name:'扫描原响应'});const lateScan=mocks.scan!;
  fireEvent.click(screen.getByRole('button',{name:'✕'}));view.unmount();
  await act(async()=>lateScan(originalResponse));
  expect(mocks.invoke.mock.calls.some(([c])=>c==='submit_vote')).toBe(false);
});
it('已广播回执保持待确认，重新打开只能恢复原会话', async () => {
  mocks.invoke.mockImplementation(async (c:string)=> c==='build_vote_request'?originalRequest:{txHash:'0xoriginal',observed:false});
  const first=renderFrozenFlow();fireEvent.click(screen.getByRole('button',{name:'赞成'}));fireEvent.click(screen.getByRole('button',{name:'生成签名请求'}));mocks.response=originalResponse;
  fireEvent.click(await screen.findByRole('button',{name:'扫描原响应'}));await screen.findByText('已广播，等待链上票据确认');
  expect(window.localStorage.length).toBe(1);first.unmount();renderFrozenFlow();
  await screen.findByRole('button',{name:'恢复原会话'});expect(screen.queryByRole('button',{name:'生成签名请求'})).toBeNull();
});

it('同人跨岗位保留两张冻结票，前端外传钱包不能改写冻结资格', async () => {
  const cid='original-CID';
  mocks.invoke.mockImplementation(async (c:string) => {
    if(c==='get_proposal_display') return null;
    if(c==='get_proposal_detail') return {meta:{proposalId:'7',kind:0,stage:0,status:0,actorCidNumber:cid,subjectCidNumbers:[cid]},voterRoleSubjects:[],frozenVoteView:{genesisHash:'A',blockHash:'final',blockNumber:'1',endBlock:'100',kind:0,stage:0,status:0,scopeError:null,actorCidNumber:cid,roles:[{cidNumber:cid,roleCode:'ROLE_1',error:null,voters:[{personCid:'fixture-person',accountId:oldWallet,bindingRevision:'1',vote:false,voteError:null}]},{cidNumber:cid,roleCode:'ROLE_2',voters:[{personCid:'fixture-person',accountId:oldWallet,bindingRevision:null,vote:null,voteError:null}],error:'人员当前绑定缺失'}]}};
    throw new Error('不应读取现任目录');
  });
  render(<ProposalDetailPage proposalId="7" cidNumber={cid} roleSigners={[{personCid:'new-person',bindingRevision:'1',roleCode:'ROLE_1',permissions:[],accountId:'0x'+'33'.repeat(32),roleName:'新任'}]} onBack={()=>{}} />);
  await screen.findByRole('heading',{name:'原提案冻结资格（1 个当前绑定 / 2 张岗位票）'});
  expect(screen.getAllByText(accountIdToSs58(oldWallet))).toHaveLength(2);
  expect(screen.queryByText(accountIdToSs58('0x'+'33'.repeat(32)))).toBeNull();
  expect(screen.queryByRole('button',{name:'投票'})).toBeNull();
});
it('切换提案后丢弃旧提案迟到结果', async () => {
  let resolveOld!: (v:unknown)=>void;
  mocks.invoke.mockImplementation(async(c:string,a:{proposalId:string})=>{
    if(c==='get_proposal_display') return null;
    if(a.proposalId==='7') return new Promise(resolve=>{resolveOld=resolve;});
    return {meta:{proposalId:'8',kind:0,stage:0,status:1,subjectCidNumbers:[]},cidFullName:'新提案机构',voterRoleSubjects:[],frozenVoteView:null};
  });
  const view=render(<ProposalDetailPage proposalId="7" roleSigners={[]} onBack={()=>{}} />);
  view.rerender(<ProposalDetailPage proposalId="8" roleSigners={[]} onBack={()=>{}} />);
  await screen.findByText('新提案机构');
  await act(async()=>resolveOld({meta:{proposalId:'7',kind:0,stage:0,status:0,subjectCidNumbers:[]},cidFullName:'旧提案迟到',voterRoleSubjects:[]}));
  expect(screen.queryByText('旧提案迟到')).toBeNull();
});

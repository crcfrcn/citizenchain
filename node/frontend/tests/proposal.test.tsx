import {beforeEach, expect, it, vi} from 'vitest';
import { fireEvent, render, screen, within } from '@testing-library/react';
import { accountIdToSs58 } from '../client/ss58';
import {proposalApi} from '../proposal/api';
import { ProposalDetailPage } from '../proposal/ProposalDetailPage';
const mocks=vi.hoisted(()=>({invoke:vi.fn()}));
vi.mock('../client/tauri',()=>({invoke:mocks.invoke,sanitizeError:String}));
const api=proposalApi;
beforeEach(() => {
  mocks.invoke.mockReset();
});
it("getInstitutionProposalPage保留唯一IPC合同且返回真实回执", async () => { const receipt = { receipt: '合成回执' }; mocks.invoke.mockResolvedValueOnce(receipt); await expect(api.getInstitutionProposalPage("44","7",20)).resolves.toBe(receipt); expect(mocks.invoke).toHaveBeenCalledExactlyOnceWith("get_institution_proposal_page", {"cidNumber":"44","startId":"7","count":20}); });
it("buildJointVoteRequest保留唯一IPC合同且返回真实回执", async () => { const receipt = { receipt: '合成回执' }; mocks.invoke.mockResolvedValueOnce(receipt); await expect(api.buildJointVoteRequest("7","public","44","role",true)).resolves.toBe(receipt); expect(mocks.invoke).toHaveBeenCalledExactlyOnceWith("build_joint_vote_request", {"proposalId":"7","signerPublicKey":"public","cidNumber":"44","voterRoleCode":"role","approve":true}); });
it("checkVoteStatus保留唯一IPC合同且返回真实回执", async () => { const receipt = { receipt: '合成回执' }; mocks.invoke.mockResolvedValueOnce(receipt); await expect(api.checkVoteStatus("7","public")).resolves.toBe(receipt); expect(mocks.invoke).toHaveBeenCalledExactlyOnceWith("check_vote_status", {"proposalId":"7","signerPublicKey":"public","cidNumber":null,"voterRoleCode":null}); });
it("submitVote保留唯一IPC合同且返回真实回执", async () => { const receipt = { receipt: '合成回执' }; mocks.invoke.mockResolvedValueOnce(receipt); await expect(api.submitVote("request","public","digest","0x0102",3,9,"response")).resolves.toBe(receipt); expect(mocks.invoke).toHaveBeenCalledExactlyOnceWith("submit_vote", {"requestId":"request","expectedPublicKey":"public","expectedPayloadHash":"digest","callDataHex":"0x0102","signNonce":3,"signBlockNumber":9,"responseJson":"response"}); });
it('IPC失败保留拒绝状态且不自动重放', async()=>{const failure=new Error('合成失败');mocks.invoke.mockRejectedValueOnce(failure);await expect(api.getInstitutionProposalPage("44","7",20)).rejects.toBe(failure);expect(mocks.invoke).toHaveBeenCalledOnce();});

it.each([0, 1])('提案类型 %i 只允许链上确认未投票的管理员岗位发起投票', async (kind) => {
  const cidNumber = 'TEST-CID';
  const roleCode = 'COMMITTEE_MEMBER';
  const accounts = ['01', '02', '03', '04'].map((byte) => '0x' + byte.repeat(32));
  const [approved, rejected, unvoted, failed] = accounts;
  const votes = new Map<string, boolean | null>([[approved, true], [rejected, false], [unvoted, null]]);
  const admins = accounts.map((accountId) => ({
    accountId, familyName: '', givenName: '',
    assignments: [{ roleCode, roleName: '委员', termRequired: false }],
  }));
  mocks.invoke.mockImplementation(async (command: string, args: { signerPublicKey: string }) => {
    switch (command) {
      case 'get_proposal_detail':
        return {
          meta: { proposalId: '7', kind, stage: kind, status: 0, subjectCidNumbers: [cidNumber] },
          voterRoleSubjects: [{ cidNumber, roleCode }],
        };
      case 'get_proposal_display': return null;
      case 'get_institution_detail': return { admins };
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
    adminSigners={accounts.map((accountId) => ({ accountId, accountLabel: '' }))}
    onBack={() => {}} />);
  await screen.findByRole('heading', { name: '管理员投票状态（4 人）' });
  // 空姓名的管理员仍有分隔点及未填写，投票资格和状态继续使用原逻辑。
  expect(screen.getAllByText('未填写')).toHaveLength(4);
  expect(document.querySelectorAll('.institution-assignment-person .institution-assignment-dot')).toHaveLength(4);

  const card = (accountId: string) =>
    within(screen.getByText(accountIdToSs58(accountId)).closest<HTMLElement>('.institution-assignment-card')!);
  expect(card(approved).getByText('赞成').classList.contains('vote-yes-tag')).toBe(true);
  expect(card(rejected).getByText('反对').classList.contains('vote-no-tag')).toBe(true);
  expect(card(failed).getByText('投票状态未知')).toBeTruthy();
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
    render(<ProposalDetailPage proposalId="7" adminSigners={[]} onBack={onBack} />);
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

import {beforeEach, expect, it, vi} from 'vitest';
import {adminsChangeApi} from '../institution/admins';
const mocks=vi.hoisted(()=>({invoke:vi.fn(),listen:vi.fn()}));
vi.mock('../client/tauri',()=>({invoke:mocks.invoke,sanitizeError:String}));
vi.mock('@tauri-apps/api/event',()=>({listen:mocks.listen}));
const api=adminsChangeApi as unknown as Record<string,(...args:unknown[])=>Promise<unknown>>;
beforeEach(()=>{
  mocks.invoke.mockReset();
  mocks.listen.mockReset().mockResolvedValue(() => {});
});
it("buildActivateAdminRequest保留唯一IPC合同且返回真实回执", async () => { const receipt = { receipt: '合成回执' }; mocks.invoke.mockResolvedValueOnce(receipt); await expect(api["buildActivateAdminRequest"](...["public","44",{"cidNumber":"44","institutionCode":"NRC"}])).resolves.toBe(receipt); expect(mocks.invoke).toHaveBeenCalledExactlyOnceWith("build_activate_admin_request", {"signerPublicKey":"public","cidNumber":"44","expectedInstitutionCode":"NRC"}); });
it("getActivatedAdmins保留唯一IPC合同且返回真实回执", async () => { const receipt = { receipt: '合成回执' }; mocks.invoke.mockResolvedValueOnce(receipt); await expect(api["getActivatedAdmins"](...["44"])).resolves.toBe(receipt); expect(mocks.invoke).toHaveBeenCalledExactlyOnceWith("get_activated_admins", {"cidNumber":"44","expectedInstitutionCode":null}); });
it("getInstitutionAdminsState保留唯一IPC合同且返回真实回执", async () => { const receipt = { receipt: '合成回执' }; mocks.invoke.mockResolvedValueOnce(receipt); await expect(api["getInstitutionAdminsState"](...[{"cidNumber":"44","institutionCode":"NRC"}])).resolves.toBe(receipt); expect(mocks.invoke).toHaveBeenCalledExactlyOnceWith("get_institution_admins_state", {"cidNumber":"44","expectedInstitutionCode":"NRC"}); });
it('IPC失败保留拒绝状态且不自动重放', async()=>{const failure=new Error('合成失败');mocks.invoke.mockRejectedValueOnce(failure);await expect(api["buildActivateAdminRequest"](...["public","44",{"cidNumber":"44","institutionCode":"NRC"}])).rejects.toBe(failure);expect(mocks.invoke).toHaveBeenCalledOnce();});

import { act, fireEvent, render, screen, within } from '@testing-library/react';
import { InstitutionDetailPage } from '../institution/InstitutionDetailPage';
import { accountIdToSs58 } from '../client/ss58';

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

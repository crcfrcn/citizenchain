// 立法与表决前端 API(对接 onchina /api/legislation/*)。
// 发起/表决返回统一链签名准备结果：CitizenWallet 签名一次并显示响应二维码，
// OnChina 回扫后通过唯一提交入口上链；
// 读法律/提案进度直读链投影。通用 http 走 platform/frontend/helpers_http.ts,本模块不另造请求封装。

import type { AdminAuth } from '../../platform/frontend/authentication_types';
import type { ChainSignPrepare } from '../../platform/frontend/core_useChainSign';
import { adminRequest } from '../../platform/frontend/helpers_http';
import { passkeySubmitHeaders } from '../../platform/frontend/admins_securityApi';
import type {
  LawView,
  LegProposalState,
  ProposableCandidate,
  ProposeLawInput,
} from './legislation_types';

/** GET 本机构可发起的提案候选(category×tier×voteTypes)。 */
export async function getProposable(auth: AdminAuth): Promise<ProposableCandidate[]> {
  return adminRequest<ProposableCandidate[]>('/api/legislation/proposable', auth);
}

/** GET 本级已生效/在册法律列表(层级由调用方传入，地域取会话)。 */
export async function listLaws(
  auth: AdminAuth,
  tier: number,
): Promise<LawView[]> {
  return adminRequest<LawView[]>(
    `/api/legislation/laws?tier=${tier}`,
    auth,
  );
}

/** GET 本节点绑定机构层级/辖区的法律(会话派生 scope,前端不传码)。 */
export async function listMyLaws(auth: AdminAuth): Promise<LawView[]> {
  return adminRequest<LawView[]>('/api/legislation/laws/mine', auth);
}

/** GET 单部法律办理端展示版本全文。 */
export async function getLaw(auth: AdminAuth, lawId: number): Promise<LawView> {
  return adminRequest<LawView>(`/api/legislation/laws/${lawId}`, auth);
}

/** GET 提案进度只读投影。 */
export async function getProposalState(
  auth: AdminAuth,
  proposalId: number,
): Promise<LegProposalState> {
  return adminRequest<LegProposalState>(`/api/legislation/proposals/${proposalId}`, auth);
}

/** POST 发起法律案，先完成一次Passkey，再返回原统一链签名准备结果。 */
export async function proposeLegislation(
  auth: AdminAuth,
  input: ProposeLawInput,
): Promise<ChainSignPrepare> {
  return adminRequest<ChainSignPrepare>('/api/legislation/propose', auth, {
    method: 'POST',
    headers: await passkeySubmitHeaders(auth, { 'content-type': 'application/json' }),
    body: JSON.stringify(input),
  });
}

/** POST 当前代表机构表决，先完成一次Passkey，再返回原统一链签名准备结果。 */
export async function castRepresentativeVote(
  auth: AdminAuth,
  proposalId: number,
  voterRoleCode: string,
  approve: boolean,
): Promise<ChainSignPrepare> {
  return adminRequest<ChainSignPrepare>('/api/legislation/representative-vote', auth, {
    method: 'POST',
    headers: await passkeySubmitHeaders(auth, { 'content-type': 'application/json' }),
    body: JSON.stringify({ proposalId, voterRoleCode, approve }),
  });
}

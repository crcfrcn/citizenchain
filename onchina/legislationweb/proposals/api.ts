// 调用法律和表决接口，取得统一签名准备及链上进度数据。

import type { AdminAuth } from '@gmb/onchina-platform/authentication/types';
import type { ChainSignPrepare } from '@gmb/onchina-platform/qr/useChainSign';
import { adminRequest } from '@gmb/onchina-platform/http';
import { passkeySubmitHeaders } from '@gmb/onchina-platform/authentication/security';
import type {
  LawView,
  LegProposalState,
  ProposableCandidate,
  ProposeLawInput,
} from './types';

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
  proposalId: string,
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
  proposalId: string,
  voterRoleCode: string,
  approve: boolean,
): Promise<ChainSignPrepare> {
  return adminRequest<ChainSignPrepare>('/api/legislation/representative-vote', auth, {
    method: 'POST',
    headers: await passkeySubmitHeaders(auth, { 'content-type': 'application/json' }),
    body: JSON.stringify({ proposalId, voterRoleCode, approve }),
  });
}

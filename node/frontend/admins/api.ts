import { invoke } from '../tauri';
import type {
  ActivateRequestResult,
  ActivatedAdmin,
  InstitutionAdminsRef,
  InstitutionAdminsState,
  InstitutionDetail,
} from './types';

// 管理员只读与本机激活 API；岗位任职变更由对应业务模块提交，不在 Node 直接改集合。
const institutionRefParams = (institutionRef: InstitutionAdminsRef) => ({
  cidNumber: institutionRef.cidNumber,
  expectedInstitutionCode: institutionRef.institutionCode ?? null,
});

export const adminsChangeApi = {
  getInstitutionDetail: (cidNumber: string) =>
    invoke<InstitutionDetail>('get_institution_detail', { cidNumber }),
  buildActivateAdminRequest: (signerPublicKey: string, cidNumber: string, institutionRef?: InstitutionAdminsRef) =>
    invoke<ActivateRequestResult>('build_activate_admin_request', {
      signerPublicKey,
      cidNumber: cidNumber,
      expectedInstitutionCode: institutionRef?.institutionCode ?? null,
    }),
  verifyActivateAdmin: (
    requestId: string,
    signerPublicKey: string,
    expectedPayloadHash: string,
    payloadHex: string,
    responseJson: string,
  ) =>
    invoke<ActivatedAdmin>('verify_activate_admin', {
      requestId: requestId,
      signerPublicKey,
      expectedPayloadHash: expectedPayloadHash,
      payloadHex: payloadHex,
      responseJson: responseJson,
    }),
  getActivatedAdmins: (cidNumber: string, institutionRef?: InstitutionAdminsRef) =>
    invoke<ActivatedAdmin[]>('get_activated_admins', {
      cidNumber: cidNumber,
      expectedInstitutionCode: institutionRef?.institutionCode ?? null,
    }),
  deactivateAdmin: (accountId: string, cidNumber: string, institutionRef: InstitutionAdminsRef, unlockPassword: string) =>
    invoke<void>('deactivate_admin', {
      accountId,
      cidNumber: cidNumber,
      expectedInstitutionCode: institutionRef.institutionCode ?? null,
      unlockPassword: unlockPassword,
    }),
  hasAnyActivatedAdmin: () => invoke<boolean>('has_any_activated_admin'),
  getInstitutionAdminsState: (institutionRef: InstitutionAdminsRef) =>
    invoke<InstitutionAdminsState | null>(
      'get_institution_admins_state',
      institutionRefParams(institutionRef),
    ),
  getAccountBalances: (accountIds: string[]) =>
    invoke<Record<string, string | null>>('get_account_balances', { accountIds }),
};

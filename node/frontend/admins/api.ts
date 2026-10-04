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
  cid_number: institutionRef.cidNumber,
  expected_institution_code: institutionRef.institutionCode ?? null,
});

export const adminsChangeApi = {
  getInstitutionDetail: (cidNumber: string) =>
    invoke<InstitutionDetail>('get_institution_detail', { cidNumber }),
  buildActivateAdminRequest: (signer_public_key: string, cidNumber: string, institutionRef?: InstitutionAdminsRef) =>
    invoke<ActivateRequestResult>('build_activate_admin_request', {
      signer_public_key,
      cid_number: cidNumber,
      expected_institution_code: institutionRef?.institutionCode ?? null,
    }),
  verifyActivateAdmin: (
    requestId: string,
    signer_public_key: string,
    expectedPayloadHash: string,
    payloadHex: string,
    responseJson: string,
  ) =>
    invoke<ActivatedAdmin>('verify_activate_admin', {
      request_id: requestId,
      signer_public_key,
      expected_payload_hash: expectedPayloadHash,
      payload_hex: payloadHex,
      response_json: responseJson,
    }),
  getActivatedAdmins: (cidNumber: string, institutionRef?: InstitutionAdminsRef) =>
    invoke<ActivatedAdmin[]>('get_activated_admins', {
      cid_number: cidNumber,
      expected_institution_code: institutionRef?.institutionCode ?? null,
    }),
  deactivateAdmin: (account_id: string, cidNumber: string, institutionRef: InstitutionAdminsRef, unlockPassword: string) =>
    invoke<void>('deactivate_admin', {
      account_id,
      cid_number: cidNumber,
      expected_institution_code: institutionRef.institutionCode ?? null,
      unlock_password: unlockPassword,
    }),
  hasAnyActivatedAdmin: () => invoke<boolean>('has_any_activated_admin'),
  getInstitutionAdminsState: (institutionRef: InstitutionAdminsRef) =>
    invoke<InstitutionAdminsState | null>(
      'get_institution_admins_state',
      institutionRefParams(institutionRef),
    ),
  getAccountBalances: (account_ids: string[]) =>
    invoke<Record<string, string | null>>('get_account_balances', { account_ids }),
};

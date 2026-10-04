// 联邦注册局管理员目录 API；岗位换届由治理业务写入 entity，本页只读。

import type { AdminAuth } from '../auth/types';
import type { InstitutionDetail } from '../subjects/api';
import { adminHeaders, request } from '../utils/http';

// 联邦注册局管理员对外行(API 返回结构)。
//
// CID 业务语义:联邦注册局管理员只有存在/更换,不存在新增/删除/停用状态字段。
export type FederalRegistryAdminRow = {
  id: number;
  province_name: string;
  account_id: string;
  family_name: string;
  given_name: string;
  role_code: string;
  role_name: string;
  term_required: boolean;
  term_start: number;
  term_end: number;
  assignment_source: number;
  assignment_source_label: string;
  assignment_source_ref: string;
  balance_fen?: string | null;
  built_in: boolean;
  created_at: string;
  /** 最近一次更新时间,null 表示从未更新。 */
  updated_at?: string | null;
};

export type OwnInstitutionAdminRow = {
  account_id: string;
  family_name: string;
  given_name: string;
  role_code: string;
  role_name: string;
  term_required: boolean;
  term_start: number;
  term_end: number;
  assignment_source: number;
  assignment_source_label: string;
  assignment_source_ref: string;
  balance_fen?: string | null;
  is_self: boolean;
};

export type OwnInstitutionAdminListOutput = {
  institution_code: string;
  cid_short_name?: string | null;
  rows: OwnInstitutionAdminRow[];
};

export type InstitutionGovernanceAdminInput = {
  account_id: string;
  family_name: string;
  given_name: string;
};

export type InstitutionRolePermissionInput = {
  module_tag: string;
  action_code: number;
  operation: 'PROPOSE' | 'VOTE';
};

export type InstitutionGovernanceRoleMutationInput = {
  mutation: 'CREATE' | 'RENAME' | 'DELETE';
  role_code?: string;
  role_name?: string;
  term_required?: boolean;
  permissions?: InstitutionRolePermissionInput[];
  assignments?: InstitutionGovernanceAssignmentTargetInput[];
};

export type InstitutionGovernanceAssignmentTargetInput = {
  account_id: string;
  term_start: number;
  term_end: number;
};

export type InstitutionGovernanceAssignmentChangeInput = {
  role_code: string;
  assignments: InstitutionGovernanceAssignmentTargetInput[];
};

export type PrepareInstitutionGovernanceInput = {
  cid_number: string;
  proposer_role_code: string;
  admins?: InstitutionGovernanceAdminInput[];
  role_mutations?: InstitutionGovernanceRoleMutationInput[];
  assignment_changes?: InstitutionGovernanceAssignmentChangeInput[];
  legal_representative_cid_number?: string | null;
  clear_legal_representative?: boolean;
};

export type PrepareRegisterInstitutionAdminsInput = {
  cid_number: string;
  admins: InstitutionGovernanceAdminInput[];
};

export type PrepareInstitutionChainOutput = {
  request_id: string;
  cid_number: string;
  chain_action: number;
  call_data_hex: string;
  sign_request: string;
  expires_at: number;
};

export async function listFederalRegistryAdmins(auth: AdminAuth): Promise<FederalRegistryAdminRow[]> {
  return request<FederalRegistryAdminRow[]>('/api/admin/federal-registry-admins', {
    method: 'GET',
    headers: adminHeaders(auth),
  });
}

export async function listOwnInstitutionAdmins(auth: AdminAuth): Promise<OwnInstitutionAdminListOutput> {
  return request<OwnInstitutionAdminListOutput>('/api/admin/own-institution-admins', {
    method: 'GET',
    headers: adminHeaders(auth),
  });
}

export async function getOwnInstitution(auth: AdminAuth): Promise<InstitutionDetail> {
  return request<InstitutionDetail>('/api/admin/own-institution', {
    method: 'GET',
    headers: adminHeaders(auth),
  });
}

export async function prepareInstitutionGovernance(
  auth: AdminAuth,
  input: PrepareInstitutionGovernanceInput,
): Promise<PrepareInstitutionChainOutput> {
  return request<PrepareInstitutionChainOutput>('/api/admin/institution/governance/prepare', {
    method: 'POST',
    headers: {
      'content-type': 'application/json',
      ...adminHeaders(auth),
    },
    body: JSON.stringify(input),
  });
}

export async function prepareRegisterInstitutionAdmins(
  auth: AdminAuth,
  input: PrepareRegisterInstitutionAdminsInput,
): Promise<PrepareInstitutionChainOutput> {
  return request<PrepareInstitutionChainOutput>('/api/admin/institution/admins/register/prepare', {
    method: 'POST',
    headers: {
      'content-type': 'application/json',
      ...adminHeaders(auth),
    },
    body: JSON.stringify(input),
  });
}

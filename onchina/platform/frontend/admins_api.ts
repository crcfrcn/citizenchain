// 通用本机构管理员与机构治理 API；注册局专属名册 API 归注册局。

import type { AdminAuth } from './authentication_types';
import type { InstitutionDetail } from './subjects_api';
import { adminHeaders, adminRequest, request } from './helpers_http';
import { passkeySubmitHeaders } from './admins_securityApi';

export type OwnInstitutionAdminRow = {
  accountId: string;
  familyName: string;
  givenName: string;
  roleCode: string;
  roleName: string;
  termRequired: boolean;
  termStart: number;
  termEnd: number;
  assignmentSource: number;
  assignmentSourceLabel: string;
  assignmentSourceRef: string;
  balanceFen?: string | null;
  isSelf: boolean;
};

export type OwnInstitutionAdminListOutput = {
  institutionCode: string;
  cidShortName?: string | null;
  rows: OwnInstitutionAdminRow[];
};

export type InstitutionGovernanceAdminInput = {
  accountId: string;
  familyName: string;
  givenName: string;
};

export type InstitutionRolePermissionInput = {
  moduleTag: string;
  actionCode: number;
  operation: 'PROPOSE' | 'VOTE';
};

export type InstitutionGovernanceRoleMutationInput = {
  mutation: 'CREATE' | 'RENAME' | 'DELETE';
  roleCode?: string;
  roleName?: string;
  termRequired?: boolean;
  permissions?: InstitutionRolePermissionInput[];
  assignments?: InstitutionGovernanceAssignmentTargetInput[];
};

export type InstitutionGovernanceAssignmentTargetInput = {
  accountId: string;
  termStart: number;
  termEnd: number;
};

export type InstitutionGovernanceAssignmentChangeInput = {
  roleCode: string;
  assignments: InstitutionGovernanceAssignmentTargetInput[];
};

export type PrepareInstitutionGovernanceInput = {
  cidNumber: string;
  proposerRoleCode: string;
  admins?: InstitutionGovernanceAdminInput[];
  roleMutations?: InstitutionGovernanceRoleMutationInput[];
  assignmentChanges?: InstitutionGovernanceAssignmentChangeInput[];
  representativeCid?: string | null;
  clearLegalRepresentative?: boolean;
};

export type PrepareInstitutionChainOutput = {
  requestId: string;
  cidNumber: string;
  chainAction: number;
  callDataHex: string;
  signRequest: string;
  expiresAt: number;
};

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

/** 链签名准备前完成一次现有Passkey认证，取消时不发送业务请求。 */
export async function prepareInstitutionGovernance(
  auth: AdminAuth,
  input: PrepareInstitutionGovernanceInput,
): Promise<PrepareInstitutionChainOutput> {
  return adminRequest<PrepareInstitutionChainOutput>('/api/admin/institution/governance/prepare', auth, {
    method: 'POST',
    headers: await passkeySubmitHeaders(auth, { 'content-type': 'application/json' }),
    body: JSON.stringify(input),
  });
}

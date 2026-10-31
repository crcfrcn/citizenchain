import React from "react";
import { type PrivateTypePageProps, PrivateTypePage } from "./PrivateTypePage";
import type { AdminAuth } from "@gmb/onchina-platform/authentication/types";
import type { CreateInstitutionInput, CreateInstitutionOutput, InstitutionListRow, PageResult, PrivateType, PartnershipKind } from "@gmb/onchina-platform/institutions/api";
import { createInstitution as createPrivateInstitution, listPrivateInstitutions } from "./api";

// 合伙企业页面。只承接合伙企业列表、新增和进入详情。







export type PartnershipPageProps = Omit<
  PrivateTypePageProps,
  'privateType' | 'title' | 'createInstitution' | 'listInstitutions'
>;

export const PartnershipPage: React.FC<PartnershipPageProps> = (props) => (
  <PrivateTypePage
    {...props}
    privateType={PARTNERSHIP_PRIVATE_TYPE}
    title={PARTNERSHIP_TITLE}
    createInstitution={createPartnershipInstitution}
    listInstitutions={listPartnershipInstitutions}
  />
);

// 合伙企业 API。调用 `/api/private/partnership`,不得回退到旧聚合接口。






export function createPartnershipInstitution(
  auth: AdminAuth,
  input: CreateInstitutionInput,
): Promise<CreateInstitutionOutput> {
  return createPrivateInstitution(auth, PARTNERSHIP_ROUTE_SEGMENT, input);
}

export function listPartnershipInstitutions(
  auth: AdminAuth,
  query: {
    provinceName: string;
    cityName?: string;
    privateType: PrivateType;
    q: string;
    cursor?: string | null;
    pageSize?: number;
  },
): Promise<PageResult<InstitutionListRow>> {
  return listPrivateInstitutions(auth, PARTNERSHIP_ROUTE_SEGMENT, query);
}

// 合伙企业前端类型边界。无限合伙=`F+GP`,有限合伙=`S+LP`。



export const PARTNERSHIP_PRIVATE_TYPE: PrivateType = 'PARTNERSHIP';
export const PARTNERSHIP_ROUTE_SEGMENT = 'partnership';
export const PARTNERSHIP_TITLE = '合伙企业';

export const PARTNERSHIP_KIND_OPTIONS: Array<{ value: PartnershipKind; label: string }> = [
  { value: 'GENERAL', label: '无限合伙' },
  { value: 'LIMITED', label: '有限合伙' },
];

export interface PartnershipProfileFields {
  partnershipKind: PartnershipKind;
  identityCode: 'GP' | 'LP';
  partnerRoles: Array<'GENERAL_PARTNER' | 'LIMITED_PARTNER'>;
}

import React from "react";
import { type PrivateTypePageProps, PrivateTypePage } from "./PrivateTypePage";
import type { AdminAuth } from "@gmb/onchina-platform/authentication/types";
import type { CreateInstitutionInput, CreateInstitutionOutput, InstitutionListRow, PageResult, PrivateType } from "@gmb/onchina-platform/institutions/api";
import { createInstitution as createPrivateInstitution, listPrivateInstitutions } from "./api";

// 股份公司页面。只承接股份公司列表、新增和进入详情。







export type CorporationPageProps = Omit<
  PrivateTypePageProps,
  'privateType' | 'title' | 'createInstitution' | 'listInstitutions'
>;

export const CorporationPage: React.FC<CorporationPageProps> = (props) => (
  <PrivateTypePage
    {...props}
    privateType={CORPORATION_PRIVATE_TYPE}
    title={CORPORATION_TITLE}
    createInstitution={createCorporationInstitution}
    listInstitutions={listCorporationInstitutions}
  />
);

// 股份公司 API。调用 `/api/private/corporation`,不得回退到旧聚合接口。






export function createCorporationInstitution(
  auth: AdminAuth,
  input: CreateInstitutionInput,
): Promise<CreateInstitutionOutput> {
  return createPrivateInstitution(auth, CORPORATION_ROUTE_SEGMENT, input);
}

export function listCorporationInstitutions(
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
  return listPrivateInstitutions(auth, CORPORATION_ROUTE_SEGMENT, query);
}

// 股份公司前端类型边界。固定 `S + GF`,管理股份和股东关系。



export const CORPORATION_PRIVATE_TYPE: PrivateType = 'CORPORATION';
export const CORPORATION_ROUTE_SEGMENT = 'corporation';
export const CORPORATION_TITLE = '股份公司';

export interface CorporationProfileFields {
  identityCode: 'GF';
  equityUnitLabel: '股份';
  shareholderRole: 'SHAREHOLDER';
  hasLegalPersonality: true;
}

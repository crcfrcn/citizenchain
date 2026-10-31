import React from "react";
import { type PrivateTypePageProps, PrivateTypePage } from "./PrivateTypePage";
import type { AdminAuth } from "@gmb/onchina-platform/authentication/types";
import type { CreateInstitutionInput, CreateInstitutionOutput, InstitutionListRow, PageResult, PrivateType } from "@gmb/onchina-platform/institutions/api";
import { createInstitution as createPrivateInstitution, listPrivateInstitutions } from "./api";

// 股权公司页面。只承接股权公司列表、新增和进入详情。







export type CompanyPageProps = Omit<
  PrivateTypePageProps,
  'privateType' | 'title' | 'createInstitution' | 'listInstitutions'
>;

export const CompanyPage: React.FC<CompanyPageProps> = (props) => (
  <PrivateTypePage
    {...props}
    privateType={COMPANY_PRIVATE_TYPE}
    title={COMPANY_TITLE}
    createInstitution={createCompanyInstitution}
    listInstitutions={listCompanyInstitutions}
  />
);

// 股权公司 API。调用 `/api/private/company`,不得回退到旧聚合接口。






export function createCompanyInstitution(
  auth: AdminAuth,
  input: CreateInstitutionInput,
): Promise<CreateInstitutionOutput> {
  return createPrivateInstitution(auth, COMPANY_ROUTE_SEGMENT, input);
}

export function listCompanyInstitutions(
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
  return listPrivateInstitutions(auth, COMPANY_ROUTE_SEGMENT, query);
}

// 股权公司前端类型边界。固定 `S + GQ`,管理股东和出资关系。



export const COMPANY_PRIVATE_TYPE: PrivateType = 'COMPANY';
export const COMPANY_ROUTE_SEGMENT = 'company';
export const COMPANY_TITLE = '股权公司';

export interface CompanyProfileFields {
  identityCode: 'GQ';
  shareholderRole: 'EQUITY_SHAREHOLDER';
  hasLegalPersonality: true;
}

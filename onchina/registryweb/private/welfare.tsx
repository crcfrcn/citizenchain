import React from "react";
import { type PrivateTypePageProps, PrivateTypePage } from "./PrivateTypePage";
import type { AdminAuth } from "@gmb/onchina-platform/authentication/types";
import type { CreateInstitutionInput, CreateInstitutionOutput, InstitutionListRow, PageResult, PrivateType } from "@gmb/onchina-platform/institutions/api";
import { createInstitution as createPrivateInstitution, listPrivateInstitutions } from "./api";

// 公益组织页面。只承接公益组织列表、新增和进入详情。







export type WelfarePageProps = Omit<
  PrivateTypePageProps,
  'privateType' | 'title' | 'createInstitution' | 'listInstitutions'
>;

export const WelfarePage: React.FC<WelfarePageProps> = (props) => (
  <PrivateTypePage
    {...props}
    privateType={WELFARE_PRIVATE_TYPE}
    title={WELFARE_TITLE}
    createInstitution={createWelfareInstitution}
    listInstitutions={listWelfareInstitutions}
  />
);

// 公益组织 API。调用 `/api/private/welfare`,不得回退到旧聚合接口。






export function createWelfareInstitution(
  auth: AdminAuth,
  input: CreateInstitutionInput,
): Promise<CreateInstitutionOutput> {
  return createPrivateInstitution(auth, WELFARE_ROUTE_SEGMENT, input);
}

export function listWelfareInstitutions(
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
  return listPrivateInstitutions(auth, WELFARE_ROUTE_SEGMENT, query);
}

// 公益组织前端类型边界。固定 `S + GY`,非营利法人。



export const WELFARE_PRIVATE_TYPE: PrivateType = 'WELFARE';
export const WELFARE_ROUTE_SEGMENT = 'welfare';
export const WELFARE_TITLE = '公益组织';

export interface WelfareProfileFields {
  identityCode: 'GY';
  p1: '0';
  purposeLabel: '公益目的';
  hasLegalPersonality: true;
}

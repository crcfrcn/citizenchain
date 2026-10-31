import React from "react";
import { type PrivateTypePageProps, PrivateTypePage } from "./PrivateTypePage";
import type { AdminAuth } from "@gmb/onchina-platform/authentication/types";
import type { CreateInstitutionInput, CreateInstitutionOutput, InstitutionListRow, PageResult, PrivateType } from "@gmb/onchina-platform/institutions/api";
import { createInstitution as createPrivateInstitution, listPrivateInstitutions } from "./api";

// 个体经营页面。只承接个体经营列表、新增和进入详情。







export type SolePageProps = Omit<
  PrivateTypePageProps,
  'privateType' | 'title' | 'createInstitution' | 'listInstitutions'
>;

export const SolePage: React.FC<SolePageProps> = (props) => (
  <PrivateTypePage
    {...props}
    privateType={SOLE_PRIVATE_TYPE}
    title={SOLE_TITLE}
    createInstitution={createSoleInstitution}
    listInstitutions={listSoleInstitutions}
  />
);

// 个体经营 API。调用 `/api/private/sole`,不得回退到旧聚合接口。






export function createSoleInstitution(
  auth: AdminAuth,
  input: CreateInstitutionInput,
): Promise<CreateInstitutionOutput> {
  return createPrivateInstitution(auth, SOLE_ROUTE_SEGMENT, input);
}

export function listSoleInstitutions(
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
  return listPrivateInstitutions(auth, SOLE_ROUTE_SEGMENT, query);
}

// 个体经营前端类型边界。固定 `F + GT`,无法人资格。



export const SOLE_PRIVATE_TYPE: PrivateType = 'SOLE';
export const SOLE_ROUTE_SEGMENT = 'sole';
export const SOLE_TITLE = '个体经营';

export interface SoleProfileFields {
  responsibleRole: 'RESPONSIBLE_PERSON';
  hasLegalPersonality: false;
  identityCode: 'GT';
}

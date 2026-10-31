import React from "react";
import { type PrivateTypePageProps, PrivateTypePage } from "./PrivateTypePage";
import type { AdminAuth } from "@gmb/onchina-platform/authentication/types";
import type { CreateInstitutionInput, CreateInstitutionOutput, InstitutionListRow, PageResult, PrivateType } from "@gmb/onchina-platform/institutions/api";
import { createInstitution as createPrivateInstitution, listPrivateInstitutions } from "./api";

// 注册协会页面。只承接注册协会列表、新增和进入详情。







export type AssociationPageProps = Omit<
  PrivateTypePageProps,
  'privateType' | 'title' | 'createInstitution' | 'listInstitutions'
>;

export const AssociationPage: React.FC<AssociationPageProps> = (props) => (
  <PrivateTypePage
    {...props}
    privateType={ASSOCIATION_PRIVATE_TYPE}
    title={ASSOCIATION_TITLE}
    createInstitution={createAssociationInstitution}
    listInstitutions={listAssociationInstitutions}
  />
);

// 注册协会 API。调用 `/api/private/association`,不得回退到旧聚合接口。






export function createAssociationInstitution(
  auth: AdminAuth,
  input: CreateInstitutionInput,
): Promise<CreateInstitutionOutput> {
  return createPrivateInstitution(auth, ASSOCIATION_ROUTE_SEGMENT, input);
}

export function listAssociationInstitutions(
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
  return listPrivateInstitutions(auth, ASSOCIATION_ROUTE_SEGMENT, query);
}

// 注册协会前端类型边界。机构码固定为 SFAS，盈利属性由每个实例显式选择。



export const ASSOCIATION_PRIVATE_TYPE: PrivateType = 'ASSOCIATION';
export const ASSOCIATION_ROUTE_SEGMENT = 'association';
export const ASSOCIATION_TITLE = '注册协会';

export interface AssociationProfileFields {
  identityCode: 'AS';
  p1: '0' | '1';
  memberRole: 'MEMBER';
  hasLegalPersonality: true;
}

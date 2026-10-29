// 注册协会 API。调用 `/api/private/association`,不得回退到旧聚合接口。

import type { AdminAuth } from '../../platform/frontend/authentication_types';
import type {
  CreateInstitutionInput,
  CreateInstitutionOutput,
  InstitutionListRow,
  PageResult,
  PrivateType,
} from '../../platform/frontend/subjects_api';
import {
  createInstitution as createPrivateInstitution,
  listPrivateInstitutions,
} from './private_api';
import { ASSOCIATION_ROUTE_SEGMENT } from './association_types';

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

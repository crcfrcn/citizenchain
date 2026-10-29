// 公益组织 API。调用 `/api/private/welfare`,不得回退到旧聚合接口。

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
import { WELFARE_ROUTE_SEGMENT } from './welfare_types';

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

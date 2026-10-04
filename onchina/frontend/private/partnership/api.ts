// 合伙企业 API。调用 `/api/private/partnership`,不得回退到旧聚合接口。

import type { AdminAuth } from '../../auth/types';
import type {
  CreateInstitutionInput,
  CreateInstitutionOutput,
  InstitutionListRow,
  PageResult,
  PrivateType,
} from '../../subjects/api';
import {
  createInstitution as createPrivateInstitution,
  listPrivateInstitutions,
} from '../common/api';
import { PARTNERSHIP_ROUTE_SEGMENT } from './types';

export function createPartnershipInstitution(
  auth: AdminAuth,
  input: CreateInstitutionInput,
): Promise<CreateInstitutionOutput> {
  return createPrivateInstitution(auth, PARTNERSHIP_ROUTE_SEGMENT, input);
}

export function listPartnershipInstitutions(
  auth: AdminAuth,
  query: {
    province_name: string;
    city_name?: string;
    private_type: PrivateType;
    q: string;
    cursor?: string | null;
    page_size?: number;
  },
): Promise<PageResult<InstitutionListRow>> {
  return listPrivateInstitutions(auth, PARTNERSHIP_ROUTE_SEGMENT, query);
}

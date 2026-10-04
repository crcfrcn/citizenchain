// 个体经营 API。调用 `/api/private/sole`,不得回退到旧聚合接口。

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
import { SOLE_ROUTE_SEGMENT } from './types';

export function createSoleInstitution(
  auth: AdminAuth,
  input: CreateInstitutionInput,
): Promise<CreateInstitutionOutput> {
  return createPrivateInstitution(auth, SOLE_ROUTE_SEGMENT, input);
}

export function listSoleInstitutions(
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
  return listPrivateInstitutions(auth, SOLE_ROUTE_SEGMENT, query);
}

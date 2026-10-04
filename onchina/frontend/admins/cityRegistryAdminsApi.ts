// 市注册局管理员 API。
// 市注册局管理员列表、创建、删除都归入 admins 管理员功能目录。

import type { AdminAuth } from '../auth/types';
import { adminHeaders, request } from '../utils/http';

export type CityRegistryAdminRow = {
  id: number;
  account_id: string;
  family_name: string;
  given_name: string;
  balance_fen?: string | null;
  institution_code: string;
  built_in: boolean;
  creator_account_id: string;
  creator_family_name: string;
  creator_given_name: string;
  created_at: string;
  city_name: string;
};

export async function listCityRegistryAdmins(auth: AdminAuth): Promise<CityRegistryAdminRow[]> {
  const data = await request<{ total: number; rows: CityRegistryAdminRow[] }>('/api/admin/city-registry-admins', {
    method: 'GET',
    headers: adminHeaders(auth),
  });
  return data.rows;
}

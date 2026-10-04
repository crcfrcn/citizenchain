import { adminHeaders, request } from '../utils/http';
import type { AdminAuth } from '../auth/types';

export type AddressNameRow = {
  province_code: string;
  city_code: string;
  town_code: string;
  address_name_code: string;
  address_name: string;
  address_count: number;
};

export type AddressRow = {
  province_code: string;
  city_code: string;
  town_code: string;
  address_name_code: string;
  address_name: string;
  address_local_no: string;
  address_detail: string;
  sort_order: number;
};

export type AddressPage<T> = {
  items: T[];
  page_size: number;
  next_cursor?: number | null;
  has_more: boolean;
};

export type AddressChainAction =
  | 'set_catalog_version'
  | 'set_address_name'
  | 'remove_address_name'
  | 'set_address'
  | 'remove_address';

export type AddressChainCallInput = {
  actor_role_code: string;
  action: AddressChainAction;
  catalog_version?: string;
  catalog_hash?: string;
  province_code?: string;
  city_code?: string;
  town_code?: string;
  address_name_code?: string;
  address_name?: string;
  address_local_no?: string;
  address_detail?: string;
};

export type AddressChainCallOutput = {
  action: number;
  pallet_index: number;
  call_index: number;
  call_data_hex: string;
  review_title: string;
};

function scopeQuery(params: {
  province_code: string;
  city_code: string;
  town_code: string;
  address_name_code?: string;
  cursor?: number | null;
  page_size?: number;
}) {
  const q = new URLSearchParams();
  q.set('province_code', params.province_code);
  q.set('city_code', params.city_code);
  q.set('town_code', params.town_code);
  if (params.address_name_code) q.set('address_name_code', params.address_name_code);
  if (typeof params.cursor === 'number') q.set('cursor', String(params.cursor));
  if (params.page_size) q.set('page_size', String(params.page_size));
  return q.toString();
}

export async function listAddressNames(
  auth: AdminAuth,
  params: { province_code: string; city_code: string; town_code: string; cursor?: number | null },
): Promise<AddressPage<AddressNameRow>> {
  return request<AddressPage<AddressNameRow>>(
    `/api/admin/address/names?${scopeQuery({ ...params, page_size: 100 })}`,
    { method: 'GET', headers: adminHeaders(auth) },
  );
}

export async function listAddressItems(
  auth: AdminAuth,
  params: {
    province_code: string;
    city_code: string;
    town_code: string;
    address_name_code: string;
    cursor?: number | null;
  },
): Promise<AddressPage<AddressRow>> {
  return request<AddressPage<AddressRow>>(
    `/api/admin/address/items?${scopeQuery({ ...params, page_size: 100 })}`,
    { method: 'GET', headers: adminHeaders(auth) },
  );
}

export async function prepareAddressChainCall(
  auth: AdminAuth,
  input: AddressChainCallInput,
): Promise<AddressChainCallOutput> {
  return request<AddressChainCallOutput>('/api/admin/address/chain-call', {
    method: 'POST',
    headers: { ...adminHeaders(auth), 'content-type': 'application/json' },
    body: JSON.stringify(input),
  });
}

import { adminHeaders, request } from '../../platform/frontend/helpers_http';
import type { AdminAuth } from '../../platform/frontend/authentication_types';

export type AddressNameRow = {
  provinceCode: string;
  cityCode: string;
  townCode: string;
  addressNameCode: string;
  addressName: string;
  addressCount: number;
};

export type AddressRow = {
  provinceCode: string;
  cityCode: string;
  townCode: string;
  addressNameCode: string;
  addressName: string;
  addressLocalNo: string;
  addressDetail: string;
  sortOrder: number;
};

export type AddressPage<T> = {
  items: T[];
  pageSize: number;
  nextCursor?: number | null;
  hasMore: boolean;
};

export type AddressChainAction =
  | 'set_catalog_version'
  | 'set_address_name'
  | 'remove_address_name'
  | 'set_address'
  | 'remove_address';

export type AddressChainCallInput = {
  actorRoleCode: string;
  action: AddressChainAction;
  catalogVersion?: string;
  catalogHash?: string;
  provinceCode?: string;
  cityCode?: string;
  townCode?: string;
  addressNameCode?: string;
  addressName?: string;
  addressLocalNo?: string;
  addressDetail?: string;
};

export type AddressChainCallOutput = {
  action: number;
  palletIndex: number;
  callIndex: number;
  callDataHex: string;
  reviewTitle: string;
};

function scopeQuery(params: {
  provinceCode: string;
  cityCode: string;
  townCode: string;
  addressNameCode?: string;
  cursor?: number | null;
  pageSize?: number;
}) {
  const q = new URLSearchParams();
  q.set('provinceCode', params.provinceCode);
  q.set('cityCode', params.cityCode);
  q.set('townCode', params.townCode);
  if (params.addressNameCode) q.set('addressNameCode', params.addressNameCode);
  if (typeof params.cursor === 'number') q.set('cursor', String(params.cursor));
  if (params.pageSize) q.set('pageSize', String(params.pageSize));
  return q.toString();
}

export async function listAddressNames(
  auth: AdminAuth,
  params: { provinceCode: string; cityCode: string; townCode: string; cursor?: number | null },
): Promise<AddressPage<AddressNameRow>> {
  return request<AddressPage<AddressNameRow>>(
    `/api/admin/address/names?${scopeQuery({ ...params, pageSize: 100 })}`,
    { method: 'GET', headers: adminHeaders(auth) },
  );
}

export async function listAddressItems(
  auth: AdminAuth,
  params: {
    provinceCode: string;
    cityCode: string;
    townCode: string;
    addressNameCode: string;
    cursor?: number | null;
  },
): Promise<AddressPage<AddressRow>> {
  return request<AddressPage<AddressRow>>(
    `/api/admin/address/items?${scopeQuery({ ...params, pageSize: 100 })}`,
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

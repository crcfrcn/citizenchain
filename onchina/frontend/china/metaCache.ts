// CID 前端轻量缓存。
// 只缓存省市代码、确定性机构展示列表和机构详情快照;普通公民/机构精确搜索结果不得放进这里。
// 教育机构缓存仅限市详情直显的确定性市公民教育委员会,不缓存学校和 F+JY 搜索结果。

import type { AdminAuth } from '../auth/types';
import type { InstitutionDetail, InstitutionListRow } from '../subjects/api';
import { getCidMeta, listCidCities, type CidCityItem, type CidMetaResult } from './api';

const CID_META_CACHE_ID = 'cid-meta';
const CID_CITY_CACHE_ID = 'cid-cities';
// 缓存 ID 与载荷形状同时校验；不匹配时直接清理并回源。
const OFFICIAL_INSTITUTION_CACHE_ID = 'official-institutions';
const EDUCATION_COMMITTEE_CACHE_ID = 'education-committees';
const INSTITUTION_DETAIL_CACHE_ID = 'institution-detail';
const GOV_MANIFEST_VERSION_KEY = 'cid:gov-manifest-version';

interface CachedPayload<T> {
  cache_id: string;
  data: T;
}

interface InstitutionRowsCachePayload {
  cache_id: string;
  account_id: string;
  institution_code: string;
  province_name: string;
  city_name: string;
  manifest_version?: string | null;
  rows: InstitutionListRow[];
}

function readCache<T>(key: string, cacheId: string): T | null {
  try {
    const raw = localStorage.getItem(key);
    if (!raw) return null;
    const parsed = JSON.parse(raw) as CachedPayload<T>;
    if (parsed.cache_id !== cacheId || typeof parsed.data === 'undefined') {
      localStorage.removeItem(key);
      return null;
    }
    return parsed.data;
  } catch {
    localStorage.removeItem(key);
    return null;
  }
}

function writeCache<T>(key: string, cacheId: string, data: T) {
  try {
    localStorage.setItem(key, JSON.stringify({ cache_id: cacheId, data } satisfies CachedPayload<T>));
  } catch {
    // 确定性元数据缓存写入失败不能阻断页面展示。
  }
}

function cidMetaCacheKey(auth: AdminAuth): string {
  return ['cid:meta', CID_META_CACHE_ID, auth.account_id, auth.institution_code].join(':');
}

function cidCitiesCacheKey(province_name: string): string {
  return ['cid:cities', CID_CITY_CACHE_ID, province_name].join(':');
}

export async function loadCachedCidMeta(auth: AdminAuth): Promise<CidMetaResult> {
  const key = cidMetaCacheKey(auth);
  const cacheId = CID_META_CACHE_ID;
  const cached = readCache<CidMetaResult>(key, cacheId);
  if (cached) return cached;
  const next = await getCidMeta(auth);
  writeCache(key, cacheId, next);
  return next;
}

// 防御字段漂移——缓存里只要存在 city_name/city_code 缺失的项就判脏,弃缓存回源。
// 背景:后端 cities 字段曾从 name/code 改为 city_name/city_code 而缓存版本未 bump,
// 旧结构缓存静默残留会导致市卡片与注册局列表 join 失败。形状校验让缓存对结构漂移自愈。
function citiesCacheUsable(rows: CidCityItem[] | null): rows is CidCityItem[] {
  return (
    Array.isArray(rows)
    && rows.every((c) =>
      typeof c.city_name === 'string'
      && c.city_name.length > 0
      && typeof c.city_code === 'string'
      && c.city_code.length > 0)
  );
}

export async function loadCachedCidCities(
  auth: AdminAuth,
  province_name: string,
): Promise<CidCityItem[]> {
  const key = cidCitiesCacheKey(province_name);
  const cacheId = CID_CITY_CACHE_ID;
  const cached = readCache<CidCityItem[]>(key, cacheId);
  if (citiesCacheUsable(cached)) return cached;
  if (cached) localStorage.removeItem(key);
  const rows = await listCidCities(auth, province_name);
  writeCache(key, cacheId, rows);
  return rows;
}

export function readCachedCidCities(province_name: string): CidCityItem[] | null {
  const cached = readCache<CidCityItem[]>(cidCitiesCacheKey(province_name), CID_CITY_CACHE_ID);
  return citiesCacheUsable(cached) ? cached : null;
}

export function officialInstitutionCacheKey(auth: AdminAuth, province_name: string, city_name: string): string {
  const scopeCity = auth.scope_city_name || city_name || 'ALL';
  const scopeProvince = auth.scope_province_name || province_name;
  return [
    'cid:official-institutions',
    OFFICIAL_INSTITUTION_CACHE_ID,
    auth.account_id,
    auth.institution_code,
    scopeProvince,
    scopeCity,
  ].join(':');
}

export function readCachedOfficialInstitutionRows(key: string): InstitutionListRow[] | null {
  try {
    const raw = localStorage.getItem(key);
    if (!raw) return null;
    const parsed = JSON.parse(raw) as InstitutionRowsCachePayload;
    if (parsed.cache_id !== OFFICIAL_INSTITUTION_CACHE_ID || !Array.isArray(parsed.rows)) {
      localStorage.removeItem(key);
      return null;
    }
    if (parsed.rows.length === 0) {
      localStorage.removeItem(key);
      return null;
    }
    const latestManifestVersion = localStorage.getItem(GOV_MANIFEST_VERSION_KEY);
    if (
      latestManifestVersion
      && parsed.manifest_version
      && parsed.manifest_version !== latestManifestVersion
    ) {
      localStorage.removeItem(key);
      return null;
    }
    return parsed.rows;
  } catch {
    localStorage.removeItem(key);
    return null;
  }
}

export function writeCachedOfficialInstitutionRows(
  key: string,
  auth: AdminAuth,
  province_name: string,
  city_name: string,
  rows: InstitutionListRow[],
  manifestVersion?: string | null,
) {
  if (rows.length === 0) return;
  try {
    if (manifestVersion) localStorage.setItem(GOV_MANIFEST_VERSION_KEY, manifestVersion);
    localStorage.setItem(
      key,
      JSON.stringify({
        cache_id: OFFICIAL_INSTITUTION_CACHE_ID,
        account_id: auth.account_id,
        institution_code: auth.institution_code,
        province_name: auth.scope_province_name ||  province_name,
        city_name: auth.scope_city_name || city_name || 'ALL',
        manifest_version: manifestVersion ?? null,
        rows,
      } satisfies InstitutionRowsCachePayload),
    );
  } catch {
    // 公权机构确定性列表缓存只是展示加速,写失败不影响后端权威结果。
  }
}

export function educationCommitteeCacheKey(auth: AdminAuth, province_name: string, city_name: string): string {
  const scopeCity = auth.scope_city_name || city_name;
  const scopeProvince = auth.scope_province_name || province_name;
  return [
    'cid:education-committees',
    EDUCATION_COMMITTEE_CACHE_ID,
    auth.account_id,
    auth.institution_code,
    scopeProvince,
    scopeCity,
  ].join(':');
}

export function readCachedEducationCommitteeRows(key: string): InstitutionListRow[] | null {
  try {
    const raw = localStorage.getItem(key);
    if (!raw) return null;
    const parsed = JSON.parse(raw) as InstitutionRowsCachePayload;
    if (parsed.cache_id !== EDUCATION_COMMITTEE_CACHE_ID || !Array.isArray(parsed.rows)) {
      localStorage.removeItem(key);
      return null;
    }
    if (parsed.rows.length === 0) {
      localStorage.removeItem(key);
      return null;
    }
    return parsed.rows;
  } catch {
    localStorage.removeItem(key);
    return null;
  }
}

export function writeCachedEducationCommitteeRows(
  key: string,
  auth: AdminAuth,
  province_name: string,
  city_name: string,
  rows: InstitutionListRow[],
) {
  try {
    if (rows.length === 0) {
      localStorage.removeItem(key);
      return;
    }
    localStorage.setItem(
      key,
      JSON.stringify({
        cache_id: EDUCATION_COMMITTEE_CACHE_ID,
        account_id: auth.account_id,
        institution_code: auth.institution_code,
        province_name: auth.scope_province_name ||  province_name,
        city_name: auth.scope_city_name ||  city_name,
        manifest_version: null,
        rows,
      } satisfies InstitutionRowsCachePayload),
    );
  } catch {
    // 教育机构确定性市教委会缓存只是首屏展示加速,写失败不影响后端权威结果。
  }
}

export function institutionDetailCacheKey(auth: AdminAuth, cidNumber: string): string {
  return [
    'cid:institution-detail',
    INSTITUTION_DETAIL_CACHE_ID,
    auth.account_id,
    auth.institution_code,
    cidNumber,
  ].join(':');
}

export function readCachedInstitutionDetail(key: string): InstitutionDetail | null {
  return readCache<InstitutionDetail>(key, INSTITUTION_DETAIL_CACHE_ID);
}

export function writeCachedInstitutionDetail(key: string, detail: InstitutionDetail) {
  writeCache(key, INSTITUTION_DETAIL_CACHE_ID, detail);
}

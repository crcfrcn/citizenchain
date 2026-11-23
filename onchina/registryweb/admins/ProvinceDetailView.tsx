// 按省市范围展示注册局详情及当前管理员名册。

import { InstitutionWalletSelfAction, InstitutionWalletRecoveryActions } from '@gmb/onchina-platform/institutions/AdminsView';
import { IdentityCidText } from '@gmb/onchina-platform/codes/identityCid';
import React, { useCallback, useEffect, useMemo, useRef, useState } from 'react';
import { Alert, Badge, Button, Card, Modal, Space, Table, Typography } from 'antd';
import { KeyOutlined } from '@ant-design/icons';
import { type ColumnsType } from 'antd/es/table';
import { useAuth } from '@gmb/onchina-platform/authentication/useAuth';
import { normalizeScopeProvinceName, useScope } from '@gmb/onchina-platform/authorization/useScope';
import { TIER2_REGISTRY_CODE, isSubordinateRegistry, isTier1Registry } from '@gmb/onchina-platform/authorization/registryTier';
import { type AdminAuth } from '@gmb/onchina-platform/authentication/types';
import { type CidCityItem } from '@gmb/onchina-platform/codes/api';
import { type CityRegistryAdminRow, type FederalRegistryAdminRow, MAX_INSTITUTION_ADMINS, type RegistryAdminsSharedState, listCityRegistryAdmins, listFederalRegistryAdmins } from './cityRegistryAdminsApi';
import { glassCardHeadStyle, glassCardStyle } from '@gmb/onchina-platform/controls/cardStyles';
import { sameHexAccount } from '@gmb/onchina-platform/institutions/adminUtils';
import { tryEncodeSs58 } from '@gmb/onchina-platform/codes/ss58';
import { usePasskeyRegistration } from '@gmb/onchina-platform/authentication/usePasskey';
import { AddCityRegistryAdminModal } from './AddCityRegistryAdminModal';
import { FederalRegistryAdminSubTab } from './FederalRegistryAdminSubTab';
import { InstitutionAssignmentDetails, assignmentDisplayLabel, formatAdminBalanceFen } from '@gmb/onchina-platform/institutions/AssignmentCard';
import { GovDetailPage } from '../government/GovDetailPage';
import { getFederalRegistry, listOfficialInstitutions } from '../government/api';
import { type InstitutionDetail, type InstitutionListRow } from '@gmb/onchina-platform/institutions/api';
import { CID_MODAL_Z_INDEX } from '@gmb/onchina-platform/controls/modalStack';

import { loadCachedCidCities, readCachedCidCities } from '@gmb/onchina-platform/codes/cache';
import { notice } from '@gmb/onchina-platform/controls/notice';
// 注册局机构详情视图(两个顶级 tab 各一个):
//   - CityRegistryView(市注册局 tab):联邦注册局管理员 城市表格→点市→该市注册局机构详情页;
//                                      市注册局管理员 直接进本市注册局机构详情页。
//   - FederalRegistryView(联邦注册局 tab):全国唯一联邦注册局机构详情页。
// 两者 leaf 都是「机构详情页 + 管理员列表 tab」(GovDetailPage.adminListSection),
// 管理员列表由详情页左侧导航显示。数据由 RegistryAdminsView 统一加载。

interface RegistryViewProps {
  state: RegistryAdminsSharedState;
}

function makeCenteredTitle(center: React.ReactNode) {
  return (
    <div style={{ position: 'relative', display: 'flex', alignItems: 'center', minHeight: 32 }}>
      <span style={{ position: 'absolute', left: '50%', transform: 'translateX(-50%)' }}>
        {center}
      </span>
    </div>
  );
}

// ── 联邦注册局 tab:机构详情页 + 本省联邦注册局管理员列表 ──

export function FederalRegistryView({ state }: RegistryViewProps) {
  const { auth } = useAuth();
  const scope = useScope(auth);
  const {
    federalRegistryAdmins,
    federalRegistryAdminsLoading,
    selectedFederalRegistry,
    federalRegistryDetail,
    federalRegistryLoading,
  } = state;

  // 稳定引用:复用 RegistryAdminsView 预加载的 detail,避免 GovDetailPage 重复触发 load。
  const loadFederalRegistry = useCallback(() => {
    if (federalRegistryDetail) return Promise.resolve(federalRegistryDetail);
    if (auth) return getFederalRegistry(auth);
    return Promise.reject(new Error('未登录'));
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [federalRegistryDetail, auth?.accessToken]);

  if (!auth) return null;

  if (!federalRegistryDetail) {
    return (
      <Card bordered={false} style={glassCardStyle} headStyle={glassCardHeadStyle}>
        <Typography.Text type="secondary">
          {federalRegistryLoading ? '加载中...' : '暂无联邦注册局数据'}
        </Typography.Text>
      </Card>
    );
  }

  return (
    <GovDetailPage
      auth={auth}
      cidNumber={federalRegistryDetail.institution.cidNumber}
      canWrite={scope.canWrite && isTier1Registry(auth.institutionCode)}
      loadDetail={loadFederalRegistry}
      initialActiveKey="admins"
      adminListSection={(
        // tab 外壳恒渲染,不再随 selectedFederalRegistry(依赖读链定位)出现/消失;
        // 未定位/读链中时由 SubTab 内部显示加载态,tab 本身立即可见。
        <FederalRegistryAdminSubTab
          selectedFederalRegistry={selectedFederalRegistry}
          federalRegistryAdmins={federalRegistryAdmins}
          federalRegistryAdminsLoading={federalRegistryAdminsLoading}
          federalRegistryCidShortName={federalRegistryDetail.institution.cidShortName}
          institutionCidNumber={federalRegistryDetail.institution.cidNumber}
        />
      )}
    />
  );
}

// ── 市注册局 tab:城市表格(联邦) / 机构详情页 + 本市市注册局管理员列表 ──

export function CityRegistryView({ state }: RegistryViewProps) {
  const { auth } = useAuth();
  const scope = useScope(auth);
  const {
    selectedFederalRegistry,
    selectedCity,
    setSelectedCity,
    cityRegistryAdmins,
    cityRegistryAdminsLoading,
    cityRegistryAdminsError,
    refreshCityRegistryAdmins,
    cityRegistryAdminListPage,
    setCityRegistryListPage,
    cityRegistryAdminCities,
    cityRegistryAdminCitiesLoading,
    setAddCityRegistryOpen,
    onDeleteCityRegistry,
    cityRegistryCid,
    cityRegistryLoading,
    cityRegistryError,
    reloadCityRegistry,
  } = state;

  if (!auth) return null;

  const effectiveProvince = selectedFederalRegistry?.provinceName ?? scope.lockedProvinceName;
  const effectiveCity = selectedCity ?? scope.lockedCityName;
  // 名册只按当前详情页准确CID读取；不以本地行、城市同名或联邦管理员定位结果反推归属。
  const cityRegistryAdminsForProvince = cityRegistryAdmins;
  // 市注册局本机构列表只读；联邦办理仍须在签名前取得真实有效岗位及目标辖区授权。
  const canEditCityRegistryAdmins = scope.canWrite && isTier1Registry(auth.institutionCode);

  let body: React.ReactNode;
  if (!effectiveCity) {
    // 联邦注册局管理员未选市 → 城市表格(套 Card 显示省份标题)
    body = (
      <Card
        title={makeCenteredTitle(effectiveProvince ?? '市注册局')}
        bordered={false}
        style={glassCardStyle}
        headStyle={glassCardHeadStyle}
      >
        <CityRegistryListTable
          auth={auth}
          provinceName={effectiveProvince ?? ''}
          cities={cityRegistryAdminCities.filter((c) => c.cityCode !== '000')}
          citiesLoading={cityRegistryAdminCitiesLoading}
          onSelectCity={setSelectedCity}
        />
      </Card>
    );
  } else if (cityRegistryCid) {
    // 选中市(联邦) / 锁定市(市注册局管理员) → 市注册局机构详情页 + 本市市注册局管理员列表
    const canGoBack = !scope.skipCityList;
    body = (
      <GovDetailPage
        auth={auth}
        cidNumber={cityRegistryCid}
        canWrite={scope.canWrite}
        onBack={canGoBack ? () => { setSelectedCity(null); setCityRegistryListPage(1); } : undefined}
        backLabel="返回列表"
        adminListSection={
          <CityRegistryAdminsView
            institutionCidNumber={cityRegistryCid}
            canEditCityRegistryAdmins={canEditCityRegistryAdmins}
            cityRegistryAdmins={cityRegistryAdminsForProvince.filter((op) => op.institutionCidNumber === cityRegistryCid)}
            error={cityRegistryAdminsError}
            onRetry={() => void refreshCityRegistryAdmins().catch(() => {})}
            cityRegistryAdminsLoading={cityRegistryAdminsLoading}
            cityRegistryAdminListPage={cityRegistryAdminListPage}
            setCityRegistryListPage={setCityRegistryListPage}
            setAddCityRegistryOpen={setAddCityRegistryOpen}
            onDeleteCityRegistry={onDeleteCityRegistry}
          />
        }
      />
    );
  } else {
    // effectiveCity 必有对应市注册局(CITY_TEMPLATES 必生成);cid 解析失败会单独 toast 报错。
    body = (
      <Card bordered={false} style={glassCardStyle} headStyle={glassCardHeadStyle}>
        {cityRegistryLoading ? <Typography.Text type="secondary">加载中...</Typography.Text> : <Alert type="error" message="市注册局读取失败" description={cityRegistryError ?? '当前页面目标未就绪'} action={<Button onClick={reloadCityRegistry}>重试</Button>} />}
      </Card>
    );
  }

  return (
    <>
      {body}
      <AddCityRegistryAdminModal key={`${auth.accessToken}|${auth.accountId}|${cityRegistryCid}`} state={state} />
    </>
  );
}

// ── 市注册局城市入口表格 ──

const CITY_REGISTRY_PAGE_SIZE = 20;

function areaText(row: InstitutionListRow | null, provinceName: string, cityName: string) {
  if (!row) return [provinceName, cityName].filter(Boolean).join('/') || '-';
  return [row.provinceName, row.cityName, row.townName].filter(Boolean).join('/') || '-';
}

// 机构名只取 cid_full_name/cid_short_name 单一真源;查无机构行时显示「-」,绝不用 city_name 伪造名字。
function nameText(row: InstitutionListRow | null) {
  return row?.cidFullName || row?.cidShortName || '-';
}

function CityRegistryListTable({ auth, provinceName, cities, citiesLoading, onSelectCity }: {
  auth: AdminAuth;
  provinceName: string;
  cities: CidCityItem[];
  citiesLoading: boolean;
  onSelectCity: (cityName: string) => void;
}) {
  const [registryRows, setRegistryRows] = useState<InstitutionListRow[]>([]);
  const [registryLoading, setRegistryLoading] = useState(false);
  const [page, setPage] = useState(1);

  useEffect(() => {
    if (!provinceName) {
      setRegistryRows([]);
      setRegistryLoading(false);
      return;
    }
    let cancelled = false;
    setRegistryLoading(true);
    listOfficialInstitutions(auth, { provinceName, institutionCode: TIER2_REGISTRY_CODE, pageSize: 300 })
      .then((res) => {
        if (!cancelled) {
          setRegistryRows(res.items);
        }
      })
      .catch(() => {
        if (!cancelled) setRegistryRows([]);
      })
      .finally(() => {
        if (!cancelled) setRegistryLoading(false);
      });
    return () => {
      cancelled = true;
    };
  }, [auth.accessToken, provinceName]);

  useEffect(() => {
    setPage(1);
  }, [provinceName, cities.length]);

  const rows = useMemo(() => {
    return cities.map((cityItem) => {
      const registry = registryRows.find((row) => row.cityName === cityItem.cityName) ?? null;
      return { cityItem, registry };
    });
  }, [cities, registryRows]);

  const totalPages = Math.max(1, Math.ceil(rows.length / CITY_REGISTRY_PAGE_SIZE));
  const displayRows = rows.slice(
    (page - 1) * CITY_REGISTRY_PAGE_SIZE,
    page * CITY_REGISTRY_PAGE_SIZE,
  );

  const columns = useMemo<ColumnsType<(typeof rows)[number]>>(() => [
    {
      title: '序号',
      width: 70,
      align: 'center',
      render: (_v, _row, index) => (page - 1) * CITY_REGISTRY_PAGE_SIZE + index + 1,
    },
    {
      title: '身份CID号',
      width: 260,
      align: 'center',
      render: (_v, row) => <IdentityCidText value={row.registry?.cidNumber ?? '-'} />,
    },
    {
      title: '市注册局名称',
      width: 180,
      align: 'center',
      render: (_v, row) => nameText(row.registry),
    },
    {
      title: '所属行政区',
      width: 180,
      align: 'center',
      render: (_v, row) => areaText(row.registry, provinceName, row.cityItem.cityName),
    },
    {
      title: '管理员', width: 150, align: 'center', render: () => '进入详情查看链上名册',
    },
  ], [page, provinceName]);

  if (!citiesLoading && cities.length === 0) {
    return <Typography.Text type="secondary">暂无城市数据</Typography.Text>;
  }

  return (
    <div>
      <Table<(typeof rows)[number]>
        rowKey={(row) => row.cityItem.cityCode}
        loading={citiesLoading || registryLoading}
        dataSource={displayRows}
        pagination={false}
        onRow={(row) => ({
          onClick: () => row.registry && onSelectCity(row.cityItem.cityName),
          style: row.registry ? { cursor: 'pointer' } : { color: '#94a3b8' },
        })}
        columns={columns}
      />
      <Space style={{ marginTop: 12 }} wrap>
        <Typography.Text type="secondary">
          共 {totalPages} 页 / 第 {page} 页
        </Typography.Text>
        <Typography.Text type="secondary">共 {rows.length} 条</Typography.Text>
        <Button disabled={citiesLoading || registryLoading || page <= 1} onClick={() => setPage((p) => Math.max(1, p - 1))}>
          上一页
        </Button>
        <Button disabled={citiesLoading || registryLoading || page >= totalPages} onClick={() => setPage((p) => Math.min(totalPages, p + 1))}>
          下一页
        </Button>
      </Space>
    </div>
  );
}

// ── 某市的市注册局管理员列表(显示在市注册局机构详情页的“管理员列表”tab) ──

function CityRegistryAdminsView({ institutionCidNumber, error, onRetry, canEditCityRegistryAdmins, cityRegistryAdmins, cityRegistryAdminsLoading, cityRegistryAdminListPage, setCityRegistryListPage, setAddCityRegistryOpen, onDeleteCityRegistry }: {
  institutionCidNumber: string;
  error: string | null;
  onRetry: () => void;
  canEditCityRegistryAdmins: boolean;
  cityRegistryAdmins: CityRegistryAdminRow[];
  cityRegistryAdminsLoading: boolean;
  cityRegistryAdminListPage: number;
  setCityRegistryListPage: (v: number) => void;
  setAddCityRegistryOpen: (v: boolean) => void;
  onDeleteCityRegistry: (row: CityRegistryAdminRow) => void;
}) {
  const { auth } = useAuth();
  const { registered: passkeyRegistered, busy: passkeyBusy, register: doRegisterPasskey } =
    usePasskeyRegistration();
  const [detailTarget, setDetailTarget] = useState<CityRegistryAdminRow | null>(null);
  // 本列表按准确身份CID号读取实际钱包；钱包登记不变更岗位人员，完整维护入口在机构治理页。
  const cityLimitReached = cityRegistryAdmins.length >= MAX_INSTITUTION_ADMINS;
  return (
    <Card
      type="inner"
      title="市注册局管理员列表"
      extra={
        <Space size="middle" align="center">
          <Typography.Text type="secondary" style={{ fontWeight: 400, fontSize: 13 }}>
            用户数：{error ? '读取失败' : cityRegistryAdminsLoading ? '读取中' : `${cityRegistryAdmins.length} / ${MAX_INSTITUTION_ADMINS}`}
          </Typography.Text>
          {canEditCityRegistryAdmins ? (
            <Button
              type="primary"
              disabled={cityLimitReached || cityRegistryAdminsLoading || !!error}
              title={cityLimitReached ? `本市市注册局管理员已满 ${MAX_INSTITUTION_ADMINS} 人` : undefined}
              onClick={() => setAddCityRegistryOpen(true)}
            >
              新增市注册局管理员
            </Button>
          ) : null}
        </Space>
      }
    >
      <>
        {error && <Alert type="error" message="链上管理员读取失败" description={error} action={<Button onClick={onRetry}>重试</Button>} />}
        <InstitutionWalletRecoveryActions institutionCidNumber={institutionCidNumber} onSubmitted={onRetry}/>
        <Table<CityRegistryAdminRow>
          rowKey={(r) => `${r.id}-${r.accountId}`}
          scroll={{ x: 1100 }}
          loading={cityRegistryAdminsLoading}
          dataSource={cityRegistryAdmins}
          pagination={{
            pageSize: 10, current: cityRegistryAdminListPage,
            onChange: (page) => setCityRegistryListPage(page),
            showSizeChanger: false,
            showTotal: (total) => `共 ${total} 条`,
          }}
          columns={[
            {
              title: '序号',
              width: 72,
              align: 'center' as const,
              render: (_v, _row, index) => (cityRegistryAdminListPage - 1) * 10 + index + 1,
            },
            {
              title: '岗位',
              dataIndex: 'roleName',
              width: 140,
              align: 'center' as const,
            },
            {
              title: '姓名',
              dataIndex: 'name',
              render: (_v, row) => assignmentDisplayLabel(row) || '-',
            },
            {
              title: '钱包账户',
              dataIndex: 'accountId',
              width: 360,
              render: (value: string) => (
                <span style={{ fontSize: 11, fontFamily: 'monospace', wordBreak: 'break-all' }}>
                  {tryEncodeSs58(value)}
                </span>
              ),
            },
            {
              title: '余额',
              dataIndex: 'balanceFen',
              width: 160,
              align: 'right' as const,
              render: (_v, row) => formatAdminBalanceFen(row.balanceFen) || '-',
            },
            {
              title: '操作',
              width: 220,
              align: 'center' as const,
              render: (_v: unknown, row: CityRegistryAdminRow) => {
                const isSelf = sameHexAccount(row.accountId, auth?.accountId);
                return (
                  <Space onClick={(event) => event.stopPropagation()}>
                    {canEditCityRegistryAdmins ? <Button size="small" danger onClick={() => onDeleteCityRegistry(row)}>删除</Button> : null}
                    <InstitutionWalletSelfAction accountId={row.accountId} institutionCidNumber={row.institutionCidNumber} onSubmitted={onRetry}/>
                    {/* passkey 登录密钥 self-only:只能为当前登录管理员自己注册本机认证器。 */}
                    {isSelf ? (
                      <Badge dot={passkeyRegistered === false} size="small">
                        <Button size="small" icon={<KeyOutlined />} loading={passkeyBusy} onClick={doRegisterPasskey}>
                          密钥
                        </Button>
                      </Badge>
                    ) : null}
                  </Space>
                );
              },
            },
          ]}
          onRow={(row) => ({
            onClick: () => setDetailTarget(row),
            style: { cursor: 'pointer' },
          })}
        />
        <Modal
          title="管理员完整信息"
          open={!!detailTarget}
          footer={null}
          centered
          onCancel={() => setDetailTarget(null)}
          zIndex={CID_MODAL_Z_INDEX.business}
        >
          {detailTarget ? (
            <InstitutionAssignmentDetails assignment={detailTarget} areaLabel="城市" areaValue={detailTarget.cityName} />
          ) : null}
        </Modal>
      </>
    </Card>
  );
}



























// 原注册局页面及交互整体迁入，组件内容保持。
export interface RegistryAdminsViewProps {
  /// 'city-registry' = 市注册局 tab(城市表格→市注册局机构详情页);
  /// 'federal-registry' = 联邦注册局 tab(联邦注册局机构详情页)
  mode: 'city-registry' | 'federal-registry';
}



const ADMIN_LIST_CACHE_ID = 'cid-admin-list';

interface CachedAdminListPayload<T> {
  cacheId: string;
  rows: T[];
}

function adminListCacheKey(
  kind: 'federal-registry-admins' | 'city-registry-admins',
  auth: AdminAuth,
  mode: RegistryAdminsViewProps['mode'],
): string {
  return [
    'cid:admin-list',
    ADMIN_LIST_CACHE_ID,
    kind,
    auth.accountId,
    auth.institutionCode,
    normalizeScopeProvinceName(auth.scopeProvinceName) || 'NO_PROVINCE_SCOPE',
    auth.scopeCityName || 'NO_CITY_SCOPE',
    mode,
  ].join(':');
}

function readCachedAdminList<T>(key: string): T[] | null {
  try {
    const raw = localStorage.getItem(key);
    if (!raw) return null;
    const parsed = JSON.parse(raw) as CachedAdminListPayload<T>;
    if (parsed.cacheId !== ADMIN_LIST_CACHE_ID || !Array.isArray(parsed.rows)) {
      localStorage.removeItem(key);
      return null;
    }
    return parsed.rows;
  } catch {
    localStorage.removeItem(key);
    return null;
  }
}

function writeCachedAdminList<T>(key: string, rows: T[]) {
  try {
    localStorage.setItem(
      key,
      JSON.stringify({ cacheId: ADMIN_LIST_CACHE_ID, rows } satisfies CachedAdminListPayload<T>),
    );
  } catch {
    // 注册局管理员列表缓存只是减少重复转圈,写失败不能影响业务操作。
  }
}

export function RegistryAdminsView({ mode }: RegistryAdminsViewProps) {
  const { auth } = useAuth();
  const authContext = `${auth?.accessToken}|${auth?.accountId}|${auth?.institutionCidNumber}`;
  const currentAuth = useRef(authContext); currentAuth.current = authContext;

  const [federalRegistryAdmins, setFederalRegistryAdmins] = useState<FederalRegistryAdminRow[]>([]);
  const [federalRegistryAdminsLoading, setFederalRegistryAdminsLoading] = useState(false);
  const [selectedFederalRegistry, setSelectedFederalRegistry] = useState<FederalRegistryAdminRow | null>(null);
  const [selectedCity, setSelectedCity] = useState<string | null>(null);
  const [adminDetailTab, setAdminDetailTab] = useState<'city-registry-admin' | 'federal-registry-admin'>('city-registry-admin');
  const adminDetailTabRef = useRef<'city-registry-admin' | 'federal-registry-admin'>('city-registry-admin');
  const lastSelectedFederalRegistryKey = useRef<string | null>(null);

  const [cityRegistryAdmins, setCityRegistryAdmins] = useState<CityRegistryAdminRow[]>([]);
  const [cityRegistryAdminsLoading, setCityRegistryAdminsLoading] = useState(false);
  const [cityRegistryAdminListPage, setCityRegistryListPage] = useState(1);

  const initialScopeProvinceName = normalizeScopeProvinceName(auth?.scopeProvinceName);
  const initialCityRegistryCities = initialScopeProvinceName
    ? readCachedCidCities(initialScopeProvinceName)
    : null;
  const [cityRegistryAdminCities, setCityRegistryCities] = useState<CidCityItem[]>(initialCityRegistryCities ?? []);
  const [cityRegistryAdminCitiesLoading, setCityRegistryCitiesLoading] = useState(
    !!initialScopeProvinceName && !initialCityRegistryCities,
  );

  const [addCityRegistryOpen, setAddCityRegistryOpen] = useState(false);
  const [deleteCityRegistryAdmin, setDeleteCityRegistryAdmin] = useState<CityRegistryAdminRow | null>(null);
  const [cityRegistryAdminsError, setCityRegistryAdminsError] = useState<string | null>(null);
  const cityRequest = useRef(0);

  // ── 注册局机构详情页数据源(任务卡 20260608) ──
  // 联邦注册局:全国唯一机构,走 scope-bypass 接口,所有联邦注册局管理员可读。
  const [federalRegistryDetail, setFederalRegistryDetail] = useState<InstitutionDetail | null>(null);
  const [federalRegistryLoading, setFederalRegistryLoading] = useState(false);
  // 市注册局:当前活动市对应的机构 cid_number。
  const [cityRegistryTarget, setCityRegistryTarget] = useState<{ scope: string; cid: string } | null>(null);
  const [cityRegistryLoading, setCityRegistryLoading] = useState(false);
  const [cityRegistryError, setCityRegistryError] = useState<string | null>(null);
  const [targetRevision, setTargetRevision] = useState(0);

  // 活动省/市:联邦注册局管理员看 selectedFederalRegistry + selectedCity;市注册局管理员锁定本省本市。
  const activeProvince = selectedFederalRegistry?.provinceName ?? normalizeScopeProvinceName(auth?.scopeProvinceName);
  const activeCity = selectedCity ?? (isSubordinateRegistry(auth?.institutionCode) ? auth?.scopeCityName ?? null : null);

  useEffect(() => {
    adminDetailTabRef.current = adminDetailTab;
  }, [adminDetailTab]);

  // ── 数据加载 ──

  const refreshFederalRegistryAdmins = async (): Promise<FederalRegistryAdminRow[]> => {
    if (!auth) return [];
    const owner = authContext;
    const cacheKey = adminListCacheKey('federal-registry-admins', auth, mode);
    const cached = readCachedAdminList<FederalRegistryAdminRow>(cacheKey);
    if (cached !== null) {
      setFederalRegistryAdmins(cached);
      setFederalRegistryAdminsLoading(false);
    } else {
      setFederalRegistryAdminsLoading(true);
    }
    try {
      const rows = await listFederalRegistryAdmins(auth);
      const list = Array.isArray(rows) ? rows : [];
      if (owner !== currentAuth.current) return [];
      setFederalRegistryAdmins(list);
      writeCachedAdminList(cacheKey, list);
      return list;
    } catch (err) {
      if (owner === currentAuth.current) notice.error(err, '加载联邦注册局管理员失败');
      return [];
    } finally {
      if (owner === currentAuth.current) setFederalRegistryAdminsLoading(false);
    }
  };

  const targetScope = `${auth?.accessToken}|${auth?.accountId}|${activeProvince}|${activeCity}`;
  const cityRegistryCid = cityRegistryTarget?.scope === targetScope ? cityRegistryTarget.cid : null;
  const rosterContext = `${auth?.accessToken}|${auth?.accountId}|${cityRegistryCid}|${activeProvince}|${activeCity}`;
  const rosterContextRef = useRef(rosterContext);
  rosterContextRef.current = rosterContext;
  const refreshCityRegistryAdmins = useCallback(async (): Promise<CityRegistryAdminRow[]> => {
    if (!auth || !cityRegistryCid) return [];
    const request = ++cityRequest.current;
    const context = rosterContext;
    setCityRegistryAdminsLoading(true);
    setCityRegistryAdminsError(null);
    try {
      const rows = await listCityRegistryAdmins(auth, cityRegistryCid);
      if (request === cityRequest.current && context === rosterContextRef.current) setCityRegistryAdmins(rows);
      return rows;
    } catch (error) {
      if (request === cityRequest.current && context === rosterContextRef.current) {
        setCityRegistryAdmins([]);
        setCityRegistryAdminsError(error instanceof Error ? error.message : String(error));
      }
      throw error;
    } finally {
      if (request === cityRequest.current && context === rosterContextRef.current) setCityRegistryAdminsLoading(false);
    }
  }, [auth?.accessToken, auth?.accountId, cityRegistryCid, rosterContext]);

  useEffect(() => {
    setCityRegistryAdmins([]);
    setCityRegistryAdminsError(null);
    setAddCityRegistryOpen(false);
    setDeleteCityRegistryAdmin(null);
    if (mode === 'city-registry' && cityRegistryCid) void refreshCityRegistryAdmins().catch(() => {});
    return () => { cityRequest.current++; };
  }, [refreshCityRegistryAdmins, mode]);

  useEffect(() => {
    setSelectedFederalRegistry(null); setSelectedCity(null); setFederalRegistryAdmins([]);
    lastSelectedFederalRegistryKey.current = null;
  }, [authContext]);

  // 首次挂载 / auth 变化时加载数据。
  // 角色分流由 ProvinceDetailView + useScope 自动处理,这里只负责加载数据
  // 和按当前登录角色定位 selectedFederalRegistry。
  useEffect(() => {
    let cancelled = false;
    const init = async () => {
      if (!auth) return;
      // 联邦只读目录用于当前省显示；市名册等当前页面CID就绪后单独读取。
      const rows = await refreshFederalRegistryAdmins();
      if (cancelled) return;
      // 自动定位到当前登录角色所属省的 FederalRegistry
      {
        let target: FederalRegistryAdminRow | null = null;
        if (isTier1Registry(auth.institutionCode)) {
          target = rows.find((r) => sameHexAccount(r.accountId, auth.accountId)) || null;
        }
        if (!cancelled && target) setSelectedFederalRegistry(target);
      }
    };
    void init();
    return () => { cancelled = true; };
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [authContext, mode]);

  // 切换省域时:
  //   1. 预加载该省份的城市列表
  //   2. 重置 sub-tab 到默认(市注册局管理员列表)
  //   3. 重置市注册局管理员列表分页到第 1 页
  useEffect(() => {
    if (!auth) {
      setCityRegistryCities([]);
      setCityRegistryCitiesLoading(false);
      return;
    }
    const provinceForCities = selectedFederalRegistry?.provinceName ?? normalizeScopeProvinceName(auth.scopeProvinceName);
    if (!provinceForCities) {
      lastSelectedFederalRegistryKey.current = null;
      setCityRegistryCities([]);
      setCityRegistryCitiesLoading(false);
      return;
    }
    const selectedKey = selectedFederalRegistry
      ? `${selectedFederalRegistry.provinceName}:${selectedFederalRegistry.accountId}`
      : `${provinceForCities}:auth-scope`;
    const isRealProvinceSwitch = lastSelectedFederalRegistryKey.current !== null
      && lastSelectedFederalRegistryKey.current !== selectedKey;
    lastSelectedFederalRegistryKey.current = selectedKey;
    const cachedRows = readCachedCidCities(provinceForCities);
    if (cachedRows) {
      setCityRegistryCities(cachedRows);
      setCityRegistryCitiesLoading(false);
    } else {
      setCityRegistryCities([]);
      setCityRegistryCitiesLoading(true);
    }
    // 首次自动定位当前省时不能覆盖用户刚点击的“联邦注册局管理员列表”。
    // 只有用户真正切换到另一个省时,才把子页签重置回市列表。
    if (isRealProvinceSwitch && adminDetailTabRef.current !== 'federal-registry-admin') {
      setAdminDetailTab('city-registry-admin');
    }
    setCityRegistryListPage(1);
    let cancelled = false;
    loadCachedCidCities(auth, provinceForCities)
      .then((rows) => {
        if (!cancelled) setCityRegistryCities(rows);
      })
      .catch(() => {
        if (!cancelled) setCityRegistryCities([]);
      })
      .finally(() => {
        if (!cancelled) setCityRegistryCitiesLoading(false);
      });
    return () => {
      cancelled = true;
    };
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [selectedFederalRegistry?.accountId, auth?.accessToken, auth?.scopeProvinceName]);

  // ── 联邦注册局机构详情:进入联邦注册局 tab 时加载一次(scope-bypass) ──
  useEffect(() => {
    if (!auth || mode !== 'federal-registry') return;
    let cancelled = false;
    setFederalRegistryLoading(true);
    getFederalRegistry(auth)
      .then((d) => { if (!cancelled) setFederalRegistryDetail(d); })
      .catch((err) => { if (!cancelled) notice.error(err, '加载联邦注册局失败'); })
      .finally(() => { if (!cancelled) setFederalRegistryLoading(false); });
    return () => { cancelled = true; };
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [auth?.accessToken, mode]);

  // ── 市注册局机构 cid 解析:从该市公权机构目录里筛 institution_code='CREG' 那条 ──
  useEffect(() => {
    if (!auth || mode !== 'city-registry' || !activeProvince || !activeCity) {
      setCityRegistryTarget(null);
      setCityRegistryLoading(false);
      return;
    }
    let cancelled = false;
    setCityRegistryTarget(null);
    setCityRegistryLoading(true);
    setCityRegistryError(null);
    listOfficialInstitutions(auth, { provinceName: activeProvince, cityName: activeCity, institutionCode: TIER2_REGISTRY_CODE, pageSize: 300 })
      .then((res) => {
        if (cancelled) return;
        const rows = res.items.filter((r) => isSubordinateRegistry(r.institutionCode));
        if (rows.length !== 1) throw new Error('当前市注册局目标缺失或不唯一');
        setCityRegistryTarget({ scope: targetScope, cid: rows[0].cidNumber });
      })
      .catch((err) => { if (!cancelled) setCityRegistryError(err instanceof Error ? err.message : String(err)); })
      .finally(() => { if (!cancelled) setCityRegistryLoading(false); });
    return () => { cancelled = true; };
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [auth?.accessToken, auth?.accountId, mode, activeProvince, activeCity, targetScope, targetRevision]);

  const onDeleteCityRegistry = (row: CityRegistryAdminRow) => {
    if (!cityRegistryCid || row.institutionCidNumber !== cityRegistryCid) return;
    setDeleteCityRegistryAdmin(row);
    setAddCityRegistryOpen(true);
  };

  // ── 组装共享状态 ──

  const shared: RegistryAdminsSharedState = {
    federalRegistryAdmins,
    federalRegistryAdminsLoading,
    refreshFederalRegistryAdmins,
    selectedFederalRegistry,
    setSelectedFederalRegistry,
    selectedCity,
    setSelectedCity,
    adminDetailTab,
    setAdminDetailTab,
    cityRegistryAdmins,
    cityRegistryAdminsLoading,
    cityRegistryAdminListPage,
    setCityRegistryListPage,
    cityRegistryAdminCities,
    cityRegistryAdminCitiesLoading,
    addCityRegistryOpen,
    setAddCityRegistryOpen: (open) => { if (!open) setDeleteCityRegistryAdmin(null); setAddCityRegistryOpen(open); },
    deleteCityRegistryAdmin,
    setDeleteCityRegistryAdmin,
    refreshCityRegistryAdmins,
    cityRegistryAdminsError,
    onDeleteCityRegistry,
    federalRegistryDetail,
    federalRegistryLoading,
    cityRegistryCid,
    cityRegistryLoading,
    cityRegistryError,
    reloadCityRegistry: () => setTargetRevision(r => r + 1),
  };

  // ── 渲染:按 mode 分派 ──

  const content = mode === 'city-registry'
    ? <CityRegistryView state={shared} />
    : <FederalRegistryView state={shared} />;

  return (
    <>
      {content}
    </>
  );
}

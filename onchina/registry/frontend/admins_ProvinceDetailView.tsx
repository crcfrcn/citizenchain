import React, { useCallback, useEffect, useMemo, useRef, useState } from 'react';
import { Badge, Button, Card, Form, Modal, type ModalProps, Space, Table, Tag, Typography } from 'antd';
import { KeyOutlined } from '@ant-design/icons';
import { type ColumnsType } from 'antd/es/table';
import { useAuth } from '../../platform/frontend/hooks_useAuth';
import { normalizeScopeProvinceName, useScope } from '../../platform/frontend/hooks_useScope';
import { TIER2_REGISTRY_CODE, isSubordinateRegistry, isTier1Registry } from '../../platform/frontend/platform_registryTier';
import { type AdminAuth } from '../../platform/frontend/authentication_types';
import { type CidCityItem } from '../../platform/frontend/china_api';
import { type AccountScanTarget, type CityRegistryAdminRow, type FederalRegistryAdminRow, MAX_CITY_REGISTRY_ADMINS_PER_CITY, type RegistryAdminsSharedState, listCityRegistryAdmins, listFederalRegistryAdmins } from './admins_cityRegistryAdminsApi';
import { glassCardHeadStyle, glassCardStyle } from '../../platform/frontend/core_cardStyles';
import { sameHexAccount } from '../../platform/frontend/admins_adminUtils';
import { usePasskeyRegistration } from '../../platform/frontend/authentication_usePasskey';
import { AddCityRegistryAdminModal } from './admins_AddCityRegistryAdminModal';
import { FederalRegistryAdminSubTab } from './admins_FederalRegistryAdminSubTab';
import { InstitutionAssignmentDetails, assignmentDisplayLabel, formatAdminBalanceFen } from '../../platform/frontend/admins_InstitutionAssignmentCard';
import { GovDetailPage } from './government_GovDetailPage';
import { getFederalRegistry, listOfficialInstitutions } from './government_api';
import { type InstitutionDetail, type InstitutionListRow } from '../../platform/frontend/subjects_api';
import { CID_MODAL_Z_INDEX } from '../../platform/frontend/core_modalStack';

import { type AdminActionType, commitAdminAction, formatAdminCreateError, prepareAdminAction } from '../../platform/frontend/admins_securityApi';
import { loadCachedCidCities, readCachedCidCities } from '../../platform/frontend/china_metaCache';
import { decodeSs58 } from '../../platform/frontend/helpers_ss58';
import { parseSignedReceiptPayload } from '../../platform/frontend/helpers_parseSignedPayload';
import { CitizenSignatureModal } from '../../platform/frontend/core_CitizenSignatureModal';
import { notice } from '../../platform/frontend/helpers_notice';
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
  }, [federalRegistryDetail, auth?.access_token]);

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
      cidNumber={federalRegistryDetail.institution.cid_number}
      canWrite={scope.canWrite && isTier1Registry(auth.institution_code)}
      loadDetail={loadFederalRegistry}
      initialActiveKey="admins"
      adminListSection={(
        // tab 外壳恒渲染,不再随 selectedFederalRegistry(依赖读链定位)出现/消失;
        // 未定位/读链中时由 SubTab 内部显示加载态,tab 本身立即可见。
        <FederalRegistryAdminSubTab
          selectedFederalRegistry={selectedFederalRegistry}
          federalRegistryAdmins={federalRegistryAdmins}
          federalRegistryAdminsLoading={federalRegistryAdminsLoading}
          federalRegistryCidShortName={federalRegistryDetail.institution.cid_short_name}
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
    cityRegistryAdminListPage,
    setCityRegistryListPage,
    cityRegistryAdminCities,
    cityRegistryAdminCitiesLoading,
    setAddCityRegistryOpen,
    onDeleteCityRegistry,
    cityRegistryCid,
  } = state;

  if (!auth) return null;

  const effectiveProvince = selectedFederalRegistry?.province_name ?? scope.lockedProvinceName;
  const effectiveCity = selectedCity ?? scope.lockedCityName;
  // 市注册局管理员列表由后端按登录省域过滤;前端不能再依赖
  // selectedFederalRegistry 是否自动定位成功,否则会把合法省域误渲染为空列表。
  const cityRegistryAdminsForProvince = cityRegistryAdmins;
  // 市注册局管理员只读;Tier1 创世注册局管理员可增删改(后端按登录省域二次校验)。
  const canEditCityRegistryAdmins = scope.canWrite && isTier1Registry(auth.institution_code);

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
          province_name={effectiveProvince ?? ''}
          cities={cityRegistryAdminCities.filter((c) => c.city_code !== '000')}
          citiesLoading={cityRegistryAdminCitiesLoading}
          cityRegistryAdmins={cityRegistryAdminsForProvince}
          cityRegistryAdminsLoading={cityRegistryAdminsLoading}
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
            canEditCityRegistryAdmins={canEditCityRegistryAdmins}
            cityRegistryAdmins={cityRegistryAdminsForProvince.filter((op) => op.city_name === effectiveCity)}
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
        <Typography.Text type="secondary">加载中...</Typography.Text>
      </Card>
    );
  }

  return (
    <>
      {body}
      <AddCityRegistryAdminModal state={state} />
    </>
  );
}

// ── 市注册局城市入口表格 ──

const CITY_REGISTRY_PAGE_SIZE = 20;

function areaText(row: InstitutionListRow | null, province_name: string, city_name: string) {
  if (!row) return [province_name, city_name].filter(Boolean).join('/') || '-';
  return [row.province_name, row.city_name, row.town_name].filter(Boolean).join('/') || '-';
}

// 机构名只取 cid_full_name/cid_short_name 单一真源;查无机构行时显示「-」,绝不用 city_name 伪造名字。
function nameText(row: InstitutionListRow | null) {
  return row?.cid_full_name || row?.cid_short_name || '-';
}

function CityRegistryListTable({ auth, province_name, cities, citiesLoading, cityRegistryAdmins, cityRegistryAdminsLoading, onSelectCity }: {
  auth: AdminAuth;
  province_name: string;
  cities: CidCityItem[];
  citiesLoading: boolean;
  cityRegistryAdmins: CityRegistryAdminRow[];
  cityRegistryAdminsLoading: boolean;
  onSelectCity: (city_name: string) => void;
}) {
  const [registryRows, setRegistryRows] = useState<InstitutionListRow[]>([]);
  const [registryLoading, setRegistryLoading] = useState(false);
  const [page, setPage] = useState(1);

  useEffect(() => {
    if (!province_name) {
      setRegistryRows([]);
      setRegistryLoading(false);
      return;
    }
    let cancelled = false;
    setRegistryLoading(true);
    listOfficialInstitutions(auth, { province_name, institution_code: TIER2_REGISTRY_CODE, page_size: 300 })
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
  }, [auth.access_token, province_name]);

  useEffect(() => {
    setPage(1);
  }, [province_name, cities.length]);

  const rows = useMemo(() => {
    return cities.map((city_item) => {
      const registry = registryRows.find((row) => row.city_name === city_item.city_name) ?? null;
      const adminCount = cityRegistryAdmins.filter((admin) => admin.city_name === city_item.city_name).length;
      return { city_item, registry, adminCount };
    });
  }, [cities, registryRows, cityRegistryAdmins]);

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
      title: '身份ID',
      width: 260,
      align: 'center',
      render: (_v, row) => row.registry?.cid_number ?? '-',
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
      render: (_v, row) => areaText(row.registry, province_name, row.city_item.city_name),
    },
    {
      title: '管理员数',
      width: 150,
      align: 'center',
      render: (_v, row) => (
        <Tag color={row.adminCount >= MAX_CITY_REGISTRY_ADMINS_PER_CITY ? 'red' : 'teal'}>
          {row.adminCount} / {MAX_CITY_REGISTRY_ADMINS_PER_CITY}
        </Tag>
      ),
    },
  ], [page, province_name]);

  if (!citiesLoading && cities.length === 0) {
    return <Typography.Text type="secondary">暂无城市数据</Typography.Text>;
  }

  return (
    <div>
      <Table<(typeof rows)[number]>
        rowKey={(row) => row.city_item.city_code}
        loading={citiesLoading || registryLoading || cityRegistryAdminsLoading}
        dataSource={displayRows}
        pagination={false}
        onRow={(row) => ({
          onClick: () => row.registry && onSelectCity(row.city_item.city_name),
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

function CityRegistryAdminsView({ canEditCityRegistryAdmins, cityRegistryAdmins, cityRegistryAdminsLoading, cityRegistryAdminListPage, setCityRegistryListPage, setAddCityRegistryOpen, onDeleteCityRegistry }: {
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
  // 本列表已经按当前市过滤,所以长度就是该市市注册局管理员数量。
  const cityLimitReached = cityRegistryAdmins.length >= MAX_CITY_REGISTRY_ADMINS_PER_CITY;
  return (
    <Card
      type="inner"
      title="市注册局管理员列表"
      extra={
        <Space size="middle" align="center">
          <Typography.Text type="secondary" style={{ fontWeight: 400, fontSize: 13 }}>
            用户数：{cityRegistryAdmins.length} / {MAX_CITY_REGISTRY_ADMINS_PER_CITY}
          </Typography.Text>
          {canEditCityRegistryAdmins ? (
            <Button
              type="primary"
              disabled={cityLimitReached}
              title={cityLimitReached ? `本市市注册局管理员已满 ${MAX_CITY_REGISTRY_ADMINS_PER_CITY} 人` : undefined}
              onClick={() => setAddCityRegistryOpen(true)}
            >
              新增市注册局管理员
            </Button>
          ) : null}
        </Space>
      }
    >
      <>
        <Table<CityRegistryAdminRow>
          rowKey={(r) => `${r.id}-${r.account_id}`}
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
              title: '城市',
              dataIndex: 'city_name',
              width: 140,
              align: 'center' as const,
            },
            {
              title: '姓名',
              dataIndex: 'name',
              render: (_v, row) => assignmentDisplayLabel(row) || '-',
            },
            {
              title: '余额',
              dataIndex: 'balance_fen',
              width: 160,
              align: 'right' as const,
              render: (_v, row) => formatAdminBalanceFen(row.balance_fen) || '-',
            },
            {
              title: '操作',
              width: 220,
              align: 'center' as const,
              render: (_v: unknown, row: CityRegistryAdminRow) => {
                const isSelf = sameHexAccount(row.account_id, auth?.account_id);
                return (
                  <Space onClick={(event) => event.stopPropagation()}>
                    {canEditCityRegistryAdmins ? <Button size="small" danger onClick={() => onDeleteCityRegistry(row)}>删除</Button> : null}
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
            <InstitutionAssignmentDetails assignment={detailTarget} areaLabel="城市" areaValue={detailTarget.city_name} />
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

type AdminActionModalState = {
  actionId: string;
  signRequest: string;
  payloadHash: string;
  actionType: AdminActionType;
  resolve: (value: unknown) => void;
  reject: (reason?: unknown) => void;
};

const centeredConfirmFooter: ModalProps['footer'] = (_originNode, { OkBtn, CancelBtn }) => (
  <div style={{ display: 'flex', justifyContent: 'center', gap: 8 }}>
    <CancelBtn />
    <OkBtn />
  </div>
);

const ADMIN_LIST_CACHE_ID = 'cid-admin-list';

interface CachedAdminListPayload<T> {
  cache_id: string;
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
    auth.account_id,
    auth.institution_code,
    normalizeScopeProvinceName(auth.scope_province_name) || 'NO_PROVINCE_SCOPE',
    auth.scope_city_name || 'NO_CITY_SCOPE',
    mode,
  ].join(':');
}

function readCachedAdminList<T>(key: string): T[] | null {
  try {
    const raw = localStorage.getItem(key);
    if (!raw) return null;
    const parsed = JSON.parse(raw) as CachedAdminListPayload<T>;
    if (parsed.cache_id !== ADMIN_LIST_CACHE_ID || !Array.isArray(parsed.rows)) {
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
      JSON.stringify({ cache_id: ADMIN_LIST_CACHE_ID, rows } satisfies CachedAdminListPayload<T>),
    );
  } catch {
    // 注册局管理员列表缓存只是减少重复转圈,写失败不能影响业务操作。
  }
}

export function RegistryAdminsView({ mode }: RegistryAdminsViewProps) {
  const { auth } = useAuth();

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

  const initialScopeProvinceName = normalizeScopeProvinceName(auth?.scope_province_name);
  const initialCityRegistryCities = initialScopeProvinceName
    ? readCachedCidCities(initialScopeProvinceName)
    : null;
  const [cityRegistryAdminCities, setCityRegistryCities] = useState<CidCityItem[]>(initialCityRegistryCities ?? []);
  const [cityRegistryAdminCitiesLoading, setCityRegistryCitiesLoading] = useState(
    !!initialScopeProvinceName && !initialCityRegistryCities,
  );

  const [addCityRegistryOpen, setAddCityRegistryOpen] = useState(false);
  const [addCityRegistryLoading, setAddCityRegistryLoading] = useState(false);

  const [accountScanTarget, setAccountScanTarget] = useState<AccountScanTarget>(null);

  // ── 注册局机构详情页数据源(任务卡 20260608) ──
  // 联邦注册局:全国唯一机构,走 scope-bypass 接口,所有联邦注册局管理员可读。
  const [federalRegistryDetail, setFederalRegistryDetail] = useState<InstitutionDetail | null>(null);
  const [federalRegistryLoading, setFederalRegistryLoading] = useState(false);
  // 市注册局:当前活动市对应的机构 cid_number。
  const [cityRegistryCid, setCityRegistryCid] = useState<string | null>(null);
  const [cityRegistryLoading, setCityRegistryLoading] = useState(false);

  // 活动省/市:联邦注册局管理员看 selectedFederalRegistry + selectedCity;市注册局管理员锁定本省本市。
  const activeProvince = selectedFederalRegistry?.province_name ?? normalizeScopeProvinceName(auth?.scope_province_name);
  const activeCity = selectedCity ?? (isSubordinateRegistry(auth?.institution_code) ? auth?.scope_city_name ?? null : null);

  const [addCityRegistryForm] = Form.useForm<{ city_registry_account: string; family_name: string; given_name: string; city_scope_city_name: string }>();
  const [adminActionModal, setAdminActionModal] = useState<AdminActionModalState | null>(null);
  const [adminActionLoading, setAdminActionLoading] = useState(false);
  const [adminActionCommitLoading, setAdminActionCommitLoading] = useState(false);

  useEffect(() => {
    adminDetailTabRef.current = adminDetailTab;
  }, [adminDetailTab]);

  // ── 数据加载 ──

  const refreshFederalRegistryAdmins = async (): Promise<FederalRegistryAdminRow[]> => {
    if (!auth) return [];
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
      setFederalRegistryAdmins(list);
      writeCachedAdminList(cacheKey, list);
      return list;
    } catch (err) {
      notice.error(err, '加载联邦注册局管理员失败');
      return cached ?? [];
    } finally {
      if (cached === null) setFederalRegistryAdminsLoading(false);
    }
  };

  const refreshCityRegistryAdmins = async (): Promise<CityRegistryAdminRow[]> => {
    if (!auth) return [];
    const cacheKey = adminListCacheKey('city-registry-admins', auth, mode);
    const cached = readCachedAdminList<CityRegistryAdminRow>(cacheKey);
    if (cached !== null) {
      setCityRegistryAdmins(cached);
      setCityRegistryAdminsLoading(false);
    } else {
      setCityRegistryAdminsLoading(true);
    }
    try {
      const rows = await listCityRegistryAdmins(auth);
      const list = Array.isArray(rows) ? rows : [];
      setCityRegistryAdmins(list);
      writeCachedAdminList(cacheKey, list);
      return list;
    } catch (err) {
      notice.error(err, '加载市注册局管理员失败');
      return cached ?? [];
    } finally {
      if (cached === null) setCityRegistryAdminsLoading(false);
    }
  };

  // 首次挂载 / auth 变化时加载数据。
  // 角色分流由 ProvinceDetailView + useScope 自动处理,这里只负责加载数据
  // 和按当前登录角色定位 selectedFederalRegistry。
  useEffect(() => {
    let cancelled = false;
    const init = async () => {
      if (!auth) return;
      // 注册局视图(city-registry / federal-registry):联邦/市注册局管理员数据都要
      const [rows, ops] = await Promise.all([refreshFederalRegistryAdmins(), refreshCityRegistryAdmins()]);
      if (cancelled) return;
      // 自动定位到当前登录角色所属省的 FederalRegistry
      if (!selectedFederalRegistry) {
        let target: FederalRegistryAdminRow | null = null;
        if (isTier1Registry(auth.institution_code)) {
          target = rows.find((r) => sameHexAccount(r.account_id, auth.account_id)) || null;
        } else if (isSubordinateRegistry(auth.institution_code)) {
          const me = ops.find((o) => sameHexAccount(o.account_id, auth.account_id));
          if (me) {
            target = rows.find((r) => sameHexAccount(r.account_id, me.creator_account_id)) || null;
          }
        }
        if (!cancelled && target) setSelectedFederalRegistry(target);
      }
    };
    void init();
    return () => { cancelled = true; };
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [auth?.access_token, mode]);

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
    const provinceForCities = selectedFederalRegistry?.province_name ?? normalizeScopeProvinceName(auth.scope_province_name);
    if (!provinceForCities) {
      lastSelectedFederalRegistryKey.current = null;
      setCityRegistryCities([]);
      setCityRegistryCitiesLoading(false);
      return;
    }
    const selectedKey = selectedFederalRegistry
      ? `${selectedFederalRegistry.province_name}:${selectedFederalRegistry.account_id}`
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
  }, [selectedFederalRegistry?.account_id, auth?.access_token, auth?.scope_province_name]);

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
  }, [auth?.access_token, mode]);

  // ── 市注册局机构 cid 解析:从该市公权机构目录里筛 institution_code='CREG' 那条 ──
  useEffect(() => {
    if (!auth || mode !== 'city-registry' || !activeProvince || !activeCity) {
      setCityRegistryCid(null);
      setCityRegistryLoading(false);
      return;
    }
    let cancelled = false;
    setCityRegistryCid(null);
    setCityRegistryLoading(true);
    listOfficialInstitutions(auth, { province_name: activeProvince, city_name: activeCity, page_size: 300 })
      .then((res) => {
        if (cancelled) return;
        const row = res.items.find((r) => isSubordinateRegistry(r.institution_code));
        setCityRegistryCid(row?.cid_number ?? null);
      })
      .catch((err) => { if (!cancelled) notice.error(err, '加载市注册局失败'); })
      .finally(() => { if (!cancelled) setCityRegistryLoading(false); });
    return () => { cancelled = true; };
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [auth?.access_token, mode, activeProvince, activeCity]);

  const runSecuredAction = async <T,>(actionType: AdminActionType, payload: unknown): Promise<T> => {
    if (!auth) throw new Error('请先登录');
    setAdminActionLoading(true);
    try {
      const prepared = await prepareAdminAction(auth, actionType, payload);
      const signRequest = prepared.sign_request;
      if (prepared.auth_type !== 'PASSKEY_COLD_SIGN' || !signRequest) {
        throw new Error('该治理操作缺少公民钱包签名请求');
      }
      return await new Promise<T>((resolve, reject) => {
        setAdminActionModal({
          actionId: prepared.action_id,
          signRequest,
          payloadHash: prepared.payload_hash,
          actionType: prepared.action_type,
          resolve: resolve as (value: unknown) => void,
          reject,
        });
      });
    } finally {
      setAdminActionLoading(false);
    }
  };

  const handleAdminActionSignedResponse = useCallback(async (raw: string) => {
    if (!auth || !adminActionModal) return;
    setAdminActionCommitLoading(true);
    try {
      const signed = parseSignedReceiptPayload(raw, adminActionModal.actionId);
      if (signed.challenge_id !== adminActionModal.actionId) {
        throw new Error('签名响应与当前请求不匹配');
      }
      if (!signed.account_id) {
        throw new Error('签名响应缺少 account_id');
      }
      const result = await commitAdminAction(auth, {
        action_id: adminActionModal.actionId,
        account_id: signed.account_id,
        signature: signed.signature,
        payload_hash: adminActionModal.payloadHash,
        action_type: adminActionModal.actionType,
      });
      adminActionModal.resolve(result);
      setAdminActionModal(null);
    } catch (error) {
      notice.error(error, '签名响应处理失败');
      adminActionModal.reject(error);
    } finally {
      setAdminActionCommitLoading(false);
    }
  }, [adminActionModal, auth]);

  // ── 事件处理 ──

  const onCreateCityRegistry = async (values: { city_registry_account: string; family_name: string; given_name: string; city_name?: string }) => {
    if (!auth) return;
    const inputAddr = values.city_registry_account?.trim();
    const family_name = values.family_name?.trim();
    const given_name = values.given_name?.trim();
    const city = (values.city_name ?? '').trim();
    if (!inputAddr) {
      notice.error('请输入管理员账户');
      return;
    }
    if (!family_name || !given_name) {
      notice.error('请分别输入管理员姓和名');
      return;
    }
    if (!city) {
      notice.error('请选择市');
      return;
    }
    const cityCityRegistryCount = cityRegistryAdmins.filter((item) => item.city_name === city).length;
    if (cityCityRegistryCount >= MAX_CITY_REGISTRY_ADMINS_PER_CITY) {
      notice.error(`本市市注册局管理员已满 ${MAX_CITY_REGISTRY_ADMINS_PER_CITY} 人，不能继续新增`);
      return;
    }
    let account_id: string;
    try {
      account_id = decodeSs58(inputAddr);
    } catch (err) {
      notice.error(err, '');
      return;
    }
    setAddCityRegistryLoading(true);
    try {
      const created = await runSecuredAction<CityRegistryAdminRow>('CREATE_SUBORDINATE_REGISTRY', {
        account_id,
        family_name,
        given_name,
        city_name: city,
      });
      notice.success('管理员新增成功');
      addCityRegistryForm.resetFields();
      setAddCityRegistryOpen(false);
      setCityRegistryAdmins((prev) => {
        const rest = prev.filter((item) => item.account_id !== created.account_id);
        return [created, ...rest];
      });
      await refreshCityRegistryAdmins();
    } catch (err) {
      const msg = formatAdminCreateError(err, '新增管理员失败');
      notice.error(msg);
    } finally {
      setAddCityRegistryLoading(false);
    }
  };

  const onDeleteCityRegistry = (row: CityRegistryAdminRow) => {
    if (!auth) return;
    notice.confirm({
      title: <div style={{ textAlign: 'center', width: '100%' }}>删除市注册局管理员</div>,
      icon: null,
      centered: true,
      zIndex: CID_MODAL_Z_INDEX.business,
      footer: centeredConfirmFooter,
      content: (
        <div>
          <Typography.Paragraph style={{ marginBottom: 8 }}>确认删除该市注册局管理员?</Typography.Paragraph>
          <InstitutionAssignmentDetails assignment={row} areaLabel="城市" areaValue={row.city_name} />
        </div>
      ),
      okText: '确认删除',
      okButtonProps: { danger: true },
      cancelText: '取消',
      onOk: async () => {
        setCityRegistryAdminsLoading(true);
        try {
          await runSecuredAction('DELETE_SUBORDINATE_REGISTRY', { id: row.id });
          notice.success('市注册局管理员已删除');
          await refreshCityRegistryAdmins();
        } catch (err) {
          notice.error(err, '删除市注册局管理员失败');
          throw err;
        } finally {
          setCityRegistryAdminsLoading(false);
        }
      },
    });
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
    setAddCityRegistryOpen,
    addCityRegistryLoading,
    accountScanTarget,
    setAccountScanTarget,
    addCityRegistryForm,
    onCreateCityRegistry,
    onDeleteCityRegistry,
    runSecuredAction,
    federalRegistryDetail,
    federalRegistryLoading,
    cityRegistryCid,
    cityRegistryLoading,
  };

  // ── 渲染:按 mode 分派 ──

  const content = mode === 'city-registry'
    ? <CityRegistryView state={shared} />
    : <FederalRegistryView state={shared} />;

  return (
    <>
      {content}
      <CitizenSignatureModal
        title="公民钱包签名确认"
        open={!!adminActionModal}
        onCancel={() => {
          adminActionModal?.reject(new Error('admin action cancelled'));
          setAdminActionModal(null);
          setAdminActionCommitLoading(false);
        }}
        qrTitle="签名二维码"
        qrValue={adminActionModal?.signRequest}
        qrHint="使用当前管理员公民钱包扫码签名"
        scannerHint="扫描公民钱包生成的签名响应二维码"
        scannerDisabled={adminActionCommitLoading}
        scannerLoading={adminActionCommitLoading}
        onDetected={handleAdminActionSignedResponse}
        onScannerError={(msg) => notice.error(msg)}
      />
    </>
  );
}

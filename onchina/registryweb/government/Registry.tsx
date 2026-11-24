// 注册局机构浏览只读机构资料；岗位、任职和法代均进入统一详情页。

import { useCallback, useEffect, useState } from 'react';
import { Alert, Button, Card, Space, Spin, Table } from 'antd';
import type { AdminAuth } from '@gmb/onchina-platform/authentication/types';
import type { InstitutionDetail, InstitutionListRow } from '@gmb/onchina-platform/institutions/api';
import { IdentityCidText } from '@gmb/onchina-platform/codes/identityCid';
import { useScope, normalizeScopeProvinceName } from '@gmb/onchina-platform/authorization/useScope';
import { isSubordinateRegistry, isTier1Registry, TIER2_REGISTRY_CODE } from '@gmb/onchina-platform/authorization/registryTier';
import { getFederalRegistry, listOfficialInstitutions } from './api';
import { GovDetailPage } from './GovDetailPage';

type Props = { auth: AdminAuth; mode: 'city-registry' | 'federal-registry' };

export function RegistryView(props: Props) {
  const { auth, mode } = props;
  const context = JSON.stringify([auth.accessToken, auth.accountId, auth.institutionCidNumber, auth.scopeProvinceName, auth.scopeCityName, auth.scopeTownName, mode]);
  return <RegistryPanel key={context} {...props}/>;
}

function RegistryPanel({ auth, mode }: Props) {
  const scope = useScope(auth);
  const [federal, setFederal] = useState<InstitutionDetail | null>(null);
  const [selected, setSelected] = useState<InstitutionListRow | null>(null);
  const [rows, setRows] = useState<InstitutionListRow[]>([]);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);
  const [reloadCount, setReloadCount] = useState(0);
  const [cursors, setCursors] = useState<(string | null)[]>([null]);
  const [next, setNext] = useState<string | null>(null);
  const cursor = cursors[cursors.length - 1];
  const ownCity = isSubordinateRegistry(auth.institutionCode);
  const reload = () => setReloadCount(value => value + 1);
  const loadFederal = useCallback(() => federal ? Promise.resolve(federal) : getFederalRegistry(auth), [auth, federal]);

  useEffect(() => {
    if (mode !== 'federal-registry') return;
    let cancelled = false; setLoading(true); setError(null);
    getFederalRegistry(auth).then(value => { if (!cancelled) setFederal(value); })
      .catch(failure => { if (!cancelled) setError(failure instanceof Error ? failure.message : String(failure)); })
      .finally(() => { if (!cancelled) setLoading(false); });
    return () => { cancelled = true; };
  }, [auth, mode, reloadCount]);

  useEffect(() => {
    if (mode !== 'city-registry' || ownCity || selected) return;
    let cancelled = false; setLoading(true); setError(null); setRows([]); setNext(null);
    // 范围与分页仍由机构目录后端核验，不以人员钱包或本地名单定位机构。
    listOfficialInstitutions(auth, { institutionCode: TIER2_REGISTRY_CODE, provinceName: normalizeScopeProvinceName(auth.scopeProvinceName) ?? undefined,
      cityName: scope.lockedCityName ?? undefined, cursor, pageSize: 20 }).then(page => {
      if (cancelled) return;
      if (page.items.some(row => row.institutionCode !== TIER2_REGISTRY_CODE)) throw new Error('机构目录返回了非市注册局资料');
      if (page.has_more && !page.next_cursor) throw new Error('机构目录分页游标缺失');
      setRows(page.items); setNext(page.has_more ? page.next_cursor! : null);
    }).catch(failure => { if (!cancelled) setError(failure instanceof Error ? failure.message : String(failure)); })
      .finally(() => { if (!cancelled) setLoading(false); });
    return () => { cancelled = true; };
  }, [auth, mode, ownCity, selected, scope.lockedCityName, cursor, reloadCount]);

  if (mode === 'federal-registry') {
    if (error) return <Alert type="error" message="联邦注册局读取失败" description={error} action={<Button onClick={reload}>重试</Button>}/>;
    if (!federal) return <Spin tip="读取联邦注册局"><div style={{ minHeight: 80 }}/></Spin>;
    return <GovDetailPage auth={auth} cidNumber={federal.institution.cidNumber} loadDetail={loadFederal} canWrite={scope.canWrite && isTier1Registry(auth.institutionCode)}/>;
  }
  if (ownCity) return <GovDetailPage auth={auth} cidNumber={auth.institutionCidNumber} canWrite={scope.canWrite}/>;
  if (selected) return <GovDetailPage auth={auth} cidNumber={selected.cidNumber} canWrite={scope.canWriteCity(selected.provinceName, selected.cityName)} onBack={() => setSelected(null)} backLabel="返回市注册局"/>;
  return <Card title="市注册局">
    {error ? <Alert type="error" message="市注册局读取失败" description={error} action={<Button onClick={reload}>重试</Button>}/> : <Table rowKey="cidNumber" loading={loading} dataSource={rows} pagination={false}
      onRow={row => ({ onClick: () => setSelected(row), style: { cursor: 'pointer' } })} columns={[
        { title: '身份CID号', render: (_, row) => <IdentityCidText value={row.cidNumber}/> },
        { title: '机构名称', render: (_, row) => row.cidFullName || row.cidShortName || '-' },
        { title: '行政区', render: (_, row) => [row.provinceName, row.cityName, row.townName].filter(Boolean).join(' / ') || '-' },
      ]}/>}
    <Space style={{ marginTop: 12 }}>
      <Button disabled={loading || cursors.length === 1} onClick={() => setCursors(value => value.slice(0, -1))}>上一页</Button>
      <span>第 {cursors.length} 页</span>
      <Button disabled={loading || !!error || !next} onClick={() => { if (next) setCursors(value => [...value, next]); }}>下一页</Button>
    </Space>
  </Card>;
}

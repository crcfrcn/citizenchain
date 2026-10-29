// 公权机构列表。确定性目录,进入页面直接显示。

import React, { useEffect, useMemo, useState } from 'react';
import { Button, Space, Table, Tag, Typography } from 'antd';
import type { ColumnsType } from 'antd/es/table';
import { listOfficialInstitutions } from './government_api';
import type { AdminAuth } from '../../platform/frontend/authentication_types';
import type { InstitutionListRow } from '../../platform/frontend/subjects_api';
import { useInstitutionCodeLabels } from '../../platform/frontend/subjects_institutionLabels';
import {
  officialInstitutionCacheKey,
  readCachedOfficialInstitutionRows,
  writeCachedOfficialInstitutionRows,
} from '../../platform/frontend/china_metaCache';
import { notice } from '../../platform/frontend/helpers_notice';
import { isSubordinateRegistry } from '../../platform/frontend/platform_registryTier';

const DETERMINISTIC_PAGE_SIZE = 20;

const SUBJECT_STATUS_LABEL: Record<string, string> = {
  ACTIVE: '正常',
  REVOKED: '已注销',
};

function areaText(row: InstitutionListRow) {
  return [row.provinceName, row.cityName, row.townName].filter(Boolean).join('/') || '-';
}

function nameText(row: InstitutionListRow) {
  return row.cidFullName || row.cidShortName || '';
}

function statusTag(status: string | null | undefined, labels: Record<string, string>) {
  if (!status) return <Tag>-</Tag>;
  const color = status === 'ACTIVE' ? 'green' : 'red';
  return <Tag color={color}>{labels[status] || status}</Tag>;
}

interface Props {
  auth: AdminAuth;
  provinceName: string;
  cityName: string;
  onSelectInstitution?: (cidNumber: string) => void;
  refreshKey?: number;
  searchQuery?: string;
}

export const GovListTable: React.FC<Props> = ({
  auth,
  provinceName,
  cityName,
  onSelectInstitution,
  refreshKey,
  searchQuery,
}) => {
  const [rows, setRows] = useState<InstitutionListRow[]>([]);
  const [loading, setLoading] = useState(false);
  const [deterministicPage, setDeterministicPage] = useState(1);
  const institutionLabels = useInstitutionCodeLabels();
  const cacheKey = officialInstitutionCacheKey(auth, provinceName, cityName);

  const loadRows = () => {
    const exactQuery = searchQuery?.trim() ?? '';
    let hasImmediateRows = false;
    if (!exactQuery) {
      const cachedRows = readCachedOfficialInstitutionRows(cacheKey);
      if (cachedRows) {
        setRows(cachedRows);
        setDeterministicPage(1);
        hasImmediateRows = true;
      }
    }

    let cancelled = false;
    if (!hasImmediateRows) setLoading(true);
    const request = listOfficialInstitutions(auth, {
      provinceName,
      cityName: cityName || undefined,
      q: exactQuery || undefined,
      pageSize: 300,
    });

    request
      .then((data) => {
        if (cancelled) return;
        setRows(data.items);
        setDeterministicPage(1);
        if (!exactQuery) {
          writeCachedOfficialInstitutionRows(cacheKey, auth, provinceName, cityName, data.items, data.manifest_version);
        }
      })
      .catch((err) => {
        if (!cancelled) notice.error(err, '');
      })
      .finally(() => {
        if (!cancelled && !hasImmediateRows) setLoading(false);
      });
    return () => {
      cancelled = true;
    };
  };

  useEffect(() => loadRows(), [
    auth.accessToken,
    auth.accountId,
    auth.institutionCode,
    auth.scopeProvinceName,
    auth.scopeCityName,
    provinceName,
    cityName,
    refreshKey,
    searchQuery,
  ]);

  // 市注册局(CREG)已搬进「市注册局」tab,公权机构列表不再展示;
  // 联邦注册局(FRG)是中枢省的国家级机构,按 user 决定保留在公权列表里。
  const visibleRows = rows.filter((r) => !isSubordinateRegistry(r.institutionCode));
  const totalPages = Math.max(1, Math.ceil(visibleRows.length / DETERMINISTIC_PAGE_SIZE));
  const displayRows = visibleRows.slice(
    (deterministicPage - 1) * DETERMINISTIC_PAGE_SIZE,
    deterministicPage * DETERMINISTIC_PAGE_SIZE,
  );

  const columns = useMemo<ColumnsType<InstitutionListRow>>(() => {
    return [
      {
        title: '序号',
        width: 70,
        align: 'center',
        render: (_v, _row, index) =>
          (deterministicPage - 1) * DETERMINISTIC_PAGE_SIZE + index + 1,
      },
      { title: '身份ID', dataIndex: 'cidNumber', width: 260, align: 'center' },
      {
        title: '机构全称',
        width: 180,
        align: 'center',
        render: (_v, row) => nameText(row) || <span style={{ color: '#999' }}>(未设置全称)</span>,
      },
      { title: '行政区', width: 180, align: 'center', render: (_v, row) => areaText(row) },
      {
        title: '机构类型',
        dataIndex: 'institutionCode',
        width: 130,
        align: 'center',
        render: (v: string) => institutionLabels[v] || v,
      },
      {
        title: '状态',
        dataIndex: 'status',
        width: 100,
        align: 'center',
        render: (v: string) => statusTag(v, SUBJECT_STATUS_LABEL),
      },
      { title: '账户数', dataIndex: 'accountCount', width: 90, align: 'center' },
    ];
  }, [deterministicPage, institutionLabels]);

  return (
    <div>
      <Table<InstitutionListRow>
        rowKey={(r) => r.cidNumber}
        loading={loading}
        dataSource={displayRows}
        pagination={false}
        onRow={(row) => ({
          onClick: () => onSelectInstitution?.(row.cidNumber),
          style: onSelectInstitution ? { cursor: 'pointer' } : undefined,
        })}
        columns={columns}
      />
      <Space style={{ marginTop: 12 }} wrap>
        <Typography.Text type="secondary">
          共 {totalPages} 页 / 第 {deterministicPage} 页
        </Typography.Text>
        <Typography.Text type="secondary">共 {visibleRows.length} 条</Typography.Text>
        <Button disabled={loading || deterministicPage <= 1} onClick={() => setDeterministicPage((p) => Math.max(1, p - 1))}>
          上一页
        </Button>
        <Button disabled={loading || deterministicPage >= totalPages} onClick={() => setDeterministicPage((p) => Math.min(totalPages, p + 1))}>
          下一页
        </Button>
      </Space>
    </div>
  );
};

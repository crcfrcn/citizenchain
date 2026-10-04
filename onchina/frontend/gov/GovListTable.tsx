// 公权机构列表。确定性目录,进入页面直接显示。

import React, { useEffect, useMemo, useState } from 'react';
import { Button, Space, Table, Tag, Typography } from 'antd';
import type { ColumnsType } from 'antd/es/table';
import { listOfficialInstitutions } from './api';
import type { AdminAuth } from '../auth/types';
import type { InstitutionListRow } from '../subjects/api';
import { useInstitutionCodeLabels } from '../subjects/institutionLabels';
import {
  officialInstitutionCacheKey,
  readCachedOfficialInstitutionRows,
  writeCachedOfficialInstitutionRows,
} from '../china/metaCache';
import { notice } from '../utils/notice';
import { isSubordinateRegistry } from '../platform/registryTier';

const DETERMINISTIC_PAGE_SIZE = 20;

const SUBJECT_STATUS_LABEL: Record<string, string> = {
  ACTIVE: '正常',
  REVOKED: '已注销',
};

function areaText(row: InstitutionListRow) {
  return [row.province_name, row.city_name, row.town_name].filter(Boolean).join('/') || '-';
}

function nameText(row: InstitutionListRow) {
  return row.cid_full_name || row.cid_short_name || '';
}

function statusTag(status: string | null | undefined, labels: Record<string, string>) {
  if (!status) return <Tag>-</Tag>;
  const color = status === 'ACTIVE' ? 'green' : 'red';
  return <Tag color={color}>{labels[status] || status}</Tag>;
}

interface Props {
  auth: AdminAuth;
  province_name: string;
  city_name: string;
  onSelectInstitution?: (cidNumber: string) => void;
  refreshKey?: number;
  searchQuery?: string;
}

export const GovListTable: React.FC<Props> = ({
  auth,
  province_name,
  city_name,
  onSelectInstitution,
  refreshKey,
  searchQuery,
}) => {
  const [rows, setRows] = useState<InstitutionListRow[]>([]);
  const [loading, setLoading] = useState(false);
  const [deterministicPage, setDeterministicPage] = useState(1);
  const institutionLabels = useInstitutionCodeLabels();
  const cacheKey = officialInstitutionCacheKey(auth, province_name, city_name);

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
      province_name,
      city_name: city_name || undefined,
      q: exactQuery || undefined,
      page_size: 300,
    });

    request
      .then((data) => {
        if (cancelled) return;
        setRows(data.items);
        setDeterministicPage(1);
        if (!exactQuery) {
          writeCachedOfficialInstitutionRows(cacheKey, auth, province_name, city_name, data.items, data.manifest_version);
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
    auth.access_token,
    auth.account_id,
    auth.institution_code,
    auth.scope_province_name,
    auth.scope_city_name,
    province_name,
    city_name,
    refreshKey,
    searchQuery,
  ]);

  // 市注册局(CREG)已搬进「市注册局」tab,公权机构列表不再展示;
  // 联邦注册局(FRG)是中枢省的国家级机构,按 user 决定保留在公权列表里。
  const visibleRows = rows.filter((r) => !isSubordinateRegistry(r.institution_code));
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
      { title: '身份ID', dataIndex: 'cid_number', width: 260, align: 'center' },
      {
        title: '机构全称',
        width: 180,
        align: 'center',
        render: (_v, row) => nameText(row) || <span style={{ color: '#999' }}>(未设置全称)</span>,
      },
      { title: '行政区', width: 180, align: 'center', render: (_v, row) => areaText(row) },
      {
        title: '机构类型',
        dataIndex: 'institution_code',
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
      { title: '账户数', dataIndex: 'account_count', width: 90, align: 'center' },
    ];
  }, [deterministicPage, institutionLabels]);

  return (
    <div>
      <Table<InstitutionListRow>
        rowKey={(r) => r.cid_number}
        loading={loading}
        dataSource={displayRows}
        pagination={false}
        onRow={(row) => ({
          onClick: () => onSelectInstitution?.(row.cid_number),
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

// 教育机构列表。市详情确定性市公民教育委员会先读本地缓存直接展示,再后台刷新;
// 学校和 F+JY 非法人教育机构仍保持精确搜索,避免跨省全量扫描。

import React, { useEffect, useMemo, useState } from 'react';
import { Button, Space, Table, Typography } from 'antd';
import type { ColumnsType } from 'antd/es/table';
import type { AdminAuth } from '../auth/types';
import type { InstitutionListRow } from './api';
import { listEducationInstitutions } from './api';
import { EDUCATION_TYPE_LABEL, SUBJECT_PROPERTY_LABEL } from '../subjects/labels';
import {
  educationCommitteeCacheKey,
  readCachedEducationCommitteeRows,
  writeCachedEducationCommitteeRows,
} from '../china/metaCache';
import { notice } from '../utils/notice';

interface Props {
  auth: AdminAuth;
  province_name: string;
  city_name: string;
  onSelectInstitution?: (cidNumber: string) => void;
  refreshKey?: number;
  searchQuery?: string;
}

export const EducationListTable: React.FC<Props> = ({
  auth,
  province_name,
  city_name,
  onSelectInstitution,
  refreshKey,
  searchQuery,
}) => {
  const [directRows, setDirectRows] = useState<InstitutionListRow[]>([]);
  const [searchRows, setSearchRows] = useState<InstitutionListRow[]>([]);
  const [directLoading, setDirectLoading] = useState(false);
  const [searchLoading, setSearchLoading] = useState(false);
  const [nextCursor, setNextCursor] = useState<string | null>(null);
  const [cursorStack, setCursorStack] = useState<string[]>([]);
  const directCacheKey = educationCommitteeCacheKey(auth, province_name, city_name);

  const loadDirectRows = () => {
    const cachedRows = readCachedEducationCommitteeRows(directCacheKey);
    const hasImmediateRows = Boolean(cachedRows);
    if (cachedRows) setDirectRows(cachedRows);

    let cancelled = false;
    if (!hasImmediateRows) setDirectLoading(true);
    listEducationInstitutions(auth, {
      province_name,
      city_name: city_name || undefined,
      page_size: 50,
    })
      .then((data) => {
        if (cancelled) return;
        setDirectRows(data.items);
        writeCachedEducationCommitteeRows(directCacheKey, auth, province_name, city_name, data.items);
      })
      .catch((err) => {
        if (!cancelled) notice.error(err, '');
      })
      .finally(() => {
        if (!cancelled && !hasImmediateRows) setDirectLoading(false);
      });
    return () => {
      cancelled = true;
    };
  };

  const loadSearchRows = (cursor?: string | null) => {
    const exactQuery = searchQuery?.trim() ?? '';
    if (!exactQuery) {
      setSearchRows([]);
      setNextCursor(null);
      return () => {};
    }

    let cancelled = false;
    setSearchLoading(true);
    listEducationInstitutions(auth, {
      province_name,
      city_name: city_name || undefined,
      q: exactQuery,
      cursor,
      page_size: 50,
    })
      .then((data) => {
        if (cancelled) return;
        setSearchRows(data.items);
        setNextCursor(data.next_cursor ?? null);
      })
      .catch((err) => {
        if (!cancelled) notice.error(err, '');
      })
      .finally(() => {
        if (!cancelled) setSearchLoading(false);
      });
    return () => {
      cancelled = true;
    };
  };

  useEffect(() => {
    return loadDirectRows();
  }, [
    auth.access_token,
    auth.account_id,
    auth.institution_code,
    auth.scope_province_name,
    auth.scope_city_name,
     province_name,
     city_name,
    refreshKey,
    directCacheKey,
  ]);

  useEffect(() => {
    setCursorStack([]);
    return loadSearchRows(null);
  }, [
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

  const loading = directLoading || searchLoading;
  const rows = useMemo(() => {
    const seen = new Set<string>();
    return [...directRows, ...searchRows].filter((row) => {
      if (seen.has(row.cid_number)) return false;
      seen.add(row.cid_number);
      return true;
    });
  }, [directRows, searchRows]);

  const columns = useMemo<ColumnsType<InstitutionListRow>>(
    () => [
      {
        title: '序号',
        width: 70,
        align: 'center',
        render: (_v, _row, index) => cursorStack.length * 50 + index + 1,
      },
      { title: '身份ID', dataIndex: 'cid_number', width: 260, align: 'center' },
      {
        title: '机构全称',
        dataIndex: 'cid_full_name',
        width: 180,
        align: 'center',
        render: (v: string | null) =>
          v ? v : <span style={{ color: '#999' }}>(未设置全称,待完善)</span>,
      },
      {
        title: '教育分类',
        key: 'education_type',
        width: 150,
        align: 'center',
        render: (_v, r) =>
          r.education_type
            ? EDUCATION_TYPE_LABEL[r.education_type] ?? r.education_type
            : '分校',
      },
      {
        title: '主体属性',
        key: 'subject_property',
        width: 140,
        align: 'center',
        render: (_v, r) => (
          <span>
            {SUBJECT_PROPERTY_LABEL[r.subject_property] ?? r.subject_property}
            <Typography.Text type="secondary" style={{ marginLeft: 6, fontSize: 12 }}>
              ({r.p1 === '1' ? '盈利' : '非盈利'})
            </Typography.Text>
          </span>
        ),
      },
      { title: '省/市', render: (_v, r) => `${r.province_name}/${r.city_name}`, width: 160, align: 'center' },
      { title: '账户数', dataIndex: 'account_count', width: 90, align: 'center' },
    ],
    [cursorStack.length],
  );

  const onNextPage = () => {
    if (!nextCursor) return;
    setCursorStack((prev) => [...prev, nextCursor]);
    loadSearchRows(nextCursor);
  };

  const onPrevPage = () => {
    if (cursorStack.length === 0) return;
    const stack = [...cursorStack];
    stack.pop();
    const prevCursor = stack.length > 0 ? stack[stack.length - 1] : null;
    setCursorStack(stack);
    loadSearchRows(prevCursor);
  };

  return (
    <div>
      <Table<InstitutionListRow>
        rowKey={(r) => r.cid_number}
        loading={loading}
        dataSource={rows}
        pagination={false}
        onRow={(row) => ({
          onClick: () => onSelectInstitution?.(row.cid_number),
          style: onSelectInstitution ? { cursor: 'pointer' } : undefined,
        })}
        columns={columns}
      />
      <Space style={{ marginTop: 12 }} wrap>
        <Button disabled={loading || cursorStack.length === 0} onClick={onPrevPage}>
          上一页
        </Button>
        <Button disabled={loading || !nextCursor} onClick={onNextPage}>
          下一页
        </Button>
      </Space>
    </div>
  );
};

// 教育机构列表。市详情确定性市公民教育委员会先读本地缓存直接展示,再后台刷新;
// 学校和 F+JY 非法人教育机构仍保持精确搜索,避免跨省全量扫描。

import React, { useEffect, useMemo, useState } from 'react';
import { Button, Space, Table, Typography } from 'antd';
import type { ColumnsType } from 'antd/es/table';
import type { AdminAuth } from '../../platform/frontend/authentication_types';
import type { InstitutionListRow } from './education_api';
import { listEducationInstitutions } from './education_api';
import { EDUCATION_TYPE_LABEL, SUBJECT_PROPERTY_LABEL } from '../../platform/frontend/subjects_labels';
import {
  educationCommitteeCacheKey,
  readCachedEducationCommitteeRows,
  writeCachedEducationCommitteeRows,
} from '../../platform/frontend/china_metaCache';
import { notice } from '../../platform/frontend/helpers_notice';

interface Props {
  auth: AdminAuth;
  provinceName: string;
  cityName: string;
  onSelectInstitution?: (cidNumber: string) => void;
  refreshKey?: number;
  searchQuery?: string;
}

export const EducationListTable: React.FC<Props> = ({
  auth,
  provinceName,
  cityName,
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
  const directCacheKey = educationCommitteeCacheKey(auth, provinceName, cityName);

  const loadDirectRows = () => {
    const cachedRows = readCachedEducationCommitteeRows(directCacheKey);
    const hasImmediateRows = Boolean(cachedRows);
    if (cachedRows) setDirectRows(cachedRows);

    let cancelled = false;
    if (!hasImmediateRows) setDirectLoading(true);
    listEducationInstitutions(auth, {
      provinceName,
      cityName: cityName || undefined,
      pageSize: 50,
    })
      .then((data) => {
        if (cancelled) return;
        setDirectRows(data.items);
        writeCachedEducationCommitteeRows(directCacheKey, auth, provinceName, cityName, data.items);
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
      provinceName,
      cityName: cityName || undefined,
      q: exactQuery,
      cursor,
      pageSize: 50,
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
    auth.accessToken,
    auth.accountId,
    auth.institutionCode,
    auth.scopeProvinceName,
    auth.scopeCityName,
     provinceName,
     cityName,
    refreshKey,
    directCacheKey,
  ]);

  useEffect(() => {
    setCursorStack([]);
    return loadSearchRows(null);
  }, [
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

  const loading = directLoading || searchLoading;
  const rows = useMemo(() => {
    const seen = new Set<string>();
    return [...directRows, ...searchRows].filter((row) => {
      if (seen.has(row.cidNumber)) return false;
      seen.add(row.cidNumber);
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
      { title: '身份ID', dataIndex: 'cidNumber', width: 260, align: 'center' },
      {
        title: '机构全称',
        dataIndex: 'cidFullName',
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
          r.educationType
            ? EDUCATION_TYPE_LABEL[r.educationType] ?? r.educationType
            : '分校',
      },
      {
        title: '主体属性',
        key: 'subject_property',
        width: 140,
        align: 'center',
        render: (_v, r) => (
          <span>
            {SUBJECT_PROPERTY_LABEL[r.subjectProperty] ?? r.subjectProperty}
            <Typography.Text type="secondary" style={{ marginLeft: 6, fontSize: 12 }}>
              ({r.p1 === '1' ? '盈利' : '非盈利'})
            </Typography.Text>
          </span>
        ),
      },
      { title: '省/市', render: (_v, r) => `${r.provinceName}/${r.cityName}`, width: 160, align: 'center' },
      { title: '账户数', dataIndex: 'accountCount', width: 90, align: 'center' },
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
        rowKey={(r) => r.cidNumber}
        loading={loading}
        dataSource={rows}
        pagination={false}
        onRow={(row) => ({
          onClick: () => onSelectInstitution?.(row.cidNumber),
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

// 私权机构列表。私权机构必须输入机构全称、简称或 CID 精确搜索,避免跨省全量扫描。

import React, { useEffect, useMemo, useState } from 'react';
import { Button, Space, Table, Tag } from 'antd';
import type { ColumnsType } from 'antd/es/table';
import type { AdminAuth } from '../../auth/types';
import type { InstitutionListRow, PageResult, PrivateType } from '../../subjects/api';
import { PARTNERSHIP_KIND_LABEL, PRIVATE_TYPE_LABEL } from '../../subjects/labels';
import { notice } from '../../utils/notice';

interface Props {
  auth: AdminAuth;
  province_name: string;
  city_name: string;
  privateType: PrivateType;
  listInstitutions: (auth: AdminAuth, query: {
    province_name: string;
    city_name?: string;
    private_type: PrivateType;
    q: string;
    cursor?: string | null;
    page_size?: number;
  }) => Promise<PageResult<InstitutionListRow>>;
  onSelectInstitution?: (cidNumber: string) => void;
  refreshKey?: number;
  searchQuery?: string;
}

export const PrivateListTable: React.FC<Props> = ({
  auth,
   province_name,
   city_name,
  privateType,
  listInstitutions,
  onSelectInstitution,
  refreshKey,
  searchQuery,
}) => {
  const [rows, setRows] = useState<InstitutionListRow[]>([]);
  const [loading, setLoading] = useState(false);
  const [nextCursor, setNextCursor] = useState<string | null>(null);
  const [cursorStack, setCursorStack] = useState<string[]>([]);

  const loadRows = (cursor?: string | null) => {
    const exactQuery = searchQuery?.trim() ?? '';
    // 已选定城市 → 空搜索=浏览该市全部私权机构(城市级有界,后端 drill-in 已投影本市链上私权机构)。
    // 未选城市(仅省)→ 空搜索仍不发请求,坚持精确搜索避免跨省全量扫描。
    if (!exactQuery && !city_name) {
      setRows([]);
      setNextCursor(null);
      return () => {};
    }

    let cancelled = false;
    setLoading(true);
    listInstitutions(auth, {
       province_name,
      city_name: city_name || undefined,
      private_type: privateType,
      q: exactQuery,
      cursor,
      page_size: 50,
    })
      .then((data) => {
        if (cancelled) return;
        setRows(data.items);
        setNextCursor(data.next_cursor ?? null);
      })
      .catch((err) => {
        if (!cancelled) notice.error(err, '');
      })
      .finally(() => {
        if (!cancelled) setLoading(false);
      });
    return () => {
      cancelled = true;
    };
  };

  useEffect(() => {
    setCursorStack([]);
    return loadRows(null);
  }, [
    auth.access_token,
    auth.account_id,
    auth.institution_code,
    auth.scope_province_name,
    auth.scope_city_name,
     province_name,
     city_name,
    privateType,
    listInstitutions,
    refreshKey,
    searchQuery,
  ]);

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
      { title: '省/市', render: (_v, r) => `${r.province_name}/${r.city_name}`, width: 160, align: 'center' },
      {
        title: '私权类型',
        key: 'private_type',
        width: 120,
        align: 'center',
        render: (_v, r) => r.private_type ? PRIVATE_TYPE_LABEL[r.private_type] : '',
      },
      {
        title: '法人资格',
        key: 'legal_personality',
        width: 110,
        align: 'center',
        render: (_v, r) => (
          <Tag color={r.has_legal_personality ? 'green' : 'default'}>
            {r.has_legal_personality ? '有法人资格' : '无法人资格'}
          </Tag>
        ),
      },
      {
        title: '合伙类型',
        key: 'partnership_kind',
        width: 110,
        align: 'center',
        render: (_v, r) => r.partnership_kind ? PARTNERSHIP_KIND_LABEL[r.partnership_kind] : '',
      },
      { title: '账户数', dataIndex: 'account_count', width: 90, align: 'center' },
    ],
    [cursorStack.length],
  );

  const onNextPage = () => {
    if (!nextCursor) return;
    setCursorStack((prev) => [...prev, nextCursor]);
    loadRows(nextCursor);
  };

  const onPrevPage = () => {
    if (cursorStack.length === 0) return;
    const stack = [...cursorStack];
    stack.pop();
    const prevCursor = stack.length > 0 ? stack[stack.length - 1] : null;
    setCursorStack(stack);
    loadRows(prevCursor);
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

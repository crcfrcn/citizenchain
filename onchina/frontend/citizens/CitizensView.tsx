// 注册局顶层视图 —— activeView === 'citizens' 分支。
// 包含:citizen 列表 + 搜索栏 + 表格 + 直接录入公民弹窗。

import { useCallback, useEffect, useState, type ReactNode } from 'react';
import { Button, Card, Form, Input, Space, Table, Tag, Typography } from 'antd';
import { PlusOutlined, SearchOutlined } from '@ant-design/icons';

import type { ColumnsType } from 'antd/es/table';
import {
  listCitizens,
  type CitizenRow,
} from './api';
import { useAuth } from '../hooks/useAuth';
import { useScope } from '../hooks/useScope';
import { glassCardStyle, glassCardHeadStyle } from '../core/cardStyles';
import { CityGrid } from '../core/CityGrid';
import { CitizenCreateModal } from './CitizenCreateModal';
import { CitizenDetailPage } from './CitizenDetailPage';
import { notice } from '../utils/notice';

const CITIZEN_PAGE_SIZE = 50;

function makeCitizenName(row: Pick<CitizenRow, 'family_name' | 'given_name'>) {
  return `${row.family_name ?? ''}${row.given_name ?? ''}`.trim() || '-';
}

/** 从 CID 机构段(第二段前 4 位)派生人主体类型:CTZN=公民 / NATP=居民。 */
function citizenTypeLabel(cidNumber: string): string {
  const code = cidNumber.split('-')[1]?.slice(0, 4);
  if (code === 'CTZN') return '公民';
  if (code === 'NATP') return '居民';
  return '-';
}

function makeCenteredTitle(center: ReactNode, back?: () => void) {
  return (
    <div style={{ position: 'relative', display: 'flex', alignItems: 'center', minHeight: 32 }}>
      {back && (
        <Button type="link" style={{ paddingLeft: 0 }} onClick={back}>
          ← 返回
        </Button>
      )}
      <span style={{ position: 'absolute', left: '50%', transform: 'translateX(-50%)' }}>
        {center}
      </span>
    </div>
  );
}

export function CitizensView() {
  const { auth, capabilities } = useAuth();
  const scope = useScope(auth);
  const [searchForm] = Form.useForm<{ keyword: string }>();
  const [rows, setRows] = useState<CitizenRow[]>([]);
  const [loading, setLoading] = useState(false);
  const [searchKeyword, setSearchKeyword] = useState('');
  const [nextCursor, setNextCursor] = useState<string | null>(null);
  const [cursorStack, setCursorStack] = useState<string[]>([]);

  // 直接录入弹窗控制
  const [createModalOpen, setCreateModalOpen] = useState(false);
  const [selectedCitizen, setSelectedCitizen] = useState<CitizenRow | null>(null);
  const [selectedCity, setSelectedCity] = useState<string | null>(null);

  const activeProvinceName = scope.lockedProvinceName;
  const activeCityName = selectedCity ?? (scope.skipCityList ? scope.lockedCityName : null);
  const canUseCitizenList = Boolean(auth && activeProvinceName && activeCityName);

  const refreshList = async (keyword: string, cursor?: string | null, silent?: boolean) => {
    if (!auth || !activeProvinceName || !activeCityName) return;
    const exact = keyword.trim();
    setLoading(true);
    try {
      const raw = await listCitizens(
        auth,
        exact,
        activeProvinceName,
        activeCityName,
        cursor,
        CITIZEN_PAGE_SIZE,
      );
      const list = raw.items;
      setRows(list);
      setNextCursor(raw.next_cursor ?? null);
      if (exact && list.length === 0) {
        notice.warningModal({
          title: '查询结果',
          content: '未查询到公民信息',
        });
      }
    } catch (err) {
      if (!silent) {
        notice.error(err, '查询失败');
      }
    } finally {
      setLoading(false);
    }
  };

  // 挂载时自动加载;auth 变化时(登录/登出)重新加载
  useEffect(() => {
    if (!auth) {
      setRows([]);
      setSearchKeyword('');
      setNextCursor(null);
      setCursorStack([]);
      setCreateModalOpen(false);
      setSelectedCitizen(null);
      setSelectedCity(null);
      return;
    }
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [auth]);

  useEffect(() => {
    setRows([]);
    setSearchKeyword('');
    setNextCursor(null);
    setCursorStack([]);
    setSelectedCitizen(null);
    setCreateModalOpen(false);
    searchForm.resetFields();
    if (auth && activeProvinceName && activeCityName) {
      void refreshList('', null, true);
    }
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [activeProvinceName, activeCityName, searchForm]);

  const onSearch = async (values: { keyword: string }) => {
    if (!auth) return;
    let keyword = values.keyword?.trim() || '';
    setSearchKeyword(keyword);
    setCursorStack([]);
    await refreshList(keyword);
  };

  const onNextPage = async () => {
    if (!nextCursor) return;
    setCursorStack((prev) => [...prev, nextCursor]);
    await refreshList(searchKeyword, nextCursor, true);
  };

  const onPrevPage = async () => {
    if (cursorStack.length === 0) return;
    const stack = [...cursorStack];
    stack.pop();
    const prevCursor = stack.length > 0 ? stack[stack.length - 1] : null;
    setCursorStack(stack);
    await refreshList(searchKeyword, prevCursor, true);
  };

  // 录入成功后，用返回的新身份 CID 自动回填搜索框并查询，让新公民立即显示在列表。
  // 拿不到新号(理论不应发生)时回退到沿用当前关键字刷新。
  const handleCreated = async (createdCid?: string) => {
    const next = createdCid?.trim();
    if (next) {
      searchForm.setFieldsValue({ keyword: next });
      setSearchKeyword(next);
      setCursorStack([]);
      await refreshList(next, null, true);
      return;
    }
    await refreshList(searchKeyword, null, true);
  };

  const handleCitizenUpdated = useCallback((next: CitizenRow) => {
    setSelectedCitizen(next);
    setRows((prev) => prev.map((row) => (row.cid_number === next.cid_number ? next : row)));
  }, []);

  const statusTag = (status: string | undefined) => (
    status === 'NORMAL' ? <Tag color="green">正常</Tag> : <Tag color="red">注销</Tag>
  );

  const citizenColumns: ColumnsType<CitizenRow> = [
    {
      title: '序号',
      width: 80,
      align: 'center',
      render: (_v: unknown, _r: CitizenRow, idx: number) => cursorStack.length * CITIZEN_PAGE_SIZE + idx + 1,
    },
    {
      title: '护照号',
      dataIndex: 'passport_no',
      align: 'center',
      render: (v: string | undefined) => v ?? '-',
    },
    {
      title: '身份CID',
      dataIndex: 'cid_number',
      align: 'center',
      render: (v: string | undefined) => v ?? '-',
    },
    {
      title: '类型',
      width: 90,
      align: 'center',
      render: (_v: unknown, row) =>
        row.cid_number ? <Tag color="geekblue">{citizenTypeLabel(row.cid_number)}</Tag> : '-',
    },
    {
      title: '姓名',
      align: 'center',
      render: (_v: unknown, row) => makeCitizenName(row),
    },
    {
      title: '当前绑定钱包账户',
      dataIndex: 'ss58_address',
      align: 'center',
      render: (v: string | null | undefined) => v || '-',
    },
    {
      title: '投票状态',
      dataIndex: 'vote_status',
      width: 120,
      align: 'center',
      render: (v: string | undefined) => statusTag(v),
    },
  ];

  if (auth && selectedCitizen) {
    return (
      <CitizenDetailPage
        auth={auth}
        citizen={selectedCitizen}
        canWrite={capabilities.canBusinessWrite}
        provinceName={activeProvinceName}
        cityName={activeCityName}
        onBack={() => setSelectedCitizen(null)}
        onUpdated={handleCitizenUpdated}
      />
    );
  }

  if (auth && activeProvinceName && !activeCityName) {
    return (
      <Card
        title={makeCenteredTitle(activeProvinceName)}
        bordered={false}
        style={glassCardStyle}
        headStyle={glassCardHeadStyle}
      >
        <CityGrid auth={auth} province_name={activeProvinceName} onPick={(cityName) => setSelectedCity(cityName)} />
      </Card>
    );
  }

  return (
    <>
      <Card
        title={makeCenteredTitle(
          activeProvinceName && activeCityName ? `${activeProvinceName} · ${activeCityName}` : '公民/居民列表',
          selectedCity ? () => setSelectedCity(null) : undefined,
        )}
        bordered={false}
        style={glassCardStyle}
        headStyle={glassCardHeadStyle}
      >
        <div
          style={{
            display: 'flex',
            justifyContent: 'space-between',
            alignItems: 'center',
            gap: 16,
            marginBottom: 16,
            flexWrap: 'wrap',
          }}
        >
          <Typography.Title level={4} style={{ margin: 0 }}>
            公民 / 居民列表
          </Typography.Title>
          <Form form={searchForm} layout="inline" onFinish={onSearch} style={{ rowGap: 12 }}>
            <Form.Item name="keyword" style={{ marginBottom: 0 }}>
              <Input
                style={{ width: 420, maxWidth: '72vw' }}
                placeholder="护照号/身份CID/姓名/当前绑定钱包账户"
                allowClear
                disabled={!canUseCitizenList}
                onPressEnter={() => searchForm.submit()}
                suffix={
                  <SearchOutlined
                    onClick={() => searchForm.submit()}
                    style={{ color: loading ? '#999' : '#1677ff', cursor: 'pointer' }}
                  />
                }
              />
            </Form.Item>
            {capabilities.canBusinessWrite && (
              <Form.Item style={{ marginBottom: 0 }}>
                <Button
                  type="primary"
                  icon={<PlusOutlined />}
                  disabled={!canUseCitizenList}
                  onClick={() => setCreateModalOpen(true)}
                >
                  新增公民 / 居民
                </Button>
              </Form.Item>
            )}
          </Form>
        </div>
        <Table<CitizenRow>
          rowKey={(r) => `${r.id}`}
          dataSource={rows}
          loading={loading}
          pagination={false}
          columns={citizenColumns}
          scroll={{ x: 980 }}
          onRow={(record) => ({
            onClick: (event) => {
              // 行点击进入公民详情页,详情页再承接钱包签名与链上推送。
              const target = event.target as EventTarget | null;
              if (target instanceof Element && target.closest('[data-row-action="true"]')) return;
              setSelectedCitizen(record);
            },
            style: { cursor: 'pointer' },
          })}
        />
        <Space style={{ marginTop: 12, width: '100%', justifyContent: 'flex-end' }}>
          <Typography.Text type="secondary">
            第 {cursorStack.length + 1} 页 · 每页 {CITIZEN_PAGE_SIZE} 条
          </Typography.Text>
          <Button disabled={loading || cursorStack.length === 0} onClick={onPrevPage}>
            上一页
          </Button>
          <Button disabled={loading || !nextCursor} onClick={onNextPage}>
            下一页
          </Button>
        </Space>
      </Card>

      {capabilities.canBusinessWrite && (
        <CitizenCreateModal
          auth={auth}
          open={createModalOpen}
          provinceName={activeProvinceName}
          cityName={activeCityName}
          onClose={() => setCreateModalOpen(false)}
          onCreated={handleCreated}
        />
      )}
    </>
  );
}

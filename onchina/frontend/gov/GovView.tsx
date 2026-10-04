// 公权机构页面入口。市公安局已折叠为普通公权机构,统一在公权机构列表展示。
//
// 视觉完全复用"注册局"的 Card 样式(glassCardStyle / glassCardHeadStyle),
// 保证横线颜色、毛玻璃底、绿色左竖条与注册局完全一致。
//
// Card title 布局:
//   - 左侧绝对定位返回按钮(可选)
//   - 中间绝对居中标题
//   - 右侧由 Card.extra 承载(机构表格页的"+ 新增")

import React, { useEffect, useState } from 'react';
import { Button, Card, Input } from 'antd';
import { SearchOutlined } from '@ant-design/icons';
import { ProvinceGrid } from '../core/ProvinceGrid';
import { CityGrid } from '../core/CityGrid';
import { GovListTable } from './GovListTable';
import { GovCreateModal } from './GovCreateModal';
import { GovDetailPage } from './GovDetailPage';
import { useScope } from '../hooks/useScope';
import type { AdminAuth } from '../auth/types';
import type { CidMetaResult } from '../china/api';
import { glassCardStyle, glassCardHeadStyle } from '../core/cardStyles';

interface Props {
  auth: AdminAuth;
  cidMeta: CidMetaResult | null;
  resetToken?: number;
}

/** 可复用的 Card title 布局:左(可选返回) + 中间绝对居中标题 */
function makeCenteredTitle(center: React.ReactNode, back?: () => void, backLabel?: string) {
  return (
    <div style={{ position: 'relative', display: 'flex', alignItems: 'center', minHeight: 32 }}>
      {back && (
        <Button type="link" style={{ paddingLeft: 0 }} onClick={back}>
          ← {backLabel ?? '返回'}
        </Button>
      )}
      <span style={{ position: 'absolute', left: '50%', transform: 'translateX(-50%)' }}>
        {center}
      </span>
    </div>
  );
}

export const GovView: React.FC<Props> = ({ auth, cidMeta, resetToken = 0 }) => {
  const scope = useScope(auth);
  const [selectedProvince, setSelectedProvince] = useState<string | null>(null);
  const [selectedCity, setSelectedCity] = useState<string | null>(null);
  const [selectedCidNumber, setSelectedCidNumber] = useState<string | null>(null);
  const [createOpen, setCreateOpen] = useState(false);
  const [refreshKey, setRefreshKey] = useState(0);
  // 市详情页的机构列表搜索:输入不自动触发,点搜索图标提交 → committedSearch
  const [searchInput, setSearchInput] = useState('');
  const [committedSearch, setCommittedSearch] = useState('');

  useEffect(() => {
    // 顶层 tab 切换必须打断详情页状态,避免停留在旧详情。
    setSelectedProvince(null);
    setSelectedCity(null);
    setSelectedCidNumber(null);
    setCreateOpen(false);
    setSearchInput('');
    setCommittedSearch('');
  }, [auth.account_id, resetToken]);

  const provinces = cidMeta?.provinces || [];
  const lockedProvinceName = scope.lockedProvinceName;
  const lockedCityName = scope.lockedCityName;

  // 机构详情页由公权详情页组件接管,列表页自身只维护返回状态。
  if (selectedCidNumber) {
    return (
      <GovDetailPage
        auth={auth}
        cidNumber={selectedCidNumber}
        canWrite={scope.canWrite}
        onBack={() => {
          setSelectedCidNumber(null);
          setRefreshKey((k) => k + 1);
        }}
      />
    );
  }

  const effectiveProvince = selectedProvince ?? lockedProvinceName;
  const effectiveCity = selectedCity ?? (scope.skipCityList ? lockedCityName : null);

  // 普通公权目录由后端自动生成;手动新增两能力:公权机构(G,ZF/LF/SF/JC)
  // 和公权下属非法人(F,挂公法人)。JY 教育机构统一归教育 tab。
  const onSubmitSearch = () => {
    setCommittedSearch(searchInput.trim());
  };

  let title: React.ReactNode;
  let extra: React.ReactNode;
  let body: React.ReactNode;

  if (!effectiveProvince) {
    // ── 首屏:省份列表(SHENG/SHI 角色 effectiveProvince 一定有值,此分支留作未来扩展) ──
    title = '省份列表';
    body = <ProvinceGrid provinces={provinces} onPick={(p) => setSelectedProvince(p)} />;
  } else if (!effectiveCity) {
    // ── 公权/私权的省详情 = 该省市列表 ──
    const canGoBack = !scope.skipProvinceList;
    title = makeCenteredTitle(
      effectiveProvince,
      canGoBack ? () => setSelectedProvince(null) : undefined,
      '返回省列表'
    );
    body = <CityGrid auth={auth} province_name={effectiveProvince} onPick={(c) => setSelectedCity(c)} />;
  } else {
    // ── 公权/私权的市详情 = 该市机构表格 ──
    // 不使用 Card 的 extra(否则 extra 占用 title 空间,中间标题会被挤到左侧)。
    // 整个 Card head 用三列 flex:左返回 + 中标题 + 右(搜索+新增),
    // 左右两列 flex:1 等宽,保证中间标题相对整个 Card 真正居中;
    // 搜索框 flex:1,自适应右列剩余宽度。
    const canGoBack = !scope.skipCityList;
    title = (
      <div style={{ display: 'flex', alignItems: 'center', width: '100%', gap: 16, minHeight: 32 }}>
        <div style={{ flex: '1 1 0', display: 'flex', alignItems: 'center' }}>
          {canGoBack && (
            <Button type="link" style={{ paddingLeft: 0 }} onClick={() => setSelectedCity(null)}>
              ← 返回
            </Button>
          )}
        </div>
        <div style={{ flex: '0 0 auto', fontWeight: 500 }}>
          {effectiveProvince} · {effectiveCity}
        </div>
        <div style={{ flex: '1 1 0', display: 'flex', alignItems: 'center', justifyContent: 'flex-end', gap: 8, minWidth: 0 }}>
          <Input
            value={searchInput}
            placeholder="请输入机构全称、简称或身份ID"
            allowClear
            style={{ flex: '1 1 auto', maxWidth: 360, minWidth: 0 }}
            onChange={(e) => {
              const v = e.target.value;
              setSearchInput(v);
              if (!v) setCommittedSearch('');
            }}
            onPressEnter={onSubmitSearch}
            suffix={
              <span
                style={{ cursor: 'pointer', color: '#1890ff' }}
                onClick={onSubmitSearch}
                title="搜索"
              >
                <SearchOutlined />
              </span>
            }
          />
          {scope.canWrite && (
            <Button type="primary" onClick={() => setCreateOpen(true)}>
              + 新增
            </Button>
          )}
        </div>
      </div>
    );
    extra = null;
    body = (
      <GovListTable
        auth={auth}
        province_name={effectiveProvince}
        city_name={effectiveCity}
        refreshKey={refreshKey}
        searchQuery={committedSearch}
        onSelectInstitution={(cidNumber) => setSelectedCidNumber(cidNumber)}
      />
    );
  }

  return (
    <>
      <Card
        title={title}
        extra={extra}
        bordered={false}
        style={glassCardStyle}
        headStyle={glassCardHeadStyle}
      >
        {body}
      </Card>

      <GovCreateModal
        auth={auth}
        open={createOpen}
        lockedProvinceName={effectiveProvince}
        lockedCityName={effectiveCity}
        onCancel={() => setCreateOpen(false)}
        onCreated={(result) => {
          setCreateOpen(false);
          setRefreshKey((k) => k + 1);
          // 创建成功跳详情:G 公权机构只读查看,F 非法人补名称/查看所属法人
          if (result?.cid_number) setSelectedCidNumber(result.cid_number);
        }}
      />
    </>
  );
};

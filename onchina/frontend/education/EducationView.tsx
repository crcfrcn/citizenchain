// 教育机构页面入口。JY 教育机构统一在这里管理:
// 市详情直接展示本市市公民教育委员会,学校和 F+JY 非法人教育机构按名称或身份ID精确搜索。
//
// 视觉完全复用"注册局"的 Card 样式(glassCardStyle / glassCardHeadStyle),
// 保证横线颜色、毛玻璃底、绿色左竖条与注册局完全一致。
//
// 详情页复用 gov/GovDetailPage 调度:S/F 学校(存储 category=PRIVATE_INSTITUTION)走
// PrivateDetailLayout 完整编辑布局(含 F 改挂所属法人),G 学校(GOV_INSTITUTION)走只读布局。

import React, { useState } from 'react';
import { Button, Card, Input } from 'antd';
import { SearchOutlined } from '@ant-design/icons';
import { ProvinceGrid } from '../core/ProvinceGrid';
import { CityGrid } from '../core/CityGrid';
import { EducationListTable } from './EducationListTable';
import { EducationCreateModal } from './EducationCreateModal';
import { GovDetailPage } from '../gov/GovDetailPage';
import { useScope } from '../hooks/useScope';
import type { AdminAuth } from '../auth/types';
import type { CidMetaResult } from '../china/api';
import { glassCardStyle, glassCardHeadStyle } from '../core/cardStyles';

interface Props {
  auth: AdminAuth;
  cidMeta: CidMetaResult | null;
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

export const EducationView: React.FC<Props> = ({ auth, cidMeta }) => {
  const scope = useScope(auth);
  const [selectedProvince, setSelectedProvince] = useState<string | null>(null);
  const [selectedCity, setSelectedCity] = useState<string | null>(null);
  const [selectedCidNumber, setSelectedCidNumber] = useState<string | null>(null);
  const [createOpen, setCreateOpen] = useState(false);
  const [refreshKey, setRefreshKey] = useState(0);
  // 市详情页的机构列表搜索:输入不自动触发,点搜索图标提交 → committedSearch
  const [searchInput, setSearchInput] = useState('');
  const [committedSearch, setCommittedSearch] = useState('');

  const provinces = cidMeta?.provinces || [];
  const lockedProvinceName = scope.lockedProvinceName;
  const lockedCityName = scope.lockedCityName;

  // 机构详情页由公权详情页组件按存储 category 调度,列表页自身只维护返回状态。
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

  const createLabel = '新增';

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
    // ── 省详情 = 该省市列表 ──
    const canGoBack = !scope.skipProvinceList;
    title = makeCenteredTitle(
      effectiveProvince,
      canGoBack ? () => setSelectedProvince(null) : undefined,
      '返回省列表'
    );
    body = <CityGrid auth={auth} province_name={effectiveProvince} onPick={(c) => setSelectedCity(c)} />;
  } else {
    // ── 市详情 = 该市教育机构表格 ──
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
            placeholder="请输入教育机构全称、简称或身份ID"
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
              + {createLabel}
            </Button>
          )}
        </div>
      </div>
    );
    extra = null;
    body = (
      <EducationListTable
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

      <EducationCreateModal
        auth={auth}
        open={createOpen}
        lockedProvinceName={effectiveProvince}
        lockedCityName={effectiveCity}
        onCancel={() => setCreateOpen(false)}
        onCreated={(result) => {
          setCreateOpen(false);
          setRefreshKey((k) => k + 1);
          // 创建成功直接跳详情页,继续维护账户和资料库。
          if (result?.cid_number) {
            setSelectedCidNumber(result.cid_number);
          }
        }}
      />
    </>
  );
};

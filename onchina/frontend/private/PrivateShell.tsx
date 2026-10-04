// 私权机构前端 Shell。根层只负责省市选择和详情页跳转;
// 顶层 App 已直接显示六类 Tab,本文件只渲染当前选中的那一类。

import React, { useState } from 'react';
import { Button, Card } from 'antd';
import { ProvinceGrid } from '../core/ProvinceGrid';
import { CityGrid } from '../core/CityGrid';
import { PrivateDetailPage } from './PrivateDetailPage';
import { useScope } from '../hooks/useScope';
import type { AdminAuth } from '../auth/types';
import type { CidMetaResult } from '../china/api';
import type { PrivateType } from '../subjects/api';
import { glassCardHeadStyle, glassCardStyle } from '../core/cardStyles';
import { SolePage } from './sole';
import { PartnershipPage } from './partnership';
import { CompanyPage } from './company';
import { CorporationPage } from './corporation';
import { WelfarePage } from './welfare';
import { AssociationPage } from './association';

interface Props {
  auth: AdminAuth;
  cidMeta: CidMetaResult | null;
  privateType: PrivateType;
}

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

export const PrivateShell: React.FC<Props> = ({ auth, cidMeta, privateType }) => {
  const scope = useScope(auth);
  const [selectedProvince, setSelectedProvince] = useState<string | null>(null);
  const [selectedCity, setSelectedCity] = useState<string | null>(null);
  const [selectedCidNumber, setSelectedCidNumber] = useState<string | null>(null);

  const provinces = cidMeta?.provinces || [];
  const effectiveProvince = selectedProvince ?? scope.lockedProvinceName;
  const effectiveCity = selectedCity ?? (scope.skipCityList ? scope.lockedCityName : null);

  if (selectedCidNumber) {
    return (
      <PrivateDetailPage
        auth={auth}
        cidNumber={selectedCidNumber}
        canWrite={scope.canWrite}
        onBack={() => setSelectedCidNumber(null)}
      />
    );
  }

  let title: React.ReactNode;
  let body: React.ReactNode;

  if (!effectiveProvince) {
    title = '省份列表';
    body = <ProvinceGrid provinces={provinces} onPick={(p) => setSelectedProvince(p)} />;
  } else if (!effectiveCity) {
    const canGoBack = !scope.skipProvinceList;
    title = makeCenteredTitle(
      effectiveProvince,
      canGoBack ? () => setSelectedProvince(null) : undefined,
      '返回省列表',
    );
    body = <CityGrid auth={auth} province_name={effectiveProvince} onPick={(c) => setSelectedCity(c)} />;
  } else {
    const canGoBack = !scope.skipCityList;
    const commonProps = {
      auth,
      province_name: effectiveProvince,
      city_name: effectiveCity,
      canWrite: scope.canWrite,
      onSelectInstitution: (cidNumber: string) => setSelectedCidNumber(cidNumber),
    };
    title = makeCenteredTitle(
      `${effectiveProvince} · ${effectiveCity}`,
      canGoBack ? () => setSelectedCity(null) : undefined,
      '返回',
    );
    body =
      privateType === 'SOLE' ? (
        <SolePage {...commonProps} />
      ) : privateType === 'PARTNERSHIP' ? (
        <PartnershipPage {...commonProps} />
      ) : privateType === 'COMPANY' ? (
        <CompanyPage {...commonProps} />
      ) : privateType === 'CORPORATION' ? (
        <CorporationPage {...commonProps} />
      ) : privateType === 'WELFARE' ? (
        <WelfarePage {...commonProps} />
      ) : (
        <AssociationPage {...commonProps} />
      );
  }

  return (
    <Card title={title} bordered={false} style={glassCardStyle} headStyle={glassCardHeadStyle}>
      {body}
    </Card>
  );
};

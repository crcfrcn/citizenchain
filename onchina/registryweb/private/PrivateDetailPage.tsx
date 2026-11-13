// 私权机构详情页只调度私权布局,不得承载公权机构业务。

import React, { useCallback, useEffect, useState } from 'react';
import { Typography } from 'antd';
import { getInstitution, type InstitutionDetail } from './api';
import type { AdminAuth } from '@gmb/onchina-platform/authentication/types';
import { PrivateDetailLayout } from './PrivateDetailLayout';
import {
  institutionDetailCacheKey,
  readCachedInstitutionDetail,
  writeCachedInstitutionDetail,
} from '@gmb/onchina-platform/codes/cache';

interface Props {
  auth: AdminAuth;
  cidNumber: string;
  canWrite: boolean;
  onBack: () => void;
}



export const PrivateDetailPage: React.FC<Props> = ({ auth, cidNumber, canWrite, onBack }) => {
  const detailCacheKey = institutionDetailCacheKey(auth, cidNumber);
  const [detail, setDetail] = useState<InstitutionDetail | null>(() =>
    readCachedInstitutionDetail(detailCacheKey),
  );
  const [loading, setLoading] = useState(false);

  const load = useCallback(() => {
    const cached = readCachedInstitutionDetail(detailCacheKey);
    if (cached) {
      setDetail(cached);
      setLoading(false);
    } else {
      setDetail(null);
      setLoading(true);
    }
    getInstitution(auth, cidNumber)
      .then((next) => {
        setDetail(next);
        writeCachedInstitutionDetail(detailCacheKey, next);
      })
      .catch(() => {
        // 详情后台刷新失败时保留缓存,避免切页时闪断。
      })
      .finally(() => {
        if (!cached) setLoading(false);
      });
  }, [auth.accessToken, detailCacheKey, cidNumber]);

  useEffect(() => {
    load();
  }, [load]);

  const inst = detail?.institution;

  return (
    <div>
      {loading && !inst && <Typography.Text type="secondary">加载中...</Typography.Text>}

      {inst && detail && (
        <PrivateDetailLayout
          auth={auth}
          detail={detail}
          canWrite={canWrite}
          loading={loading}
          onReload={load}
          onBack={onBack}
        />
      )}

    </div>
  );
};

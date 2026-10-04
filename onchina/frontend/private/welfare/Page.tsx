// 公益组织页面。只承接公益组织列表、新增和进入详情。

import React from 'react';
import type { PrivateTypePageProps } from '../common/PrivateTypePage';
import { PrivateTypePage } from '../common/PrivateTypePage';
import { createWelfareInstitution, listWelfareInstitutions } from './api';
import { WELFARE_PRIVATE_TYPE, WELFARE_TITLE } from './types';

export type WelfarePageProps = Omit<
  PrivateTypePageProps,
  'privateType' | 'title' | 'createInstitution' | 'listInstitutions'
>;

export const WelfarePage: React.FC<WelfarePageProps> = (props) => (
  <PrivateTypePage
    {...props}
    privateType={WELFARE_PRIVATE_TYPE}
    title={WELFARE_TITLE}
    createInstitution={createWelfareInstitution}
    listInstitutions={listWelfareInstitutions}
  />
);

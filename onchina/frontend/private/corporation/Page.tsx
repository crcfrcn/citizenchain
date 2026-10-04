// 股份公司页面。只承接股份公司列表、新增和进入详情。

import React from 'react';
import type { PrivateTypePageProps } from '../common/PrivateTypePage';
import { PrivateTypePage } from '../common/PrivateTypePage';
import { createCorporationInstitution, listCorporationInstitutions } from './api';
import { CORPORATION_PRIVATE_TYPE, CORPORATION_TITLE } from './types';

export type CorporationPageProps = Omit<
  PrivateTypePageProps,
  'privateType' | 'title' | 'createInstitution' | 'listInstitutions'
>;

export const CorporationPage: React.FC<CorporationPageProps> = (props) => (
  <PrivateTypePage
    {...props}
    privateType={CORPORATION_PRIVATE_TYPE}
    title={CORPORATION_TITLE}
    createInstitution={createCorporationInstitution}
    listInstitutions={listCorporationInstitutions}
  />
);

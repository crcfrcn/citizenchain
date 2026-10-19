// 股份公司页面。只承接股份公司列表、新增和进入详情。

import React from 'react';
import type { PrivateTypePageProps } from './private_PrivateTypePage';
import { PrivateTypePage } from './private_PrivateTypePage';
import { createCorporationInstitution, listCorporationInstitutions } from './corporation_api';
import { CORPORATION_PRIVATE_TYPE, CORPORATION_TITLE } from './corporation_types';

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

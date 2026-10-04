// 合伙企业页面。只承接合伙企业列表、新增和进入详情。

import React from 'react';
import type { PrivateTypePageProps } from '../common/PrivateTypePage';
import { PrivateTypePage } from '../common/PrivateTypePage';
import { createPartnershipInstitution, listPartnershipInstitutions } from './api';
import { PARTNERSHIP_PRIVATE_TYPE, PARTNERSHIP_TITLE } from './types';

export type PartnershipPageProps = Omit<
  PrivateTypePageProps,
  'privateType' | 'title' | 'createInstitution' | 'listInstitutions'
>;

export const PartnershipPage: React.FC<PartnershipPageProps> = (props) => (
  <PrivateTypePage
    {...props}
    privateType={PARTNERSHIP_PRIVATE_TYPE}
    title={PARTNERSHIP_TITLE}
    createInstitution={createPartnershipInstitution}
    listInstitutions={listPartnershipInstitutions}
  />
);

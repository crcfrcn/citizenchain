// 股权公司页面。只承接股权公司列表、新增和进入详情。

import React from 'react';
import type { PrivateTypePageProps } from '../common/PrivateTypePage';
import { PrivateTypePage } from '../common/PrivateTypePage';
import { createCompanyInstitution, listCompanyInstitutions } from './api';
import { COMPANY_PRIVATE_TYPE, COMPANY_TITLE } from './types';

export type CompanyPageProps = Omit<
  PrivateTypePageProps,
  'privateType' | 'title' | 'createInstitution' | 'listInstitutions'
>;

export const CompanyPage: React.FC<CompanyPageProps> = (props) => (
  <PrivateTypePage
    {...props}
    privateType={COMPANY_PRIVATE_TYPE}
    title={COMPANY_TITLE}
    createInstitution={createCompanyInstitution}
    listInstitutions={listCompanyInstitutions}
  />
);

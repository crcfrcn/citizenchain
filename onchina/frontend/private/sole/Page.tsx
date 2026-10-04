// 个体经营页面。只承接个体经营列表、新增和进入详情。

import React from 'react';
import type { PrivateTypePageProps } from '../common/PrivateTypePage';
import { PrivateTypePage } from '../common/PrivateTypePage';
import { createSoleInstitution, listSoleInstitutions } from './api';
import { SOLE_PRIVATE_TYPE, SOLE_TITLE } from './types';

export type SolePageProps = Omit<
  PrivateTypePageProps,
  'privateType' | 'title' | 'createInstitution' | 'listInstitutions'
>;

export const SolePage: React.FC<SolePageProps> = (props) => (
  <PrivateTypePage
    {...props}
    privateType={SOLE_PRIVATE_TYPE}
    title={SOLE_TITLE}
    createInstitution={createSoleInstitution}
    listInstitutions={listSoleInstitutions}
  />
);

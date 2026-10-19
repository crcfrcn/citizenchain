// 个体经营页面。只承接个体经营列表、新增和进入详情。

import React from 'react';
import type { PrivateTypePageProps } from './private_PrivateTypePage';
import { PrivateTypePage } from './private_PrivateTypePage';
import { createSoleInstitution, listSoleInstitutions } from './sole_api';
import { SOLE_PRIVATE_TYPE, SOLE_TITLE } from './sole_types';

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

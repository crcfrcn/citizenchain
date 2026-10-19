// 注册协会页面。只承接注册协会列表、新增和进入详情。

import React from 'react';
import type { PrivateTypePageProps } from './private_PrivateTypePage';
import { PrivateTypePage } from './private_PrivateTypePage';
import { createAssociationInstitution, listAssociationInstitutions } from './association_api';
import { ASSOCIATION_PRIVATE_TYPE, ASSOCIATION_TITLE } from './association_types';

export type AssociationPageProps = Omit<
  PrivateTypePageProps,
  'privateType' | 'title' | 'createInstitution' | 'listInstitutions'
>;

export const AssociationPage: React.FC<AssociationPageProps> = (props) => (
  <PrivateTypePage
    {...props}
    privateType={ASSOCIATION_PRIVATE_TYPE}
    title={ASSOCIATION_TITLE}
    createInstitution={createAssociationInstitution}
    listInstitutions={listAssociationInstitutions}
  />
);

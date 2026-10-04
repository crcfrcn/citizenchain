// 注册协会页面。只承接注册协会列表、新增和进入详情。

import React from 'react';
import type { PrivateTypePageProps } from '../common/PrivateTypePage';
import { PrivateTypePage } from '../common/PrivateTypePage';
import { createAssociationInstitution, listAssociationInstitutions } from './api';
import { ASSOCIATION_PRIVATE_TYPE, ASSOCIATION_TITLE } from './types';

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

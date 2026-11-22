// 按公立、私立及分校规则提供教育机构登记表单。

import React from 'react';
import type { AdminAuth } from '@gmb/onchina-platform/authentication/types';
import { CreateInstitutionForm } from '../controls/CreateInstitutionForm';
import {
  checkCidFullName,
  createInstitution,
  searchParentInstitutions,
  type CreateInstitutionOutput,
} from './api';

interface Props {
  auth: AdminAuth;
  open: boolean;
  lockedProvinceName: string | null;
  lockedCityName: string | null;
  onCancel: () => void;
  onCreated: (result: CreateInstitutionOutput) => void;
}

export const EducationCreateModal: React.FC<Props> = (props) => (
  <CreateInstitutionForm
    {...props}
    category="EDUCATION_FORM"
    checkCidFullName={checkCidFullName}
    createInstitution={createInstitution}
    searchParentInstitutions={searchParentInstitutions}
  />
);

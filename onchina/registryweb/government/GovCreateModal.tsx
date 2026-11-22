// 提供公法人及公权下属非法人机构的登记表单。

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

export const GovCreateModal: React.FC<Props> = (props) => (
  <CreateInstitutionForm
    {...props}
    category="GOV_INSTITUTION"
    checkCidFullName={checkCidFullName}
    createInstitution={createInstitution}
    searchParentInstitutions={searchParentInstitutions}
  />
);

// 根据六类私权机构规则提供登记弹窗并复用共用表单。

import React from 'react';
import type { AdminAuth } from '@gmb/onchina-platform/authentication/types';
import { CreateInstitutionForm } from '../controls/CreateInstitutionForm';
import type { CreateInstitutionInput, CreateInstitutionOutput, PrivateType } from '@gmb/onchina-platform/institutions/api';
import {
  checkCidFullName,
  searchParentInstitutions,
} from './api';

interface Props {
  auth: AdminAuth;
  open: boolean;
  lockedProvinceName: string | null;
  lockedCityName: string | null;
  privateType: PrivateType;
  createInstitution: (auth: AdminAuth, input: CreateInstitutionInput) => Promise<CreateInstitutionOutput>;
  onCancel: () => void;
  onCreated: (result: CreateInstitutionOutput) => void;
}

export const PrivateCreateModal: React.FC<Props> = (props) => (
  <CreateInstitutionForm
    {...props}
    category="PRIVATE_INSTITUTION"
    privateType={props.privateType}
    checkCidFullName={checkCidFullName}
    createInstitution={props.createInstitution}
    searchParentInstitutions={searchParentInstitutions}
  />
);

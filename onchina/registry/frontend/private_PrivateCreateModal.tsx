// 私权机构新增入口。类型由上层六个 Tab 传入,后端据此锁定主体属性、机构码与法人资格。
// JY 教育机构的新增统一在教育机构 tab(education 模块)。
// 表单 UI 复用 onchina/registry/frontend/core_CreateInstitutionForm.tsx,本文件只负责注入 private API。

import React from 'react';
import type { AdminAuth } from '../../platform/frontend/authentication_types';
import { CreateInstitutionForm } from './core_CreateInstitutionForm';
import type { CreateInstitutionInput, CreateInstitutionOutput, PrivateType } from '../../platform/frontend/subjects_api';
import {
  checkCidFullName,
  searchParentInstitutions,
} from './private_api';

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

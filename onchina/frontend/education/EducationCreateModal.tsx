// 教育机构新增入口。机构锁死教育委员会(JY),主体属性 G(公立)/S(私立)/F(分校,
// 必选本市学校本部为所属法人)。表单 UI 复用 core/institution,本文件只负责注入 education API。

import React from 'react';
import type { AdminAuth } from '../auth/types';
import { CreateInstitutionForm } from '../core/CreateInstitutionForm';
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

// 公权机构新增入口,两种能力:
//   G 公法人 → 新增公权机构(ZF/LF/SF/JC,排除储备体系自动目录代码,机构全称必填同市查重)
//   F 非法人 → 公权下属非法人(必选公法人为所属法人:本市市级/本省省级/国家级,盈利属性锁非盈利)
// JY 教育机构归教育 tab;普通公权目录仍由后端自动生成。
// 表单 UI 复用 core/institution,本文件只负责注入 gov API。

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

export const GovCreateModal: React.FC<Props> = (props) => (
  <CreateInstitutionForm
    {...props}
    category="GOV_INSTITUTION"
    checkCidFullName={checkCidFullName}
    createInstitution={createInstitution}
    searchParentInstitutions={searchParentInstitutions}
  />
);

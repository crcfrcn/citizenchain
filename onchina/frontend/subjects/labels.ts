// 按表单入口(category)+ subject_property 决定"新增机构"弹窗里哪些字段锁定 + 默认值。
// private/gov/education 三个新增弹窗共用这一份字段锁定规则,组件分别放在各自业务目录。
//
// 手动新增三个入口(普通官方公权目录由后端自动生成,不走手动创建):
//   PRIVATE_INSTITUTION   私权 tab:按 private_type 锁定主体属性和机构码,创建阶段写入名称
//   GOV_INSTITUTION       公权 tab:G(非自动官方目录机构)机构全称必填 / F(锁非法人组织 UNIN)挂公法人
//   EDUCATION_FORM 教育 tab:G/S/F;institution_code 不再锁死,按 subject_property×education_type 计算
//                         (公私大学 GUN/SUN、公私中小学 GSCH/SFSC),F 分校继承本部学校码(GSCH/SFSC)
//
// P1 盈利属性统一按主体属性联动(见 p1LocksForSubject,与后端号码生成器/unincorporated_org 同源):
//   G → 锁死 0(非盈利,生成器硬规则)
//   S → 可选 0/1,默认 1
//   F → 个体经营/无限合伙为独立非法人;教育分校/公权下属非法人继承所属法人
//
// 非法人(F)挂靠只用于从属非法人。个体经营(F+GT)和无限合伙(F+GP)不选择所属法人。

import type { EducationType, PartnershipKind, PrivateType } from './api';

export type ChoiceItem = { value: string; label: string };

/** 手动新增表单的入口类型(查询/存储 category 见 subjects/api.ts 的 InstitutionCategory)。 */
export type CreateFormCategory =
  | 'PRIVATE_INSTITUTION'
  | 'GOV_INSTITUTION'
  | 'EDUCATION_FORM';

export interface InstitutionFieldLocks {
  /** subject_property 的候选列表;长度=1 时锁死第一项 */
  subjectPropertyChoices: ChoiceItem[];
  /** institution 代码的初始候选列表(随主体属性变化见 institutionChoicesFor) */
  institutionChoices: ChoiceItem[];
  /** 弹窗标题 */
  modalTitle: string;
}

// ── SubjectProperty 中文映射 ──
export const SUBJECT_PROPERTY_LABEL: Record<string, string> = {
  G: '公法人',
  S: '私法人',
  F: '非法人',
  M: '公民',
  Z: '自然人',
  N: '智能人',
};

// 机构代码中文映射已删除:改由后端 /api/public/cid/labels 单源下发(primitives code.rs)。
// 消费方改用 `useInstitutionCodeLabels()`(见 subjects/institutionLabels.ts)。

export const EDUCATION_TYPE_LABEL: Record<EducationType, string> = {
  NATIONAL_CITIZEN_EDU_COMMITTEE: '国家公民教育委员会',
  CITY_CITIZEN_EDU_COMMITTEE: '市公民教育委员会',
  EARLY_SCHOOL: '初学',
  PRIMARY_SCHOOL: '小学',
  SECONDARY_SCHOOL: '中学',
  UNIVERSITY: '大学',
};

export const SCHOOL_EDUCATION_TYPE_OPTIONS: ChoiceItem[] = [
  { value: 'EARLY_SCHOOL', label: EDUCATION_TYPE_LABEL.EARLY_SCHOOL },
  { value: 'PRIMARY_SCHOOL', label: EDUCATION_TYPE_LABEL.PRIMARY_SCHOOL },
  { value: 'SECONDARY_SCHOOL', label: EDUCATION_TYPE_LABEL.SECONDARY_SCHOOL },
  { value: 'UNIVERSITY', label: EDUCATION_TYPE_LABEL.UNIVERSITY },
];

export const SCHOOL_EDUCATION_TYPES: EducationType[] = [
  'EARLY_SCHOOL',
  'PRIMARY_SCHOOL',
  'SECONDARY_SCHOOL',
  'UNIVERSITY',
];

// ── 运行期公权机构可选代码。
// 创世只写国家/省/市当前骨架;镇级公权机构以后由注册局选择 town_code 注册上链。
// CEDU 归教育 tab,储备体系/固定治理码不在本表单创建。
const GOV_MANUAL_INSTITUTIONS: ChoiceItem[] = [
  { value: 'CGOV', label: '政府' },
  { value: 'CLEG', label: '立法院' },
  { value: 'CJUD', label: '司法院' },
  { value: 'CSUP', label: '监察院' },
  { value: 'TGOV', label: '镇政府' },
  { value: 'TCWF', label: '镇民生科' },
  { value: 'THUD', label: '镇住建科' },
  { value: 'TAGR', label: '镇农业科' },
  { value: 'TFIN', label: '镇财税科' },
  { value: 'TDEF', label: '镇国防科' },
  { value: 'THSC', label: '镇国安科' },
  { value: 'TCOM', label: '镇商贸科' },
  { value: 'TENR', label: '镇能源科' },
  { value: 'TTRN', label: '镇交通科' },
  { value: 'TPOL', label: '镇公安科' },
  { value: 'TSLF', label: '镇自治会' },
  { value: 'TSUP', label: '镇监察院' },
  { value: 'TJUD', label: '镇司法院' },
];

// ── 公权下属非法人锁死非法人组织(UNIN) ──
const GOV_UNINORG_INSTITUTION_ONLY: ChoiceItem[] = [
  { value: 'UNIN', label: '非法人组织' },
];

// ── 教育机构码计算:institution_code 不是静态下拉,按 subject_property×education_type 派生 ──
//   G + UNIVERSITY → GUN(公立大学)        S + UNIVERSITY → SUN(私立大学)
//   G + 中小初学   → GSCH(公立学校)        S + 中小初学   → SFSC(私立学校)
//   F(分校)       → UNIN(非法人组织):后端模型里分校是挂学校本部(GUN/SUN/GSCH/SFSC)的
//                   非法人组织,只有 UNIN 触发 requires_parent;分校本身不带教育级别。
export const EDUCATION_UNIVERSITY_TYPE: EducationType = 'UNIVERSITY';

/**
 * 教育入口提交前派生 institution_code(与后端 number/code.rs + subjects/unincorporated_org 同源)。
 * @param subjectProperty UI 导航属性 G(公立)/S(私立)/F(分校)
 * @param educationType   本部教育级别(大学走 GUN/SUN,中小初学走 GSCH/SFSC);分校忽略
 */
export function computeEducationInstitutionCode(
  subjectProperty: string,
  educationType: EducationType | undefined,
): string {
  // 分校 = 非法人组织(UNIN)挂学校本部;UNIN 不带教育级别、由后端按父级判定地域。
  if (subjectProperty === 'F') {
    return 'UNIN';
  }
  const isPrivate = subjectProperty === 'S';
  if (educationType === EDUCATION_UNIVERSITY_TYPE) {
    return isPrivate ? 'SUN' : 'GUN';
  }
  // 初学/小学/中学
  return isPrivate ? 'SFSC' : 'GSCH';
}

// ── P1 盈利属性选项(单一来源,锁死场景取单项) ──
const P1_PROFIT: ChoiceItem = { value: '1', label: '盈利' };
const P1_NON_PROFIT: ChoiceItem = { value: '0', label: '非盈利' };

export const PRIVATE_TYPE_LABEL: Record<PrivateType, string> = {
  SOLE: '个体经营',
  PARTNERSHIP: '合伙企业',
  COMPANY: '股权公司',
  CORPORATION: '股份公司',
  WELFARE: '公益组织',
  ASSOCIATION: '注册协会',
};

export const PARTNERSHIP_KIND_LABEL: Record<PartnershipKind, string> = {
  GENERAL: '无限合伙',
  LIMITED: '有限合伙',
};

export interface PrivateTypeRule {
  privateType: PrivateType;
  partnershipKind?: PartnershipKind;
  subjectProperty: 'S' | 'F';
  institution: string;
  p1: '0' | '1';
  hasLegalPersonality: boolean;
}

export const PRIVATE_TYPE_RULES: Record<PrivateType, PrivateTypeRule> = {
  SOLE: {
    privateType: 'SOLE',
    subjectProperty: 'F',
    institution: 'SFGT',
    p1: '1',
    hasLegalPersonality: false,
  },
  PARTNERSHIP: {
    privateType: 'PARTNERSHIP',
    partnershipKind: 'GENERAL',
    subjectProperty: 'F',
    institution: 'SFGP',
    p1: '1',
    hasLegalPersonality: false,
  },
  COMPANY: {
    privateType: 'COMPANY',
    subjectProperty: 'S',
    institution: 'SFGQ',
    p1: '1',
    hasLegalPersonality: true,
  },
  CORPORATION: {
    privateType: 'CORPORATION',
    subjectProperty: 'S',
    institution: 'SFGF',
    p1: '1',
    hasLegalPersonality: true,
  },
  WELFARE: {
    privateType: 'WELFARE',
    subjectProperty: 'S',
    institution: 'SFGY',
    p1: '0',
    hasLegalPersonality: true,
  },
  ASSOCIATION: {
    privateType: 'ASSOCIATION',
    subjectProperty: 'S',
    institution: 'SFAS',
    p1: '0',
    hasLegalPersonality: true,
  },
};

export function privateRuleFor(
  privateType: PrivateType,
  partnershipKind?: PartnershipKind,
): PrivateTypeRule {
  if (privateType === 'PARTNERSHIP') {
    if (partnershipKind === 'LIMITED') {
      return {
        privateType,
        partnershipKind,
        subjectProperty: 'S',
        institution: 'SFLP',
        p1: '1',
        hasLegalPersonality: true,
      };
    }
    return { ...PRIVATE_TYPE_RULES.PARTNERSHIP, partnershipKind: 'GENERAL' };
  }
  return PRIVATE_TYPE_RULES[privateType];
}

/** 基础 locks(不依赖 subject_property 动态值的部分) */
export function locksForCategory(category: CreateFormCategory): InstitutionFieldLocks {
  switch (category) {
    case 'GOV_INSTITUTION':
      // G=新公权机构(名称必填同市查重) / F=公权下属非法人(挂公法人)
      return {
        subjectPropertyChoices: [
          { value: 'G', label: '公法人' },
          { value: 'F', label: '非法人' },
        ],
        institutionChoices: GOV_MANUAL_INSTITUTIONS,
        modalTitle: '新增公权机构',
      };
    case 'EDUCATION_FORM':
      // G=公立学校 / S=私立学校 / F=分校(挂本部);institution_code 由
      // computeEducationInstitutionCode 按 subject_property×education_type 派生,非静态下拉。
      return {
        subjectPropertyChoices: [
          { value: 'G', label: '公法人' },
          { value: 'S', label: '私法人' },
          { value: 'F', label: '非法人' },
        ],
        institutionChoices: institutionChoicesFor('EDUCATION_FORM', 'G'),
        modalTitle: '新增教育机构',
      };
    case 'PRIVATE_INSTITUTION':
      return {
        subjectPropertyChoices: [{ value: 'F', label: '非法人' }],
        institutionChoices: [{ value: 'SFGT', label: '个体经营' }],
        modalTitle: '新增机构',
      };
  }
}

/** 教育机构码中文标签(下拉占位/展示用)。 */
export const EDUCATION_INSTITUTION_CODE_LABEL: Record<string, string> = {
  GUN: '公立大学',
  SUN: '私立大学',
  JUN: '教会大学',
  GSCH: '公立学校',
  SFSC: '私立学校',
  JSCH: '教会学校',
  UNIN: '非法人组织',
};

/**
 * 机构代码选项随入口 + 主体属性联动:
 *   GOV 入口 G 建市级公权机构、F 建下属非法人锁 UNIN;
 *   EDUCATION 入口的机构码由 computeEducationInstitutionCode 派生(此处仅给出按
 *   subject_property 的学校默认码占位,提交时按 education_type 复算并覆盖)。
 */
export function institutionChoicesFor(
  category: CreateFormCategory,
  subjectProperty: string,
): ChoiceItem[] {
  if (category === 'EDUCATION_FORM') {
    // 占位默认走中小学码(GSCH/SFSC);真实值在提交时按 education_type/分校本部复算。
    const code = computeEducationInstitutionCode(subjectProperty, undefined);
    return [{ value: code, label: EDUCATION_INSTITUTION_CODE_LABEL[code] ?? code }];
  }
  if (category === 'GOV_INSTITUTION') {
    return subjectProperty === 'G' ? GOV_MANUAL_INSTITUTIONS : GOV_UNINORG_INSTITUTION_ONLY;
  }
  return [{ value: 'SFGT', label: '个体经营' }];
}

/** 非法人盈利属性附属于所属法人:公法人父恒 0,私法人父继承其 p1(与后端 unincorporated_org 同源)。 */
export function inheritedP1(parentSubjectProperty: string, parentP1: string): string {
  return parentSubjectProperty === 'G' ? '0' : parentP1;
}

/**
 * P1 盈利属性统一按主体属性联动(三入口共用,与号码生成器/unincorporated_org 同源):
 *   G → 锁死 0(非盈利);S → 可选 0/1 默认 1;
 *   F → 锁死,继承所属法人;未选父级前 value=undefined(表单必填挡提交)。
 */
export function p1LocksForSubject(
  subjectProperty: string,
  parent: { subject_property: string; p1: string } | null,
): { choices: ChoiceItem[]; value: string | undefined; locked: boolean } {
  if (subjectProperty === 'S') {
    return { choices: [P1_PROFIT, P1_NON_PROFIT], value: '1', locked: false };
  }
  if (subjectProperty === 'F') {
    if (!parent) return { choices: [], value: undefined, locked: true };
    const v = inheritedP1(parent.subject_property, parent.p1);
    return { choices: [v === '1' ? P1_PROFIT : P1_NON_PROFIT], value: v, locked: true };
  }
  // G:公法人恒非盈利
  return { choices: [P1_NON_PROFIT], value: '0', locked: true };
}

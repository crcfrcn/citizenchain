// 机构新增弹窗共享表单(私权/公权/教育三入口共用)。
// 当前只保留待第 6 步复用的资料录入界面；提交按钮固定关闭，任何入口都不能生成
// 已删除的 PublicManage/PrivateManage call 5。新业务模块接入时必须同时提交岗位、权限、
// 任职和投票规则，再重新开放提交。
//
// 主体属性统一联动(与后端号码生成器/subjects/unincorporated_org 同源):
//   G → 盈利属性锁死非盈利;公权入口建公权机构(ZF/LF/SF/JC)、教育入口建公立学校(JY),全称必填
//   S → 私权类型规则锁定 T2/P1;教育私立学校全称必填
//   F → 个体经营/无限合伙是独立非法人;分校等从属非法人必须选择所属法人,
//       搜索范围由后端按地域规则预过滤(分校→本市学校本部;公权→本市市级/本省省级/国家级)

import React, { useEffect, useMemo, useState } from 'react';
import { AutoComplete, Button, Col, Form, Input, Modal, Row, Select, Spin, Typography } from 'antd';
import { DeleteOutlined, PlusOutlined, SearchOutlined } from '@ant-design/icons';
import type { AdminAuth } from '../auth/types';
import { listCidTowns, type CidCityItem, type CidTownItem } from '../china/api';
import { loadCachedCidCities } from '../china/metaCache';
import type {
  CreateInstitutionInput,
  CreateInstitutionOutput,
  EducationType,
  ParentInstitutionRow,
  PartnershipKind,
  PrivateType,
  SearchParentsOptions,
} from '../subjects/api';
import {
  computeEducationInstitutionCode,
  EDUCATION_INSTITUTION_CODE_LABEL,
  inheritedP1,
  institutionChoicesFor,
  locksForCategory,
  p1LocksForSubject,
  privateRuleFor,
  PRIVATE_TYPE_LABEL,
  SCHOOL_EDUCATION_TYPE_OPTIONS,
  SUBJECT_PROPERTY_LABEL,
  type CreateFormCategory,
} from '../subjects/labels';
import { notice } from '../utils/notice';
import { submitChainSign, useChainSign } from './useChainSign';

// 第 6 步新原子创建业务接入前保持关闭；不得用旧 call 5 临时放开。
const INSTITUTION_CREATION_ENABLED = false;

interface FormValues {
  subject_property: string;
  p1: string;
  province_name: string;
  city_name: string;
  town_name?: string;
  institution: string;
  education_type?: EducationType;
  private_type?: PrivateType;
  partnership_kind?: PartnershipKind;
  /** 私权/教育机构/手动公权机构创建时必填全称。 */
  cid_full_name?: string;
  /** 私权/教育机构/手动公权机构创建时必填简称。 */
  cid_short_name?: string;
  /** 需挂靠的非法人必填;个体经营/无限合伙不接受所属法人。 */
  parent_cid_number?: string;
  admins: {
    account_id: string;
    family_name?: string;
    given_name?: string;
  }[];
}

type CheckCidFullName = (
  auth: AdminAuth,
  cidFullName: string,
  subject_property?: string,
  city_name?: string,
) => Promise<{ exists: boolean }>;

type CreateInstitution = (
  auth: AdminAuth,
  input: CreateInstitutionInput,
) => Promise<CreateInstitutionOutput>;

type SearchParentInstitutions = (
  auth: AdminAuth,
  q: string,
  opts: SearchParentsOptions,
) => Promise<ParentInstitutionRow[]>;

export interface CreateInstitutionFormProps {
  auth: AdminAuth;
  category: CreateFormCategory;
  privateType?: PrivateType;
  open: boolean;
  lockedProvinceName: string | null;
  lockedCityName: string | null;
  checkCidFullName: CheckCidFullName;
  createInstitution: CreateInstitution;
  searchParentInstitutions: SearchParentInstitutions;
  onCancel: () => void;
  onCreated: (result: CreateInstitutionOutput) => void;
}

export const CreateInstitutionForm: React.FC<CreateInstitutionFormProps> = ({
  auth,
  category,
  privateType,
  open,
  lockedProvinceName,
  lockedCityName,
  checkCidFullName,
  createInstitution,
  searchParentInstitutions,
  onCancel,
  onCreated,
}) => {
  const locks = locksForCategory(category);
  const [form] = Form.useForm<FormValues>();
  const { signChain, chainSignModal } = useChainSign('机构创建链交易签名');
  const [cities, setCities] = useState<CidCityItem[]>([]);
  const [towns, setTowns] = useState<CidTownItem[]>([]);
  const [citiesLoading, setCitiesLoading] = useState(false);
  const [townsLoading, setTownsLoading] = useState(false);
  const [submitting, setSubmitting] = useState(false);
  const [cidFullNameChecking, setCidFullNameChecking] = useState(false);
  const [cidFullNameAvailable, setCidFullNameAvailable] = useState<boolean | null>(null);

  const [currentSubjectProperty, setCurrentSubjectProperty] = useState<string>(
    locks.subjectPropertyChoices[0]?.value ?? '',
  );
  // 非法人(F)所属法人选择器状态:必须从搜索结果中选定真实父级
  const [selectedParent, setSelectedParent] = useState<ParentInstitutionRow | null>(null);
  const [parentOptions, setParentOptions] = useState<ParentInstitutionRow[]>([]);
  const [parentSearching, setParentSearching] = useState(false);

  const isPrivate = category === 'PRIVATE_INSTITUTION';
  const isGov = category === 'GOV_INSTITUTION';
  const isEducation = category === 'EDUCATION_FORM';
  const watchedPartnershipKind = Form.useWatch('partnership_kind', form) as PartnershipKind | undefined;
  const watchedEducationType = Form.useWatch('education_type', form) as EducationType | undefined;
  const watchedInstitution = Form.useWatch('institution', form) as string | undefined;
  const watchedProvinceName = Form.useWatch('province_name', form) as string | undefined;
  const watchedCityName = Form.useWatch('city_name', form) as string | undefined;
  const privateRule = isPrivate && privateType
    ? privateRuleFor(privateType, watchedPartnershipKind)
    : null;
  const isF = currentSubjectProperty === 'F';
  const requiresParent = isF && !isPrivate;
  const showEducationType = isEducation && !isF;
  const requiresTown = isGov && currentSubjectProperty === 'G' && (watchedInstitution ?? '').startsWith('T');

  // 机构创建阶段直接写入全称和简称,字段只允许 cid_full_name/cid_short_name。
  const collectNameInModal = isPrivate || isEducation || isGov;
  const nameLabel = isEducation ? '学校全称' : '机构全称';
  const shortNameLabel = isEducation ? '学校简称' : '机构简称';

  const subjectPropertyChoices = privateRule
    ? [{
        value: privateRule.subjectProperty,
        label: SUBJECT_PROPERTY_LABEL[privateRule.subjectProperty] ?? privateRule.subjectProperty,
      }]
    : locks.subjectPropertyChoices;
  const instChoices = useMemo(() => {
    if (privateRule && privateType !== 'ASSOCIATION') {
      return [{ value: privateRule.institution, label: PRIVATE_TYPE_LABEL[privateRule.privateType] }];
    }
    // 教育入口的机构码按 subject_property×education_type 派生(大学 GUN/SUN vs 中小初学 GSCH/SFSC,
    // 分校 UNIN),保持可见的"机构"下拉与所选教育级别一致;提交时再以同一函数复算为准。
    if (isEducation) {
      const code = computeEducationInstitutionCode(currentSubjectProperty, watchedEducationType);
      return [{ value: code, label: EDUCATION_INSTITUTION_CODE_LABEL[code] ?? code }];
    }
    return institutionChoicesFor(category, currentSubjectProperty);
  }, [category, currentSubjectProperty, privateRule, isEducation, watchedEducationType]);
  const visibleInstChoices = useMemo(() => {
    if (isGov && currentSubjectProperty === 'G' && lockedCityName === null) {
      const hasCityRegistry = instChoices.some((item) => item.value === 'CREG');
      return hasCityRegistry
        ? instChoices
        : [...instChoices, { value: 'CREG', label: '市注册局' }];
    }
    return instChoices.filter((item) => item.value !== 'CREG');
  }, [currentSubjectProperty, instChoices, isGov, lockedCityName]);
  const p1Locks = useMemo(() => {
    if (privateType === 'ASSOCIATION') {
      return {
        choices: [
          { value: '1', label: '盈利' },
          { value: '0', label: '非盈利' },
        ],
        value: undefined,
        locked: false,
      };
    }
    if (privateRule) {
      return {
        choices: [
          privateRule.p1 === '1'
            ? { value: '1', label: '盈利' }
            : { value: '0', label: '非盈利' },
        ],
        value: privateRule.p1,
        locked: true,
      };
    }
    return p1LocksForSubject(currentSubjectProperty, selectedParent);
  }, [currentSubjectProperty, selectedParent, privateRule, privateType]);

  const resetParentState = () => {
    setSelectedParent(null);
    setParentOptions([]);
  };

  useEffect(() => {
    if (!open) return;
    const defaultPartnershipKind: PartnershipKind | undefined =
      privateType === 'PARTNERSHIP' ? 'GENERAL' : undefined;
    const defaultRule = privateType ? privateRuleFor(privateType, defaultPartnershipKind) : null;
    const defaultSubjectProperty = defaultRule?.subjectProperty ?? locks.subjectPropertyChoices[0]?.value ?? '';
    setCurrentSubjectProperty(defaultSubjectProperty);
    setCidFullNameAvailable(null);
    resetParentState();
    const defaultInstitution = defaultRule?.institution ?? institutionChoicesFor(category, defaultSubjectProperty)[0]?.value;
    const defaultEducationType = category === 'EDUCATION_FORM' && defaultSubjectProperty !== 'F'
      ? SCHOOL_EDUCATION_TYPE_OPTIONS[0]?.value as EducationType
      : undefined;
    const defaultCollectName = isPrivate || isEducation || isGov;
    form.setFieldsValue({
      subject_property: defaultSubjectProperty,
      p1: privateType === 'ASSOCIATION'
        ? undefined
        : defaultRule
          ? defaultRule.p1
          : p1LocksForSubject(defaultSubjectProperty, null).value,
      province_name: lockedProvinceName ?? '',
      city_name: lockedCityName ?? '',
      town_name: undefined,
      institution: defaultInstitution,
      education_type: defaultEducationType,
      private_type: privateType,
      partnership_kind: defaultPartnershipKind,
      cid_full_name: defaultCollectName ? '' : undefined,
      cid_short_name: defaultCollectName ? '' : undefined,
      parent_cid_number: undefined,
      admins: [
        { account_id: '', family_name: '管理', given_name: '员' },
        { account_id: '', family_name: '管理', given_name: '员' },
      ],
    });
  }, [open, category, privateType, lockedProvinceName, lockedCityName]);

  useEffect(() => {
    if (!open || !lockedProvinceName) return;
    let cancelled = false;
    setCitiesLoading(true);
    loadCachedCidCities(auth, lockedProvinceName)
      .then((rows) => {
        if (!cancelled) setCities(rows.filter((c) => c.city_code !== '000'));
      })
      .catch((err) => {
        if (!cancelled) {
          setCities([]);
          notice.error(err, '');
        }
      })
      .finally(() => {
        if (!cancelled) setCitiesLoading(false);
      });
    return () => {
      cancelled = true;
    };
  }, [open, lockedProvinceName, auth.access_token]);

  useEffect(() => {
    if (!open || !requiresTown) {
      setTowns([]);
      form.setFieldsValue({ town_name: undefined });
      return;
    }
    const provinceName = (watchedProvinceName ?? '').trim();
    const cityName = (watchedCityName ?? '').trim();
    const cityCode = cities.find((c) => c.city_name === cityName)?.city_code;
    if (!provinceName || !cityCode) {
      setTowns([]);
      form.setFieldsValue({ town_name: undefined });
      return;
    }
    let cancelled = false;
    setTownsLoading(true);
    listCidTowns(auth, provinceName, cityCode)
      .then((rows) => {
        if (!cancelled) setTowns(rows);
      })
      .catch((err) => {
        if (!cancelled) {
          setTowns([]);
          notice.error(err, '');
        }
      })
      .finally(() => {
        if (!cancelled) setTownsLoading(false);
      });
    return () => {
      cancelled = true;
    };
  }, [open, requiresTown, watchedProvinceName, watchedCityName, cities, auth.access_token]);

  const onSubjectPropertyChange = (subject_property: string) => {
    setCurrentSubjectProperty(subject_property);
    setCidFullNameAvailable(null);
    // 切主体属性必须重置所属法人与 p1(F 的 p1 是父级继承值,残留会提交旧值)。
    resetParentState();
    const nextInstitution = institutionChoicesFor(category, subject_property)[0]?.value;
    const nextEducationType = isEducation && subject_property !== 'F'
      ? (form.getFieldValue('education_type') ?? SCHOOL_EDUCATION_TYPE_OPTIONS[0]?.value)
      : undefined;
    const collectName =
      isEducation || (isGov && subject_property === 'G');
    form.setFieldsValue({
      institution: nextInstitution,
      education_type: nextEducationType,
      p1: p1LocksForSubject(subject_property, null).value,
      parent_cid_number: undefined,
      town_name: undefined,
      cid_full_name: collectName ? (form.getFieldValue('cid_full_name') ?? '') : undefined,
    });
  };

  // 教育级别变化(大学/中小初学)即重算机构码:G/S 在 GUN/SUN 与 GSCH/SFSC 间切换,
  // 保持可见的"机构"下拉值与提交派生一致(分校 F 不显示本字段)。
  const onEducationTypeChange = (educationType: EducationType) => {
    form.setFieldsValue({
      institution: computeEducationInstitutionCode(currentSubjectProperty, educationType),
    });
  };

  const onPartnershipKindChange = (kind: PartnershipKind) => {
    if (!privateType) return;
    const rule = privateRuleFor(privateType, kind);
    setCurrentSubjectProperty(rule.subjectProperty);
    form.setFieldsValue({
      subject_property: rule.subjectProperty,
      p1: rule.p1,
      institution: rule.institution,
      partnership_kind: kind,
      parent_cid_number: undefined,
    });
    resetParentState();
  };

  // ── 所属法人搜索/选定(仅 F)────────────────────────────────

  const parentSearchOptions = (): SearchParentsOptions | null => {
    const province_name = (form.getFieldValue('province_name') ?? '').trim();
    const city_name = (form.getFieldValue('city_name') ?? '').trim();
    if (!province_name || !city_name) {
      notice.warning('请先选择市,所属法人按落位省市过滤');
      return null;
    }
    return {
      fInstitution: (form.getFieldValue('institution') ?? '').trim(),
      province_name:  province_name,
      city_name:  city_name,
      // 公权入口只挂公法人;教育入口(分校)由后端按学校本部过滤。
      parentProperty: isGov ? 'G' : undefined,
    };
  };

  const triggerParentSearch = async () => {
    const q = (form.getFieldValue('parent_cid_number') ?? '').trim();
    if (!q) {
      notice.warning('请先输入所属法人全称、简称或身份ID关键字');
      return;
    }
    const opts = parentSearchOptions();
    if (!opts) return;
    setParentSearching(true);
    try {
      const rows = await searchParentInstitutions(auth, q, opts);
      setParentOptions(rows);
      if (rows.length === 0) {
        notice.info(isEducation ? '本市未找到可选的学校本部' : '未找到可选的所属法人');
      }
    } catch (err) {
      notice.error(err, '');
      setParentOptions([]);
    } finally {
      setParentSearching(false);
    }
  };

  const onParentSelect = (value: string) => {
    const row = parentOptions.find((r) => r.cid_number === value);
    if (!row) return;
    setSelectedParent(row);
    // 盈利属性附属于所属法人:选定父级即重算 p1(后端 unincorporated_org 同规则复核)
    form.setFieldsValue({ p1: inheritedP1(row.subject_property, row.p1) });
  };

  const onParentInputChange = (value: string) => {
    if (selectedParent && value !== selectedParent.cid_number) {
      setSelectedParent(null);
      form.setFieldsValue({ p1: undefined });
    }
  };

  // ── 全称查重 ─────────────────────────────────────────────

  const onCheckCidFullName = async () => {
    const cidFullName = (form.getFieldValue('cid_full_name') ?? '').trim();
    if (!cidFullName) {
      notice.warning(`请先输入${nameLabel}`);
      return;
    }
    // G(公立学校/公权机构)查重是同市同 cid_full_name,S/F 全国查重。
    const isGovName = currentSubjectProperty === 'G';
    if (isGovName) {
      const city_name = (form.getFieldValue('city_name') ?? '').trim();
      if (!city_name) {
        notice.warning(`${nameLabel}查重需要先选择市`);
        return;
      }
    }
    setCidFullNameChecking(true);
    try {
      const cityVal = isGovName ? (form.getFieldValue('city_name') ?? '').trim() : undefined;
      const { exists } = await checkCidFullName(
        auth,
        cidFullName,
        form.getFieldValue('subject_property'),
        cityVal,
      );
      if (exists) {
        notice.error(isGovName ? '该市已存在同全称机构，请更换全称' : '该机构全称已被使用');
        setCidFullNameAvailable(false);
      } else {
        notice.success('机构全称可用');
        setCidFullNameAvailable(true);
      }
    } catch (err) {
      notice.error(err, '');
      setCidFullNameAvailable(null);
    } finally {
      setCidFullNameChecking(false);
    }
  };

  const onCidFullNameChange = () => {
    if (cidFullNameAvailable !== null) setCidFullNameAvailable(null);
  };

  // ── 提交 ─────────────────────────────────────────────────

  const onSubmit = async (values: FormValues) => {
    if (!INSTITUTION_CREATION_ENABLED) {
      notice.warning('机构创建业务模块尚未接入，当前不能提交');
      return;
    }
    if (collectNameInModal && cidFullNameAvailable !== true) {
      notice.warning('请先点击搜索图标检查机构全称是否可用');
      return;
    }
    if (requiresParent) {
      // 非法人必须从搜索结果中选定所属法人,手填未选定的不放行
      if (!selectedParent || selectedParent.cid_number !== (values.parent_cid_number ?? '').trim()) {
        notice.warning('请从搜索结果中选择所属法人');
        return;
      }
    }
    // 教育入口的机构码不由下拉决定:按 subject_property×education_type 派生
    //   本部 → GUN/SUN(大学)或 GSCH/SFSC(中小初学);分校(F)→ UNIN(挂学校本部)。
    // 其余入口沿用表单 institution 值(私权由后端按 private_type 再覆盖)。
    const institutionCode = isEducation
      ? computeEducationInstitutionCode(values.subject_property.trim(), values.education_type)
      : values.institution.trim();
    const admins = (values.admins ?? [])
      .map((admin) => ({
        account_id: admin.account_id.trim(),
        family_name: (admin.family_name ?? '').trim() || '管理',
        given_name: (admin.given_name ?? '').trim() || '员',
      }))
      .filter((admin) => admin.account_id);
    const uniqueAdminCount = new Set(admins.map((admin) => admin.account_id)).size;
    if (uniqueAdminCount < 2) {
      notice.warning('请至少填写 2 名初始管理员');
      return;
    }
    if (requiresTown && !(values.town_name ?? '').trim()) {
      notice.warning('请选择镇');
      return;
    }
    setSubmitting(true);
    try {
      const result = await createInstitution(auth, {
        subject_property: values.subject_property.trim(),
        p1: values.p1?.trim(),
        province_name: values.province_name.trim(),
        city_name: values.city_name.trim(),
        town_name: requiresTown ? (values.town_name ?? '').trim() : undefined,
        institution: institutionCode,
        education_type: showEducationType ? values.education_type : undefined,
        cid_full_name: collectNameInModal
          ? (values.cid_full_name ?? '').trim()
          : undefined,
        cid_short_name: collectNameInModal
          ? (values.cid_short_name ?? '').trim()
          : undefined,
        parent_cid_number: requiresParent ? (values.parent_cid_number ?? '').trim() : undefined,
        private_type: isPrivate ? privateType : undefined,
        partnership_kind: isPrivate && privateType === 'PARTNERSHIP'
          ? values.partnership_kind
          : undefined,
        admins,
      });
      const signed = await signChain(result.request_id, result.institution_create_sign_request);
      await submitChainSign(
        auth,
        result.request_id,
        signed.account_id,
        signed.signature,
      );
      if (isPrivate && privateType) {
        notice.success(`${PRIVATE_TYPE_LABEL[privateType]}已提交链上交易:${result.cid_number}`);
      } else if (isEducation) {
        notice.success(`学校机构已提交链上交易:${result.cid_number}`);
      } else if (collectNameInModal) {
        notice.success(`公权机构已提交链上交易:${result.cid_number}`);
      } else {
        notice.success(`身份ID 已提交链上交易:${result.cid_number}`);
      }
      onCreated(result);
    } catch (err) {
      const raw = err instanceof Error ? err.message : '创建机构失败';
      if (raw.includes('本省') && raw.includes('未在线')) {
        notice.error('本省登录管理员未在线,请联系联邦注册局机构管理员登录后重试');
      } else if (raw.includes('已被使用') || raw.includes('同名机构')) {
        notice.error('该市已存在同全称机构，请更换全称');
        setCidFullNameAvailable(false);
      } else {
        notice.error(err, '创建机构失败');
      }
    } finally {
      setSubmitting(false);
    }
  };

  const subjectPropertyDisabled = isPrivate || subjectPropertyChoices.length === 1;
  const instDisabled = visibleInstChoices.length === 1;
  return (
    <>
    <Modal
      title={
        <div style={{ textAlign: 'center', width: '100%' }}>
          {isPrivate && privateType ? `新增${PRIVATE_TYPE_LABEL[privateType]}` : locks.modalTitle}
        </div>
      }
      open={open}
      onCancel={onCancel}
      footer={[
        <Button key="cancel" onClick={onCancel}>
          取消
        </Button>,
        <Button
          key="submit"
          type="primary"
          loading={submitting}
          disabled={!INSTITUTION_CREATION_ENABLED}
          title="机构创建业务模块尚未接入"
          onClick={() => form.submit()}
        >
          创建入口已关闭
        </Button>,
      ]}
      destroyOnClose
    >
      <Typography.Text type="danger" style={{ display: 'block', marginBottom: 16 }}>
        机构创建业务模块尚未接入；必须能原子提交 LR、初始治理岗位、权限、任职和投票规则后才会开放。
      </Typography.Text>
      <Form form={form} layout="vertical" onFinish={onSubmit}>
        {/* 短选项字段双列排布压低弹窗高度；所属法人内容长，保持整行。 */}
        <Row gutter={16}>
          {isPrivate && privateType === 'PARTNERSHIP' && (
            <Col span={24}>
              <Form.Item label="合伙类型" name="partnership_kind" rules={[{ required: true }]}>
                <Select
                  options={[
                    { value: 'GENERAL', label: '无限合伙' },
                    { value: 'LIMITED', label: '有限合伙' },
                  ]}
                  onChange={onPartnershipKindChange}
                />
              </Form.Item>
            </Col>
          )}
          <Col span={12}>
            <Form.Item label="主体属性" name="subject_property" rules={[{ required: true }]}>
              <Select options={subjectPropertyChoices} disabled={subjectPropertyDisabled} onChange={onSubjectPropertyChange} />
            </Form.Item>
          </Col>
          <Col span={12}>
            <Form.Item
              label="盈利属性"
              name="p1"
              rules={[
                {
                  required: true,
                  message: isF ? '盈利属性继承所属法人,请先选择所属法人' : '请选择盈利属性',
                },
              ]}
            >
              <Select
                options={p1Locks.choices}
                disabled={p1Locks.locked}
                placeholder={isF ? '由所属法人决定' : undefined}
              />
            </Form.Item>
          </Col>
        </Row>
        <Row gutter={16}>
          <Col span={12}>
            <Form.Item label="省" name="province_name" rules={[{ required: true }]}>
              <Input disabled />
            </Form.Item>
          </Col>
          <Col span={12}>
            <Form.Item label="市" name="city_name" rules={[{ required: true, message: '请选择市' }]}>
              <Select
                loading={citiesLoading}
                disabled={lockedCityName !== null}
                options={cities.map((c) => ({ label: c.city_name, value: c.city_name }))}
                placeholder="请选择市"
                onChange={() => {
                  // G 全称查重按市,所属法人搜索按落位省市;换市后两者都要重来。
                  form.setFieldsValue({ town_name: undefined });
                  if (currentSubjectProperty === 'G' && cidFullNameAvailable !== null) {
                    setCidFullNameAvailable(null);
                  }
                  if (isF && (selectedParent || parentOptions.length > 0)) {
                    resetParentState();
                    form.setFieldsValue({ parent_cid_number: undefined, p1: undefined });
                  }
                }}
              />
            </Form.Item>
          </Col>
        </Row>
        <Row gutter={16}>
          <Col span={12}>
            <Form.Item label="机构" name="institution" rules={[{ required: true }]}>
              <Select
                options={visibleInstChoices}
                disabled={instDisabled}
                onChange={() => {
                  form.setFieldsValue({ town_name: undefined });
                  if (cidFullNameAvailable !== null) setCidFullNameAvailable(null);
                }}
              />
            </Form.Item>
          </Col>
          {requiresTown && (
            <Col span={12}>
              <Form.Item label="镇" name="town_name" rules={[{ required: true, message: '请选择镇' }]}>
                <Select
                  loading={townsLoading}
                  options={towns.map((t) => ({ label: t.town_name, value: t.town_name }))}
                  placeholder="请选择镇"
                />
              </Form.Item>
            </Col>
          )}
          {showEducationType && (
            <Col span={12}>
              <Form.Item
                label="教育机构类型"
                name="education_type"
                rules={[{ required: true, message: '请选择教育机构类型' }]}
              >
                <Select
                  options={SCHOOL_EDUCATION_TYPE_OPTIONS}
                  onChange={onEducationTypeChange}
                />
              </Form.Item>
            </Col>
          )}
        </Row>
        {collectNameInModal && (
          <Row gutter={16}>
            <Col span={12}>
              <Form.Item
                label={nameLabel}
                name="cid_full_name"
                rules={[
                  { required: true, message: `请输入${nameLabel}` },
                  { max: 30, message: '最多 30 个字' },
                ]}
              >
                <Input
                  placeholder={`请输入${nameLabel}`}
                  maxLength={30}
                  onChange={onCidFullNameChange}
                  suffix={
                    <span
                      style={{ cursor: 'pointer', color: cidFullNameChecking ? '#999' : '#1890ff' }}
                      onClick={cidFullNameChecking ? undefined : onCheckCidFullName}
                    >
                      {cidFullNameChecking ? <Spin size="small" /> : <SearchOutlined />}
                    </span>
                  }
                />
              </Form.Item>
            </Col>
            <Col span={12}>
              <Form.Item
                label={shortNameLabel}
                name="cid_short_name"
                rules={[
                  { required: true, message: `请输入${shortNameLabel}` },
                  { max: 30, message: '最多 30 个字' },
                ]}
              >
                <Input
                  placeholder={`请输入${shortNameLabel}`}
                  maxLength={30}
                />
              </Form.Item>
            </Col>
            <Col span={24}>
              {cidFullNameAvailable === true && (
                <div style={{ color: '#52c41a', marginTop: -16, marginBottom: 12, fontSize: 12 }}>
                  机构全称可用
                </div>
              )}
              {cidFullNameAvailable === false && (
                <div style={{ color: '#ff4d4f', marginTop: -16, marginBottom: 12, fontSize: 12 }}>
                  该机构全称已被占用，请更换
                </div>
              )}
            </Col>
          </Row>
        )}
        {requiresParent && (
          <>
            <Form.Item
              label={isEducation ? '所属法人(学校本部)' : '所属法人'}
              name="parent_cid_number"
              rules={[{ required: true, message: '请选择所属法人' }]}
            >
              <AutoComplete
                filterOption={false}
                options={parentOptions.map((row) => ({
                  value: row.cid_number,
                  label: `${row.cid_full_name}(${SUBJECT_PROPERTY_LABEL[row.subject_property] ?? row.subject_property}) ${row.province_name}/${row.city_name}`,
                }))}
                onSelect={onParentSelect}
                onChange={onParentInputChange}
              >
                <Input
                  placeholder="输入所属法人全称、简称或身份ID后点击搜索"
                  suffix={
                    <span
                      style={{
                        cursor: parentSearching ? 'default' : 'pointer',
                        color: parentSearching ? '#999' : '#1890ff',
                      }}
                      onClick={parentSearching ? undefined : triggerParentSearch}
                      title="搜索所属法人"
                    >
                      {parentSearching ? <Spin size="small" /> : <SearchOutlined />}
                    </span>
                  }
                />
              </AutoComplete>
            </Form.Item>
            <div style={{ color: '#888', fontSize: 12, marginTop: -16, marginBottom: 12 }}>
              {isEducation
                ? '分校与本部同市,盈利属性继承本部学校。'
                : isGov
                  ? '可选本市市级、本省省级或国家级公权机构,盈利属性锁定非盈利。'
                  : '可选全国私法人机构,盈利属性继承所属法人。'}
            </div>
            {selectedParent && (
              <div style={{ color: '#52c41a', marginTop: -8, marginBottom: 12, fontSize: 12 }}>
                已选:{selectedParent.cid_full_name}(
                {SUBJECT_PROPERTY_LABEL[selectedParent.subject_property] ?? selectedParent.subject_property}
                ,{selectedParent.p1 === '1' ? '盈利' : '非盈利'})
              </div>
            )}
          </>
        )}
        <Form.List name="admins">
          {(fields, { add, remove }) => (
            <div style={{ marginTop: 8 }}>
              <Typography.Text strong style={{ display: 'block', marginBottom: 8 }}>
                初始管理员（至少 2 人）
              </Typography.Text>
              {fields.map((field, index) => (
                <Row gutter={8} key={field.key} align="top">
                  <Col span={5}>
                    <Form.Item
                      label={index === 0 ? '姓' : undefined}
                      name={[field.name, 'family_name']}
                    >
                      <Input placeholder="默认 管理" />
                    </Form.Item>
                  </Col>
                  <Col span={5}>
                    <Form.Item
                      label={index === 0 ? '名' : undefined}
                      name={[field.name, 'given_name']}
                    >
                      <Input placeholder="默认 员" />
                    </Form.Item>
                  </Col>
                  <Col span={11}>
                    <Form.Item
                      label={index === 0 ? '管理员账户' : undefined}
                      name={[field.name, 'account_id']}
                      rules={[
                        { required: true, message: '请输入管理员账户 ID' },
                        {
                          pattern: /^0x[0-9a-f]{64}$/,
                          message: '账户 ID 必须是小写 0x 加 64 位十六进制',
                        },
                      ]}
                    >
                      <Input placeholder="小写 0x + 64 位十六进制" />
                    </Form.Item>
                  </Col>
                  <Col span={3}>
                    <Button
                      danger
                      icon={<DeleteOutlined />}
                      disabled={fields.length <= 2}
                      onClick={() => remove(field.name)}
                      style={{ marginTop: index === 0 ? 30 : 0, width: '100%' }}
                    />
                  </Col>
                </Row>
              ))}
              <Button
                icon={<PlusOutlined />}
                onClick={() => add({ account_id: '', family_name: '管理', given_name: '员' })}
              >
                添加管理员
              </Button>
              <div style={{ color: '#888', fontSize: 12, marginTop: 8 }}>
                姓和名分别保存；管理员账户本身不授权。新创建业务必须把初始岗位、权限、任职和投票规则与机构一起原子提交。
              </div>
            </div>
          )}
        </Form.List>
        {!collectNameInModal && (
          <div style={{ color: '#888', fontSize: 12, marginTop: -8 }}>
            提示:本步骤仅生成身份ID。生成后请在详情页设置机构全称等信息。
          </div>
        )}
      </Form>
    </Modal>
    {chainSignModal}
    </>
  );
};

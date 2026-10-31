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
import type { AdminAuth } from '@gmb/onchina-platform/authentication/types';
import { listCidTowns, type CidCityItem, type CidTownItem } from '@gmb/onchina-platform/codes/api';
import { loadCachedCidCities } from '@gmb/onchina-platform/codes/cache';
import type {
  CreateInstitutionInput,
  CreateInstitutionOutput,
  EducationType,
  ParentInstitutionRow,
  PartnershipKind,
  PrivateType,
  SearchParentsOptions,
} from '@gmb/onchina-platform/institutions/api';
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
} from '@gmb/onchina-platform/institutions/labels';
import { notice } from '@gmb/onchina-platform/controls/notice';
import { submitChainSign, useChainSign } from '@gmb/onchina-platform/qr/useChainSign';

// 第 6 步新原子创建业务接入前保持关闭；不得用旧 call 5 临时放开。
const INSTITUTION_CREATION_ENABLED = false;

interface FormValues {
  subjectProperty: string;
  p1: string;
  provinceName: string;
  cityName: string;
  townName?: string;
  institution: string;
  educationType?: EducationType;
  privateType?: PrivateType;
  partnershipKind?: PartnershipKind;
  /** 私权/教育机构/手动公权机构创建时必填全称。 */
  cidFullName?: string;
  /** 私权/教育机构/手动公权机构创建时必填简称。 */
  cidShortName?: string;
  /** 需挂靠的非法人必填;个体经营/无限合伙不接受所属法人。 */
  parentCidNumber?: string;
  admins: {
    accountId: string;
    familyName?: string;
    givenName?: string;
  }[];
}

type CheckCidFullName = (
  auth: AdminAuth,
  cidFullName: string,
  subjectProperty?: string,
  cityName?: string,
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
  const watchedPartnershipKind = Form.useWatch('partnershipKind', form) as PartnershipKind | undefined;
  const watchedEducationType = Form.useWatch('educationType', form) as EducationType | undefined;
  const watchedInstitution = Form.useWatch('institution', form) as string | undefined;
  const watchedProvinceName = Form.useWatch('provinceName', form) as string | undefined;
  const watchedCityName = Form.useWatch('cityName', form) as string | undefined;
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
      subjectProperty: defaultSubjectProperty,
      p1: privateType === 'ASSOCIATION'
        ? undefined
        : defaultRule
          ? defaultRule.p1
          : p1LocksForSubject(defaultSubjectProperty, null).value,
      provinceName: lockedProvinceName ?? '',
      cityName: lockedCityName ?? '',
      townName: undefined,
      institution: defaultInstitution,
      educationType: defaultEducationType,
      privateType: privateType,
      partnershipKind: defaultPartnershipKind,
      cidFullName: defaultCollectName ? '' : undefined,
      cidShortName: defaultCollectName ? '' : undefined,
      parentCidNumber: undefined,
      admins: [
        { accountId: '', familyName: '管理', givenName: '员' },
        { accountId: '', familyName: '管理', givenName: '员' },
      ],
    });
  }, [open, category, privateType, lockedProvinceName, lockedCityName]);

  useEffect(() => {
    if (!open || !lockedProvinceName) return;
    let cancelled = false;
    setCitiesLoading(true);
    loadCachedCidCities(auth, lockedProvinceName)
      .then((rows) => {
        if (!cancelled) setCities(rows.filter((c) => c.cityCode !== '000'));
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
  }, [open, lockedProvinceName, auth.accessToken]);

  useEffect(() => {
    if (!open || !requiresTown) {
      setTowns([]);
      form.setFieldsValue({ townName: undefined });
      return;
    }
    const provinceName = (watchedProvinceName ?? '').trim();
    const cityName = (watchedCityName ?? '').trim();
    const cityCode = cities.find((c) => c.cityName === cityName)?.cityCode;
    if (!provinceName || !cityCode) {
      setTowns([]);
      form.setFieldsValue({ townName: undefined });
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
  }, [open, requiresTown, watchedProvinceName, watchedCityName, cities, auth.accessToken]);

  const onSubjectPropertyChange = (subjectProperty: string) => {
    setCurrentSubjectProperty(subjectProperty);
    setCidFullNameAvailable(null);
    // 切主体属性必须重置所属法人与 p1(F 的 p1 是父级继承值,残留会提交旧值)。
    resetParentState();
    const nextInstitution = institutionChoicesFor(category, subjectProperty)[0]?.value;
    const nextEducationType = isEducation && subjectProperty !== 'F'
      ? (form.getFieldValue('educationType') ?? SCHOOL_EDUCATION_TYPE_OPTIONS[0]?.value)
      : undefined;
    const collectName =
      isEducation || (isGov && subjectProperty === 'G');
    form.setFieldsValue({
      institution: nextInstitution,
      educationType: nextEducationType,
      p1: p1LocksForSubject(subjectProperty, null).value,
      parentCidNumber: undefined,
      townName: undefined,
      cidFullName: collectName ? (form.getFieldValue('cidFullName') ?? '') : undefined,
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
      subjectProperty: rule.subjectProperty,
      p1: rule.p1,
      institution: rule.institution,
      partnershipKind: kind,
      parentCidNumber: undefined,
    });
    resetParentState();
  };

  // ── 所属法人搜索/选定(仅 F)────────────────────────────────

  const parentSearchOptions = (): SearchParentsOptions | null => {
    const provinceName = (form.getFieldValue('provinceName') ?? '').trim();
    const cityName = (form.getFieldValue('cityName') ?? '').trim();
    if (!provinceName || !cityName) {
      notice.warning('请先选择市,所属法人按落位省市过滤');
      return null;
    }
    return {
      fInstitution: (form.getFieldValue('institution') ?? '').trim(),
      provinceName:  provinceName,
      cityName:  cityName,
      // 公权入口只挂公法人;教育入口(分校)由后端按学校本部过滤。
      parentProperty: isGov ? 'G' : undefined,
    };
  };

  const triggerParentSearch = async () => {
    const q = (form.getFieldValue('parentCidNumber') ?? '').trim();
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
    const row = parentOptions.find((r) => r.cidNumber === value);
    if (!row) return;
    setSelectedParent(row);
    // 盈利属性附属于所属法人:选定父级即重算 p1(后端 unincorporated_org 同规则复核)
    form.setFieldsValue({ p1: inheritedP1(row.subjectProperty, row.p1) });
  };

  const onParentInputChange = (value: string) => {
    if (selectedParent && value !== selectedParent.cidNumber) {
      setSelectedParent(null);
      form.setFieldsValue({ p1: undefined });
    }
  };

  // ── 全称查重 ─────────────────────────────────────────────

  const onCheckCidFullName = async () => {
    const cidFullName = (form.getFieldValue('cidFullName') ?? '').trim();
    if (!cidFullName) {
      notice.warning(`请先输入${nameLabel}`);
      return;
    }
    // G(公立学校/公权机构)查重是同市同 cid_full_name,S/F 全国查重。
    const isGovName = currentSubjectProperty === 'G';
    if (isGovName) {
      const cityName = (form.getFieldValue('cityName') ?? '').trim();
      if (!cityName) {
        notice.warning(`${nameLabel}查重需要先选择市`);
        return;
      }
    }
    setCidFullNameChecking(true);
    try {
      const cityVal = isGovName ? (form.getFieldValue('cityName') ?? '').trim() : undefined;
      const { exists } = await checkCidFullName(
        auth,
        cidFullName,
        form.getFieldValue('subjectProperty'),
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
      if (!selectedParent || selectedParent.cidNumber !== (values.parentCidNumber ?? '').trim()) {
        notice.warning('请从搜索结果中选择所属法人');
        return;
      }
    }
    // 教育入口的机构码不由下拉决定:按 subject_property×education_type 派生
    //   本部 → GUN/SUN(大学)或 GSCH/SFSC(中小初学);分校(F)→ UNIN(挂学校本部)。
    // 其余入口沿用表单 institution 值(私权由后端按 private_type 再覆盖)。
    const institutionCode = isEducation
      ? computeEducationInstitutionCode(values.subjectProperty.trim(), values.educationType)
      : values.institution.trim();
    const admins = (values.admins ?? [])
      .map((admin) => ({
        accountId: admin.accountId.trim(),
        familyName: (admin.familyName ?? '').trim() || '管理',
        givenName: (admin.givenName ?? '').trim() || '员',
      }))
      .filter((admin) => admin.accountId);
    const uniqueAdminCount = new Set(admins.map((admin) => admin.accountId)).size;
    if (uniqueAdminCount < 2) {
      notice.warning('请至少填写 2 名初始管理员');
      return;
    }
    if (requiresTown && !(values.townName ?? '').trim()) {
      notice.warning('请选择镇');
      return;
    }
    setSubmitting(true);
    try {
      const result = await createInstitution(auth, {
        subjectProperty: values.subjectProperty.trim(),
        p1: values.p1?.trim(),
        provinceName: values.provinceName.trim(),
        cityName: values.cityName.trim(),
        townName: requiresTown ? (values.townName ?? '').trim() : undefined,
        institution: institutionCode,
        educationType: showEducationType ? values.educationType : undefined,
        cidFullName: collectNameInModal
          ? (values.cidFullName ?? '').trim()
          : undefined,
        cidShortName: collectNameInModal
          ? (values.cidShortName ?? '').trim()
          : undefined,
        parentCidNumber: requiresParent ? (values.parentCidNumber ?? '').trim() : undefined,
        privateType: isPrivate ? privateType : undefined,
        partnershipKind: isPrivate && privateType === 'PARTNERSHIP'
          ? values.partnershipKind
          : undefined,
        admins,
      });
      const signed = await signChain(result.requestId, result.institutionCreateSignRequest);
      await submitChainSign(
        auth,
        result.requestId,
        signed.accountId,
        signed.signature,
      );
      if (isPrivate && privateType) {
        notice.success(`${PRIVATE_TYPE_LABEL[privateType]}已提交链上交易:${result.cidNumber}`);
      } else if (isEducation) {
        notice.success(`学校机构已提交链上交易:${result.cidNumber}`);
      } else if (collectNameInModal) {
        notice.success(`公权机构已提交链上交易:${result.cidNumber}`);
      } else {
        notice.success(`身份ID 已提交链上交易:${result.cidNumber}`);
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
              <Form.Item label="合伙类型" name="partnershipKind" rules={[{ required: true }]}>
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
            <Form.Item label="主体属性" name="subjectProperty" rules={[{ required: true }]}>
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
            <Form.Item label="省" name="provinceName" rules={[{ required: true }]}>
              <Input disabled />
            </Form.Item>
          </Col>
          <Col span={12}>
            <Form.Item label="市" name="cityName" rules={[{ required: true, message: '请选择市' }]}>
              <Select
                loading={citiesLoading}
                disabled={lockedCityName !== null}
                options={cities.map((c) => ({ label: c.cityName, value: c.cityName }))}
                placeholder="请选择市"
                onChange={() => {
                  // G 全称查重按市,所属法人搜索按落位省市;换市后两者都要重来。
                  form.setFieldsValue({ townName: undefined });
                  if (currentSubjectProperty === 'G' && cidFullNameAvailable !== null) {
                    setCidFullNameAvailable(null);
                  }
                  if (isF && (selectedParent || parentOptions.length > 0)) {
                    resetParentState();
                    form.setFieldsValue({ parentCidNumber: undefined, p1: undefined });
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
                  form.setFieldsValue({ townName: undefined });
                  if (cidFullNameAvailable !== null) setCidFullNameAvailable(null);
                }}
              />
            </Form.Item>
          </Col>
          {requiresTown && (
            <Col span={12}>
              <Form.Item label="镇" name="townName" rules={[{ required: true, message: '请选择镇' }]}>
                <Select
                  loading={townsLoading}
                  options={towns.map((t) => ({ label: t.townName, value: t.townName }))}
                  placeholder="请选择镇"
                />
              </Form.Item>
            </Col>
          )}
          {showEducationType && (
            <Col span={12}>
              <Form.Item
                label="教育机构类型"
                name="educationType"
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
                name="cidFullName"
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
                name="cidShortName"
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
              name="parentCidNumber"
              rules={[{ required: true, message: '请选择所属法人' }]}
            >
              <AutoComplete
                filterOption={false}
                options={parentOptions.map((row) => ({
                  value: row.cidNumber,
                  label: `${row.cidFullName}(${SUBJECT_PROPERTY_LABEL[row.subjectProperty] ?? row.subjectProperty}) ${row.provinceName}/${row.cityName}`,
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
                已选:{selectedParent.cidFullName}(
                {SUBJECT_PROPERTY_LABEL[selectedParent.subjectProperty] ?? selectedParent.subjectProperty}
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
                onClick={() => add({ accountId: '', familyName: '管理', givenName: '员' })}
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

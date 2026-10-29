import React, { useEffect, useState } from 'react';
import { Alert, AutoComplete, Button, Card, Checkbox, Col, Descriptions, Divider, Form, Input, Row, Select, Space, Spin, Tag, Typography, Upload } from 'antd';
import { SearchOutlined, UploadOutlined } from '@ant-design/icons';
import { type AdminAuth } from '../../platform/frontend/authentication_types';
import { LegalRepresentativePhoto } from '../../platform/frontend/subjects_LegalRepresentativePhoto';
import { type AdminActionType, type AdminSecurityGrantOutput } from '../../platform/frontend/admins_securityApi';
import { EDUCATION_TYPE_LABEL, PARTNERSHIP_KIND_LABEL, PRIVATE_TYPE_LABEL } from '../../platform/frontend/subjects_labels';
import { useInstitutionCodeLabels } from '../../platform/frontend/subjects_institutionLabels';
import { type InstitutionDetail, type ParentInstitutionRow, buildInstitutionUpdateSecurityPayload, checkCidFullName, searchParentInstitutions, updateInstitution, uploadLegalRepresentativePhoto } from './private_api';
import { searchLegalRepresentativeCitizens } from './citizens_api';
import { AccountList } from '../../platform/frontend/accounts_AccountList';
import { DocsLibrary } from '../../platform/frontend/docs_DocsLibrary';
import { notice } from '../../platform/frontend/helpers_notice';
import { InstitutionDetailNavLayout } from '../../platform/frontend/core_InstitutionDetailNavLayout';
import { OperationRecords } from './government_OperationRecords';
import { submitChainSign, useChainSign } from '../../platform/frontend/core_useChainSign';
import { type InstitutionGovernanceAdminInput, type InstitutionGovernanceAssignmentChangeInput, type InstitutionGovernanceRoleMutationInput, prepareInstitutionGovernance } from '../../platform/frontend/admins_api';
// 私权机构详情页布局 — 共享详情壳承载标题、左侧 tab 和右侧业务内容。
//
// 机构信息 tab:
//   一整块 Card(标题 = 机构信息,编辑/取消/保存按钮在 Card extra 右上角):
//     ┌ 左 Col:CID 信息(只读)──────────────┐  ┌ 右 Col:机构信息 ──────────────┐
//     │ 身份ID / 省 / 市 / 盈利属性 / 机构(均纯中文) │  │ 机构全称 + 搜索查重图标       │
//     │ 创建时间 / 创建用户                  │  │ 私权类型/法人资格只读展示     │
//     │                                      │  │ 所属法人 AutoComplete(需挂靠 F)│
//     └──────────────────────────────────────┘  └───────────────────────────────┘
//
// 右板块交互:
//   默认态 = 只读 Descriptions 展示,右上角显示"编辑"按钮
//   编辑态 = Form 可操作,右上角切换为"取消" + "保存"
//   机构全称右侧搜索图标:输入后点击查重;重名则禁止保存;全称未改动视为已通过
//   需挂靠的非法人所属法人:输入后点搜索图标触发模糊搜索(/institution/search-parents)
//
// 账户列表(AccountList)展示后端按机构类型生成的默认账户与自定义账户,
// 自定义账户创建后只登记在 CID;链上状态由区块链软件同步回来。

// 创建者角色中文映射(与列表页保持一致)。
const CREATOR_INSTITUTION_LABEL: Record<string, string> = {
  FEDERAL_REGISTRY: '联邦注册局管理员',
  CITY_REGISTRY: '市注册局管理员',
};

interface Props {
  auth: AdminAuth;
  detail: InstitutionDetail;
  canWrite: boolean;
  loading: boolean;
  onReload: () => void;
  /** 机构信息更新(INSTITUTION_UPDATE)走 PASSKEY_COLD_SIGN;账户增删已移交机构工作台。 */
  createScanSignGrant: (
    actionType: AdminActionType,
    payload: unknown,
  ) => Promise<AdminSecurityGrantOutput>;
  onBack?: () => void;
  backLabel?: string;
}

interface InfoFormValues {
  cidFullName: string;
  /** 需挂靠的非法人所属法人 cid_number */
  parentCidNumber?: string;
  familyName: string;
  givenName: string;
  representativeCid: string;
  photoPath: string;
  photoName: string;
  photoMime: string;
  photoSize?: number;
}

interface GovernanceFormValues {
  adminsText?: string;
  proposerRoleCode: string;
  roleCode?: string;
  roleName?: string;
  roleMutation?: 'CREATE' | 'RENAME' | 'DELETE';
  termRequired?: boolean;
  rolePermissionsText?: string;
  roleInitialAssignmentsText?: string;
  assignmentsText?: string;
  representativeCid?: string;
  clearLegalRepresentative?: boolean;
}

function parseGovernanceAdmins(text?: string): InstitutionGovernanceAdminInput[] {
  return (text ?? '')
    .split(/\r?\n/)
    .map((line) => line.trim())
    .filter(Boolean)
    .map((line) => {
      const [familyName, givenName, accountId] = line.split(/[,，]/).map((part) => part.trim());
      if (!familyName || !givenName || !accountId) throw new Error('管理员集合每行格式必须是：姓,名,账户');
      return { accountId, familyName: familyName, givenName: givenName };
    });
}

function parseRolePermissions(text?: string) {
  return (text ?? '').split(/\r?\n/).map((line) => line.trim()).filter(Boolean).map((line) => {
    const [moduleTag, actionCodeRaw, operation] = line.split(/[,，]/).map((part) => part.trim());
    const actionCode = Number(actionCodeRaw);
    if (!moduleTag || !Number.isInteger(actionCode) || actionCode < 0 || !['PROPOSE', 'VOTE'].includes(operation)) {
      throw new Error('岗位权限每行格式必须是：模块标签,动作码,PROPOSE或VOTE');
    }
    return { moduleTag: moduleTag, actionCode: actionCode, operation: operation as 'PROPOSE' | 'VOTE' };
  });
}

function parseInitialAssignments(text?: string) {
  return (text ?? '').split(/\r?\n/).map((line) => line.trim()).filter(Boolean).map((line) => {
    const [account, termStartRaw = '0', termEndRaw = '0'] = line.split(/[,，]/).map((part) => part.trim());
    const termStart = Number(termStartRaw || 0);
    const termEnd = Number(termEndRaw || 0);
    if (!account || !Number.isInteger(termStart) || !Number.isInteger(termEnd) || termStart < 0 || termEnd < 0) {
      throw new Error('初始任职每行格式必须是：管理员账户,任期开始,任期结束');
    }
    return { accountId: account, termStart: termStart, termEnd: termEnd };
  });
}

function parseGovernanceAssignments(text?: string): InstitutionGovernanceAssignmentChangeInput[] {
  const byRole = new Map<string, InstitutionGovernanceAssignmentChangeInput>();
  for (const raw of (text ?? '').split(/\r?\n/)) {
    const line = raw.trim();
    if (!line) continue;
    const [roleCode, account, termStartRaw = '0', termEndRaw = '0'] = line
      .split(/[,，]/)
      .map((part) => part.trim());
    if (!roleCode || !account) throw new Error('任职每行格式必须是：岗位码,管理员账户,任期开始,任期结束');
    const termStart = Number(termStartRaw || 0);
    const termEnd = Number(termEndRaw || 0);
    if (!Number.isInteger(termStart) || !Number.isInteger(termEnd) || termStart < 0 || termEnd < 0) {
      throw new Error('任期必须是非负整数日序');
    }
    const row = byRole.get(roleCode) ?? { roleCode: roleCode, assignments: [] };
    row.assignments.push({ accountId: account, termStart: termStart, termEnd: termEnd });
    byRole.set(roleCode, row);
  }
  return Array.from(byRole.values());
}

export const PrivateDetailLayout: React.FC<Props> = ({
  auth,
  detail,
  canWrite,
  loading,
  onReload,
  createScanSignGrant,
  onBack,
  backLabel,
}) => {
  const inst = detail.institution;
  const institutionLabels = useInstitutionCodeLabels();
  const accounts = detail.accounts;
  const [governanceForm] = Form.useForm<GovernanceFormValues>();
  const [governanceSubmitting, setGovernanceSubmitting] = useState(false);
  const { signChain: signGovernanceChain, chainSignModal: governanceChainSignModal } =
    useChainSign('机构治理链交易签名');

  // ── 右板块:编辑/只读切换 ──
  const [editing, setEditing] = useState(false);
  const [form] = Form.useForm<InfoFormValues>();
  const [savingInfo, setSavingInfo] = useState(false);

  // ── 机构全称查重状态 ──
  // null = 未查 / 未改名(视为 ok);true = 查重通过;false = 已占用
  const [cidFullNameChecking, setCidFullNameChecking] = useState(false);
  const [cidFullNameAvailable, setCidFullNameAvailable] = useState<boolean | null>(null);
  const [currentCidFullName, setCurrentCidFullName] = useState<string>(inst.cidFullName ?? '');
  const [legalRepSearching, setLegalRepSearching] = useState(false);
  const [legalRepOptions, setLegalRepOptions] = useState<string[]>([]);
  const [photoUploading, setPhotoUploading] = useState(false);
  const [photoName, setPhotoName] = useState<string>(inst.photoName ?? '');

  const isF = inst.subjectProperty === 'F';
  const requiresParent = isF && !['GT', 'GP'].includes(inst.institutionCode);
  // 完善判断:全称必填;仅需挂靠的非法人要求 parent_cid_number;私权类型由创建时编码确定。
  const needsCompletion =
    !inst.cidFullName ||
    (requiresParent && !inst.parentCidNumber) ||
    !inst.legalRepresentative?.family_name ||
    !inst.legalRepresentative?.given_name ||
    !inst.legalRepresentative?.cid_number ||
    !inst.photoPath;

  // ── 需挂靠非法人所属法人搜索 ──
  const [parentSearchOpts, setParentSearchOpts] = useState<ParentInstitutionRow[]>([]);
  const [parentSearching, setParentSearching] = useState(false);
  // 当前选中的法人(用于展示已选项全称;首次进入若 inst.parent_cid_number 有值,也要一次性拿到显示名)
  const [selectedParent, setSelectedParent] = useState<ParentInstitutionRow | null>(null);

  // detail 变更 → 需挂靠的非法人若有 parent_cid_number,则拉一次展示全称。
  useEffect(() => {
    if (!requiresParent || !inst.parentCidNumber) {
      setSelectedParent(null);
      return;
    }
    // 用 cid_number 自身作为查询词反查 full/short(传机构自身落位省市,既有挂靠必然满足地域规则)
    let cancelled = false;
    searchParentInstitutions(auth, inst.parentCidNumber, {
      fInstitution: inst.institutionCode,
      provinceName: inst.provinceName,
      cityName: inst.cityName,
    })
      .then((rows) => {
        if (cancelled) return;
        const hit = rows.find((r) => r.cidNumber === inst.parentCidNumber);
        setSelectedParent(hit ?? null);
      })
      .catch(() => {
        if (!cancelled) setSelectedParent(null);
      });
    return () => {
      cancelled = true;
    };
  }, [requiresParent, inst.parentCidNumber, auth.accessToken]);

  // 搜索(仅在用户点击搜索图标时触发,不自动 onSearch)
  const onParentSearch = async (value: string) => {
    const q = value.trim();
    if (!q) {
      notice.warning('请先输入 CID、机构全称或机构简称');
      setParentSearchOpts([]);
      return;
    }
    setParentSearching(true);
    try {
      // 改挂与创建同源:后端按 onchina/platform/institutions/subjects_unincorporated_org.rs 地域规则预过滤候选父级
      const rows = await searchParentInstitutions(auth, q, {
        fInstitution: inst.institutionCode,
        provinceName: inst.provinceName,
        cityName: inst.cityName,
      });
      setParentSearchOpts(rows);
      if (rows.length === 0) {
        notice.info('未找到匹配的法人机构');
      }
    } catch (err) {
      notice.error(err, '');
      setParentSearchOpts([]);
    } finally {
      setParentSearching(false);
    }
  };

  const triggerParentSearch = () => {
    if (parentSearching) return;
    const q = (form.getFieldValue('parentCidNumber') ?? '') as string;
    onParentSearch(q);
  };

  // detail 重新加载(保存成功后 onReload)→ 重置编辑态
  useEffect(() => {
    setEditing(false);
    setCidFullNameAvailable(null);
    setCurrentCidFullName(inst.cidFullName ?? '');
    form.setFieldsValue({
      cidFullName: inst.cidFullName ?? '',
      parentCidNumber: inst.parentCidNumber ?? undefined,
      familyName: inst.legalRepresentative?.family_name ?? '',
      givenName: inst.legalRepresentative?.given_name ?? '',
      representativeCid: inst.legalRepresentative?.cid_number ?? '',
      photoPath: inst.photoPath ?? '',
      photoName: inst.photoName ?? '',
      photoMime: inst.photoMime ?? '',
      photoSize: inst.photoSize ?? undefined,
    });
    setPhotoName(inst.photoName ?? '');
    setLegalRepOptions([]);
    governanceForm.setFieldsValue({
      roleMutation: 'CREATE',
      termRequired: false,
      rolePermissionsText: 'pri-mgmt,3,PROPOSE\npri-mgmt,3,VOTE',
    });
  }, [
    inst.cidNumber,
    inst.cidFullName,
    inst.parentCidNumber,
    inst.legalRepresentative,
    inst.photoPath,
    governanceForm,
  ]);

  const onClickEdit = () => {
    setEditing(true);
    setCidFullNameAvailable(null);
    form.setFieldsValue({
      cidFullName: inst.cidFullName ?? '',
      parentCidNumber: inst.parentCidNumber ?? undefined,
      familyName: inst.legalRepresentative?.family_name ?? '',
      givenName: inst.legalRepresentative?.given_name ?? '',
      representativeCid: inst.legalRepresentative?.cid_number ?? '',
      photoPath: inst.photoPath ?? '',
      photoName: inst.photoName ?? '',
      photoMime: inst.photoMime ?? '',
      photoSize: inst.photoSize ?? undefined,
    });
    setPhotoName(inst.photoName ?? '');
    setCurrentCidFullName(inst.cidFullName ?? '');
  };

  const onClickCancel = () => {
    setEditing(false);
    setCidFullNameAvailable(null);
    form.setFieldsValue({
      cidFullName: inst.cidFullName ?? '',
      parentCidNumber: inst.parentCidNumber ?? undefined,
      familyName: inst.legalRepresentative?.family_name ?? '',
      givenName: inst.legalRepresentative?.given_name ?? '',
      representativeCid: inst.legalRepresentative?.cid_number ?? '',
      photoPath: inst.photoPath ?? '',
      photoName: inst.photoName ?? '',
      photoMime: inst.photoMime ?? '',
      photoSize: inst.photoSize ?? undefined,
    });
    setPhotoName(inst.photoName ?? '');
    setCurrentCidFullName(inst.cidFullName ?? '');
  };

  const onCidFullNameInputChange = (e: React.ChangeEvent<HTMLInputElement>) => {
    const v = e.target.value;
    setCurrentCidFullName(v);
    // 全称改动 → 需要重新查重。
    if (cidFullNameAvailable !== null) setCidFullNameAvailable(null);
  };

  const isCidFullNameUnchanged = () => {
    return currentCidFullName.trim() === (inst.cidFullName ?? '').trim();
  };

  const onCheckCidFullName = async () => {
    const cidFullName = currentCidFullName.trim();
    if (!cidFullName) {
      notice.warning('请先输入机构全称');
      return;
    }
    if (isCidFullNameUnchanged()) {
      // 与原名一致,直接视为可用
      setCidFullNameAvailable(true);
      return;
    }
    setCidFullNameChecking(true);
    try {
      // 私权机构全国唯一查重(不传 subject_property/city 即走全国范围;后端会排除自身全称不在此函数,
      // 所以必须在全称改动时才调用;未改名的场景已在 isCidFullNameUnchanged 提前返回)
      const { exists } = await checkCidFullName(auth, cidFullName);
      if (exists) {
        notice.error('该机构全称已被使用,请更换全称');
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

  const triggerLegalRepSearch = async () => {
    const q = (form.getFieldValue('representativeCid') ?? '').trim();
    if (!q) {
      notice.warning('请先输入法定代表人身份ID关键字');
      return;
    }
    const parentValue = ((form.getFieldValue('parentCidNumber') ?? '') as string).trim();
    const parentChanged = requiresParent && parentValue !== (inst.parentCidNumber ?? '').trim();
    if (parentChanged && (!selectedParent || selectedParent.cidNumber !== parentValue)) {
      notice.warning('请先从搜索结果中选择所属法人');
      return;
    }
    setLegalRepSearching(true);
    try {
      const rows = await searchLegalRepresentativeCitizens(
        auth,
        q,
        parentChanged
          ? {
              provinceName: inst.provinceName,
              cityName: inst.cityName,
              subjectProperty: inst.subjectProperty,
              institution: inst.institutionCode,
              educationType: inst.educationType ?? undefined,
              parentCidNumber: parentValue,
            }
          : { targetCidNumber: inst.cidNumber },
      );
      setLegalRepOptions(rows);
      if (rows.length === 0) {
        notice.info('未找到正常状态公民');
      }
    } catch (err) {
      notice.error(err, '');
      setLegalRepOptions([]);
    } finally {
      setLegalRepSearching(false);
    }
  };

  const handlePhotoUpload = async (file: File) => {
    setPhotoUploading(true);
    try {
      const photo = await uploadLegalRepresentativePhoto(auth, file);
      form.setFieldsValue({
        photoPath: photo.filePath,
        photoName: photo.fileName,
        photoMime: photo.mimeType,
        photoSize: photo.fileSize,
      });
      setPhotoName(photo.fileName);
      notice.success('证件照已上传');
    } catch (err) {
      notice.error(err, '证件照上传失败');
    } finally {
      setPhotoUploading(false);
    }
    return false;
  };

  const onSaveInfo = async (values: InfoFormValues) => {
    const cidFullName = values.cidFullName.trim();
    if (!cidFullName) {
      notice.error('机构全称不能为空');
      return;
    }
    if (requiresParent && !values.parentCidNumber) {
      notice.error('请选择所属法人机构');
      return;
    }
    const familyName = values.familyName?.trim();
    const givenName = values.givenName?.trim();
    const representativeCid = values.representativeCid?.trim();
    if (
      !familyName ||
      !givenName ||
      !representativeCid ||
      !values.photoPath
    ) {
      notice.error('请完整填写法定代表人姓、名、身份ID和证件照');
      return;
    }
    // 机构全称变了必须查重通过才能保存。
    if (!isCidFullNameUnchanged() && cidFullNameAvailable !== true) {
      notice.warning('请点击搜索图标检查机构全称是否可用');
      return;
    }
    setSavingInfo(true);
    try {
      const input = {
        cidFullName: cidFullName,
        parentCidNumber: requiresParent ? values.parentCidNumber : undefined,
        familyName: familyName,
        givenName: givenName,
        representativeCid: representativeCid,
        photoPath: values.photoPath,
        photoName: values.photoName,
        photoMime: values.photoMime,
        photoSize: values.photoSize,
      };
      // 机构详情更新走 PASSKEY_COLD_SIGN。这里先按后端 grant_payload 同形生成冷签授权,
      // 再由 API 层补 Passkey assertion 与 grant 头,避免授权 hash 与正式请求漂移。
      const grant = await createScanSignGrant(
        'INSTITUTION_UPDATE',
        buildInstitutionUpdateSecurityPayload(inst.cidNumber, input),
      );
      await updateInstitution(auth, inst.cidNumber, input, grant);
      notice.success('机构信息已保存');
      setEditing(false);
      onReload();
    } catch (err) {
      const raw = err instanceof Error ? err.message : '保存失败';
      if (raw.includes('已被使用') || raw.includes('同名机构')) {
        notice.error('该机构全称已被使用,请更换全称');
        setCidFullNameAvailable(false);
      } else {
        notice.error(err, '保存失败');
      }
    } finally {
      setSavingInfo(false);
    }
  };

  const titleText = inst.cidFullName || '(未设置全称)';
  const createdByLabel = (() => {
    const roleLabel = detail.creatorInstitutionCode
      ? CREATOR_INSTITUTION_LABEL[detail.creatorInstitutionCode] ?? detail.creatorInstitutionCode
      : '';
    const creatorName = `${detail.creatorFamilyName ?? ''}${detail.creatorGivenName ?? ''}`;
    // 三态:姓名+角色 / 仅角色(内置管理员未设姓名)/ 完全未知
    if (creatorName) {
      return (
        <span>
          {creatorName}
          {roleLabel && (
            <Typography.Text type="secondary" style={{ marginLeft: 6, fontSize: 12 }}>
              ({roleLabel})
            </Typography.Text>
          )}
        </span>
      );
    }
    if (roleLabel) {
      return <span>{roleLabel}</span>;
    }
    return <span style={{ color: '#999' }}>未知</span>;
  })();

  // 保存按钮可用判断
  const saveEnabled = isCidFullNameUnchanged() || cidFullNameAvailable === true;

  // 右板块右上角按钮组
  const rightExtra = canWrite ? (
    !editing ? (
      <Button type="primary" onClick={onClickEdit}>
        编辑
      </Button>
    ) : (
      <Space>
        <Button onClick={onClickCancel}>取消</Button>
        <Button
          type="primary"
          loading={savingInfo}
          disabled={!saveEnabled}
          onClick={() => form.submit()}
          style={saveEnabled ? { backgroundColor: '#52c41a', borderColor: '#52c41a' } : undefined}
        >
          保存
        </Button>
      </Space>
    )
  ) : null;

  const institutionInfoSection = (
      <Card
        title={<span style={{ fontSize: 18, fontWeight: 600 }}>机构信息</span>}
        extra={rightExtra}
      >
        <Row gutter={24}>
          {/* 左:CID 不可编辑身份信息 */}
          <Col xs={24} md={12}>
            <Descriptions column={1} size="small">
              <Descriptions.Item label="身份ID">
                <Typography.Text style={{ fontSize: 12, wordBreak: 'break-all' }}>
                  {inst.cidNumber}
                </Typography.Text>
              </Descriptions.Item>
              <Descriptions.Item label="省份">{inst.provinceName}</Descriptions.Item>
              <Descriptions.Item label="城市">{inst.cityName}</Descriptions.Item>
              {/* p1/机构代码是系统编码,前端只显示中文;映射缺失时回退原代码兜底 */}
              <Descriptions.Item label="盈利属性">
                {inst.p1 === '0' ? '非盈利' : '盈利'}
              </Descriptions.Item>
              <Descriptions.Item label="机构">
                {institutionLabels[inst.institutionCode] || inst.institutionCode}
              </Descriptions.Item>
              {inst.educationType && (
                <Descriptions.Item label="教育分类">
                  {EDUCATION_TYPE_LABEL[inst.educationType] || inst.educationType}
                </Descriptions.Item>
              )}
              <Descriptions.Item label="创建时间">
                {new Date(inst.createdAt).toLocaleString('zh-CN')}
              </Descriptions.Item>
              <Descriptions.Item label="创建用户">{createdByLabel}</Descriptions.Item>
            </Descriptions>
          </Col>

          {/* 右:机构信息 — 直接展示 Form/Descriptions,不包额外 Card;按钮在外层 Card 的 extra */}
          <Col xs={24} md={12}>
            {needsCompletion && canWrite && !editing && (
              <Alert
                type="warning"
                showIcon
                message="请先完善机构全称和法定代表人资料"
                style={{ marginBottom: 12 }}
              />
            )}

            {editing ? (
                <Form<InfoFormValues>
                  form={form}
                  layout="vertical"
                  onFinish={onSaveInfo}
                  initialValues={{
                    cidFullName: inst.cidFullName ?? '',
                    parentCidNumber: inst.parentCidNumber ?? undefined,
                    familyName: inst.legalRepresentative?.family_name ?? '',
                    givenName: inst.legalRepresentative?.given_name ?? '',
                    representativeCid: inst.legalRepresentative?.cid_number ?? '',
                    photoPath: inst.photoPath ?? '',
                    photoName: inst.photoName ?? '',
                    photoMime: inst.photoMime ?? '',
                    photoSize: inst.photoSize ?? undefined,
                  }}
                >
                  <Form.Item
                    label="机构全称"
                    name="cidFullName"
                    rules={[
	                      { required: true, message: '请输入机构全称' },
                      { max: 30, message: '最多 30 个字' },
                    ]}
                    extra={
                      isCidFullNameUnchanged()
                        ? '未修改机构全称,无需查重'
                        : cidFullNameAvailable === true
                          ? '机构全称可用'
                          : cidFullNameAvailable === false
                            ? '该机构全称已被占用,请更换'
                            : '修改后点击右侧搜索图标检查是否重名'
                    }
                  >
                    <Input
	                      placeholder="请输入机构全称(最多 30 字)"
                      maxLength={30}
                      onChange={onCidFullNameInputChange}
                      suffix={
                        <span
                          style={{
                            cursor: cidFullNameChecking ? 'default' : 'pointer',
                            color: cidFullNameChecking ? '#999' : '#1890ff',
                          }}
                          onClick={cidFullNameChecking ? undefined : onCheckCidFullName}
	                          title="检查机构全称是否重名"
                        >
                          {cidFullNameChecking ? <Spin size="small" /> : <SearchOutlined />}
                        </span>
                      }
                    />
                  </Form.Item>
                  {requiresParent && (
                    <Form.Item
                      label="所属法人"
                      name="parentCidNumber"
                      rules={[{ required: true, message: '请选择所属法人机构' }]}
	                      extra="输入 CID、机构全称或机构简称后点击右侧搜索图标,从下拉结果中选择;必须是私法人(S)或公法人(G)"
                    >
                      <AutoComplete
                        // 不提供 onSearch → 用户输入时不自动请求,仅点搜索图标触发
                        filterOption={false}
                        notFoundContent={null}
                        options={parentSearchOpts.map((r) => ({
                          value: r.cidNumber,
                          label: (
                            <div>
                              <div style={{ fontWeight: 500 }}>{r.cidFullName}</div>
                              <div style={{ fontSize: 11, color: '#888' }}>
                                {r.cidNumber} · {r.subjectProperty} · {r.provinceName}/{r.cityName}
                              </div>
                            </div>
                          ),
                        }))}
                        onSelect={(val) => {
                          // 选中后,把选中机构缓存到 selectedParent 便于只读态展示
                          const hit = parentSearchOpts.find((o) => o.cidNumber === val);
                          if (hit) setSelectedParent(hit);
                          setLegalRepOptions([]);
                        }}
                      >
                        <Input
	                          placeholder="输入 CID、机构全称或机构简称后点击右侧搜索图标"
                          suffix={
                            <span
                              style={{
                                cursor: parentSearching ? 'default' : 'pointer',
                                color: parentSearching ? '#999' : '#1890ff',
                              }}
                              onClick={triggerParentSearch}
                              title="搜索法人机构"
                            >
                              {parentSearching ? <Spin size="small" /> : <SearchOutlined />}
                            </span>
                          }
                        />
                      </AutoComplete>
                    </Form.Item>
                  )}
                  <Form.Item
                    label="法定代表人姓"
                    name="familyName"
                    rules={[
                      { required: true, message: '请输入法定代表人姓' },
                      { max: 30, message: '最多 30 个字' },
                    ]}
                  >
                    <Input placeholder="请输入法定代表人姓" maxLength={30} />
                  </Form.Item>
                  <Form.Item
                    label="法定代表人名"
                    name="givenName"
                    rules={[
                      { required: true, message: '请输入法定代表人名' },
                      { max: 30, message: '最多 30 个字' },
                    ]}
                  >
                    <Input placeholder="请输入法定代表人名" maxLength={30} />
                  </Form.Item>
                  <Form.Item
                    label="法定代表人身份ID"
                    name="representativeCid"
                    rules={[{ required: true, message: '请选择法定代表人身份ID' }]}
                  >
                    <AutoComplete
                      filterOption={false}
                      options={legalRepOptions.map((cidNumber) => ({
                        value: cidNumber,
                        label: cidNumber,
                      }))}
                    >
                      <Input
                        placeholder="输入身份ID后点击搜索"
                        suffix={
                          <span
                            style={{
                              cursor: legalRepSearching ? 'default' : 'pointer',
                              color: legalRepSearching ? '#999' : '#1890ff',
                            }}
                            onClick={legalRepSearching ? undefined : triggerLegalRepSearch}
                            title="搜索正常状态公民"
                          >
                            {legalRepSearching ? <Spin size="small" /> : <SearchOutlined />}
                          </span>
                        }
                      />
                    </AutoComplete>
                  </Form.Item>
                  <Form.Item label="法定代表人证件照" required>
                    <Upload
                      accept="image/jpeg,image/png,image/webp"
                      showUploadList={false}
                      beforeUpload={(file) => handlePhotoUpload(file as File)}
                    >
                      <Button icon={<UploadOutlined />} loading={photoUploading}>
                        上传证件照
                      </Button>
                    </Upload>
                    {photoName && (
                      <div style={{ color: '#52c41a', marginTop: 8, fontSize: 12 }}>
                        {photoName}
                      </div>
                    )}
                  </Form.Item>
                  <Form.Item
                    name="photoPath"
                    rules={[{ required: true, message: '请上传法定代表人证件照' }]}
                    hidden
                  >
                    <Input />
                  </Form.Item>
                  <Form.Item name="photoName" hidden><Input /></Form.Item>
                  <Form.Item name="photoMime" hidden><Input /></Form.Item>
                  <Form.Item name="photoSize" hidden><Input type="number" /></Form.Item>
                </Form>
              ) : (
                // 只读展示
                <Descriptions column={1} size="small">
	                  <Descriptions.Item label="机构全称">
                    {inst.cidFullName || (
                      <span style={{ color: '#999' }}>(未命名)</span>
                    )}
                  </Descriptions.Item>
                  {requiresParent && (
                    <Descriptions.Item label="所属法人">
                      {inst.parentCidNumber ? (
                        selectedParent ? (
                          <span>
                            {selectedParent.cidFullName}
                            <Typography.Text
                              type="secondary"
                              style={{ marginLeft: 6, fontSize: 12 }}
                            >
                              ({selectedParent.cidNumber})
                            </Typography.Text>
                          </span>
                        ) : (
                          <Typography.Text code style={{ fontSize: 12 }}>
                            {inst.parentCidNumber}
                          </Typography.Text>
                        )
                      ) : (
                        <span style={{ color: '#999' }}>(未设置)</span>
                      )}
                    </Descriptions.Item>
                  )}
                  {inst.privateType && (
                    <Descriptions.Item label="私权类型">
                      {PRIVATE_TYPE_LABEL[inst.privateType] || inst.privateType}
                    </Descriptions.Item>
                  )}
                  {inst.partnershipKind && (
                    <Descriptions.Item label="合伙类型">
                      {PARTNERSHIP_KIND_LABEL[inst.partnershipKind] || inst.partnershipKind}
                    </Descriptions.Item>
                  )}
                  {inst.hasLegalPersonality !== null &&
                    inst.hasLegalPersonality !== undefined && (
                    <Descriptions.Item label="法人资格">
                      <Tag color={inst.hasLegalPersonality ? 'green' : 'default'}>
                        {inst.hasLegalPersonality ? '有法人资格' : '无法人资格'}
                      </Tag>
                    </Descriptions.Item>
                  )}
                  <Descriptions.Item label="法定代表人姓名">
                    {inst.legalRepresentative
                      ? `${inst.legalRepresentative.family_name}${inst.legalRepresentative.given_name}`
                      : <span style={{ color: '#999' }}>(未填写)</span>}
                  </Descriptions.Item>
                  <Descriptions.Item label="法定代表人身份ID">
                    {inst.legalRepresentative?.cid_number ? (
                      <Typography.Text style={{ fontSize: 12, wordBreak: 'break-all' }}>
                        {inst.legalRepresentative.cid_number}
                      </Typography.Text>
                    ) : (
                      <span style={{ color: '#999' }}>(未填写)</span>
                    )}
                  </Descriptions.Item>
                  <Descriptions.Item label="法定代表人证件照">
                    <LegalRepresentativePhoto auth={auth} path={inst.photoPath} name={inst.photoName} />
                  </Descriptions.Item>
                </Descriptions>
              )}
          </Col>
        </Row>
      </Card>
  );

  const accountListSection = (
      // 注册局详情页账户列表只读:能看不能增删,增删归机构自己的工作台。
      <Card type="inner" title={`账户列表(${accounts.length})`}>
        <AccountList accounts={accounts} loading={loading} canDelete={false} />
      </Card>
  );

  const submitGovernance = async () => {
    if (!canWrite) return;
    setGovernanceSubmitting(true);
    try {
      const values = await governanceForm.validateFields();
      const proposerRoleCode = values.proposerRoleCode.trim();
      if (!proposerRoleCode) throw new Error('必须填写提案发起岗位码');
      const admins = parseGovernanceAdmins(values.adminsText);
      const roleMutations: InstitutionGovernanceRoleMutationInput[] = [];
      const mutation = values.roleMutation;
      const roleCode = values.roleCode?.trim() ?? '';
      const roleName = values.roleName?.trim() ?? '';
      if (mutation === 'CREATE' && roleName) {
        roleMutations.push({ mutation, roleName: roleName, termRequired: Boolean(values.termRequired), permissions: parseRolePermissions(values.rolePermissionsText), assignments: parseInitialAssignments(values.roleInitialAssignmentsText) });
      } else if (mutation === 'RENAME' && (roleCode || roleName)) {
        if (!roleCode || !roleName) throw new Error('岗位改名必须同时填写岗位码和新名称');
        roleMutations.push({ mutation, roleCode: roleCode, roleName: roleName });
      } else if (mutation === 'DELETE' && roleCode) {
        roleMutations.push({ mutation, roleCode: roleCode });
      }
      const representativeCid = values.representativeCid?.trim() || undefined;
      const clearLegalRepresentative = Boolean(values.clearLegalRepresentative);
      if (representativeCid && clearLegalRepresentative) {
        throw new Error('任命/更换法定代表人和解除法定代表人不能同时提交');
      }
      const prepared = await prepareInstitutionGovernance(auth, {
        cidNumber: inst.cidNumber,
        proposerRoleCode: proposerRoleCode,
        admins: admins.length ? admins : undefined,
        roleMutations: roleMutations.length ? roleMutations : undefined,
        assignmentChanges: parseGovernanceAssignments(values.assignmentsText),
        representativeCid: representativeCid,
        clearLegalRepresentative: clearLegalRepresentative || undefined,
      });
      const signed = await signGovernanceChain(prepared.requestId, prepared.signRequest);
      const output = await submitChainSign(
        auth,
        prepared.requestId,
        signed.accountId,
        signed.signature,
      );
      notice.success(`链交易已提交：${output.txHash}`);
      onReload();
    } catch (err) {
      notice.error(err, '');
    } finally {
      setGovernanceSubmitting(false);
    }
  };

  const governanceSection = (
    <Card title="机构治理">
      <Form form={governanceForm} layout="vertical" disabled={!canWrite || governanceSubmitting}>
        <Form.Item
          label="提案发起岗位码"
          name="proposerRoleCode"
          rules={[{ required: true, message: '请输入当前任职且拥有提案权限的岗位码' }]}
        >
          <Input placeholder="例如 GENESIS_PRODUCT_MANAGER；动态岗位填写链上岗位码" maxLength={64} />
        </Form.Item>
        <Form.Item label="管理员集合" name="adminsText">
          <Input.TextArea rows={4} placeholder={'张,三,w5...\n李,四,w5...'} />
        </Form.Item>
        <Divider orientation="left">岗位</Divider>
        <Row gutter={12}>
          <Col xs={24} md={6}>
            <Form.Item label="岗位操作" name="roleMutation">
              <Select options={[{ label: '创建', value: 'CREATE' }, { label: '改名', value: 'RENAME' }, { label: '删除', value: 'DELETE' }]} />
            </Form.Item>
          </Col>
          <Col xs={24} md={6}>
            <Form.Item label="岗位码（改名/删除）" name="roleCode">
              <Input placeholder="创建时留空，由 runtime 生成" />
            </Form.Item>
          </Col>
          <Col xs={24} md={8}>
            <Form.Item label="岗位名称" name="roleName">
              <Input placeholder="例如：财务负责人" />
            </Form.Item>
          </Col>
          <Col xs={24} md={4}>
            <Form.Item name="termRequired" valuePropName="checked" label="任期">
              <Checkbox>要求任期</Checkbox>
            </Form.Item>
          </Col>
        </Row>
        <Form.Item label="创建岗位权限" name="rolePermissionsText">
          <Input.TextArea rows={3} placeholder={'pri-mgmt,3,PROPOSE\npri-mgmt,3,VOTE'} />
        </Form.Item>
        <Form.Item label="创建岗位初始任职" name="roleInitialAssignmentsText">
          <Input.TextArea rows={3} placeholder={'w5...,0,0'} />
        </Form.Item>
        <Form.Item label="岗位任职" name="assignmentsText">
          <Input.TextArea rows={4} placeholder={'RABCD,w5...,0,0'} />
        </Form.Item>
        <Form.Item label="法定代表人公民 CID" name="representativeCid">
          <Input placeholder="只填公民 CID；姓名和账户 ID由后端读取公民档案" />
        </Form.Item>
        <Form.Item name="clearLegalRepresentative" valuePropName="checked">
          <Checkbox>解除法定代表人并清空链上完整法定代表人结构</Checkbox>
        </Form.Item>
        <Button type="primary" loading={governanceSubmitting} disabled={!canWrite} onClick={submitGovernance}>
          发起本机构治理
        </Button>
      </Form>
    </Card>
  );

  return (
    <>
      <InstitutionDetailNavLayout
        backAction={onBack ? { label: backLabel ?? '返回列表', onClick: onBack } : undefined}
        title={titleText}
        subtitle={`身份ID：${inst.cidNumber}`}
        status={
          <Tag color={inst.status === 'ACTIVE' ? 'green' : 'red'}>
            {inst.status === 'ACTIVE' ? '正常' : '已注销'}
          </Tag>
        }
        items={[
          { key: 'info', label: '机构信息', content: institutionInfoSection },
          ...(canWrite
            ? [{ key: 'governance', label: '机构治理', content: governanceSection }]
            : []),
          { key: 'accounts', label: '账户列表', badge: accounts.length, content: accountListSection },
          {
            key: 'documents',
            label: '资料库',
            content: (
              <DocsLibrary
                auth={auth}
                cidNumber={inst.cidNumber}
                canWrite={canWrite}
              />
            ),
          },
          {
            key: 'operations',
            label: '操作记录',
            content: <OperationRecords auth={auth} cidNumber={inst.cidNumber} />,
          },
        ]}
      />
      {governanceChainSignModal}
    </>
  );
};

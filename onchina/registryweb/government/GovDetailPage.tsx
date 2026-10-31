import React, { useCallback, useEffect, useState } from 'react';
import { Button, Card, Checkbox, Col, Descriptions, Divider, Form, Input, Row, Select, Space, Tag, Typography } from 'antd';
import { EDUCATION_TYPE_LABEL } from '@gmb/onchina-platform/institutions/labels';
import { useInstitutionCodeLabels } from '@gmb/onchina-platform/institutions/institutionLabels';
import { type InstitutionDetail, getInstitution } from './api';
import { type AdminAuth } from '@gmb/onchina-platform/authentication/types';
import { LegalRepresentativePhoto } from '@gmb/onchina-platform/institutions/Photo';
import { AccountList } from '@gmb/onchina-platform/accounts/AccountList';
import { notice } from '@gmb/onchina-platform/controls/notice';
import { PrivateDetailLayout } from '../private/PrivateDetailLayout';
import { DocsLibrary } from '@gmb/onchina-platform/documents/Library';
import { type AdminActionType, type AdminSecurityGrantOutput, commitAdminAction, prepareAdminAction } from '@gmb/onchina-platform/authentication/security';
import { parseSignedReceiptPayload } from '@gmb/onchina-platform/qr/parseSignedPayload';
import { CitizenSignatureModal } from '@gmb/onchina-platform/qr/CitizenSignatureModal';
import { institutionDetailCacheKey, readCachedInstitutionDetail, writeCachedInstitutionDetail } from '@gmb/onchina-platform/codes/cache';
import { InstitutionDetailNavLayout } from '@gmb/onchina-platform/workspace/DetailLayout';
import { OperationRecords } from './OperationRecords';
import { submitChainSign, useChainSign } from '@gmb/onchina-platform/qr/useChainSign';
import { type InstitutionGovernanceAdminInput, type InstitutionGovernanceAssignmentChangeInput, type InstitutionGovernanceRoleMutationInput, prepareInstitutionGovernance } from '@gmb/onchina-platform/institutions/governance';
import { prepareRegisterInstitutionAdmins } from '../admins/cityRegistryAdminsApi';
import { isSubordinateRegistry, isTier1Registry } from '@gmb/onchina-platform/authorization/registryTier';
// 机构详情页(调度器)。各类机构统一使用左侧导航详情布局;
// 私权机构仍由 PrivateDetailLayout 承接本模块独有编辑逻辑。

interface Props {
  auth: AdminAuth;
  cidNumber: string;
  canWrite: boolean;
  /** 不传则隐藏返回按钮(注册局 tab 里市注册局管理员直接进详情、或联邦注册局无上一级时)。 */
  onBack?: () => void;
  /** 返回按钮文案,默认「返回列表」。 */
  backLabel?: string;
  /**
   * 详情数据加载覆盖。不传则走默认 getInstitution(auth, cidNumber)(带 scope 校验)。
   * 联邦注册局走 scope-bypass 的 getFederalRegistry,通过此 prop 注入。
   */
  loadDetail?: () => Promise<InstitutionDetail>;
  /** 注册局机构详情页:有管理员数据时显示“管理员列表”tab;普通机构不传。 */
  adminListSection?: React.ReactNode;
  /** 注册局管理员进入详情页时可默认打开管理员列表。 */
  initialActiveKey?: string;
}

const SUBJECT_STATUS_LABEL: Record<string, string> = {
  ACTIVE: '正常',
  REVOKED: '已注销',
};

type SecurityModalState = {
  actionId: string;
  signRequest: string;
  payloadHash: string;
  actionType: AdminActionType;
  resolve: (value: AdminSecurityGrantOutput) => void;
  reject: (reason?: unknown) => void;
};

type GovernanceFormValues = {
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
};

function parseAdminsText(text?: string): InstitutionGovernanceAdminInput[] {
  return (text ?? '')
    .split(/\r?\n/)
    .map((line) => line.trim())
    .filter(Boolean)
    .map((line) => {
      const [familyName, givenName, account] = line.split(/[,，]/).map((part) => part.trim());
      if (!familyName || !givenName || !account) {
        throw new Error('管理员集合每行格式必须是：姓,名,账户');
      }
      return { accountId: account, familyName: familyName, givenName: givenName };
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

function parseAssignmentsText(text?: string): InstitutionGovernanceAssignmentChangeInput[] {
  const byRole = new Map<string, InstitutionGovernanceAssignmentChangeInput>();
  for (const raw of (text ?? '').split(/\r?\n/)) {
    const line = raw.trim();
    if (!line) continue;
    const [roleCode, account, termStartRaw = '0', termEndRaw = '0'] = line
      .split(/[,，]/)
      .map((part) => part.trim());
    if (!roleCode || !account) {
      throw new Error('任职每行格式必须是：岗位码,管理员账户,任期开始,任期结束');
    }
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

function InstitutionGovernancePanel({
  auth,
  cidNumber,
  canWrite,
  onSubmitted,
}: {
  auth: AdminAuth;
  cidNumber: string;
  canWrite: boolean;
  onSubmitted: () => void;
}) {
  const [form] = Form.useForm<GovernanceFormValues>();
  const [submitting, setSubmitting] = useState(false);
  const { signChain, chainSignModal } = useChainSign('机构治理链交易签名');
  const canRegistryRegister =
    isTier1Registry(auth.institutionCode) || isSubordinateRegistry(auth.institutionCode);

  useEffect(() => {
    form.setFieldsValue({
      roleMutation: 'CREATE',
      termRequired: false,
      rolePermissionsText: 'pub-mgmt,3,PROPOSE\npub-mgmt,3,VOTE',
    });
  }, [form, cidNumber]);

  const submitPrepared = async (requestId: string, signRequest: string) => {
    const signed = await signChain(requestId, signRequest);
    const output = await submitChainSign(
      auth,
      requestId,
      signed.accountId,
      signed.signature,
    );
    notice.success(`链交易已提交：${output.txHash}`);
    onSubmitted();
  };

  const valuesToGovernancePayload = (values: GovernanceFormValues) => {
    const proposerRoleCode = values.proposerRoleCode.trim();
    if (!proposerRoleCode) throw new Error('必须填写提案发起岗位码');
    const admins = parseAdminsText(values.adminsText);
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
    return {
      cidNumber: cidNumber,
      proposerRoleCode: proposerRoleCode,
      admins: admins.length ? admins : undefined,
      roleMutations: roleMutations.length ? roleMutations : undefined,
      assignmentChanges: parseAssignmentsText(values.assignmentsText),
      representativeCid: representativeCid,
      clearLegalRepresentative: clearLegalRepresentative || undefined,
    };
  };

  const onProposeGovernance = async () => {
    if (!canWrite) return;
    setSubmitting(true);
    try {
      const values = await form.validateFields();
      const prepared = await prepareInstitutionGovernance(auth, valuesToGovernancePayload(values));
      await submitPrepared(prepared.requestId, prepared.signRequest);
    } catch (err) {
      notice.error(err, '');
    } finally {
      setSubmitting(false);
    }
  };

  const onRegisterAdmins = async () => {
    if (!canWrite || !canRegistryRegister) return;
    setSubmitting(true);
    try {
      const values = await form.validateFields(['admins_text']);
      const admins = parseAdminsText(values.adminsText);
      if (admins.length < 2) throw new Error('注册局直接登记管理员至少需要 2 人');
      const prepared = await prepareRegisterInstitutionAdmins(auth, {
        cidNumber: cidNumber,
        admins,
        actorRoleCode: (form.getFieldValue('proposerRoleCode') as string | undefined)?.trim() ?? '',
      });
      await submitPrepared(prepared.requestId, prepared.signRequest);
    } catch (err) {
      notice.error(err, '');
    } finally {
      setSubmitting(false);
    }
  };

  return (
    <Card title="机构治理">
      <Form form={form} layout="vertical" disabled={!canWrite || submitting}>
        <Form.Item
          label="提案发起岗位码"
          name="proposerRoleCode"
          rules={[{ required: true, message: '请输入当前任职且拥有提案权限的岗位码' }]}
        >
          <Input placeholder="例如 COMMITTEE_MEMBER；动态岗位填写链上岗位码" maxLength={64} />
        </Form.Item>
        <Form.Item label="管理员集合" name="adminsText">
          <Input.TextArea
            rows={4}
            placeholder={'张,三,w5...\n李,四,w5...'}
          />
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
          <Input.TextArea rows={3} placeholder={'pub-mgmt,3,PROPOSE\npub-mgmt,3,VOTE'} />
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
        <Space wrap>
          <Button type="primary" loading={submitting} disabled={!canWrite} onClick={onProposeGovernance}>
            发起本机构治理
          </Button>
          <Button loading={submitting} disabled={!canWrite || !canRegistryRegister} onClick={onRegisterAdmins}>
            注册局直接登记管理员
          </Button>
        </Space>
      </Form>
      {chainSignModal}
    </Card>
  );
}

export const GovDetailPage: React.FC<Props> = ({ auth, cidNumber, canWrite, onBack, backLabel, loadDetail, adminListSection, initialActiveKey }) => {
  const detailCacheKey = institutionDetailCacheKey(auth, cidNumber);
  const [detail, setDetail] = useState<InstitutionDetail | null>(() =>
    readCachedInstitutionDetail(detailCacheKey),
  );
  const [loading, setLoading] = useState(false);

  const [securityCommitLoading, setSecurityCommitLoading] = useState(false);
  const [securityModal, setSecurityModal] = useState<SecurityModalState | null>(null);

  const load = useCallback(() => {
    const cached = readCachedInstitutionDetail(detailCacheKey);
    if (cached) {
      setDetail(cached);
      setLoading(false);
    } else {
      setDetail(null);
      setLoading(true);
    }
    const fetchDetail = loadDetail ?? (() => getInstitution(auth, cidNumber));
    fetchDetail()
      .then((next) => {
        setDetail(next);
        writeCachedInstitutionDetail(detailCacheKey, next);
      })
      .catch(() => { /* 静默：后台刷新失败不弹窗 */ })
      .finally(() => {
        if (!cached) setLoading(false);
      });
  }, [auth.accessToken, detailCacheKey, cidNumber, loadDetail]);

  useEffect(() => {
    load();
  }, [load]);

  const runScanSignGrant = async (
    actionType: AdminActionType,
    payload: unknown,
  ): Promise<AdminSecurityGrantOutput> => {
    const prepared = await prepareAdminAction(auth, actionType, payload);
    if (prepared.authType !== 'PASSKEY_COLD_SIGN' || !prepared.signRequest) {
      throw new Error('该操作缺少公民钱包签名请求');
    }
    return new Promise<AdminSecurityGrantOutput>((resolve, reject) => {
      setSecurityModal({
        actionId: prepared.actionId,
        signRequest: prepared.signRequest || '',
        payloadHash: prepared.payloadHash,
          actionType: prepared.actionType,
        resolve,
        reject,
      });
    });
  };

  const handleSecuritySignedResponse = useCallback(async (raw: string) => {
    if (!securityModal) return;
    setSecurityCommitLoading(true);
    try {
      const signed = parseSignedReceiptPayload(raw, securityModal.actionId);
      if (signed.challengeId !== securityModal.actionId) {
        throw new Error('签名响应与当前请求不匹配');
      }
      if (!signed.accountId) {
        throw new Error('签名响应缺少 account_id');
      }
      const grant = await commitAdminAction<AdminSecurityGrantOutput>(auth, {
        actionId: securityModal.actionId,
        accountId: signed.accountId,
        signature: signed.signature,
        payloadHash: securityModal.payloadHash,
        actionType: securityModal.actionType,
      });
      securityModal.resolve(grant);
      setSecurityModal(null);
    } catch (err) {
      securityModal.reject(err);
      notice.error(err, '');
    } finally {
      setSecurityCommitLoading(false);
    }
  }, [auth, securityModal]);

  const inst = detail?.institution;
  const accounts = detail?.accounts || [];
  const institutionLabels = useInstitutionCodeLabels();
  const administrativeArea = inst
    ? [inst.provinceName, inst.cityName, inst.townName].filter(Boolean).join('/') || '-'
    : '-';

  const renderOfficialDetail = () => {
    if (!inst || !detail || inst.category === 'PRIVATE_INSTITUTION') return null;

    const institutionInfoSection = (
      <Card
        title={
          <span style={{ fontSize: 18, fontWeight: 600 }}>
            机构信息
          </span>
        }
      >
        <Row gutter={24}>
          <Col xs={24} md={24}>
            <Descriptions column={1} size="small">
              <Descriptions.Item label="身份ID">
                <Typography.Text style={{ fontSize: 12, wordBreak: 'break-all' }}>
                  {inst.cidNumber}
                </Typography.Text>
              </Descriptions.Item>
              <Descriptions.Item label="全称">{inst.cidFullName || '-'}</Descriptions.Item>
              <Descriptions.Item label="简称">{inst.cidShortName || inst.cidFullName || '-'}</Descriptions.Item>
              <Descriptions.Item label="行政区">{administrativeArea}</Descriptions.Item>
              <Descriptions.Item label="机构类型">
                {institutionLabels[inst.institutionCode] || inst.institutionCode}
              </Descriptions.Item>
              {inst.educationType && (
                <Descriptions.Item label="教育分类">
                  {EDUCATION_TYPE_LABEL[inst.educationType] || inst.educationType}
                </Descriptions.Item>
              )}
              <Descriptions.Item label="状态">
                <Tag color={inst.status === 'ACTIVE' ? 'green' : 'red'}>
                  {SUBJECT_STATUS_LABEL[inst.status] || inst.status}
                </Tag>
              </Descriptions.Item>
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
              <Descriptions.Item label="创建时间">
                {new Date(inst.createdAt).toLocaleString('zh-CN')}
              </Descriptions.Item>
            </Descriptions>
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

    return (
      <>
        <InstitutionDetailNavLayout
          backAction={onBack ? { label: backLabel ?? '返回列表', onClick: onBack } : undefined}
          title={inst.cidFullName ?? inst.cidShortName ?? '(未设置全称)'}
          subtitle={`身份ID：${inst.cidNumber}`}
          status={
            <Tag color={inst.status === 'ACTIVE' ? 'green' : 'red'}>
              {SUBJECT_STATUS_LABEL[inst.status] || inst.status}
            </Tag>
          }
          items={[
            { key: 'info', label: '机构信息', content: institutionInfoSection },
            ...(adminListSection
              ? [{ key: 'admins', label: '管理员列表', content: adminListSection }]
              : []),
            ...(canWrite
              ? [{
                  key: 'governance',
                  label: '机构治理',
                  content: (
                    <InstitutionGovernancePanel
                      auth={auth}
                      cidNumber={inst.cidNumber}
                      canWrite={canWrite}
                      onSubmitted={load}
                    />
                  ),
                }]
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
          initialActiveKey={initialActiveKey}
        />
      </>
    );
  };

  return (
    <div>
      {loading && !inst && <Typography.Text type="secondary">加载中...</Typography.Text>}

      {inst && detail && (
        <>
          {/* ── 私权机构:保留独立编辑逻辑,接入共享左侧导航布局。 ── */}
          {inst.category === 'PRIVATE_INSTITUTION' ? (
            <PrivateDetailLayout
              auth={auth}
              detail={detail}
              canWrite={canWrite}
              loading={loading}
              onReload={load}
              createScanSignGrant={runScanSignGrant}
              onBack={onBack}
              backLabel={backLabel}
            />
          ) : renderOfficialDetail()}
        </>
      )}
      <CitizenSignatureModal
        title="公民钱包签名确认"
        open={!!securityModal}
        onCancel={() => {
          securityModal?.reject(new Error('已取消签名确认'));
          setSecurityModal(null);
          setSecurityCommitLoading(false);
        }}
        qrTitle="签名二维码"
        qrValue={securityModal?.signRequest}
        qrHint="使用当前注册局管理员公民钱包扫码签名"
        scannerHint="扫描公民钱包生成的签名响应二维码"
        scannerDisabled={securityCommitLoading}
        scannerLoading={securityCommitLoading}
        onDetected={handleSecuritySignedResponse}
        onScannerError={(msg) => notice.error(msg)}
      />
    </div>
  );
};

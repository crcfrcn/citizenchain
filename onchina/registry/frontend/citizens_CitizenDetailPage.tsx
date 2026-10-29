import { encodeSs58 } from '../../platform/frontend/helpers_ss58';
// 公民详情页承接单个公民档案展示与链上身份推送。
// 本地建档不要求钱包;只有本页推送链上身份时才录入钱包并要求目标钱包签名。

import { useCallback, useEffect, useMemo, useRef, useState } from 'react';
import {
  Alert,
  Button,
  Card,
  Descriptions,
  Form,
  Input,
  Modal,
  Popconfirm,
  Select,
  Space,
  Table,
  Tag,
  Typography,
  Upload,
} from 'antd';
import {
  ArrowLeftOutlined,
  CloudUploadOutlined,
  DeleteOutlined,
  DownloadOutlined,
  EditOutlined,
  QrcodeOutlined,
  ScanOutlined,
  UploadOutlined,
  WalletOutlined,
} from '@ant-design/icons';
import type { UploadFile } from 'antd/es/upload/interface';

import type { AdminAuth } from '../../platform/frontend/authentication_types';
import { glassCardHeadStyle, glassCardStyle } from '../../platform/frontend/core_cardStyles';
import { CitizenSignatureModal } from '../../platform/frontend/core_CitizenSignatureModal';
import { EditCitizenModal } from './citizens_EditCitizenModal';
import { ScanAccountModal } from '../../platform/frontend/core_ScanAccountModal';
import { submitChainSign, useChainSign } from '../../platform/frontend/core_useChainSign';
import { notice } from '../../platform/frontend/helpers_notice';
import {
  CITIZEN_DOCUMENT_TYPES,
  completeCitizenOnchainSignature,
  fetchFinalizedCitizenBinding,
  prepareCitizenRevoke,
  prepareCitizenRebind,
  submitCitizenRebind,
  deleteCitizenDocument,
  downloadCitizenDocument,
  listCitizenDocuments,
  prepareCitizenOnchainSignature,
  uploadCitizenDocument,
  type CitizenDocument,
  type CitizenDocumentType,
  type CitizenOnchainIdentityLevel,
  type CitizenRow,
  type FinalizedCitizenBinding,
  type PrepareCitizenOnchainResult,
} from './citizens_api';

type Props = {
  auth: AdminAuth;
  citizen: CitizenRow;
  canWrite: boolean;
  provinceName?: string | null;
  cityName?: string | null;
  onBack: () => void;
  onUpdated: (next: CitizenRow) => void;
};

type OnchainForm = {
  accountId: string;
  actorRoleCode: string;
  identityLevel: CitizenOnchainIdentityLevel;
};

function makeCitizenName(row: Pick<CitizenRow, 'familyName' | 'givenName'>) {
  return `${row.familyName ?? ''}${row.givenName ?? ''}`.trim() || '-';
}

function formatDate(value?: string) {
  if (!value) return '-';
  const parts = value.split('-');
  if (parts.length !== 3) return value;
  return `${parts[0]}年${parts[1]}月${parts[2]}日`;
}

function formatDateRange(from?: string, until?: string) {
  if (!from || !until) return '-';
  return `${formatDate(from)}-${formatDate(until)}`;
}

function sexText(sex?: string) {
  if (sex === 'MALE') return '男';
  if (sex === 'FEMALE') return '女';
  return '-';
}

function statusTag(status?: string) {
  if (status === 'NORMAL') return <Tag color="green">正常</Tag>;
  if (status === 'REVOKED') return <Tag color="red">注销</Tag>;
  return <Tag>-</Tag>;
}

function statusText(status?: string) {
  if (status === 'NORMAL') return '正常';
  if (status === 'REVOKED') return '注销';
  return '-';
}

function areaText(province?: string, city?: string, town?: string) {
  return [province, city, town].filter((v) => v?.trim()).join(' / ') || '-';
}

function formatFileSize(bytes?: number): string {
  if (!bytes || bytes <= 0) return '0 B';
  if (bytes < 1024) return `${bytes} B`;
  if (bytes < 1024 * 1024) return `${(bytes / 1024).toFixed(1)} KB`;
  return `${(bytes / (1024 * 1024)).toFixed(1)} MB`;
}

function formatDateTime(value?: string) {
  if (!value) return '-';
  const date = new Date(value);
  if (Number.isNaN(date.getTime())) return value;
  return date.toLocaleString('zh-CN');
}

function calculateAgeYears(birthDate?: string) {
  if (!birthDate) return null;
  const birth = new Date(`${birthDate}T00:00:00`);
  if (Number.isNaN(birth.getTime())) return null;
  const today = new Date();
  let age = today.getFullYear() - birth.getFullYear();
  const beforeBirthday =
    today.getMonth() < birth.getMonth() ||
    (today.getMonth() === birth.getMonth() && today.getDate() < birth.getDate());
  if (beforeBirthday) age -= 1;
  return age;
}

export function CitizenDetailPage({
  auth,
  citizen,
  canWrite,
  provinceName,
  cityName,
  onBack,
  onUpdated,
}: Props) {
  const [form] = Form.useForm<OnchainForm>();
  const [current, setCurrent] = useState(citizen);
  const currentRef = useRef(current);
  currentRef.current = current;
  const [editOpen, setEditOpen] = useState(false);
  const [scanOpen, setScanOpen] = useState(false);
  const [prepareLoading, setPrepareLoading] = useState(false);
  const [completeLoading, setCompleteLoading] = useState(false);
  const [prepared, setPrepared] = useState<PrepareCitizenOnchainResult | null>(null);
  const [documents, setDocuments] = useState<CitizenDocument[]>([]);
  const [documentsLoading, setDocumentsLoading] = useState(false);
  const [documentUploading, setDocumentUploading] = useState(false);
  const [selectedDocumentType, setSelectedDocumentType] = useState<CitizenDocumentType>('其他材料');
  const { signChain, chainSignModal } = useChainSign('注册局链交易签名');
  const [chainSubmitting, setChainSubmitting] = useState(false);
  // 换绑钱包:新钱包本人扫「换绑签名」,再由管理员冷签(复用上面的 signChain)。
  const { signChain: signNewWallet, chainSignModal: newWalletSignModal } = useChainSign(
    '请新钱包本人用公民钱包 / 公民App 扫码换绑',
  );
  const [rebindRoleCode, setRebindRoleCode] = useState('');
  const [rebinding, setRebinding] = useState(false);

  const ageYears = useMemo(() => calculateAgeYears(current.citizenBirthDate), [current.citizenBirthDate]);
  const canPushOnchain =
    canWrite &&
    current.citizenStatus === 'NORMAL' &&
    current.identityStatus === 'NORMAL' &&
    current.votingEligible &&
    typeof ageYears === 'number' &&
    ageYears >= 16;

  const titleText = provinceName && cityName ? `${provinceName} · ${cityName}` : '公民详情';

  const loadDocuments = useCallback(() => {
    setDocumentsLoading(true);
    listCitizenDocuments(auth, current.cidNumber)
      .then(setDocuments)
      .catch((err) => notice.error(err, '公民资料库加载失败'))
      .finally(() => setDocumentsLoading(false));
  }, [auth, current.cidNumber]);

  useEffect(() => {
    loadDocuments();
  }, [loadDocuments]);

  const applyFinalizedBinding = useCallback(
    (binding: FinalizedCitizenBinding) => {
      const accountId = binding.bindingActive ? binding.accountId ?? null : null;
      const next: CitizenRow = {
        ...currentRef.current,
        accountId,
        bindingRevision: binding.bindingRevision ?? 0,
        bindingBlockNumber: binding.finalizedBlockNumber,
        bindingBlockHash: binding.finalizedBlockHash,
      };
      currentRef.current = next;
      setCurrent(next);
      onUpdated(next);
      form.setFieldValue('accountId', accountId ?? '');
    },
    [form, onUpdated],
  );

  const loadFinalizedBinding = useCallback(async () => {
    const binding = await fetchFinalizedCitizenBinding(auth, current.cidNumber);
    applyFinalizedBinding(binding);
    return binding;
  }, [applyFinalizedBinding, auth, current.cidNumber]);

  useEffect(() => {
    void loadFinalizedBinding().catch((err) =>
      notice.error(err, 'finalized 钱包绑定加载失败'),
    );
  }, [loadFinalizedBinding]);

  const prepareOnchainSignature = async () => {
    const values = await form.validateFields();
    setPrepareLoading(true);
    setPrepared(null);
    try {
      const output = await prepareCitizenOnchainSignature(
        auth,
        current.cidNumber,
        values.accountId.trim(),
        values.actorRoleCode.trim(),
        values.identityLevel,
      );
      setPrepared(output);
      notice.success('公民钱包签名二维码已生成');
    } catch (err) {
      notice.error(err, '生成签名二维码失败');
    } finally {
      setPrepareLoading(false);
    }
  };

  const completeOnchainSignature = async (raw: string) => {
    const values = form.getFieldsValue();
    const accountId = values.accountId?.trim();
    if (!accountId) {
      notice.warning('请先录入账户 ID');
      return;
    }
    const identityLevel = values.identityLevel;
    if (!identityLevel) {
      notice.warning('请先选择身份类型');
      return;
    }
    const actorRoleCode = values.actorRoleCode?.trim();
    if (!actorRoleCode) {
      notice.warning('请先输入注册局岗位码');
      return;
    }
    setCompleteLoading(true);
    try {
      const output = await completeCitizenOnchainSignature(
        auth,
        current.cidNumber,
        accountId,
        actorRoleCode,
        identityLevel,
        raw,
      );
      setPrepared(null);
      notice.success('公民钱包签名已验证，请用注册局管理员公民钱包签名并提交上链');
      // D7：身份上链交易由 OnChina 组装提交，注册局管理员 CitizenWallet 只签名一次并显示响应二维码。
      const signed = await signChain(output.requestId, output.identitySignRequest);
      setChainSubmitting(true);
      try {
        const submitted = await submitChainSign(
          auth,
          output.requestId,
          signed.accountId,
          signed.signature,
        );
        notice.success(`公民身份已上链,交易哈希：${submitted.txHash}`);
        // 页面只采用 finalized 六读闭环结果，不信任准备阶段或扫码响应中的账户。
        await loadFinalizedBinding();
      } finally {
        setChainSubmitting(false);
      }
    } catch (err) {
      notice.error(err, '公民身份上链失败');
    } finally {
      setCompleteLoading(false);
    }
  };

  const revokeOnchain = async () => {
    const values = form.getFieldsValue();
    const accountId = values.accountId?.trim();
    const actorRoleCode = values.actorRoleCode?.trim();
    if (!accountId) {
      notice.warning('请先录入账户 ID');
      return;
    }
    if (!actorRoleCode) {
      notice.warning('请先填写当前任职岗位码');
      return;
    }
    setChainSubmitting(true);
    try {
      // 吊销同样只做一次 Passkey 和一次最终链签。
      const prep = await prepareCitizenRevoke(
        auth,
        current.cidNumber,
        actorRoleCode,
      );
      const signed = await signChain(prep.requestId, prep.signRequest);
      const submitted = await submitChainSign(
        auth,
        prep.requestId,
        signed.accountId,
        signed.signature,
      );
      notice.success(`公民身份已吊销,交易哈希：${submitted.txHash}`);
      await loadFinalizedBinding();
    } catch (err) {
      notice.error(err, '公民身份吊销失败');
    } finally {
      setChainSubmitting(false);
    }
  };

  const rebindWallet = async () => {
    const roleCode = rebindRoleCode.trim();
    if (!roleCode) {
      notice.warning('请先填写当前任职岗位码');
      return;
    }
    setRebinding(true);
    try {
      // 段1:发起换绑,返回给新钱包的域签名 QR。
      const prep = await prepareCitizenRebind(auth, current.cidNumber, roleCode);
      // 段1→2:新钱包本人扫码换绑(占即绑,自填本账户)。
      const newWalletSigned = await signNewWallet(prep.requestId, prep.walletSignRequest);
      const admin = await submitCitizenRebind(
        auth,
        prep.requestId,
        newWalletSigned.accountId,
        newWalletSigned.signature,
        newWalletSigned.currentAccountId,
        newWalletSigned.currentAccountSignature,
      );
      // 段2→3:注册局管理员冷签换绑 extrinsic。
      const adminSigned = await signChain(admin.requestId, admin.signRequest);
      const submitted = await submitChainSign(
        auth,
        admin.requestId,
        adminSigned.accountId,
        adminSigned.signature,
      );
      notice.success(`换绑上链成功,交易哈希：${submitted.txHash}`);
      // 新钱包签名只证明控制权；显示账户必须重新读取 finalized 链上闭环。
      await loadFinalizedBinding();
      setRebindRoleCode('');
    } catch (err) {
      notice.error(err, '换绑钱包失败');
    } finally {
      setRebinding(false);
    }
  };

  const uploadDocument = async (file: UploadFile) => {
    const rawFile = file as unknown as File;
    if (!rawFile || !rawFile.name) return false;
    setDocumentUploading(true);
    try {
      await uploadCitizenDocument(auth, current.cidNumber, rawFile, selectedDocumentType);
      notice.success('公民资料上传成功');
      loadDocuments();
    } catch (err) {
      notice.error(err, '公民资料上传失败');
    } finally {
      setDocumentUploading(false);
    }
    return false;
  };

  const downloadDocument = async (doc: CitizenDocument) => {
    try {
      await downloadCitizenDocument(auth, current.cidNumber, doc.id, doc.fileName);
    } catch (err) {
      notice.error(err, '公民资料下载失败');
    }
  };

  const deleteDocument = async (doc: CitizenDocument) => {
    try {
      await deleteCitizenDocument(auth, current.cidNumber, doc.id);
      notice.success('公民资料已删除');
      loadDocuments();
    } catch (err) {
      notice.error(err, '公民资料删除失败');
    }
  };

  return (
    <>
      <Card
        title={
          <div style={{ position: 'relative', display: 'flex', alignItems: 'center', minHeight: 32 }}>
            <Button type="link" icon={<ArrowLeftOutlined />} style={{ paddingLeft: 0 }} onClick={onBack}>
              返回
            </Button>
            <span style={{ position: 'absolute', left: '50%', transform: 'translateX(-50%)' }}>
              {titleText}
            </span>
            {canWrite && (
              <Button
                type="link"
                icon={<EditOutlined />}
                style={{ position: 'absolute', right: 0, paddingRight: 0 }}
                onClick={() => setEditOpen(true)}
              >
                编辑资料
              </Button>
            )}
          </div>
        }
        bordered={false}
        style={glassCardStyle}
        headStyle={glassCardHeadStyle}
      >
        <Typography.Title level={4} style={{ marginTop: 0, marginBottom: 16 }}>
          公民详情
        </Typography.Title>

        <Descriptions column={1} size="small" bordered>
          <Descriptions.Item label="护照号">{current.passportNo || '-'}</Descriptions.Item>
          <Descriptions.Item label="身份CID">{current.cidNumber || '-'}</Descriptions.Item>
          <Descriptions.Item label="姓名">{makeCitizenName(current)}</Descriptions.Item>
          <Descriptions.Item label="性别">{sexText(current.citizenSex)}</Descriptions.Item>
          <Descriptions.Item label="出生日期">{formatDate(current.citizenBirthDate)}</Descriptions.Item>
          <Descriptions.Item label="年龄">{typeof ageYears === 'number' ? `${ageYears}周岁` : '-'}</Descriptions.Item>
          <Descriptions.Item label="当前绑定钱包账户">{current.accountId ? encodeSs58(current.accountId) : '-'}</Descriptions.Item>
          <Descriptions.Item label="绑定版本">{current.bindingRevision || '-'}</Descriptions.Item>
          <Descriptions.Item label="绑定 finalized 区块">
            {current.bindingBlockNumber ?? '-'}
          </Descriptions.Item>
          <Descriptions.Item label="选举权利">{current.votingEligible ? '有' : '无'}</Descriptions.Item>
          <Descriptions.Item label="公民状态">{statusTag(current.citizenStatus)}</Descriptions.Item>
          <Descriptions.Item label="投票身份状态">{statusText(current.identityStatus)}</Descriptions.Item>
          <Descriptions.Item label="居住地">
            {areaText(current.provinceName, current.cityName, current.townName)}
          </Descriptions.Item>
          <Descriptions.Item label="出生地">
            {areaText(current.birthProvinceName, current.birthCityName, current.birthTownName)}
          </Descriptions.Item>
          <Descriptions.Item label="有效期">
            {formatDateRange(current.passportValidFrom, current.passportValidUntil)}
          </Descriptions.Item>
          <Descriptions.Item label="档案哈希">{current.archiveHash || '-'}</Descriptions.Item>
          <Descriptions.Item label="链上交易">{current.onchainTxHash || '-'}</Descriptions.Item>
        </Descriptions>

        <div style={{ marginTop: 20, borderTop: '1px solid #e5e7eb', paddingTop: 18 }}>
          <Typography.Title level={5} style={{ marginTop: 0 }}>
            链上身份上链
          </Typography.Title>
          {!canPushOnchain && (
            <Alert
              type="warning"
              showIcon
              style={{ marginBottom: 16 }}
              message="当前档案不能推送链上身份"
              description="公民必须年满16周岁、档案正常且具有选举资格。"
            />
          )}
          <Form
            form={form}
            layout="inline"
            initialValues={{
              accountId: current.accountId ?? '',
              actorRoleCode: '',
              identityLevel: 'voting',
            }}
            style={{ rowGap: 12 }}
          >
            <Form.Item
              name="identityLevel"
              rules={[{ required: canPushOnchain, message: '请选择身份类型' }]}
              style={{ minWidth: 180, marginBottom: 0 }}
            >
              <Select<CitizenOnchainIdentityLevel>
                disabled={!canPushOnchain || prepareLoading || completeLoading}
                options={[
                  { value: 'voting', label: '投票身份' },
                  { value: 'candidate', label: '参选身份' },
                ]}
              />
            </Form.Item>
            <Form.Item
              name="actorRoleCode"
              rules={[{ required: canPushOnchain, message: '请输入注册局岗位码' }]}
              style={{ minWidth: 260, marginBottom: 0 }}
            >
              <Input
                placeholder="注册局岗位码"
                disabled={!canPushOnchain || prepareLoading || completeLoading}
                allowClear
              />
            </Form.Item>
            <Form.Item
              name="accountId"
              rules={[{ required: canPushOnchain, message: '请输入账户 ID' }]}
              style={{ minWidth: 460, marginBottom: 0 }}
            >
              <Input
                prefix={<WalletOutlined />}
                placeholder="推送链上身份时录入公民账户 ID"
                disabled={!canPushOnchain || prepareLoading || completeLoading}
                allowClear
              />
            </Form.Item>
            <Form.Item style={{ marginBottom: 0 }}>
              <Button
                icon={<ScanOutlined />}
                disabled={!canPushOnchain || prepareLoading || completeLoading}
                onClick={() => setScanOpen(true)}
              >
                扫描钱包
              </Button>
            </Form.Item>
            <Form.Item style={{ marginBottom: 0 }}>
              <Button
                type="primary"
                icon={<QrcodeOutlined />}
                loading={prepareLoading}
                disabled={!canPushOnchain || completeLoading}
                onClick={prepareOnchainSignature}
              >
                生成签名二维码
              </Button>
            </Form.Item>
            <Form.Item style={{ marginBottom: 0 }}>
              <Button
                danger
                icon={<CloudUploadOutlined />}
                loading={chainSubmitting}
                disabled={!canPushOnchain || current.citizenStatus === 'REVOKED'}
                onClick={revokeOnchain}
              >
                吊销身份(墓碑)
              </Button>
            </Form.Item>
          </Form>
        </div>

        <div style={{ marginTop: 20, borderTop: '1px solid #e5e7eb', paddingTop: 18 }}>
          <Typography.Title level={5} style={{ marginTop: 0 }}>
            换绑钱包账户
          </Typography.Title>
          <Alert
            type="info"
            showIcon
            style={{ marginBottom: 12 }}
            message="CID 换绑:匿名 CID 可由任一在册 CREG/FRG 办理；实名 CID 仅本市 CREG/对应省 FRG。新钱包本人签名，不要求当前钱包；授权锁定创世哈希、当前账户、绑定版本和过期时间。"
          />
          <Space wrap>
            <Input
              placeholder="注册局岗位码"
              value={rebindRoleCode}
              onChange={(event) => setRebindRoleCode(event.target.value)}
              disabled={!canWrite || rebinding}
              style={{ width: 260 }}
              allowClear
            />
            <Popconfirm
              title={`确认换绑 ${current.cidNumber} 的钱包账户?`}
              okText="换绑"
              cancelText="取消"
              disabled={!canWrite || rebinding}
              onConfirm={rebindWallet}
            >
              <Button
                type="primary"
                icon={<WalletOutlined />}
                loading={rebinding}
                disabled={!canWrite}
              >
                换绑钱包
              </Button>
            </Popconfirm>
          </Space>
        </div>
      </Card>

      <Card
        title="资料库"
        bordered={false}
        style={{ ...glassCardStyle, marginTop: 16 }}
        headStyle={glassCardHeadStyle}
        extra={
          canWrite && (
            <Space wrap>
              <Select<CitizenDocumentType>
                value={selectedDocumentType}
                onChange={setSelectedDocumentType}
                style={{ width: 140 }}
                options={CITIZEN_DOCUMENT_TYPES.map((documentType) => ({
                  value: documentType,
                  label: documentType,
                }))}
              />
              <Upload
                beforeUpload={uploadDocument}
                showUploadList={false}
                accept=".pdf,.jpg,.jpeg,.png,.webp"
              >
                <Button type="primary" icon={<UploadOutlined />} loading={documentUploading}>
                  上传资料
                </Button>
              </Upload>
            </Space>
          )
        }
      >
        <Table<CitizenDocument>
          rowKey="id"
          loading={documentsLoading}
          dataSource={documents}
          pagination={documents.length > 10 ? { pageSize: 10 } : false}
          scroll={{ x: 860 }}
          columns={[
            { title: '文件名', dataIndex: 'fileName', ellipsis: true },
            {
              title: '资料类型',
              dataIndex: 'documentType',
              width: 130,
              render: (value: CitizenDocumentType) => <Tag color="blue">{value}</Tag>,
            },
            {
              title: '大小',
              dataIndex: 'fileSize',
              width: 100,
              render: (value: number) => formatFileSize(value),
            },
            {
              title: '上传人',
              dataIndex: 'uploaderAccountId',
              ellipsis: true,
            },
            {
              title: '上传时间',
              dataIndex: 'uploadedAt',
              width: 180,
              render: (value: string) => formatDateTime(value),
            },
            {
              title: '操作',
              width: 120,
              align: 'center',
              render: (_value, row) => (
                <Space size={4}>
                  <Button
                    type="link"
                    size="small"
                    icon={<DownloadOutlined />}
                    onClick={() => downloadDocument(row)}
                  />
                  {canWrite && (
                    <Popconfirm
                      title={`确认删除 "${row.fileName}"?`}
                      okText="删除"
                      okButtonProps={{ danger: true }}
                      cancelText="取消"
                      onConfirm={() => deleteDocument(row)}
                    >
                      <Button type="link" size="small" danger icon={<DeleteOutlined />} />
                    </Popconfirm>
                  )}
                </Space>
              ),
            },
          ]}
        />
      </Card>

      <EditCitizenModal
        auth={auth}
        open={editOpen}
        citizen={current}
        onClose={() => setEditOpen(false)}
        onSaved={(next) => {
          setCurrent(next);
          onUpdated(next);
        }}
      />

      <ScanAccountModal
        open={scanOpen}
        onClose={() => setScanOpen(false)}
        onResolved={(accountId) => {
          form.setFieldsValue({ accountId: accountId });
          setScanOpen(false);
        }}
      />


      <CitizenSignatureModal
        title={prepared?.actionLabelZh ?? '公民签名确认'}
        open={!!prepared}
        onCancel={() => setPrepared(null)}
        qrTitle="身份载荷签名二维码"
        qrValue={prepared?.signRequest}
        qrHint="使用该公民本人的公民钱包扫码签名"
        scannerHint="扫描该公民钱包生成的签名响应二维码"
        scannerDisabled={completeLoading}
        scannerLoading={completeLoading}
        onDetected={completeOnchainSignature}
        onScannerError={(msg) => notice.error(msg)}
      />

      {chainSignModal}
      {newWalletSignModal}
    </>
  );
}

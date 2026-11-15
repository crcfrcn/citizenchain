import { encodeSs58 } from '@gmb/onchina-platform/codes/ss58';
// 公民详情页承接单个公民档案展示与链上身份推送。
// 身份授权直接使用最新finalized绑定钱包；本页不再重复录入或扫描账户。

import { useCallback, useEffect, useMemo, useRef, useState } from 'react';
import {
  Alert,
  Button,
  Card,
  Descriptions,
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
  UploadOutlined,
} from '@ant-design/icons';
import type { UploadFile } from 'antd/es/upload/interface';

import type { AdminAuth } from '@gmb/onchina-platform/authentication/types';
import { glassCardHeadStyle, glassCardStyle } from '@gmb/onchina-platform/controls/cardStyles';
import { CitizenSignatureModal } from '@gmb/onchina-platform/qr/CitizenSignatureModal';
import { EditCitizenModal } from './EditCitizenModal';
import { submitChainSign, useChainSign } from '@gmb/onchina-platform/qr/useChainSign';
import { notice } from '@gmb/onchina-platform/controls/notice';
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
} from './api';

type Props = {
  auth: AdminAuth;
  citizen: CitizenRow;
  canWrite: boolean;
  provinceName?: string | null;
  cityName?: string | null;
  onBack: () => void;
  onUpdated: (next: CitizenRow) => void;
};

type IdentityChoice = {
  source: 'none' | CitizenOnchainIdentityLevel;
  level?: CitizenOnchainIdentityLevel;
  editing: boolean;
};

function aborted(error: unknown) {
  return error instanceof DOMException && error.name === 'AbortError';
}

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

// 更换公民或登录会话时销毁旧选择、二维码与迟到的读取，不能跨上下文沿用草稿。
export function CitizenDetailPage(props: Props) {
  return <CitizenDetailContent key={`${props.auth.accessToken}|${props.auth.accountId}|${props.auth.institutionCidNumber}|${props.citizen.cidNumber}`} {...props} />;
}

function CitizenDetailContent({
  auth,
  citizen,
  canWrite,
  provinceName,
  cityName,
  onBack,
  onUpdated,
}: Props) {
  const [current, setCurrent] = useState(citizen);
  const [binding, setBinding] = useState<FinalizedCitizenBinding | null>(null);
  const [bindingLoading, setBindingLoading] = useState(true);
  const [bindingError, setBindingError] = useState<string | null>(null);
  const [identityChoice, setIdentityChoice] = useState<IdentityChoice | null>(null);
  const alive = useRef(true);
  const bindingSequence = useRef(0);
  const onUpdatedRef = useRef(onUpdated);
  onUpdatedRef.current = onUpdated;
  const currentRef = useRef(current);
  currentRef.current = current;
  const [editOpen, setEditOpen] = useState(false);
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
  const [rebinding, setRebinding] = useState(false);
  const [rebindOpen, setRebindOpen] = useState(false);

  const ageYears = useMemo(() => calculateAgeYears(current.citizenBirthDate), [current.citizenBirthDate]);
  const canPushOnchain =
    canWrite &&
    current.citizenStatus === 'NORMAL' &&
    current.identityStatus === 'NORMAL' &&
    current.votingEligible &&
    typeof ageYears === 'number' &&
    ageYears >= 16;

  // 竞选身份同时持有投票身份，必须优先按竞选处理；钱包版本不是身份等级版本。
  const actualIdentity = binding?.candidateIdentity ? 'candidate' : binding?.votingIdentity ? 'voting' : null;
  const choice = identityChoice?.source === (actualIdentity ?? 'none') ? identityChoice : null;
  const editingIdentity = actualIdentity === 'voting' && !!choice?.editing;
  const selectedIdentity = choice?.level;
  const identityValue = actualIdentity === 'candidate' ? 'candidate'
    : actualIdentity === 'voting' ? editingIdentity ? selectedIdentity ?? 'voting' : 'voting' : selectedIdentity;
  const changingIdentity = actualIdentity === 'voting' && !editingIdentity;
  const busy = prepareLoading || completeLoading || chainSubmitting || rebinding || !!prepared;
  const bindingReady = !!binding && !bindingLoading && !bindingError && binding.bindingActive
    && binding.cidStatus === 'ACTIVE' && !!binding.accountId;
  const identityEditable = bindingReady && canPushOnchain && !busy
    && (actualIdentity === null || editingIdentity);
  const canSignIdentity = bindingReady && canPushOnchain && !busy
    && (actualIdentity === null && !!selectedIdentity
      || actualIdentity === 'voting' && editingIdentity && selectedIdentity === 'candidate');
  const canChangeIdentity = bindingReady && canPushOnchain && !busy && changingIdentity;

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
      setBinding(binding);
      currentRef.current = next;
      setCurrent(next);
      onUpdatedRef.current(next);
    },
    [],
  );

  const loadFinalizedBinding = useCallback(async () => {
    const sequence = ++bindingSequence.current;
    setBindingLoading(true);
    setBindingError(null);
    try {
      const next = await fetchFinalizedCitizenBinding(auth, currentRef.current.cidNumber);
      if (!alive.current || sequence !== bindingSequence.current) throw new DOMException('stale binding response', 'AbortError');
      applyFinalizedBinding(next);
      return next;
    } catch (error) {
      if (alive.current && sequence === bindingSequence.current && !aborted(error)) {
        setBindingError(error instanceof Error ? error.message : String(error));
      }
      throw error;
    } finally {
      if (alive.current && sequence === bindingSequence.current) setBindingLoading(false);
    }
  }, [applyFinalizedBinding, auth.accessToken, auth.accountId, auth.institutionCidNumber]);

  useEffect(() => {
    alive.current = true;
    void loadFinalizedBinding().catch(error => {
      if (!aborted(error)) notice.error(error, '链上身份和钱包绑定加载失败');
    });
    return () => { alive.current = false; bindingSequence.current++; };
  }, [loadFinalizedBinding]);

  const prepareOnchainSignature = async () => {
    if (!canSignIdentity || !selectedIdentity) return;
    const target = selectedIdentity;
    let requested = false;
    setPrepareLoading(true);
    setPrepared(null);
    try {
      // 准备前重读：别人可能已经登记或升级，不能用旧选择生成重复授权。
      const next = await loadFinalizedBinding();
      if (!next.bindingActive || !next.accountId) throw new Error('公民当前没有有效绑定钱包，不能生成身份授权');
      if (next.candidateIdentity || next.votingIdentity && target !== 'candidate') {
        throw new Error('链上身份已变化，请按当前身份重新操作');
      }
      requested = true;
      const output = await prepareCitizenOnchainSignature(auth, currentRef.current.cidNumber, next.accountId, target);
      if (!alive.current) return;
      setPrepared(output);
      notice.success('公民钱包签名二维码已生成');
    } catch (error) {
      if (!aborted(error) && alive.current) {
        notice.error(error, '生成签名二维码失败');
        if (requested) await loadFinalizedBinding().catch(() => {});
      }
    } finally {
      if (alive.current) setPrepareLoading(false);
    }
  };

  const completeOnchainSignature = async (raw: string) => {
    if (!prepared) return;
    const { accountId, identityLevel } = prepared;
    setCompleteLoading(true);
    try {
      const output = await completeCitizenOnchainSignature(
        auth,
        current.cidNumber,
        accountId,
        identityLevel,
        raw,
      );
      if (!alive.current) return;
      setPrepared(null);
      notice.success('公民钱包签名已验证，请用注册局管理员公民钱包签名并提交上链');
      // D7：身份上链交易由 OnChina 组装提交，注册局管理员 CitizenWallet 只签名一次并显示响应二维码。
      const signed = await signChain(output.requestId, output.identitySignRequest);
      if (!alive.current) return;
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
      setPrepared(null);
      if (alive.current && !aborted(err)) {
        notice.error(err, '公民身份上链失败');
        // 冲突、冷签取消或回执丢失后也以真实链上结果收敛，不能保留可编辑的旧身份。
        await loadFinalizedBinding().catch(() => {});
      }
    } finally {
      setCompleteLoading(false);
    }
  };

  const revokeOnchain = async () => {
    if (!canWrite || current.citizenStatus === 'REVOKED' || chainSubmitting || prepareLoading || completeLoading || rebinding) return;
    setChainSubmitting(true);
    try {
      // 吊销同样只做一次 Passkey 和一次最终链签。
      const prep = await prepareCitizenRevoke(
        auth,
        current.cidNumber,
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
    if (!canWrite || !current.accountId || rebinding || chainSubmitting || prepareLoading || completeLoading) return;
    setRebinding(true);
    try {
      // 段1:发起换绑,返回给新钱包的域签名 QR。
      const prep = await prepareCitizenRebind(auth, current.cidNumber);
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
              <Space style={{ position: 'absolute', right: 0 }}>
                <Button
                  type="link"
                  danger
                  icon={<CloudUploadOutlined />}
                  loading={chainSubmitting}
                  disabled={current.citizenStatus === 'REVOKED' || chainSubmitting || prepareLoading || completeLoading || rebinding}
                  onClick={revokeOnchain}
                >
                  吊销身份
                </Button>
                <Button type="link" icon={<EditOutlined />} style={{ paddingRight: 0 }} onClick={() => setEditOpen(true)}>
                  编辑资料
                </Button>
              </Space>
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

        <div style={{ display: 'grid', gridTemplateColumns: 'repeat(2, minmax(0, 1fr))', gap: 24, alignItems: 'start' }}>
          <Descriptions column={1} size="small" bordered style={{ minWidth: 0 }} styles={{ label: { width: 120, minWidth: 120, maxWidth: 120, boxSizing: 'border-box' }, content: { overflowWrap: 'anywhere' } }}>
            <Descriptions.Item label="护照号">{current.passportNo || '-'}</Descriptions.Item>
            <Descriptions.Item label="公民CID号">{current.cidNumber || '-'}</Descriptions.Item>
            <Descriptions.Item label="姓名">{makeCitizenName(current)}</Descriptions.Item>
            <Descriptions.Item label="性别">{sexText(current.citizenSex)}</Descriptions.Item>
            <Descriptions.Item label="出生日期">{formatDate(current.citizenBirthDate)}</Descriptions.Item>
            <Descriptions.Item label="年龄">{typeof ageYears === 'number' ? `${ageYears}周岁` : '-'}</Descriptions.Item>
            <Descriptions.Item label="居住地">
              {areaText(current.provinceName, current.cityName, current.townName)}
            </Descriptions.Item>
            <Descriptions.Item label="出生地">
              {areaText(current.birthProvinceName, current.birthCityName, current.birthTownName)}
            </Descriptions.Item>
            <Descriptions.Item label="有效期">
              {formatDateRange(current.passportValidFrom, current.passportValidUntil)}
            </Descriptions.Item>
          </Descriptions>
          <Descriptions column={1} size="small" bordered style={{ minWidth: 0 }} styles={{ label: { width: 120, minWidth: 120, maxWidth: 120, boxSizing: 'border-box' }, content: { overflowWrap: 'anywhere' } }}>
            <Descriptions.Item label="绑定钱包账户">
              <div style={{ display: 'flex', alignItems: 'center', gap: 12, width: '100%' }}>
                <span style={{ flex: 1, minWidth: 0, overflowWrap: 'anywhere' }}>{current.accountId ? encodeSs58(current.accountId) : '-'}</span>
                {canWrite && <Button type="link" style={{ flexShrink: 0, paddingRight: 0 }} loading={rebinding}
                  disabled={!current.accountId || prepareLoading || completeLoading || chainSubmitting}
                  onClick={() => setRebindOpen(true)}>更换</Button>}
              </div>
            </Descriptions.Item>
            <Descriptions.Item label="身份上链">
              <div style={{ display: 'flex', alignItems: 'center', gap: 12, width: '100%' }}>
                <div style={{ flex: 1, minWidth: 0 }}>
                  <Select<CitizenOnchainIdentityLevel>
                    aria-label="链上身份"
                    placeholder="选择身份"
                    value={identityValue}
                    disabled={!identityEditable}
                    style={{ width: '100%', maxWidth: 180 }}
                    onChange={level => setIdentityChoice({ source: actualIdentity ?? 'none', level, editing: editingIdentity })}
                    options={[
                      { value: 'voting', label: '投票身份', disabled: actualIdentity === 'voting' },
                      { value: 'candidate', label: '竞选身份' },
                    ]}
                  />
                </div>
                <Button type="link" danger={changingIdentity}
                  style={{ flexShrink: 0, paddingRight: 0, ...(!changingIdentity && canSignIdentity ? { color: '#1677ff' } : {}) }}
                  disabled={changingIdentity ? !canChangeIdentity : !canSignIdentity}
                  loading={prepareLoading || completeLoading || chainSubmitting}
                  onClick={() => {
                    if (changingIdentity) setIdentityChoice({ source: 'voting', editing: true });
                    else void prepareOnchainSignature();
                  }}>{changingIdentity ? '更换' : '签名'}</Button>
              </div>
              {bindingError && <Space style={{ marginTop: 8 }}><Typography.Text type="danger">链上身份读取失败</Typography.Text><Button type="link" size="small" onClick={() => { void loadFinalizedBinding().catch(() => {}); }}>重试</Button></Space>}
              {!canPushOnchain && <Alert type="warning" showIcon style={{ marginTop: 8 }} message="当前档案或登录权限不满足身份上链条件" description="公民必须年满16周岁、档案正常且具有选举资格。" />}
            </Descriptions.Item>
            <Descriptions.Item label="绑定版本">{current.bindingRevision || '-'}</Descriptions.Item>
            <Descriptions.Item label="绑定区块号">
              {current.bindingBlockNumber ?? '-'}
            </Descriptions.Item>
            <Descriptions.Item label="选举权利">{current.votingEligible ? '有' : '无'}</Descriptions.Item>
            <Descriptions.Item label="公民状态">{statusTag(current.citizenStatus)}</Descriptions.Item>
            <Descriptions.Item label="投票身份状态">{statusText(current.identityStatus)}</Descriptions.Item>
            <Descriptions.Item label="档案哈希">{current.archiveHash || '-'}</Descriptions.Item>
            <Descriptions.Item label="链上交易">{current.onchainTxHash || '-'}</Descriptions.Item>
          </Descriptions>
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

      <Modal
        title="确认更换绑定钱包账户"
        open={rebindOpen}
        onCancel={() => setRebindOpen(false)}
        okText="确认更换"
        cancelText="取消"
        okButtonProps={{ disabled: !current.accountId || rebinding }}
        onOk={() => { setRebindOpen(false); void rebindWallet(); }}
      >
        <Typography.Paragraph>确认更换该公民CID的绑定钱包账户？</Typography.Paragraph>
        <Typography.Paragraph>公民CID：{current.cidNumber}</Typography.Paragraph>
        <Typography.Paragraph style={{ overflowWrap: 'anywhere' }}>当前绑定钱包：{current.accountId ? encodeSs58(current.accountId) : '-'}</Typography.Paragraph>
        <Typography.Paragraph>确认后由新钱包签名，并由当前管理员验证密钥、冷签提交。</Typography.Paragraph>
      </Modal>

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

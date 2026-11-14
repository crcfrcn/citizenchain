import { RolesView } from '@gmb/onchina-platform/institutions/RolesView';
import { AssignmentsView } from '@gmb/onchina-platform/institutions/AssignmentsView';
import { RepresentativeView } from '@gmb/onchina-platform/institutions/RepresentativeView';
import React, { useCallback, useEffect, useState } from 'react';
import { Button, Card, Col, Descriptions, Row, Space, Tag, Typography } from 'antd';
import { EDUCATION_TYPE_LABEL } from '@gmb/onchina-platform/institutions/labels';
import { useInstitutionCodeLabels } from '@gmb/onchina-platform/institutions/institutionLabels';
import { type InstitutionDetail, getInstitution } from './api';
import { type AdminAuth } from '@gmb/onchina-platform/authentication/types';
import { LegalRepresentativePhoto } from '@gmb/onchina-platform/institutions/Photo';
import { AccountList } from '@gmb/onchina-platform/accounts/AccountList';
import { notice } from '@gmb/onchina-platform/controls/notice';
import { PrivateDetailLayout } from '../private/PrivateDetailLayout';
import { DocsLibrary } from '@gmb/onchina-platform/documents/Library';
import { institutionDetailCacheKey, readCachedInstitutionDetail, writeCachedInstitutionDetail } from '@gmb/onchina-platform/codes/cache';
import { InstitutionDetailNavLayout } from '@gmb/onchina-platform/workspace/DetailLayout';
import { OperationRecords } from './OperationRecords';
import { InstitutionAdministratorsPanel as InstitutionGovernancePanel } from '../admins/AddCityRegistryAdminModal';
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



export const GovDetailPage: React.FC<Props> = ({ auth, cidNumber, canWrite, onBack, backLabel, loadDetail, adminListSection, initialActiveKey }) => {
  const detailCacheKey = institutionDetailCacheKey(auth, cidNumber);
  const [detail, setDetail] = useState<InstitutionDetail | null>(() =>
    readCachedInstitutionDetail(detailCacheKey),
  );
  const [loading, setLoading] = useState(false);


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
            {key:'roles',label:'岗位管理',content:<RolesView auth={auth} cidNumber={inst.cidNumber} canWrite={canWrite}/>},
            {key:'assignments',label:'任职管理',content:<AssignmentsView auth={auth} cidNumber={inst.cidNumber} canWrite={canWrite}/>},
            {key:'representative',label:'法定代表人',content:<RepresentativeView auth={auth} cidNumber={inst.cidNumber} canWrite={canWrite}/>},
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
              onBack={onBack}
              backLabel={backLabel}
            />
          ) : renderOfficialDetail()}
        </>
      )}
    </div>
  );
};

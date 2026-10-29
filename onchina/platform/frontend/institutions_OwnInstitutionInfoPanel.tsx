// 平台通用本机构信息面板。保留原有链读取、字段、样式、hooks 与交互。

import { useEffect, useState } from 'react';
import { Card, Descriptions, Spin, Tag } from 'antd';
import type { AdminAuth } from './authentication_types';
import { getOwnInstitution } from './admins_api';
import { InstitutionCategoryLabel, type Institution, type InstitutionAccount, type InstitutionDetail } from './subjects_api';
import {
  EDUCATION_TYPE_LABEL,
  PARTNERSHIP_KIND_LABEL,
  PRIVATE_TYPE_LABEL,
} from './subjects_labels';
import { useInstitutionCodeLabels } from './subjects_institutionLabels';
import { tryEncodeSs58 } from './helpers_ss58';
import { notice } from './helpers_notice';

export type OwnInstitutionInfoProps = {
  auth: AdminAuth;
};

const STATUS_LABEL: Record<string, string> = {
  ACTIVE: '正常',
  REVOKED: '已注销',
};

const ADMIN_LEVEL_LABEL: Record<string, string> = {
  NATIONAL: '国家级',
  PROVINCE: '省级',
  CITY: '市级',
  TOWN: '镇级',
};

const CHAIN_STATUS_LABEL: Record<string, string> = {
  NOT_ON_CHAIN: '未上链',
  PENDING_ON_CHAIN: '待上链',
  ACTIVE_ON_CHAIN: '已上链',
  REVOKED_ON_CHAIN: '已注销',
};

const P1_LABEL: Record<string, string> = {
  '0': '非盈利',
  '1': '盈利',
};

function displayValue(value: string | number | null | undefined): string {
  if (value === null || value === undefined) return '-';
  const text = String(value).trim();
  return text || '-';
}

function formatScope(auth: AdminAuth, inst?: Institution): string {
  const values = inst
    ? [inst.provinceName, inst.cityName, inst.townName]
    : [auth.scopeProvinceName, auth.scopeCityName, auth.scopeTownName];
  return values.filter(Boolean).join(' / ') || '全国';
}

function primaryAccount(accounts: InstitutionAccount[]): InstitutionAccount | null {
  return (
    accounts.find((item) => ['main', '主账户'].includes(item.accountName.trim().toLowerCase())) ??
    accounts[0] ??
    null
  );
}

function accountText(accountId?: string | null): string {
  if (!accountId) return '-';
  return tryEncodeSs58(accountId);
}

export function OwnInstitutionInfoPanel({ auth }: OwnInstitutionInfoProps) {
  const [detail, setDetail] = useState<InstitutionDetail | null>(null);
  const [loading, setLoading] = useState(false);
  const institutionLabels = useInstitutionCodeLabels();

  useEffect(() => {
    let cancelled = false;
    setLoading(true);
    getOwnInstitution(auth)
      .then((result) => {
        if (!cancelled) setDetail(result);
      })
      .catch((err) => {
        if (!cancelled) notice.error(err, '加载本机构信息失败');
      })
      .finally(() => {
        if (!cancelled) setLoading(false);
      });
    return () => {
      cancelled = true;
    };
  }, [auth.accessToken]);

  const inst = detail?.institution;
  const account = primaryAccount(detail?.accounts ?? []);
  const status = inst?.status ? STATUS_LABEL[inst.status] || inst.status : '-';
  const chainStatus = account?.chainStatus ? CHAIN_STATUS_LABEL[account.chainStatus] || account.chainStatus : '-';
  const institutionCode = inst?.institutionCode ?? auth.institutionCode;
  const institutionCodeTitle = institutionLabels[institutionCode] || institutionCode;

  const scopeText = [auth.scopeProvinceName, auth.scopeCityName, auth.scopeTownName]
    .filter(Boolean)
    .join(' / ') || '全国';

  // 机构详情来自当前 active binding,不接受前端传入 cid_number,因此这里仅负责展示。
  return (
    <Card title="本机构信息" bordered={false} style={{ background: '#ffffff', borderRadius: 8 }}>
      <Spin spinning={loading}>
        <Descriptions column={{ xs: 1, sm: 2, lg: 3 }} size="small">
          <Descriptions.Item label="机构全称">
            {displayValue(inst?.cidFullName ?? auth.cidShortName)}
          </Descriptions.Item>
          <Descriptions.Item label="机构简称">
            {displayValue(inst?.cidShortName ?? auth.cidShortName)}
          </Descriptions.Item>
          <Descriptions.Item label="身份ID">{displayValue(inst?.cidNumber)}</Descriptions.Item>
          <Descriptions.Item label="机构码">
            {institutionCodeTitle === institutionCode ? institutionCode : `${institutionCodeTitle}(${institutionCode})`}
          </Descriptions.Item>
          <Descriptions.Item label="机构类别">
            {inst?.category ? InstitutionCategoryLabel[inst.category] || inst.category : '-'}
          </Descriptions.Item>
          <Descriptions.Item label="主体状态">
            {inst?.status ? <Tag color={inst.status === 'ACTIVE' ? 'green' : 'red'}>{status}</Tag> : '-'}
          </Descriptions.Item>
          <Descriptions.Item label="行政层级">
            {auth.adminLevel ? ADMIN_LEVEL_LABEL[auth.adminLevel] || auth.adminLevel : '-'}
          </Descriptions.Item>
          <Descriptions.Item label="辖区">{inst ? formatScope(auth, inst) : scopeText}</Descriptions.Item>
          <Descriptions.Item label="盈利属性">{inst?.p1 ? P1_LABEL[inst.p1] || inst.p1 : '-'}</Descriptions.Item>
          {inst?.educationType ? (
            <Descriptions.Item label="教育分类">
              {EDUCATION_TYPE_LABEL[inst.educationType] || inst.educationType}
            </Descriptions.Item>
          ) : null}
          {inst?.privateType ? (
            <Descriptions.Item label="私权类型">
              {PRIVATE_TYPE_LABEL[inst.privateType] || inst.privateType}
            </Descriptions.Item>
          ) : null}
          {inst?.partnershipKind ? (
            <Descriptions.Item label="合伙类型">
              {PARTNERSHIP_KIND_LABEL[inst.partnershipKind] || inst.partnershipKind}
            </Descriptions.Item>
          ) : null}
          {inst?.hasLegalPersonality !== undefined && inst?.hasLegalPersonality !== null ? (
            <Descriptions.Item label="法人资格">
              {inst.hasLegalPersonality ? '具有法人资格' : '不具有法人资格'}
            </Descriptions.Item>
          ) : null}
          {inst?.parentCidNumber ? (
            <Descriptions.Item label="所属法人身份ID">{inst.parentCidNumber}</Descriptions.Item>
          ) : null}
          <Descriptions.Item label="主账户">{displayValue(account?.accountName)}</Descriptions.Item>
          <Descriptions.Item label="主账户地址">{accountText(account?.accountId)}</Descriptions.Item>
          <Descriptions.Item label="主账户状态">{chainStatus}</Descriptions.Item>
          <Descriptions.Item label="账户数量">{detail?.accounts.length ?? '-'}</Descriptions.Item>
          {inst?.legalRepresentative ? (
            <Descriptions.Item label="法定代表人">
              {`${inst.legalRepresentative.family_name}${inst.legalRepresentative.given_name}`}
            </Descriptions.Item>
          ) : null}
          {inst?.legalRepresentative?.cid_number ? (
            <Descriptions.Item label="法定代表人身份ID">
              {inst.legalRepresentative.cid_number}
            </Descriptions.Item>
          ) : null}
          {detail?.creatorFamilyName || detail?.creatorGivenName ? (
            <Descriptions.Item label="登记管理员">
              {`${detail.creatorFamilyName ?? ''}${detail.creatorGivenName ?? ''}`}
            </Descriptions.Item>
          ) : null}
          {detail?.creatorInstitutionCode ? (
            <Descriptions.Item label="登记机构码">{detail.creatorInstitutionCode}</Descriptions.Item>
          ) : null}
          <Descriptions.Item label="创建时间">{displayValue(inst?.createdAt)}</Descriptions.Item>
        </Descriptions>
      </Spin>
    </Card>
  );
}


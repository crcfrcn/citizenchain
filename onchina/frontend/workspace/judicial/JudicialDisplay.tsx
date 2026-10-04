// 司法院显示页。机构资料与管理员列表都只读链上/本地投影,不在前端决定权限。

import { useEffect, useState } from 'react';
import { Card, Descriptions, Spin, Tag } from 'antd';
import type { AdminAuth } from '../../auth/types';
import { getOwnInstitution } from '../../admins/api';
import { OwnInstitutionAdminsView } from '../../admins/RegistryAdminsView';
import { InstitutionCategoryLabel, type Institution, type InstitutionAccount, type InstitutionDetail } from '../../subjects/api';
import {
  EDUCATION_TYPE_LABEL,
  PARTNERSHIP_KIND_LABEL,
  PRIVATE_TYPE_LABEL,
} from '../../subjects/labels';
import { useInstitutionCodeLabels } from '../../subjects/institutionLabels';
import { tryEncodeSs58 } from '../../utils/ss58';
import { notice } from '../../utils/notice';

export type JudicialDisplayProps = {
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
    ? [inst.province_name, inst.city_name, inst.town_name]
    : [auth.scope_province_name, auth.scope_city_name, auth.scope_town_name];
  return values.filter(Boolean).join(' / ') || '全国';
}

function primaryAccount(accounts: InstitutionAccount[]): InstitutionAccount | null {
  return (
    accounts.find((item) => ['main', '主账户'].includes(item.account_name.trim().toLowerCase())) ??
    accounts[0] ??
    null
  );
}

function accountText(account_id?: string | null): string {
  if (!account_id) return '-';
  return tryEncodeSs58(account_id);
}

export function OwnInstitutionInfoPanel({ auth }: JudicialDisplayProps) {
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
  }, [auth.access_token]);

  const inst = detail?.institution;
  const account = primaryAccount(detail?.accounts ?? []);
  const status = inst?.status ? STATUS_LABEL[inst.status] || inst.status : '-';
  const chainStatus = account?.chain_status ? CHAIN_STATUS_LABEL[account.chain_status] || account.chain_status : '-';
  const institutionCode = inst?.institution_code ?? auth.institution_code;
  const institutionCodeTitle = institutionLabels[institutionCode] || institutionCode;

  const scopeText = [auth.scope_province_name, auth.scope_city_name, auth.scope_town_name]
    .filter(Boolean)
    .join(' / ') || '全国';

  // 机构详情来自当前 active binding,不接受前端传入 cid_number,因此这里仅负责展示。
  return (
    <Card title="本机构信息" bordered={false} style={{ background: '#ffffff', borderRadius: 8 }}>
      <Spin spinning={loading}>
        <Descriptions column={{ xs: 1, sm: 2, lg: 3 }} size="small">
          <Descriptions.Item label="机构全称">
            {displayValue(inst?.cid_full_name ?? auth.cid_short_name)}
          </Descriptions.Item>
          <Descriptions.Item label="机构简称">
            {displayValue(inst?.cid_short_name ?? auth.cid_short_name)}
          </Descriptions.Item>
          <Descriptions.Item label="身份ID">{displayValue(inst?.cid_number)}</Descriptions.Item>
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
            {auth.admin_level ? ADMIN_LEVEL_LABEL[auth.admin_level] || auth.admin_level : '-'}
          </Descriptions.Item>
          <Descriptions.Item label="辖区">{inst ? formatScope(auth, inst) : scopeText}</Descriptions.Item>
          <Descriptions.Item label="盈利属性">{inst?.p1 ? P1_LABEL[inst.p1] || inst.p1 : '-'}</Descriptions.Item>
          {inst?.education_type ? (
            <Descriptions.Item label="教育分类">
              {EDUCATION_TYPE_LABEL[inst.education_type] || inst.education_type}
            </Descriptions.Item>
          ) : null}
          {inst?.private_type ? (
            <Descriptions.Item label="私权类型">
              {PRIVATE_TYPE_LABEL[inst.private_type] || inst.private_type}
            </Descriptions.Item>
          ) : null}
          {inst?.partnership_kind ? (
            <Descriptions.Item label="合伙类型">
              {PARTNERSHIP_KIND_LABEL[inst.partnership_kind] || inst.partnership_kind}
            </Descriptions.Item>
          ) : null}
          {inst?.has_legal_personality !== undefined && inst?.has_legal_personality !== null ? (
            <Descriptions.Item label="法人资格">
              {inst.has_legal_personality ? '具有法人资格' : '不具有法人资格'}
            </Descriptions.Item>
          ) : null}
          {inst?.parent_cid_number ? (
            <Descriptions.Item label="所属法人身份ID">{inst.parent_cid_number}</Descriptions.Item>
          ) : null}
          <Descriptions.Item label="主账户">{displayValue(account?.account_name)}</Descriptions.Item>
          <Descriptions.Item label="主账户地址">{accountText(account?.account_id)}</Descriptions.Item>
          <Descriptions.Item label="主账户状态">{chainStatus}</Descriptions.Item>
          <Descriptions.Item label="账户数量">{detail?.accounts.length ?? '-'}</Descriptions.Item>
          {inst?.legal_representative ? (
            <Descriptions.Item label="法定代表人">
              {`${inst.legal_representative.family_name}${inst.legal_representative.given_name}`}
            </Descriptions.Item>
          ) : null}
          {inst?.legal_representative?.cid_number ? (
            <Descriptions.Item label="法定代表人身份ID">
              {inst.legal_representative.cid_number}
            </Descriptions.Item>
          ) : null}
          {detail?.creator_family_name || detail?.creator_given_name ? (
            <Descriptions.Item label="登记管理员">
              {`${detail.creator_family_name ?? ''}${detail.creator_given_name ?? ''}`}
            </Descriptions.Item>
          ) : null}
          {detail?.creator_institution_code ? (
            <Descriptions.Item label="登记机构码">{detail.creator_institution_code}</Descriptions.Item>
          ) : null}
          <Descriptions.Item label="创建时间">{displayValue(inst?.created_at)}</Descriptions.Item>
        </Descriptions>
      </Spin>
    </Card>
  );
}

export function JudicialDisplay({ auth }: JudicialDisplayProps) {
  return (
    <div style={{ display: 'flex', flexDirection: 'column', gap: 16 }}>
      <OwnInstitutionInfoPanel auth={auth} />
      <OwnInstitutionAdminsView layout="cards" />
    </div>
  );
}

import type { ReactNode } from 'react';
import { Descriptions, Typography } from 'antd';
import { tryEncodeSs58 } from '../codes/ss58';

/** 管理员账户与机构岗位的一条任职关系；姓名按链上管理员记录的两个字段投影。 */
export type InstitutionAssignmentLike = {
  accountId: string;
  familyName?: string | null;
  givenName?: string | null;
  provinceName?: string | null;
  cityName?: string | null;
  institutionCode?: string | null;
  roleCode?: string | null;
  roleName?: string | null;
  termRequired?: boolean | null;
  termStart?: number | null;
  termEnd?: number | null;
  assignmentSourceLabel?: string | null;
  assignmentSourceRef?: string | null;
  balanceFen?: string | null;
  builtIn?: boolean | null;
  createdAt?: string | null;
  updatedAt?: string | null;
};

type Props = { assignment: InstitutionAssignmentLike; index?: number; action?: ReactNode; status?: ReactNode; actionPlacement?: 'top' | 'balance-row' };

export function assignmentTermText(assignment: InstitutionAssignmentLike): string {
  if (!assignment.termRequired) return '无任期';
  if (!assignment.termStart && !assignment.termEnd) return '';
  return `${assignment.termStart ?? ''} ~ ${assignment.termEnd ?? ''}（自纪元日序）`;
}

export function formatAdminBalanceFen(fen?: string | null): string {
  if (fen == null) return '';
  try {
    const value = BigInt(fen);
    const negative = value < 0n;
    const abs = negative ? -value : value;
    const yuan = abs / 100n;
    const cents = abs % 100n;
    const yuanText = yuan.toString().replace(/\B(?=(\d{3})+(?!\d))/g, ',');
    return `${negative ? '-' : ''}${yuanText}.${cents.toString().padStart(2, '0')} 元`;
  } catch { return ''; }
}

export function assignmentDisplayLabel(assignment: InstitutionAssignmentLike): string {
  return `${assignment.familyName?.trim() ?? ''}${assignment.givenName?.trim() ?? ''}`;
}

function Field({ label, value, trailing }: { label: string; value: string; trailing?: ReactNode }) {
  return <div style={{ display: 'grid', gridTemplateColumns: trailing ? 'max-content minmax(0, 1fr) auto' : 'max-content minmax(0, 1fr)', gap: 6, minHeight: 20 }}>
    <span style={{ color: '#93a4b8', fontSize: 12 }}>{label}:</span>
    <span style={{ color: '#e5edf6', fontSize: 12, wordBreak: 'break-all', fontFamily: 'ui-monospace, SFMono-Regular, Menlo, Consolas, monospace' }}>{value}</span>
    {trailing ? <span>{trailing}</span> : null}
  </div>;
}

export function InstitutionAssignmentCard({ assignment, index, action, status, actionPlacement = 'top' }: Props) {
  const topAction = actionPlacement === 'top' ? action ?? status : status;
  const balanceAction = actionPlacement === 'balance-row' ? action : null;
  const accountText = assignment.accountId ? tryEncodeSs58(assignment.accountId) : '';
  return <div style={{ display: 'grid', gap: 8, minWidth: 0, padding: 12, borderRadius: 8, border: '1px solid rgba(148, 163, 184, 0.28)', background: 'rgba(15, 23, 42, 0.92)' }}>
    <div style={{ display: 'flex', minHeight: 28, alignItems: 'center', justifyContent: 'space-between', gap: 10 }}>
      {index != null ? <span style={{ color: '#bfdbfe', fontWeight: 700 }}>{index}</span> : <span />}
      <span>{topAction}</span>
    </div>
    <Field label="姓名" value={assignmentDisplayLabel(assignment)} />
    <Field label="岗位" value={assignment.roleName ?? ''} />
    <Field label="岗位码" value={assignment.roleCode ?? ''} />
    <Field label="任期" value={assignmentTermText(assignment)} />
    <Field label="任职来源" value={assignment.assignmentSourceLabel ?? ''} />
    <Field label="来源引用" value={assignment.assignmentSourceRef ?? ''} />
    <Field label="管理员账户" value={accountText} />
    <Field label="余额" value={formatAdminBalanceFen(assignment.balanceFen)} trailing={balanceAction} />
  </div>;
}

export function InstitutionAssignmentDetails({ assignment, areaLabel, areaValue }: { assignment: InstitutionAssignmentLike; areaLabel?: string; areaValue?: string | null }) {
  const ss58 = assignment.accountId ? tryEncodeSs58(assignment.accountId) : '';
  return <Descriptions column={1} size="small" bordered>
    {areaLabel ? <Descriptions.Item label={areaLabel}>{areaValue || '-'}</Descriptions.Item> : null}
    <Descriptions.Item label="姓名">{assignmentDisplayLabel(assignment) || '-'}</Descriptions.Item>
    <Descriptions.Item label="岗位">{assignment.roleName || '-'}</Descriptions.Item>
    <Descriptions.Item label="岗位码">{assignment.roleCode || '-'}</Descriptions.Item>
    <Descriptions.Item label="任期">{assignmentTermText(assignment) || '-'}</Descriptions.Item>
    <Descriptions.Item label="任职来源">{assignment.assignmentSourceLabel || '-'}</Descriptions.Item>
    <Descriptions.Item label="来源引用">{assignment.assignmentSourceRef || '-'}</Descriptions.Item>
    <Descriptions.Item label="管理员账户">{ss58 ? <Typography.Text style={{ wordBreak: 'break-all' }}>{ss58}</Typography.Text> : '-'}</Descriptions.Item>
    <Descriptions.Item label="账户 Hex">{assignment.accountId || '-'}</Descriptions.Item>
    <Descriptions.Item label="余额">{formatAdminBalanceFen(assignment.balanceFen) || '-'}</Descriptions.Item>
    {assignment.institutionCode ? <Descriptions.Item label="机构码">{assignment.institutionCode}</Descriptions.Item> : null}
  </Descriptions>;
}

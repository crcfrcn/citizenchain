import type { ReactNode } from 'react';
import { Descriptions, Typography } from 'antd';
import { tryEncodeSs58 } from '../utils/ss58';

/** 管理员账户与机构岗位的一条任职关系；姓名按链上管理员记录的两个字段投影。 */
export type InstitutionAssignmentLike = {
  account_id: string;
  family_name?: string | null;
  given_name?: string | null;
  province_name?: string | null;
  city_name?: string | null;
  institution_code?: string | null;
  role_code?: string | null;
  role_name?: string | null;
  term_required?: boolean | null;
  term_start?: number | null;
  term_end?: number | null;
  assignment_source_label?: string | null;
  assignment_source_ref?: string | null;
  balance_fen?: string | null;
  built_in?: boolean | null;
  created_at?: string | null;
  updated_at?: string | null;
};

type Props = { assignment: InstitutionAssignmentLike; index?: number; action?: ReactNode; status?: ReactNode; actionPlacement?: 'top' | 'balance-row' };

export function assignmentTermText(assignment: InstitutionAssignmentLike): string {
  if (!assignment.term_required) return '无任期';
  if (!assignment.term_start && !assignment.term_end) return '';
  return `${assignment.term_start ?? ''} ~ ${assignment.term_end ?? ''}（自纪元日序）`;
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
  return `${assignment.family_name?.trim() ?? ''}${assignment.given_name?.trim() ?? ''}`;
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
  const accountText = assignment.account_id ? tryEncodeSs58(assignment.account_id) : '';
  return <div style={{ display: 'grid', gap: 8, minWidth: 0, padding: 12, borderRadius: 8, border: '1px solid rgba(148, 163, 184, 0.28)', background: 'rgba(15, 23, 42, 0.92)' }}>
    <div style={{ display: 'flex', minHeight: 28, alignItems: 'center', justifyContent: 'space-between', gap: 10 }}>
      {index != null ? <span style={{ color: '#bfdbfe', fontWeight: 700 }}>{index}</span> : <span />}
      <span>{topAction}</span>
    </div>
    <Field label="姓名" value={assignmentDisplayLabel(assignment)} />
    <Field label="岗位" value={assignment.role_name ?? ''} />
    <Field label="岗位码" value={assignment.role_code ?? ''} />
    <Field label="任期" value={assignmentTermText(assignment)} />
    <Field label="任职来源" value={assignment.assignment_source_label ?? ''} />
    <Field label="来源引用" value={assignment.assignment_source_ref ?? ''} />
    <Field label="管理员账户" value={accountText} />
    <Field label="余额" value={formatAdminBalanceFen(assignment.balance_fen)} trailing={balanceAction} />
  </div>;
}

export function InstitutionAssignmentDetails({ assignment, areaLabel, areaValue }: { assignment: InstitutionAssignmentLike; areaLabel?: string; areaValue?: string | null }) {
  const ss58 = assignment.account_id ? tryEncodeSs58(assignment.account_id) : '';
  return <Descriptions column={1} size="small" bordered>
    {areaLabel ? <Descriptions.Item label={areaLabel}>{areaValue || '-'}</Descriptions.Item> : null}
    <Descriptions.Item label="姓名">{assignmentDisplayLabel(assignment) || '-'}</Descriptions.Item>
    <Descriptions.Item label="岗位">{assignment.role_name || '-'}</Descriptions.Item>
    <Descriptions.Item label="岗位码">{assignment.role_code || '-'}</Descriptions.Item>
    <Descriptions.Item label="任期">{assignmentTermText(assignment) || '-'}</Descriptions.Item>
    <Descriptions.Item label="任职来源">{assignment.assignment_source_label || '-'}</Descriptions.Item>
    <Descriptions.Item label="来源引用">{assignment.assignment_source_ref || '-'}</Descriptions.Item>
    <Descriptions.Item label="管理员账户">{ss58 ? <Typography.Text style={{ wordBreak: 'break-all' }}>{ss58}</Typography.Text> : '-'}</Descriptions.Item>
    <Descriptions.Item label="账户 Hex">{assignment.account_id || '-'}</Descriptions.Item>
    <Descriptions.Item label="余额">{formatAdminBalanceFen(assignment.balance_fen) || '-'}</Descriptions.Item>
    {assignment.institution_code ? <Descriptions.Item label="机构码">{assignment.institution_code}</Descriptions.Item> : null}
  </Descriptions>;
}

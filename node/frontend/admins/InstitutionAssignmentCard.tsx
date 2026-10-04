import type { ReactNode } from 'react';

import { formatBalance } from '../shared/format';
import { accountIdToSs58 } from '../shared/ss58';
import type {
  InstitutionAdminInfo,
  InstitutionRoleAssignmentInfo,
} from './types';

type Props = {
  admin: InstitutionAdminInfo;
  index?: number;
  balanceFen?: string | null;
  className?: string;
  action?: ReactNode;
  status?: ReactNode;
};

function formatDay(day: number): string {
  if (!day) return '';
  const date = new Date(day * 86400 * 1000);
  const year = date.getUTCFullYear();
  const month = `${date.getUTCMonth() + 1}`.padStart(2, '0');
  const dayOfMonth = `${date.getUTCDate()}`.padStart(2, '0');
  return `${year}-${month}-${dayOfMonth}`;
}

function termText(assignment: InstitutionRoleAssignmentInfo): string {
  if (!assignment.termRequired) return '无固定任期';
  const start = formatDay(assignment.termStart);
  const end = formatDay(assignment.termEnd);
  if (!start && !end) return '待写入';
  return `${start || '—'} ~ ${end || '—'}`;
}

function AssignmentField({ label, value }: { label: string; value: string }) {
  return (
    <div className="institution-assignment-field">
      <span className="institution-assignment-label">{label}:</span>
      <span className="institution-assignment-value">{value}</span>
    </div>
  );
}

export function InstitutionAssignmentCard({
  admin,
  index,
  balanceFen,
  className,
  action,
  status,
}: Props) {
  const topAction = action ?? status;
  const accountText = admin.account_id ? accountIdToSs58(admin.account_id) : '';
  const personName = `${admin.familyName}${admin.givenName}`;
  const balanceText = balanceFen != null ? formatBalance(balanceFen) : '';

  return (
    <div className={`metric-card institution-assignment-card ${className ?? ''}`}>
      <div className="institution-assignment-top">
        {index != null ? <span className="admin-card-index">{index}</span> : null}
        <div className="institution-assignment-top-action">{topAction}</div>
      </div>

      <div className="institution-assignment-account">
        <AssignmentField label="管理员姓名" value={personName} />
        <AssignmentField label="管理员账户" value={accountText} />
        <AssignmentField label="账户余额" value={balanceText} />
      </div>

      <div className="institution-assignment-list">
        {admin.assignments.length === 0 ? (
          <div className="institution-assignment-item">暂无岗位</div>
        ) : admin.assignments.map((assignment) => (
          <div className="institution-assignment-item" key={assignment.roleCode}>
            <div className="institution-assignment-role">
              {assignment.roleName}
            </div>
            <AssignmentField label="任期" value={termText(assignment)} />
            <AssignmentField
              label="来源"
              value={assignment.assignmentSourceLabel}
            />
            {assignment.assignmentSourceRef && (
              <AssignmentField
                label="来源依据"
                value={assignment.assignmentSourceRef}
              />
            )}
          </div>
        ))}
      </div>
    </div>
  );
}

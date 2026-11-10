import { formatBalance } from "../client/format";
import { accountIdToSs58 } from "../client/ss58";
import type { InstitutionAdminInfo, InstitutionRoleAssignmentInfo } from "./administrators";
import type { ReactNode } from 'react';

type Props = {
  admin: InstitutionAdminInfo;
  index?: number;
  balanceFen?: string | null;
  className?: string;
  action?: ReactNode;
  status?: ReactNode;
  compactVote?: boolean;
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

function assignmentSummary(
  assignments: InstitutionRoleAssignmentInfo[],
  format: (assignment: InstitutionRoleAssignmentInfo) => string,
): string {
  const values = assignments.map(format);
  return new Set(values).size > 1
    ? assignments.map((assignment, i) => `${assignment.roleName} ${values[i]}`).join('；')
    : values[0] ?? '—';
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
  compactVote = false,
}: Props) {
  const topAction = action ?? status;
  const accountText = admin.accountId ? accountIdToSs58(admin.accountId) : '';
  const personName = `${admin.familyName}${admin.givenName}`;
  const balanceText = balanceFen != null ? formatBalance(balanceFen) : '';
  const assignments = admin.assignments;
  const term = assignmentSummary(assignments, termText);

  return (
    <div className={`metric-card institution-assignment-card ${compactVote ? 'institution-assignment-compact-vote' : ''} ${className ?? ''}`}>
      <div className="institution-assignment-top">
        {index != null ? <span className="admin-card-index">{index}</span> : null}
        <span className="institution-assignment-role">
          {assignments.map((assignment) => assignment.roleName).join('、') || '暂无岗位'}
        </span>
        <div className="institution-assignment-top-action">{topAction}</div>
      </div>

      {compactVote ? (
        <div className="institution-assignment-account">
          <div className="institution-assignment-name-term">
            <AssignmentField label="管理员姓名" value={personName.trim() || '未填写'} />
            <span className="institution-assignment-compact-term">任期：{term.replace(/ ~ /g, '–')}</span>
          </div>
          <AssignmentField label="账户余额" value={balanceText} />
          <AssignmentField label="管理员账户" value={accountText} />
        </div>
      ) : (
        <div className="institution-assignment-account">
          <div className="institution-assignment-list-row">
            <AssignmentField label="管理员姓名" value={personName} />
            <AssignmentField label="任期" value={term} />
          </div>
          <div className="institution-assignment-list-row">
            <AssignmentField label="账户余额" value={balanceText} />
            <AssignmentField label="来源" value={assignmentSummary(assignments, (assignment) =>
              assignment.assignmentSourceRef
                ? `${assignment.assignmentSourceLabel}（依据：${assignment.assignmentSourceRef}）`
                : assignment.assignmentSourceLabel
            )} />
          </div>
          <AssignmentField label="管理员账户" value={accountText} />
        </div>
      )}
    </div>
  );
}

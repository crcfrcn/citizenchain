// 清算行机构管理员列表子页(机构详情下的折叠卡片入口)。
//
import { useEffect, useMemo, useState } from 'react';
import { adminsChangeApi } from '../../admins/api';
import { InstitutionAssignmentCard } from '../../admins/InstitutionAssignmentCard';
import type { InstitutionAdminInfo } from '../../governance/types';
import '../../admins/styles.css';

type Props = {
  cidNumber: string;
  admins: InstitutionAdminInfo[];
  threshold: number;
  adminsLen: number;
  onBack: () => void;
};

export function ClearingBankAdminListPage({
  cidNumber,
  admins,
  threshold,
  adminsLen,
  onBack,
}: Props) {
  const accounts = useMemo(
    () => Array.from(new Set(admins.map((profile) => profile.account_id))),
    [admins],
  );
  const [balanceByAccount, setBalanceByAccount] = useState<Record<string, string | null>>({});

  useEffect(() => {
    if (accounts.length === 0) {
      setBalanceByAccount({});
      return;
    }
    let cancelled = false;
    // 清算行管理员卡片没有机构详情余额字段,这里统一补 finalized 链上余额。
    adminsChangeApi.getAccountBalances(accounts)
      .then((balances) => {
        if (!cancelled) setBalanceByAccount(balances);
      })
      .catch(() => {
        if (!cancelled) setBalanceByAccount({});
      });
    return () => {
      cancelled = true;
    };
  }, [accounts.join('|')]);

  return (
    <>
      <button className="back-button" onClick={onBack}>← 返回</button>
      <div className="admin-list-header">
        <h2>管理员列表（{admins.length} 人,阈值 {threshold}/{adminsLen}）</h2>
        <code className="admin-card-address">{cidNumber}</code>
      </div>

      {admins.length === 0 ? (
        <p className="no-data">机构尚未配置管理员</p>
      ) : (
        <div className="admin-grid">
          {admins.map((profile, idx) => (
            <InstitutionAssignmentCard
              key={profile.account_id}
              admin={profile}
              index={idx + 1}
              balanceFen={balanceByAccount[profile.account_id] ?? null}
            />
          ))}
        </div>
      )}
    </>
  );
}

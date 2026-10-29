import { accountIdToSs58 } from '../shared/ss58';
import { formatBalance } from '../shared/format';
// 其他账户列表子页(机构详情下的折叠卡片入口)。
//
// 显示主账户/费用账户之外的所有自定义账户:账户名 / SS58 地址 / 链上余额。
// 与管理员列表的展示风格保持一致。

import type { AccountWithBalance } from './types';

type Props = {
  cidNumber: string;
  otherAccounts: AccountWithBalance[];
  onBack: () => void;
};

export function OtherAccountsListPage({ cidNumber, otherAccounts, onBack }: Props) {
  return (
    <>
      <button className="back-button" onClick={onBack}>← 返回</button>
      <div className="admin-list-header">
        <h2>其他账户列表（{otherAccounts.length} 个）</h2>
        <code className="admin-card-address">{cidNumber}</code>
      </div>

      {otherAccounts.length === 0 ? (
        <p className="no-data">该机构没有其他账户(仅主账户 + 费用账户)</p>
      ) : (
        <div className="admin-grid">
          {otherAccounts.map((acc) => (
            <div key={acc.accountId} className="metric-card admin-card">
              <div>
                <strong>{acc.accountName}</strong>
                {acc.accountKind !== 'named' && (
                  <span className="status-badge status-registered" style={{ marginLeft: 8 }}>
                    协议账户
                  </span>
                )}
              </div>
              <code className="admin-card-address">{accountIdToSs58(acc.accountId)}</code>
              <span className="muted">余额:{formatBalance(acc.balanceFen)}</span>
            </div>
          ))}
        </div>
      )}
    </>
  );
}

// 展示机构其他账户及其公开余额资料。

import { IdentityCidText } from '../client/identityCid';
import { accountIdToSs58 } from "../client/ss58";
import { formatBalance } from "../client/format";
import type { AccountWithBalance } from "./types";


type Props = {
  cidNumber: string;
  otherAccounts: AccountWithBalance[];
  onBack: () => void;
};

export function OtherAccountsListPage({ cidNumber, otherAccounts, onBack }: Props) {
  return (
    <>
      <button className="back-button" onClick={onBack}>← 返回</button>
      <div className="institution-list-header">
        <h2>其他账户列表（{otherAccounts.length} 个）</h2>
        <code className="institution-card-address"><IdentityCidText value={cidNumber} /></code>
      </div>

      {otherAccounts.length === 0 ? (
        <p className="no-data">该机构没有其他账户(仅主账户 + 费用账户)</p>
      ) : (
        <div className="institution-grid">
          {otherAccounts.map((acc) => (
            <div key={acc.accountId} className="metric-card institution-card">
              <div>
                <strong>{acc.accountName}</strong>
                {acc.accountKind !== 'named' && (
                  <span className="status-badge status-registered" style={{ marginLeft: 8 }}>
                    协议账户
                  </span>
                )}
              </div>
              <code className="institution-card-address">{accountIdToSs58(acc.accountId)}</code>
              <span className="muted">余额:{formatBalance(acc.balanceFen)}</span>
            </div>
          ))}
        </div>
      )}
    </>
  );
}

// 首页身份信息展示：节点角色、本机节点地址和当前链创世哈希。
import type { NodeIdentity } from '../types';

type Props = {
  identity: NodeIdentity;
};

export function IdentitySection({ identity }: Props) {
  return (
    <section className="section home-summary-section">
      <h2>身份</h2>
      <dl className="identity-details">
        <div className="identity-detail-row">
          <dt className="identity-detail-key">节点角色：</dt>
          <dd className="identity-detail-value">{identity.role ?? '全节点'}</dd>
        </div>
        <div className="identity-detail-row">
          <dt className="identity-detail-key">节点地址：</dt>
          <dd className="identity-detail-value identity-detail-code">
            {identity.peerId ? `/p2p/${identity.peerId}` : '-'}
          </dd>
        </div>
        <div className="identity-detail-row">
          <dt className="identity-detail-key">创世哈希：</dt>
          <dd className="identity-detail-value identity-detail-code">
            {identity.genesisHash ?? '-'}
          </dd>
        </div>
      </dl>
    </section>
  );
}

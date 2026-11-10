import type { NodeIdentity } from "./types";


type Props = {
  identity: NodeIdentity;
  minerAddress: string | null;
};

export function IdentitySection({ identity, minerAddress }: Props) {
  return (
    <section className="section home-summary-section">
      <h2>身份</h2>
      <dl className="identity-details">
        <div className="identity-detail-row">
          <dt className="identity-detail-key">节点角色：</dt>
          <dd className="identity-detail-value">{identity.role ?? '全节点'}</dd>
        </div>
        <div className="identity-detail-row">
          <dt className="identity-detail-key">矿工账户：</dt>
          <dd className="identity-detail-value identity-detail-code">{minerAddress || '-'}</dd>
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

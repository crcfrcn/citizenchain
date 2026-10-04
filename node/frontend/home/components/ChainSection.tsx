import type { ChainStatus } from '../types';

type Props = {
  chain: ChainStatus;
  nodeRunning: boolean;
};

export function ChainSection({ chain, nodeRunning }: Props) {
  const status = (() => {
    if (!nodeRunning) return '暂停同步';
    if (chain.syncing === true) return '同步中';
    if (chain.syncing === false) {
      if (chain.blockHeight != null && chain.finalizedHeight != null) {
        return chain.blockHeight - chain.finalizedHeight <= 1 ? '已同步' : '同步中';
      }
      return '已同步';
    }
    if (chain.blockHeight != null && chain.finalizedHeight != null) {
      return chain.blockHeight - chain.finalizedHeight <= 1 ? '已同步' : '同步中';
    }
    return '同步中';
  })();

  return (
    <section className="section home-summary-section">
      <h2>区块</h2>
      <p>当前高度: {chain.blockHeight ?? '-'}</p>
      <p>最终高度: {chain.finalizedHeight ?? '-'}</p>
    </section>
  );
}

import { MetricHelp } from "../desktop/MetricHelp";
import type { ChainStatus } from "./types";


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
      <h2 className="metric-label-with-help">
        <span>区块</span>
        <span className="metric-label">
          <MetricHelp title="公民链说明" dialogClassName="home-metric-help-dialog" items={[
            '公民链采用PoW工作量证明共识出块+GRANDPA最终性验证区块。',
            '公民链采用动态难度的PoW工作量证明共识，出块时间设计为预计6分钟。',
            '公民链采用安全优先的链上交易+效率优先的链下交易双层交易模式设计。',
            '链上由PoW+GRANDPA混合共识保证安全，链下由注册局注册的清算行节点保证效率。',
            '公民链由公民协议、节点程序和链上中国三大部分组成，公民协议即Runtime运行时。',
            '公民链节点程序和公民协议均拒绝空块上链，没有交易的空块将被抛弃。',
            '最终性投票由44个权威节点按严格超过 2/3 的投票固化形成最终区块。',
          ]} />
        </span>
      </h2>
      <p>当前高度: {chain.blockHeight ?? '-'}</p>
      <p>最终高度: {chain.finalizedHeight ?? '-'}</p>
    </section>
  );
}

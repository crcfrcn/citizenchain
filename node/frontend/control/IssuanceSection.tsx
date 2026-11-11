import { formatAmount } from "../client/format";
import { MetricHelp } from "../desktop/MetricHelp";
import type { TotalIssuance, TotalStake, TotalFrozen } from "./types";


type Props = {
  issuance: TotalIssuance;
  stake: TotalStake;
  frozen: TotalFrozen;
};

export function IssuanceSection({ issuance, stake, frozen }: Props) {
  return (
    <section className="section home-summary-section mining-section">
      <h2>发行</h2>
      <div className="issuance-stack-grid">
        <div className="metric-card">
          <div className="metric-label metric-label-with-help">
            <span>全链发行总额</span>
            <MetricHelp title="全链发行总额说明" items={[
              '显示截至最新已最终性确认区块的链上现有公民币的发行总量。',
              '总额包含 43 个省储行的永久质押金额。新增发行会增加总额，销毁会减少总额。',
              '新增发行统一由公民协议「即公民链的Runtime」的发行模组执行。',
              '销毁统一由公民协议「即公民链的Runtime」的决议销毁模块执行。',
              '在国家储委会、省储委会、省储行之间通过联合投票决议发行，各机构内部投票决议销毁。',
            ]} />
          </div>
          <div className="metric-value">
            {issuance.totalIssuance ? (
              <>
                {formatAmount(issuance.totalIssuance)}元
                <span className="metric-value-currency">（公民币）</span>
              </>
            ) : (
              '-'
            )}
          </div>
        </div>
        <div className="metric-card">
          <div className="metric-label metric-label-with-help">
            <span>永久质押金额</span>
            <MetricHelp title="永久质押金额说明" items={[
              '显示 43 个创世成立的省储行永久质押账户中的余额合计。',
              '这笔资金在公民链创世时发行，分别存入各省储行的永久质押地址，不可转出。',
              '各省储行的永久质押地址可接收公民币，并由公民协议和节点守卫同时限制转出。',
              '各省储行作为权益节点，其永久质押的公民币为权益节点合法性的依据。',
            ]} />
          </div>
          <div className="metric-value">
            {stake.totalStake ? (
              <>
                {formatAmount(stake.totalStake)}元
                <span className="metric-value-currency">（公民币）</span>
              </>
            ) : (
              '-'
            )}
          </div>
        </div>
        <div className="metric-card">
          <div className="metric-label metric-label-with-help">
            <span>临时冻结金额</span>
            <MetricHelp title="全链冻结金额说明" items={[
              '所有市注册局的「主账户」达标前只进不出，达标前冻结市注册局主账户。',
              <>
                市注册局达标规则如下：
                <table aria-label="市注册局达标规则">
                  <thead><tr><th scope="col">全国达标顺序</th><th scope="col">该市注册局累计注册公民数</th></tr></thead>
                  <tbody>
                    <tr><td>第 1～100 个</td><td>≥ 1,001 人</td></tr>
                    <tr><td>第 101～1,000 个</td><td>≥ 10,001 人</td></tr>
                    <tr><td>第 1,001 个起</td><td>≥ 100,001 人</td></tr>
                  </tbody>
                </table>
              </>,
              '市注册局的注册公民人数达标后，主账户冻结资金自动解冻。',
            ]} />
          </div>
          <div className="metric-value">
            {frozen.totalFrozen != null ? (
              <>
                {formatAmount(frozen.totalFrozen)}元
                <span className="metric-value-currency">（公民币）</span>
              </>
            ) : '-'}
          </div>
        </div>
      </div>
    </section>
  );
}

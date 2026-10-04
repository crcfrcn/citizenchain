import { formatAmount } from '../../shared/format';
import type { TotalIssuance, TotalStake } from '../types';

type Props = {
  issuance: TotalIssuance;
  stake: TotalStake;
};

export function IssuanceSection({ issuance, stake }: Props) {
  return (
    <section className="section home-summary-section mining-section">
      <h2>发行</h2>
      <div className="issuance-stack-grid">
        <div className="metric-card">
          <div className="metric-label">全链发行总额 <span className="metric-hint">（含永久质押金额）</span></div>
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
          <div className="metric-label">永久质押金额 <span className="metric-hint">（成立43个省储行的创立发行总额，永久质押于各省储行质押地址）</span></div>
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
      </div>
    </section>
  );
}

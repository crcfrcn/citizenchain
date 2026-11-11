import { sanitizeError } from "../client/tauri";
import { MetricHelp } from "../desktop/MetricHelp";
import { miningApi as api } from "./api";
import type { MiningDashboard } from "./types";
import { NetworkInlineSection } from "./NetworkInlineSection";
import { useCallback, useEffect, useRef, useState } from 'react';

const FEE_HELP = [
  '本机矿工出块后，可获得该区块的链上交易手续费 80% 的收益。',
  '分成发到出块时已绑定的「矿工收款账户」，未绑定的，矿工份额直接销毁。',
  '手续费累计统计本机矿工产出且已获最终确认的区块。',
  '链上交易手续费为交易金额的0.1%，单笔最低0.1元，不足0.1元的以0.1元计算，由付款方支付。',
];

const REWARD_HELP = [
  '在区块高度 1 至 9,999,999（含）区间，每成功铸造一个区块，链发行 9,999.00 元公民币。',
  '铸块发行的公民币全部用于奖励铸块的矿工，超过上述高度区间后永久停止铸块发行。',
  '已绑定「矿工收款账户」的，奖励发到该账户；未绑定的，发到矿工自身账户。',
  '铸块奖励累计统计本机矿工产出且已获最终确认的区块，更换矿工的重新统计。',
];

function formatIncomeDisplay(raw: string): string {
  const normalized = raw.replace(/,/g, '').trim();
  const amount = Number(normalized);
  if (!Number.isFinite(amount)) {
    return raw;
  }
  return amount.toLocaleString('en-US', {
    minimumFractionDigits: 2,
    maximumFractionDigits: 2,
  });
}

export function MiningDashboardSection() {
  const [mining, setMining] = useState<MiningDashboard>({
    income: {
      totalIncome: '0.00',
      totalFeeIncome: '0.00',
      totalRewardIncome: '0.00',
      todayIncome: '0.00',
    },
    records: [],
    warning: null,
  });
  const [error, setError] = useState<string | null>(null);
  const [loading, setLoading] = useState<boolean>(true);
  const mountedRef = useRef<boolean>(true);
  const requestIdRef = useRef<number>(0);

  const loadMining = useCallback(async () => {
    const requestId = requestIdRef.current + 1;
    requestIdRef.current = requestId;
    try {
      const data = await api.getMiningDashboard();
      if (!mountedRef.current || requestId !== requestIdRef.current) {
        return;
      }
      setMining(data);
      setError(null);
    } catch (e) {
      if (!mountedRef.current || requestId !== requestIdRef.current) {
        return;
      }
      setError(sanitizeError(e));
    } finally {
      if (mountedRef.current && requestId === requestIdRef.current) {
        setLoading(false);
      }
    }
  }, []);

  useEffect(() => {
    mountedRef.current = true;
    void loadMining();
    const timer = globalThis.setInterval(() => {
      void loadMining();
    }, 10000);
    return () => {
      mountedRef.current = false;
      globalThis.clearInterval(timer);
    };
  }, [loadMining]);

  const incomeCards = [
    { label: '累计收益额', amount: mining.income.totalIncome },
    { label: '手续费收益', amount: mining.income.totalFeeIncome, help: FEE_HELP },
    { label: '铸块奖励', amount: mining.income.totalRewardIncome, help: REWARD_HELP },
    { label: '今日收益', amount: mining.income.todayIncome },
  ];

  return (
    <>
      <NetworkInlineSection />

      <section className="section mining-section">
        <h2>挖矿收益</h2>
        <div className="mining-income-grid">
          {incomeCards.map(({ label, amount, help }) => (
            <div key={label} className="metric-card">
              <div className="metric-label metric-label-with-help">
                <span>{label}</span>
                {help && <MetricHelp title={`${label}说明`} items={help} />}
              </div>
              <div className="metric-value">
                {loading ? '加载中...' : `${formatIncomeDisplay(amount)}元`}
              </div>
            </div>
          ))}
        </div>
      </section>

      <section className="section mining-records-section">
        <h2>出块记录</h2>
        <div className="table-wrap">
          <table className="mining-table">
            <thead>
              <tr>
                <th>区块高度</th>
                <th>时间</th>
                <th>手续费收益</th>
                <th>铸块奖励</th>
                <th>区块作者</th>
              </tr>
            </thead>
            <tbody>
              {mining.records.length === 0 ? (
                <tr>
                  <td colSpan={5} className="empty-cell">{loading ? '加载中...' : '暂无数据'}</td>
                </tr>
              ) : (
                mining.records.map((row) => (
                  <tr key={row.blockHeight}>
                    <td>{row.blockHeight}</td>
                    <td>{row.timestampMs ? new Date(row.timestampMs).toLocaleString() : '未知'}</td>
                    <td>{row.fee}</td>
                    <td>{row.blockReward}</td>
                    <td>{row.author}</td>
                  </tr>
                ))
              )}
            </tbody>
          </table>
        </div>
      </section>
      {mining.warning ? <pre className="error">{mining.warning}</pre> : null}
      {error ? <pre className="error">{error}</pre> : null}
    </>
  );
}

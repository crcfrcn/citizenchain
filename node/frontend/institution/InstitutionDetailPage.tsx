import { councilApi } from './council';
import './styles.css';
import { proposalApi } from '../proposal/api';
import { sanitizeError } from "../client/tauri";
import { formatBalance } from "../client/format";
import { accountIdToSs58 } from "../client/ss58";
import { adminsChangeApi } from "./admins";
import type { ActivatedAdmin, AdminSignerMatch, InstitutionBalanceUpdate, InstitutionDetail } from "./council";
import type { ProposalListItem } from "../proposal/types";
// 机构详情页：机构信息 → 管理员入口 → 发起提案 → 提案列表（分页）。
// 管理员列表折叠到独立的 AdminListPage，点击入口卡片右箭头进入。
import { useEffect, useState, useCallback } from 'react';

type Props = {
  cidNumber: string;
  onBack: () => void;
  onOpenAdminList?: (cidNumber: string, orgType: number) => void;
  onSelectProposal?: (proposalId: number, adminSigners: AdminSignerMatch[], cidNumber: string) => void;
  onCreateProposal?: (cidNumber: string, orgType: number, cidFullName: string, institutionAccountId: string, adminSigners: AdminSignerMatch[]) => void;
  onCreateProtocolUpgrade?: (adminSigners: AdminSignerMatch[]) => void;
  onChangeGrandpaKey?: (adminSigners: AdminSignerMatch[]) => void;
  onCreateSafetyFund?: (cidNumber: string, institutionAccountId: string, adminSigners: AdminSignerMatch[]) => void;
  onCreateSweep?: (cidNumber: string, institutionAccountId: string, cidFullName: string, adminSigners: AdminSignerMatch[]) => void;
  /** 隐藏返回按钮（用于直接作为 Tab 内容显示时）。 */
  hideBackButton?: boolean;
};

export function InstitutionDetailPage({ cidNumber, onBack, onOpenAdminList, onSelectProposal, onCreateProposal, onCreateProtocolUpgrade, onChangeGrandpaKey, onCreateSafetyFund, onCreateSweep, hideBackButton }: Props) {
  const [detail, setDetail] = useState<InstitutionDetail | null>(null);
  const [proposals, setProposals] = useState<ProposalListItem[]>([]);
  const [proposalHasMore, setProposalHasMore] = useState(false);
  const [proposalNextStartId, setProposalNextStartId] = useState<number | null>(null);
  const [loadingMoreProposals, setLoadingMoreProposals] = useState(false);
  const [activatedAdmins, setActivatedAdmins] = useState<ActivatedAdmin[]>([]);
  const [error, setError] = useState<string | null>(null);
  const [loading, setLoading] = useState(true);

  const PROPOSAL_PAGE_SIZE = 10;
  const isAdmin = activatedAdmins.length > 0;

  // 将已激活管理员转换为 AdminSignerMatch 格式,账户标签不复用机构名称字段。
  const adminSigners: AdminSignerMatch[] = activatedAdmins.map(a => ({

    accountId: a.accountId,
    accountLabel: '',
  }));
  // GRANDPA 换钥必须同时满足本机构 CID、委员岗位码和管理员账户；前端只展示
  // 当前机构委员任职账户，Runtime 仍执行同一授权的最终校验。
  const committeeAccountIds = new Set(
    (detail?.admins ?? [])
      .filter((admin) => admin.assignments.some((assignment) => assignment.roleCode === 'COMMITTEE_MEMBER'))
      .map((admin) => admin.accountId),
  );
  const grandpaAdminSigners = adminSigners.filter((admin) => committeeAccountIds.has(admin.accountId));

  useEffect(() => {
    setLoading(true);
    councilApi.getInstitutionDetail(cidNumber)
      .then(async (d) => {
        const aa = await adminsChangeApi
          .getActivatedAdmins(cidNumber, { cidNumber })
          .catch(() => [] as ActivatedAdmin[]);
        setDetail(d);
        setActivatedAdmins(aa);
        try {
          // 双层 ID v1:不再需要 getNextProposalId 找起点 — 反向索引内部按 startId 过滤,
          // 用 Number.MAX_SAFE_INTEGER 作首页起点等价于"从最新一条开始取"。
          const page = await proposalApi.getInstitutionProposalPage(
            cidNumber,
            Number.MAX_SAFE_INTEGER,
            PROPOSAL_PAGE_SIZE,
          );
          setProposals(page.items);
          setProposalHasMore(page.hasMore);
          if (page.items.length > 0) {
            const lastId = page.items[page.items.length - 1].proposalId;
            setProposalNextStartId(lastId > 0 ? lastId - 1 : null);
          }
        } catch (_) {
          setProposals([]);
        }
        setError(null);
      })
      .catch((e) => setError(sanitizeError(e)))
      .finally(() => setLoading(false));
  }, [cidNumber]);

  // 只刷新链上金额和告警，不改现有页面结构。
  useEffect(() => {
    let cancelled = false;
    let unlisten: (() => void) | undefined;

    (async () => {
      try {
        const { listen } = await import('@tauri-apps/api/event');
        unlisten = await listen<InstitutionBalanceUpdate>(
          'governance-balance-updated',
          (event) => {
            if (cancelled || event.payload.cidNumber !== cidNumber) return;
            setDetail((prev) => (prev ? { ...prev, ...event.payload } : prev));
          },
        );
        await councilApi.startGovernanceBalanceWatch(cidNumber);
      } catch {
        // 监听不可用时静默降级为详情页初次加载结果。
      }
    })();

    return () => {
      cancelled = true;
      unlisten?.();
      councilApi.stopGovernanceBalanceWatch(cidNumber).catch(() => undefined);
    };
  }, [cidNumber]);

  // 加载更多提案
  const loadMoreProposals = useCallback(() => {
    if (loadingMoreProposals || proposalNextStartId == null || !proposalHasMore) return;
    setLoadingMoreProposals(true);
    proposalApi.getInstitutionProposalPage(cidNumber, proposalNextStartId, PROPOSAL_PAGE_SIZE)
      .then((page) => {
        setProposals(prev => [...prev, ...page.items]);
        setProposalHasMore(page.hasMore);
        if (page.items.length > 0) {
          const lastId = page.items[page.items.length - 1].proposalId;
          setProposalNextStartId(lastId > 0 ? lastId - 1 : null);
        } else {
          setProposalHasMore(false);
        }
      })
      .catch(() => setProposalHasMore(false))
      .finally(() => setLoadingMoreProposals(false));
  }, [cidNumber, loadingMoreProposals, proposalNextStartId, proposalHasMore]);

  if (loading) {
    return <div className="governance-section"><p>加载中…</p></div>;
  }

  if (error) {
    return (
      <div className="governance-section">
        <button className="back-button" onClick={onBack}>{hideBackButton ? '← 返回' : '← 返回机构列表'}</button>
        <div className="error">{error}</div>
      </div>
    );
  }

  if (!detail) return null;

  const activatedCount = activatedAdmins.length;
  const feeAccountId = detail.feeAccountId ?? detail.cbAccountId ?? detail.nrcAccountId;

  const proposalActions = (
    <div className="proposal-type-grid">
      <button
        className="proposal-type-button"
        disabled={!isAdmin}
        onClick={() => isAdmin && onCreateProposal?.(
          cidNumber, detail.orgType, detail.cidFullName, detail.mainAccountId, adminSigners
        )}
      >转账</button>
      <button className="proposal-type-button" disabled title="即将上线">决议销毁</button>
      {(detail.orgType === 0 || detail.orgType === 2) && (
        <button
          className="proposal-type-button"
          disabled={!isAdmin || !feeAccountId}
          onClick={() => isAdmin && feeAccountId && onCreateSweep?.(cidNumber, feeAccountId, detail.cidFullName, adminSigners)}
        >手续费划转</button>
      )}
      {detail.orgType === 0 && (
        <button
          className="proposal-type-button"
          disabled={!isAdmin || !detail.fundAccountId}
          onClick={() => isAdmin && detail.fundAccountId && onCreateSafetyFund?.(cidNumber, detail.fundAccountId, adminSigners)}
        >安全基金转账</button>
      )}
      {(detail.orgType === 0 || detail.orgType === 1) && (
        <>
          <button className="proposal-type-button" disabled title="即将上线">决议发行</button>
          <button
            className="proposal-type-button"
            disabled={grandpaAdminSigners.length === 0 || !onChangeGrandpaKey}
            onClick={() => grandpaAdminSigners.length > 0 && onChangeGrandpaKey?.(grandpaAdminSigners)}
          >验证密钥</button>
          <button
            className="proposal-type-button"
            disabled={!isAdmin || !onCreateProtocolUpgrade}
            onClick={() => isAdmin && onCreateProtocolUpgrade?.(adminSigners)}
          >协议升级</button>
        </>
      )}
    </div>
  );
  const proposalTitle = <h3>提案列表{proposals.length > 0 ? `（${proposals.length}${proposalHasMore ? '+' : ''}）` : ''}</h3>;
  const proposalList = (
    <>
      {proposals.length === 0 ? (
        <p className="no-data">暂无提案</p>
      ) : (
        <div className="proposal-list">
          {proposals.map((item) => (
            <div
              key={item.proposalId}
              className="proposal-card clickable"
              onClick={() => {
                onSelectProposal?.(item.proposalId, adminSigners, cidNumber);
              }}
            >
              <div className="proposal-card-header">
                <span className="proposal-id">{item.displayId}</span>
                <div className="proposal-card-header-right">
                  {isAdmin && item.status === 0 && (
                    <span className="vote-entry-badge">可投票</span>
                  )}
                  <span className={`proposal-status status-${item.status}`}>
                    {item.statusLabel}
                  </span>
                </div>
              </div>
              <div className="proposal-card-body">
                <div className="proposal-card-tags">
                  <span className="proposal-tag">{item.kindLabel}</span>
                  {item.kind === 1 && (
                    <span className="proposal-tag">{item.stageLabel}</span>
                  )}
                </div>
                <div className="proposal-summary">{item.summary}</div>
              </div>
            </div>
          ))}
        </div>
      )}
      {proposalHasMore && (
        <button
          className="load-more-button"
          onClick={loadMoreProposals}
          disabled={loadingMoreProposals}
        >
          {loadingMoreProposals ? '加载中…' : '加载更多'}
        </button>
      )}
    </>
  );

  const accounts: { name: string; accountId: string | null; balanceFen: string | null }[] = [
    { name: '主账户', accountId: detail.mainAccountId, balanceFen: detail.balanceFen },
  ];
  if (detail.orgType === 0) {
    accounts.push(
      { name: '费用账户', accountId: detail.nrcAccountId, balanceFen: detail.nrcFeeFen },
      { name: '安全基金账户', accountId: detail.fundAccountId, balanceFen: detail.fundBalanceFen },
      { name: '两和基金账户', accountId: detail.heAccountId, balanceFen: detail.heBalanceFen },
    );
  } else if (detail.orgType === 1) {
    accounts.push({ name: '费用账户', accountId: detail.cbAccountId, balanceFen: detail.cbFeeFen });
  } else if (detail.orgType === 2) {
    accounts.push(
      { name: '永久质押账户', accountId: detail.stakeAccountId, balanceFen: detail.stakingBalanceFen },
      { name: '费用账户', accountId: detail.feeAccountId, balanceFen: detail.feeBalanceFen },
    );
  }

  return (
    <div className="governance-section institution-overview">
      {!hideBackButton && (
        <button type="button" className="back-button" onClick={onBack}>← 返回机构列表</button>
      )}
      <header className="institution-overview-header">
        <div className="institution-overview-heading">
          <h2>
            {detail.cidFullName}
            <span className="institution-overview-voting-rules">
              （ 内部投票阈值 <strong>{detail.internalThreshold}票</strong>
              {' ｜ 联合投票权重 '}<strong>{detail.jointVoteWeight}票</strong> ）
            </span>
          </h2>
          <button
            type="button"
            className="institution-overview-admin-button"
            onClick={() => onOpenAdminList?.(cidNumber, detail.orgType)}
          >
            管理员（{detail.admins.length}人）
            <span aria-hidden="true">›</span>
          </button>
        </div>
        <p className="institution-overview-cid">
          机构CID号 <code>{detail.cidNumber}</code>
          {activatedCount > 0 && <span className="admin-badge">已激活 {activatedCount} 人</span>}
        </p>
      </header>

      {detail.warning && <div className="warning">{detail.warning}</div>}

      <section className="institution-overview-accounts" aria-labelledby="institution-overview-accounts-title">
        <h3 id="institution-overview-accounts-title">账户列表</h3>
        <table className="institution-overview-account-table">
          <thead>
            <tr>
              <th scope="col">账户名称</th>
              <th scope="col">账户地址</th>
              <th scope="col">链上余额（元）</th>
            </tr>
          </thead>
          <tbody>
            {accounts.map(({ name, accountId, balanceFen }) => (
              <tr key={name}>
                <th scope="row">{name}</th>
                <td><code>{accountId ? accountIdToSs58(accountId) : '—'}</code></td>
                <td>{balanceFen == null ? '—' : formatBalance(balanceFen).replace(/ 元$/, '')}</td>
              </tr>
            ))}
          </tbody>
        </table>
      </section>

      <section className="institution-overview-proposals">
        <div className="institution-overview-proposal-toolbar">
          {proposalTitle}
          {proposalActions}
        </div>
        {proposalList}
      </section>
    </div>
  );
}

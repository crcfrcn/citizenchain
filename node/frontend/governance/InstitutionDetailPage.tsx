// 机构详情页：机构信息 → 管理员入口 → 发起提案 → 提案列表（分页）。
// 管理员列表折叠到独立的 AdminListPage，点击入口卡片右箭头进入。
import { useEffect, useState, useCallback } from 'react';
import { sanitizeError } from '../tauri';
import { formatBalance } from '../shared/format';
import { accountIdToSs58 } from '../shared/ss58';
import { adminsChangeApi } from '../admins/api';
import { governanceApi as api } from './api';
import type {
  ActivatedAdmin,
  AdminSignerMatch,
  InstitutionBalanceUpdate,
  InstitutionDetail,
  ProposalListItem,
} from './types';

type Props = {
  cidNumber: string;
  onBack: () => void;
  onOpenAdminList?: (cidNumber: string, orgType: number) => void;
  onSelectProposal?: (proposalId: number, adminSigners: AdminSignerMatch[], cidNumber: string) => void;
  onCreateProposal?: (cidNumber: string, orgType: number, cidFullName: string, institution_account_id: string, adminSigners: AdminSignerMatch[]) => void;
  onCreateProtocolUpgrade?: (adminSigners: AdminSignerMatch[]) => void;
  onChangeGrandpaKey?: (adminSigners: AdminSignerMatch[]) => void;
  onCreateSafetyFund?: (cidNumber: string, institution_account_id: string, adminSigners: AdminSignerMatch[]) => void;
  onCreateSweep?: (cidNumber: string, institution_account_id: string, cidFullName: string, adminSigners: AdminSignerMatch[]) => void;
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
    ss58_address: accountIdToSs58(a.account_id),
    account_id: a.account_id,
    account_label: '',
  }));
  // GRANDPA 换钥必须同时满足本机构 CID、委员岗位码和管理员账户；前端只展示
  // 当前机构委员任职账户，Runtime 仍执行同一授权的最终校验。
  const committeeAccountIds = new Set(
    (detail?.admins ?? [])
      .filter((admin) => admin.assignments.some((assignment) => assignment.roleCode === 'COMMITTEE_MEMBER'))
      .map((admin) => admin.account_id),
  );
  const grandpaAdminSigners = adminSigners.filter((admin) => committeeAccountIds.has(admin.account_id));

  useEffect(() => {
    setLoading(true);
    api.getInstitutionDetail(cidNumber)
      .then(async (d) => {
        const aa = await adminsChangeApi
          .getActivatedAdmins(cidNumber, { cidNumber })
          .catch(() => [] as ActivatedAdmin[]);
        setDetail(d);
        setActivatedAdmins(aa);
        try {
          // 双层 ID v1:不再需要 getNextProposalId 找起点 — 反向索引内部按 startId 过滤,
          // 用 Number.MAX_SAFE_INTEGER 作首页起点等价于"从最新一条开始取"。
          const page = await api.getInstitutionProposalPage(
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
        await api.startGovernanceBalanceWatch(cidNumber);
      } catch {
        // 监听不可用时静默降级为详情页初次加载结果。
      }
    })();

    return () => {
      cancelled = true;
      unlisten?.();
      api.stopGovernanceBalanceWatch(cidNumber).catch(() => undefined);
    };
  }, [cidNumber]);

  // 加载更多提案
  const loadMoreProposals = useCallback(() => {
    if (loadingMoreProposals || proposalNextStartId == null || !proposalHasMore) return;
    setLoadingMoreProposals(true);
    api.getInstitutionProposalPage(cidNumber, proposalNextStartId, PROPOSAL_PAGE_SIZE)
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
        <button className="back-button" onClick={onBack}>← 返回</button>
        <div className="error">{error}</div>
      </div>
    );
  }

  if (!detail) return null;

  const activatedCount = activatedAdmins.length;
  const fee_account_id = detail.fee_account_id ?? detail.cb_fee_account_id ?? detail.nrc_fee_account_id;

  return (
    <div className="governance-section">
      {!hideBackButton && (
        <button className="back-button" onClick={onBack}>← 返回机构列表</button>
      )}

      <div className="institution-title-row">
        <h2>{detail.cidFullName}</h2>
        {isAdmin && (
          <span className="admin-badge">管理员</span>
        )}
      </div>

      {detail.warning && <div className="warning">{detail.warning}</div>}

      {/* 机构信息卡片 */}
      <div className="institution-detail-grid">
        <div className="metric-card">
          <div className="metric-label">机构类型 /身份CID号 <code className="metric-label-id">{detail.cidNumber}</code></div>
          <div className="metric-value">{detail.orgTypeLabel}</div>
        </div>
        <div className="metric-card">
          <div className="metric-label">主账户 <code className="metric-label-id">{accountIdToSs58(detail.main_account_id)}</code></div>
          <div className="metric-value">
            {detail.balanceFen != null
              ? formatBalance(detail.balanceFen)
              : '—'}
          </div>
        </div>
        <div className="metric-card">
          <div className="metric-label">内部投票阈值</div>
          <div className="metric-value">{detail.internalThreshold} 票</div>
        </div>
        <div className="metric-card">
          <div className="metric-label">联合投票权重</div>
          <div className="metric-value">{detail.jointVoteWeight} 票</div>
        </div>
        {detail.orgType === 2 && detail.stake_account_id && (
          <div className="metric-card">
            <div className="metric-label">永久质押账户 <code className="metric-label-id">{accountIdToSs58(detail.stake_account_id)}</code></div>
            <div className="metric-value">
              {detail.stakingBalanceFen != null
                ? formatBalance(detail.stakingBalanceFen)
                : '—'}
            </div>
          </div>
        )}
        {detail.orgType === 2 && detail.fee_account_id && (
          <div className="metric-card">
            <div className="metric-label">费用账户 <code className="metric-label-id">{accountIdToSs58(detail.fee_account_id)}</code></div>
            <div className="metric-value">
              {detail.feeBalanceFen != null
                ? formatBalance(detail.feeBalanceFen)
                : '—'}
            </div>
          </div>
        )}
        {detail.orgType === 1 && detail.cb_fee_account_id && (
          <div className="metric-card">
            <div className="metric-label">费用账户 <code className="metric-label-id">{accountIdToSs58(detail.cb_fee_account_id)}</code></div>
            <div className="metric-value">
              {detail.cbFeeBalanceFen != null
                ? formatBalance(detail.cbFeeBalanceFen)
                : '—'}
            </div>
          </div>
        )}
        {detail.nrc_fee_account_id && (
          <div className="metric-card">
            <div className="metric-label">费用账户 <code className="metric-label-id">{accountIdToSs58(detail.nrc_fee_account_id)}</code></div>
            <div className="metric-value">
              {detail.nrcFeeBalanceFen != null
                ? formatBalance(detail.nrcFeeBalanceFen)
                : '—'}
            </div>
          </div>
        )}
        {detail.safety_fund_account_id && (
          <div className="metric-card">
            <div className="metric-label">安全基金账户 <code className="metric-label-id">{accountIdToSs58(detail.safety_fund_account_id)}</code></div>
            <div className="metric-value">
              {detail.safetyFundBalanceFen != null
                ? formatBalance(detail.safetyFundBalanceFen)
                : '—'}
            </div>
          </div>
        )}
      </div>

      {/* 管理员入口卡片（折叠，点击进入管理员列表页） */}
      <div className="institution-info-section">
        <div
          className="metric-card admin-entry-card"
          onClick={() => detail && onOpenAdminList?.(cidNumber, detail.orgType)}
          role="button"
          tabIndex={0}
          onKeyDown={(e) => e.key === 'Enter' && detail && onOpenAdminList?.(cidNumber, detail.orgType)}
        >
          <div className="admin-entry-left">
            <div className="admin-entry-title">管理员列表（{detail.admins.length} 人）</div>
            {activatedCount > 0 && (
              <div className="admin-entry-activated">已激活 {activatedCount} 人</div>
            )}
          </div>
          <span className="admin-entry-arrow">→</span>
        </div>
      </div>

      {/* 提案类型入口（始终显示，未激活时灰色禁用） */}
      <div className="institution-info-section">
        <h3>发起提案</h3>
        <div className="proposal-type-grid">
          <button
            className="proposal-type-button"
            disabled={!isAdmin}
            onClick={() => isAdmin && onCreateProposal?.(
              cidNumber, detail.orgType, detail.cidFullName, detail.main_account_id, adminSigners
            )}
          >转账</button>
          <button className="proposal-type-button" disabled title="即将上线">决议销毁</button>
          {(detail.orgType === 0 || detail.orgType === 2) && (
            <button
              className="proposal-type-button"
              disabled={!isAdmin || !fee_account_id}
              onClick={() => isAdmin && fee_account_id && onCreateSweep?.(cidNumber, fee_account_id, detail.cidFullName, adminSigners)}
            >手续费划转</button>
          )}
          {detail.orgType === 0 && (
            <button
              className="proposal-type-button"
              disabled={!isAdmin || !detail.safety_fund_account_id}
              onClick={() => isAdmin && detail.safety_fund_account_id && onCreateSafetyFund?.(cidNumber, detail.safety_fund_account_id, adminSigners)}
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
        {!isAdmin && (
          <p className="no-data">激活管理员后可操作提案功能</p>
        )}
      </div>

      {/* 提案列表（分页） */}
      <div className="institution-info-section">
        <h3>提案列表{proposals.length > 0 ? `（${proposals.length}${proposalHasMore ? '+' : ''}）` : ''}</h3>
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
      </div>
    </div>
  );
}

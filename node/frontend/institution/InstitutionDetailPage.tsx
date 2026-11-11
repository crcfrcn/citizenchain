import { councilApi } from './council';
import './styles.css';
import { proposalApi } from '../proposal/api';
import { sanitizeError } from "../client/tauri";
import { formatBalance } from "../client/format";
import { accountIdToSs58 } from "../client/ss58";
import { adminsChangeApi } from "./admins";
import { CreateMultisigTransferPage } from "../multisig/CreateProposalPage";
import { SafetyFundProposalPage } from "../multisig/SafetyFundProposalPage";
import { SweepProposalPage } from "../multisig/SweepProposalPage";
import { GrandpaKeyChangePage } from "../grandpakey/GrandpaKeyChangePage";
import { ProtocolUpgradeProposalPage } from "../upgrade/index";
import type { ActivatedAdmin, AdminSignerMatch, InstitutionBalanceUpdate, InstitutionDetail } from "./council";
import type { ProposalListItem } from "../proposal/types";
// 机构详情页：机构信息 → 管理员入口 → 发起提案 → 提案列表（分页）。
// 管理员列表折叠到独立的 AdminListPage，点击入口卡片右箭头进入。
import { useEffect, useState, useCallback, useRef, type MouseEvent } from 'react';
import { createPortal } from 'react-dom';

type Props = {
  cidNumber: string;
  onBack: () => void;
  onOpenAdminList?: (cidNumber: string, orgType: number) => void;
  onSelectProposal?: (proposalId: number, adminSigners: AdminSignerMatch[], cidNumber: string) => void;
  /** 隐藏返回按钮（用于直接作为 Tab 内容显示时）。 */
  hideBackButton?: boolean;
};

type TransferKind = 'transfer' | 'he' | 'safety' | 'sweep';
type ActiveProposal =
  | { kind: TransferKind; accountId: string; signers: AdminSignerMatch[] }
  | { kind: 'grandpa-key' | 'protocol-upgrade'; signers: AdminSignerMatch[] };

const PROPOSAL_PAGE_SIZE = 10;

function VotingHelpDialog({ onClose }: { onClose: () => void }) {
  const panelRef = useRef<HTMLDivElement>(null);
  useEffect(() => {
    panelRef.current?.focus();
  }, []);

  return createPortal(
    <div
      className="institution-vote-help-overlay"
      onClick={(event) => {
        if (event.target === event.currentTarget) onClose();
      }}
    >
      <div
        ref={panelRef}
        className="institution-vote-help-dialog"
        role="dialog"
        aria-modal="true"
        aria-labelledby="institution-vote-help-title"
        tabIndex={-1}
        onKeyDown={(event) => {
          if (event.key === 'Escape') {
            event.preventDefault();
            onClose();
          } else if (event.key === 'Tab') {
            // 说明弹窗没有其他可聚焦元素，保持焦点在正文区域。
            event.preventDefault();
          }
        }}
      >
        <h2 id="institution-vote-help-title">投票说明</h2>
        <p className="institution-vote-help-intro">0.公民链投票引擎由内部投票、联合投票、立法投票和选举投票共4个模块组成。</p>
        <section>
          <h3>一、内部投票</h3>
          <ol>
            <li>投票引擎的内部投票模块组织本机构管理员投票，投票资格以提案快照为准。</li>
            <li>省储委会和省储行每机构共 9 名管理员，赞成票达到 6 票即通过。</li>
            <li>国家储委会共 19 名管理员，赞成票达到 13 票即通过。</li>
            <li>即使剩余选民全部赞成也达不到阈值，或满 30 天仍未通过，提案即否决。</li>
          </ol>
        </section>
        <section>
          <h3>二、联合投票</h3>
          <ol>
            <li>投票引擎的联合投票模块由内部投票阶段和联合公投阶段组成。</li>
            <li>联合投票模块仅适用于全体治理机构，各治理机构在同一联合投票提案内形成结果。</li>
            <li>治理机构包括 1 个国家储委会、43 个省储委会和 43 个省储行。</li>
            <li>国家储委会的票权为 19 票，每个省储委会和省储行各 1 票，共 105 票。</li>
            <li>所有机构内部投票均通过，累计 105 票赞成，联合投票立即通过。</li>
            <li>任一机构否决，或 30 天期满仍未全票通过，则转入联合公投。</li>
          </ol>
        </section>
        <section>
          <h3>三、联合公投</h3>
          <ol>
            <li>投票引擎的联合公投流程由提案人口快照中的合资格链上公民投票。</li>
            <li>赞成票<strong>严格超过提案人口快照中全体合资格选民的 50%</strong>，提案即通过。</li>
            <li>反对票<strong>达到提案人口快照中全体合资格选民的 50%</strong>，提案即否决。</li>
            <li>从进入联合公投阶段起重新计 30 天，期满仍未达到赞成票门槛，提案即否决。</li>
          </ol>
        </section>
      </div>
    </div>,
    document.body,
  );
}

export function InstitutionDetailPage({ cidNumber, onBack, onOpenAdminList, onSelectProposal, hideBackButton }: Props) {
  const [detail, setDetail] = useState<InstitutionDetail | null>(null);
  const [proposals, setProposals] = useState<ProposalListItem[]>([]);
  const [proposalHasMore, setProposalHasMore] = useState(false);
  const [proposalNextStartId, setProposalNextStartId] = useState<number | null>(null);
  const [loadingMoreProposals, setLoadingMoreProposals] = useState(false);
  const [proposalRefreshError, setProposalRefreshError] = useState<string | null>(null);
  const proposalPageGeneration = useRef(0);
  const [activatedAdmins, setActivatedAdmins] = useState<ActivatedAdmin[]>([]);
  const [error, setError] = useState<string | null>(null);
  const [loading, setLoading] = useState(true);
  const [showVotingHelp, setShowVotingHelp] = useState(false);
  const votingHelpButtonRef = useRef<HTMLButtonElement>(null);
  const [activeProposal, setActiveProposal] = useState<ActiveProposal | null>(null);
  const proposalButtonRef = useRef<HTMLButtonElement | null>(null);
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

  const refreshProposals = useCallback(async () => {
    const generation = ++proposalPageGeneration.current;
    setLoadingMoreProposals(true);
    setProposalRefreshError(null);
    try {
      const page = await proposalApi.getInstitutionProposalPage(cidNumber, Number.MAX_SAFE_INTEGER, PROPOSAL_PAGE_SIZE);
      if (generation !== proposalPageGeneration.current) return;
      setProposals(page.items);
      setProposalHasMore(page.hasMore);
      const lastId = page.items[page.items.length - 1]?.proposalId ?? 0;
      setProposalNextStartId(lastId > 0 ? lastId - 1 : null);
    } catch (e) {
      if (generation === proposalPageGeneration.current) setProposalRefreshError(sanitizeError(e));
    } finally {
      if (generation === proposalPageGeneration.current) setLoadingMoreProposals(false);
    }
  }, [cidNumber]);

  useEffect(() => {
    let cancelled = false;
    setLoading(true);
    setProposals([]);
    setProposalHasMore(false);
    setProposalNextStartId(null);
    setActiveProposal(null);
    councilApi.getInstitutionDetail(cidNumber)
      .then(async (d) => {
        const aa = await adminsChangeApi
          .getActivatedAdmins(cidNumber, { cidNumber })
          .catch(() => [] as ActivatedAdmin[]);
        if (cancelled) return;
        setDetail(d);
        setActivatedAdmins(aa);
        setError(null);
        await refreshProposals();
      })
      .catch((e) => { if (!cancelled) setError(sanitizeError(e)); })
      .finally(() => { if (!cancelled) setLoading(false); });
    return () => {
      cancelled = true;
      proposalPageGeneration.current++;
    };
  }, [cidNumber, refreshProposals]);

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
    const generation = proposalPageGeneration.current;
    setLoadingMoreProposals(true);
    proposalApi.getInstitutionProposalPage(cidNumber, proposalNextStartId, PROPOSAL_PAGE_SIZE)
      .then((page) => {
        if (generation !== proposalPageGeneration.current) return;
        setProposals(prev => [...prev, ...page.items]);
        setProposalHasMore(page.hasMore);
        if (page.items.length > 0) {
          const lastId = page.items[page.items.length - 1].proposalId;
          setProposalNextStartId(lastId > 0 ? lastId - 1 : null);
        } else {
          setProposalHasMore(false);
        }
      })
      .catch(() => { if (generation === proposalPageGeneration.current) setProposalHasMore(false); })
      .finally(() => { if (generation === proposalPageGeneration.current) setLoadingMoreProposals(false); });
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
  const proposerRoleCode = detail.orgType === 2 ? 'DIRECTOR' : 'COMMITTEE_MEMBER';
  const feeAccountId = detail.orgType === 0 ? detail.nrcAccountId
    : detail.orgType === 1 ? detail.cbAccountId : detail.feeAccountId;
  // 省储委会入口随链上岗位权限生效，旧 Runtime 不提供可签名的入口。
  const sweepSigners = detail.orgType === 1
    ? adminSigners.filter(({ accountId }) => detail.admins.some((admin) =>
      admin.accountId === accountId && admin.assignments.some((assignment) =>
        assignment.roleCode === 'COMMITTEE_MEMBER' &&
        'permissions' in assignment && Array.isArray(assignment.permissions) &&
        assignment.permissions.some((permission) =>
          permission.moduleTag === 'multisig' && permission.actionCode === 2 && permission.operation === 0))))
    : adminSigners;

  const openProposal = (event: MouseEvent<HTMLButtonElement>, proposal: ActiveProposal) => {
    if (proposal.signers.length === 0) return;
    proposalButtonRef.current = event.currentTarget;
    setActiveProposal(proposal);
  };
  const openTransfer = (
    event: MouseEvent<HTMLButtonElement>, kind: TransferKind,
    accountId: string | null, signers = adminSigners,
  ) => {
    if (accountId) openProposal(event, { kind, accountId, signers });
  };
  const closeProposal = () => {
    const trigger = proposalButtonRef.current;
    setActiveProposal(null);
    requestAnimationFrame(() => trigger?.focus({ preventScroll: true }));
  };
  const completeProposal = () => {
    closeProposal();
    void refreshProposals();
  };

  const proposalActions = (
    <div className="proposal-type-grid">
      <button
        type="button"
        className="proposal-type-button"
        disabled={!isAdmin}
        aria-haspopup="dialog"
        onClick={(event) => openTransfer(event, 'transfer', detail.mainAccountId)}
      >转账</button>
      <button className="proposal-type-button" disabled title="即将上线">决议销毁</button>
      <button
        type="button"
        className="proposal-type-button"
        disabled={!feeAccountId || sweepSigners.length === 0}
        aria-haspopup="dialog"
        title={detail.orgType === 1 && sweepSigners.length === 0 ? '需已激活且拥有链上划转发起权限的省储委会委员' : undefined}
        onClick={(event) => openTransfer(event, 'sweep', feeAccountId, sweepSigners)}
      >手续费划转</button>
      {detail.orgType === 0 && (
        <>
          <button
            type="button"
            className="proposal-type-button"
            disabled={!isAdmin || !detail.fundAccountId}
            aria-haspopup="dialog"
            onClick={(event) => openTransfer(event, 'safety', detail.fundAccountId)}
          >安全基金转账</button>
          <button
            type="button"
            className="proposal-type-button"
            disabled={!isAdmin || !detail.heAccountId}
            aria-haspopup="dialog"
            onClick={(event) => openTransfer(event, 'he', detail.heAccountId)}
          >两和基金转账</button>
        </>
      )}
      {(detail.orgType === 0 || detail.orgType === 1) && (
        <>
          <button className="proposal-type-button" disabled title="即将上线">决议发行</button>
          <button
            type="button"
            className="proposal-type-button"
            disabled={grandpaAdminSigners.length === 0}
            aria-haspopup="dialog"
            onClick={(event) => openProposal(event, { kind: 'grandpa-key', signers: grandpaAdminSigners })}
          >验证密钥</button>
          <button
            type="button"
            className="proposal-type-button"
            disabled={!isAdmin}
            aria-haspopup="dialog"
            onClick={(event) => openProposal(event, { kind: 'protocol-upgrade', signers: adminSigners })}
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
                <div className="proposal-card-identity">
                  <span className="proposal-id">{item.displayId}</span>
                  <span className="proposal-card-vote-kind">{item.kindLabel}</span>
                </div>
                <span className={`proposal-status status-${item.status}`}>
                  {item.statusLabel}
                </span>
              </div>
              <div className="proposal-card-body">
                <div className="proposal-card-action">
                  <span>{item.card?.actionLabel ?? '未知提案'}</span>
                  {item.card?.amountFen != null && (
                    <span className="proposal-card-amount">{formatBalance(item.card.amountFen)}</span>
                  )}
                </div>
                {item.card?.remark && (
                  <div className="proposal-summary proposal-card-remark">{item.card.remark}</div>
                )}
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
          <div className="institution-overview-proposal-heading">
            {proposalTitle}
            <button
              ref={votingHelpButtonRef}
              type="button"
              className="institution-overview-vote-help-button"
              aria-haspopup="dialog"
              onClick={() => setShowVotingHelp(true)}
            >
              投票说明
            </button>
          </div>
          {proposalActions}
        </div>
        {proposalRefreshError && <p className="error" role="alert">提案列表刷新失败：{proposalRefreshError}</p>}
        {proposalList}
      </section>
      {activeProposal?.kind === 'sweep' && (
        <SweepProposalPage actorCidNumber={cidNumber} cidFullName={detail.cidFullName}
          proposerRoleCode={proposerRoleCode}
          institutionAccountId={activeProposal.accountId} mainAccountId={detail.mainAccountId}
          adminSigners={activeProposal.signers} onBack={closeProposal} onSuccess={completeProposal} />
      )}
      {activeProposal?.kind === 'safety' && (
        <SafetyFundProposalPage actorCidNumber={cidNumber} cidFullName={detail.cidFullName}
          proposerRoleCode={proposerRoleCode}
          institutionAccountId={activeProposal.accountId} adminSigners={activeProposal.signers}
          onBack={closeProposal} onSuccess={completeProposal} />
      )}
      {(activeProposal?.kind === 'transfer' || activeProposal?.kind === 'he') && (
        <CreateMultisigTransferPage cidNumber={cidNumber} cidFullName={detail.cidFullName}
          proposerRoleCode={proposerRoleCode}
          title={activeProposal.kind === 'he' ? '两和基金转账提案' : '发起转账提案'}
          sourceAccountLabel={activeProposal.kind === 'he' ? '两和基金账户' : '主账户'}
          institutionAccountId={activeProposal.accountId} adminSigners={activeProposal.signers}
          onBack={closeProposal} onSuccess={completeProposal} />
      )}
      {activeProposal?.kind === 'grandpa-key' && (
        <GrandpaKeyChangePage actorCidNumber={cidNumber} institutionName={detail.cidFullName}
          adminSigners={activeProposal.signers} onBack={closeProposal} onSuccess={completeProposal} />
      )}
      {activeProposal?.kind === 'protocol-upgrade' && (
        <ProtocolUpgradeProposalPage actorCidNumber={cidNumber} institutionName={detail.cidFullName}
          adminSigners={activeProposal.signers} onBack={closeProposal} onSuccess={completeProposal} />
      )}
      {showVotingHelp && (
        <VotingHelpDialog onClose={() => {
          setShowVotingHelp(false);
          votingHelpButtonRef.current?.focus();
        }} />
      )}
    </div>
  );
}

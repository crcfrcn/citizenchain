// 展示提案详情、阶段和计票，提供当前用户可用的投票入口。

import { proposalApi } from './api';
import { sanitizeError } from "../client/tauri";
import { accountIdToSs58 } from "../client/ss58";
import { MultisigTransferProposalDetailSection } from "../multisig/ProposalDetailSection";
import type { ProposalFullInfo, FrozenVoteBinding } from "./types";
import type { RoleSigner } from "../institution/council";
import { VoteSigningFlow, readPendingFrozenVote, clearPendingFrozenVote } from "./VoteSigningFlow";
// 提案详情页：提案元数据 + 业务详情模块挂载 + 投票进度 + 冻结人员投票状态列表。
// 投票资格来自原提案；现任人员不继承旧票。
import { useEffect, useState, useCallback, useRef } from 'react';
import '../institution/styles.css';

type Props = {
  proposalId: string;
  roleSigners: RoleSigner[];
  cidNumber?: string;
  onBack: () => void;
};

// 不从当前任职目录重算历史投票人。
export function ProposalDetailPage({ proposalId, roleSigners: externalSigners, cidNumber: externalCidNumber, onBack }: Props) {
  const [info, setInfo] = useState<ProposalFullInfo | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [loading, setLoading] = useState(true);
  const [votingTicket, setVotingTicket] = useState<FrozenVoteBinding | null>(null);
  const [displayMeta, setDisplayMeta] = useState<import('./types').ProposalDisplayMeta | null>(null);
  const epoch = useRef(0);
  const previousChain = useRef<string | null>(null);
  const signersKey = JSON.stringify(externalSigners.map(s => [s.personCid, s.roleCode, s.accountId, s.bindingRevision]).sort());
  const refreshData = useCallback(async () => {
    const token = epoch.current;
    try {
      const data = await proposalApi.getProposalDetail(proposalId);
      if (token !== epoch.current) return;
      const chain = data.frozenVoteView?.genesisHash ?? null;
      if (previousChain.current !== chain) setVotingTicket(null);
      previousChain.current = chain;
      setInfo(data); setError(null);
      // 只在原票据明确记录同一选择时清理待确认签名。
      for (const role of data.frozenVoteView?.roles ?? []) for (const voter of role.voters) {
        if (!voter.personCid || !voter.accountId || voter.bindingRevision == null || voter.voteError || voter.vote == null) continue;
        const binding = { genesisHash: data.frozenVoteView!.genesisHash, proposalId,
          cidNumber: role.cidNumber, roleCode: role.roleCode, accountId: voter.accountId, bindingRevision: voter.bindingRevision,personCid:voter.personCid! };
        const pending = readPendingFrozenVote(binding);
        if (pending?.approve === voter.vote) clearPendingFrozenVote(binding);
      }
    } catch (e) { if (token === epoch.current) setError(sanitizeError(e)); }
    finally { if (token === epoch.current) setLoading(false); }
  }, [proposalId]);
  useEffect(() => {
    const token = ++epoch.current;
    setLoading(true); setInfo(null); setError(null); setVotingTicket(null); setDisplayMeta(null);
    void refreshData();
    void proposalApi.getProposalDisplay(proposalId).then(d => {
      if (token === epoch.current) setDisplayMeta(d);
    }).catch(() => {});
    const timer = setInterval(() => { void refreshData(); }, 5000);
    return () => { ++epoch.current; clearInterval(timer); };
  }, [proposalId, externalCidNumber, signersKey, refreshData]);
  const handleVoteSuccess = useCallback(() => { setVotingTicket(null); void refreshData(); }, [refreshData]);
  if (loading) {
    return <div className="governance-section"><p>加载提案详情…</p></div>;
  }
  if (error && !info) {
    return (
      <div className="governance-section">
        <button className="back-button" onClick={onBack}>← 返回</button>
        <div className="error">{error}</div>
      </div>
    );
  }
  if (!info) return null;

  const { meta } = info;
  const displayId = formatProposalId(meta.proposalId, displayMeta);

  // 分类、展示号和机构名称共用同一标题，随业务详情进入卡片。
  const heading = (
    <div className="proposal-detail-heading">
      <h3>
        <span>{kindLabel(meta.kind)}</span>
        <span className="proposal-detail-number">{displayId}</span>
      </h3>
      {info.cidFullName && (
        <p className="proposal-institution-name">{info.cidFullName}</p>
      )}
    </div>
  );

  return (
    <div className="governance-section proposal-detail-page">
      <header className="proposal-detail-toolbar">
        <button className="back-button" onClick={onBack}>← 返回</button>
        <span className="proposal-detail-status" data-status={meta.status}>
          {statusLabel(meta.status)}
        </span>
      </header>

      {error && <div role="alert" className="error">读取失败：{error}</div>}
      {votingTicket && (
        <VoteSigningFlow key={JSON.stringify(votingTicket)}
          proposalId={meta.proposalId} proposalKind={meta.kind}
          roleSigners={[{...votingTicket,roleName:votingTicket.roleCode,permissions:[]}]}
          frozenTicket={votingTicket} cidNumber={votingTicket.cidNumber}
          onClose={() => setVotingTicket(null)} onSuccess={handleVoteSuccess}
        />
      )}

      <MultisigTransferProposalDetailSection info={info} heading={heading} />
      {!info.transferDetail && !info.safetyFundDetail && !info.sweepDetail && !info.runtimeUpgradeDetail && (
        <div className="institution-info-section">{heading}</div>
      )}

      {/* 协议升级提案详情 */}
      {info.runtimeUpgradeDetail && (
        <div className="institution-info-section">
          {heading}
          <div className="proposal-detail-table">
            <div className="detail-row">
              <span className="detail-label">原因</span>
              <span className="detail-value">{info.runtimeUpgradeDetail.reason}</span>
            </div>
            <div className="detail-row">
              <span className="detail-label">代码哈希</span>
              <code className="detail-value">0x{info.runtimeUpgradeDetail.codeHashHex}</code>
            </div>
            <div className="detail-row">
              <span className="detail-label">提案人</span>
              <code className="detail-value">{accountIdToSs58(info.runtimeUpgradeDetail.proposerAccountId)}</code>
            </div>
          </div>
        </div>
      )}

      {/* 投票进度 */}
      <div className="institution-info-section">
        <div className="proposal-detail-tally-heading">
          <h3>投票进度</h3>
          {meta.kind === 1 && <span>当前阶段：{stageLabel(meta.stage)}</span>}
        </div>
        {info.internalTally && (
          <VoteTallyBar title="内部投票" yes={info.internalTally.yes} no={info.internalTally.no} />
        )}
        {info.jointTally && (
          <VoteTallyBar title="联合投票" yes={info.jointTally.yes} no={info.jointTally.no} threshold={105} />
        )}
        {info.referendumTally && (
          <VoteTallyBar title="公民投票" yes={info.referendumTally.yes} no={info.referendumTally.no} />
        )}
        {!info.internalTally && !info.jointTally && !info.referendumTally && (
          <p className="no-data">暂无投票数据</p>
        )}
      </div>

      {/* 同钱包跨岗位分别显示，状态只取原票据。 */}
      {info.frozenVoteView && (() => {
        const view = info.frozenVoteView;
        const cid = externalCidNumber ?? meta.actorCidNumber ?? meta.subjectCidNumbers[0];
        const roles = view.roles.filter(role => role.cidNumber === cid);
        const ticketCount = roles.reduce((n, r) => n + r.voters.length, 0);
        const walletCount = new Set(roles.flatMap(r => r.voters.filter(v => v.accountId && v.bindingRevision != null).map(v => v.accountId))).size;
        return <div className="institution-info-section">
          <h3>原提案冻结资格（{walletCount} 个当前绑定 / {ticketCount} 张岗位票）</h3>
          {view.scopeError && <p className="error">{view.scopeError}</p>}
          {!roles.length && <p className="error">原提案中没有该机构的冻结票据</p>}
          <div className="institution-grid proposal-detail-vote-grid">
            {roles.map(role => <div key={role.cidNumber + ':' + role.roleCode}>
              {role.error && <p className="error">{role.error}</p>}
              {role.voters.map(voter => {
                const binding: FrozenVoteBinding | null = !voter.personCid||!voter.accountId||voter.bindingRevision == null ? null : {
                  genesisHash: view.genesisHash, proposalId, cidNumber: role.cidNumber,
                  roleCode: role.roleCode, accountId: voter.accountId, bindingRevision: voter.bindingRevision,personCid:voter.personCid!,
                };
                let pending: ReturnType<typeof readPendingFrozenVote> = null;
                let pendingError = '';
                try { pending = binding ? readPendingFrozenVote(binding) : null; } catch (e) { pendingError = sanitizeError(e); }
                const canVote = !pendingError && !error && !view.scopeError && !role.error && !voter.voteError && binding && voter.vote == null &&
                  view.status === 0 && ((view.kind === 0 && view.stage === 0) || (view.kind === 1 && view.stage === 1)) &&
                  BigInt(view.blockNumber) <= BigInt(view.endBlock);
                return <div className="institution-assignment-card" key={voter.personCid??voter.accountId??role.roleCode}>
                  <p><code>{voter.accountId?accountIdToSs58(voter.accountId):'无有效当前绑定'}</code></p>
                  <p>原岗位：{role.roleCode} / 人员CID：{voter.personCid??'历史钱包快照'}</p>
                  <p>姓名及岗位名称：历史资料未记录</p>
                  {voter.voteError ? <p className="error">投票状态未知：{voter.voteError}</p> :
                    voter.vote === true ? <span className="vote-yes-tag">赞成</span> :
                    voter.vote === false ? <span className="vote-no-tag">反对</span> :
                    <span>{pending ? '已签名，等待链上确认' : '未投票'}</span>}
                  {pendingError && <p className="error">{pendingError}</p>}
                  {binding && pending ? <button onClick={() => setVotingTicket(binding)}>恢复原签名</button> :
                    canVote ? <button onClick={() => setVotingTicket(binding)}>投票</button> : null}
                </div>;
              })}
            </div>)}
          </div>
        </div>;
      })()}

    </div>
  );
}

function VoteTallyBar({ title, yes, no, threshold }: {
  title: string; yes: number; no: number; threshold?: number;
}) {
  const total = yes + no;
  const yesPercent = total > 0 ? Math.round((yes / total) * 100) : 0;
  return (
    <div className="vote-tally">
      <div className="vote-tally-header">
        <span className="vote-tally-title">{title}</span>
        <span className="vote-tally-counts">
          赞成 {yes} 票 / 反对 {no} 票
          {threshold != null && <span className="vote-threshold">（通过线 {threshold}）</span>}
        </span>
      </div>
      <div className="vote-tally-bar">
        <div className="vote-tally-bar-yes" style={{ width: total > 0 ? `${yesPercent}%` : '0%' }} />
      </div>
    </div>
  );
}

/// 提案展示号格式化(双层 ID v1):`2026000123` 风格(年份 + 6 位补零序号)。
/// 主键 `proposalId` 与展示号解耦,展示号由 `getProposalDisplay` 反查得到。
/// `meta=null` 时(链上未写入)fallback 到 `#<id>` 形式。
export function formatProposalId(
  id: string,
  meta?: import('./types').ProposalDisplayMeta | null,
): string {
  if (meta == null) return `#${id}`;
  const seq = String(meta.seqInYear).padStart(6, '0');
  return `${meta.year}${seq}`;
}
function kindLabel(kind: number): string {
  return kind === 0 ? '内部投票' : kind === 1 ? '联合投票' : '未知';
}
function stageLabel(stage: number): string {
  switch (stage) { case 0: return '内部阶段'; case 1: return '联合阶段'; case 2: return '公民阶段'; default: return '未知'; }
}
function statusLabel(status: number): string {
  switch (status) { case 0: return '投票中'; case 1: return '已通过'; case 2: return '已否决'; case 3: return '已执行'; case 4: return '执行失败'; default: return '未知'; }
}

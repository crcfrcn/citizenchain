// 提案详情页：提案元数据 + 业务详情模块挂载 + 投票进度 + 管理员投票状态列表。
// 从提案列表和机构页面进入时行为一致：自动检测管理员账户权限。
import { useEffect, useState, useCallback, useRef } from 'react';
import { sanitizeError } from '../tauri';
import { accountIdToSs58 } from '../shared/ss58';
import { MultisigTransferProposalDetailSection } from '../transaction/multisig/ProposalDetailSection';
import { adminsChangeApi } from '../admins/api';
import { InstitutionAssignmentCard } from '../admins/InstitutionAssignmentCard';
import { governanceApi as api } from './api';
import type { ProposalFullInfo, AdminSignerMatch, UserVoteStatus, InstitutionDetail } from './types';
import { VoteSigningFlow } from './VoteSigningFlow';
import '../admins/styles.css';

type Props = {
  proposalId: number;
  adminSigners: AdminSignerMatch[];
  cidNumber?: string;
  onBack: () => void;
};

export function ProposalDetailPage({ proposalId, adminSigners: externalAdminSigners, cidNumber: externalCidNumber, onBack }: Props) {
  const [info, setInfo] = useState<ProposalFullInfo | null>(null);
  const [institution, setInstitution] = useState<InstitutionDetail | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [loading, setLoading] = useState(true);
  const [votingSigner, setVotingSigner] = useState<AdminSignerMatch | null>(null);
  const [voteStatuses, setVoteStatuses] = useState<Record<string, UserVoteStatus>>({});
  const [detectedAdminSigners, setDetectedAdminSigners] = useState<AdminSignerMatch[]>([]);
  const [resolvedCidNumber, setResolvedCidNumber] = useState<string | undefined>(externalCidNumber);
  // 投票中（已提交但未确认上链）的签名账户 ID → 提交时间
  const [pendingVotes, setPendingVotes] = useState<Map<string, number>>(new Map());
  // 双层 ID v1:展示号反查值,链上 ProposalDisplayId[id] 拉取
  const [displayMeta, setDisplayMeta] = useState<import('./types').ProposalDisplayMeta | null>(
    null,
  );
  const pollTimerRef = useRef<ReturnType<typeof setInterval> | null>(null);
  // 超过 5 分钟未确认的投票视为丢失
  const PENDING_TIMEOUT_MS = 5 * 60 * 1000;

  const adminSigners = externalAdminSigners.length > 0 ? externalAdminSigners : detectedAdminSigners;
  const cidNumber = resolvedCidNumber;

  const fetchVoteStatuses = useCallback(async (
    pid: number, admins: InstitutionDetail['admins'], sid: string | undefined,
    voterRoleSubjects: ProposalFullInfo['voterRoleSubjects'],
  ) => {
    const eligibleRoleCodes = new Set(
      voterRoleSubjects
        .filter((subject) => subject.cidNumber === sid)
        .map((subject) => subject.roleCode),
    );
    const results: Record<string, UserVoteStatus> = {};
    await Promise.all(
      admins.flatMap((admin) => {
        const assignments = admin.assignments.filter((assignment) =>
          eligibleRoleCodes.has(assignment.roleCode));
        return assignments.map(async (assignment) => {
          try {
            const roleCode = assignment?.roleCode;
            const vs = await api.checkVoteStatus(pid, admin.account_id, sid, roleCode);
            results[`${admin.account_id}:${roleCode ?? ''}`] = vs;
          } catch (_) {}
        });
      }),
    );
    setVoteStatuses(results);
    return results;
  }, []);

  const refreshData = useCallback(async (inst?: InstitutionDetail | null, sid?: string) => {
    const curInst = inst ?? institution;
    const curSid = sid ?? cidNumber;
    let d: ProposalFullInfo | null = null;
    try {
      d = await api.getProposalDetail(proposalId);
      setInfo(d);
    } catch (_) {}
    if (curInst && curSid && d) {
      const statuses = await fetchVoteStatuses(
        proposalId,
        curInst.admins,
        curSid,
        d.voterRoleSubjects,
      );
      // 已确认上链的投票或超时的投票，从 pending 中移除
      setPendingVotes((prev) => {
        const next = new Map(prev);
        const now = Date.now();
        for (const [ticketKey, submittedAt] of prev) {
          const voteStatus = statuses[ticketKey];
          const voted = voteStatus &&
            (voteStatus.internalVote != null || voteStatus.jointVote != null);
          const timedOut = now - submittedAt > PENDING_TIMEOUT_MS;
          if (voted || timedOut) next.delete(ticketKey);
        }
        return next;
      });
    }
  }, [proposalId, institution, cidNumber, fetchVoteStatuses]);

  useEffect(() => {
    setLoading(true);
    const fetchAll = async () => {
      const [d, dm] = await Promise.all([
        api.getProposalDetail(proposalId),
        api.getProposalDisplay(proposalId).catch(() => null),
      ]);
      setInfo(d);
      setDisplayMeta(dm ?? null);
      let sid = externalCidNumber;
      if (!sid && d.meta.subjectCidNumbers.length > 0) {
        sid = d.meta.subjectCidNumbers[0];
        setResolvedCidNumber(sid);
      }
      let signers = externalAdminSigners;
      if (sid) {
        try {
          const inst = await api.getInstitutionDetail(sid);
          setInstitution(inst);
          if (externalAdminSigners.length === 0) {
            const activated = await adminsChangeApi.getActivatedAdmins(sid, {
              cidNumber: sid,
            });
            signers = activated.map((admin) => ({
              ss58_address: accountIdToSs58(admin.account_id),
              account_id: admin.account_id,
              account_label: '',
            }));
            setDetectedAdminSigners(signers);
          }
          await fetchVoteStatuses(proposalId, inst.admins, sid, d.voterRoleSubjects);
        } catch (_) {}
      }
    };
    fetchAll()
      .catch((e) => setError(sanitizeError(e)))
      .finally(() => setLoading(false));
  }, [proposalId]);

  // 有 pending 投票时轮询刷新
  useEffect(() => {
    if (pendingVotes.size > 0) {
      pollTimerRef.current = setInterval(() => refreshData(), 5000);
    }
    return () => {
      if (pollTimerRef.current) { clearInterval(pollTimerRef.current); pollTimerRef.current = null; }
    };
  }, [pendingVotes.size, refreshData]);

  // 投票提交成功回调：标记为 pending，关闭弹窗
  const handleVoteSuccess = useCallback((txHash: string, account_id: string, voterRoleCode: string | null) => {
    if (account_id) {
      const ticketKey = `${account_id}:${voterRoleCode ?? ''}`;
      setPendingVotes((prev) => new Map(prev).set(ticketKey, Date.now()));
    }
    setVotingSigner(null);
    // 立即刷新一次
    refreshData();
  }, [refreshData]);

  if (loading) {
    return <div className="governance-section"><p>加载提案详情…</p></div>;
  }
  if (error) {
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
  const displayStatus = { code: meta.status, label: statusLabel(meta.status) };

  return (
    <div className="governance-section">
      <button className="back-button" onClick={onBack}>← 返回</button>
      <h2>提案 {displayId}</h2>
      {info.cidFullName && (
        <p className="proposal-institution-name">{info.cidFullName}</p>
      )}

      {votingSigner && (
        <VoteSigningFlow
          proposalId={meta.proposalId}
          proposalKind={meta.kind}
          adminSigners={[votingSigner]}
          cidNumber={cidNumber}
          onClose={() => setVotingSigner(null)}
          onSuccess={handleVoteSuccess}
        />
      )}

      {/* 元数据卡片 */}
      <div className="institution-detail-grid">
        <div className="metric-card">
          <div className="metric-label">提案类型</div>
          <div className="metric-value">{kindLabel(meta.kind)}</div>
        </div>
        {meta.kind === 1 && (
          <div className="metric-card">
            <div className="metric-label">当前阶段</div>
            <div className="metric-value">{stageLabel(meta.stage)}</div>
          </div>
        )}
        <div className="metric-card">
          <div className="metric-label">状态</div>
          <div className={`metric-value status-text-${displayStatus.code}`}>
            {displayStatus.label}
          </div>
        </div>
        {meta.internalCode && (
          <div className="metric-card">
            <div className="metric-label">机构码</div>
            <div className="metric-value">{meta.internalCode}</div>
          </div>
        )}
      </div>

      <MultisigTransferProposalDetailSection info={info} />

      {/* 协议升级提案详情 */}
      {info.runtimeUpgradeDetail && (
        <div className="institution-info-section">
          <h3>协议升级详情</h3>
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
              <code className="detail-value">{accountIdToSs58(info.runtimeUpgradeDetail.proposer_account_id)}</code>
            </div>
          </div>
        </div>
      )}

      {/* 投票进度 */}
      <div className="institution-info-section">
        <h3>投票进度</h3>
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

      {/* 管理员投票状态列表 */}
      {institution && (
        <div className="institution-info-section">
          <h3>管理员投票状态（{institution.admins.length} 人）</h3>
          <div className="admin-grid">
            {institution.admins.map((admin, i) => {
              const accountId = admin.account_id;
              const localSigner = adminSigners.find(
                (signer) => signer.account_id === accountId,
              );
              const eligibleAssignments = admin.assignments.filter((assignment) =>
                info.voterRoleSubjects.some((subject) =>
                  subject.cidNumber === cidNumber && subject.roleCode === assignment.roleCode));
              const assignmentVotes = eligibleAssignments.map((assignment) => {
                const vs = voteStatuses[`${accountId}:${assignment.roleCode}`];
                return meta.kind === 1 ? vs?.jointVote : vs?.internalVote;
              });
              const votedCount = assignmentVotes.filter((vote) => vote != null).length;
              const hasVoted = eligibleAssignments.length > 0 && votedCount === eligibleAssignments.length;
              const voted = hasVoted && assignmentVotes.every((vote) => vote === true);
              const hasPendingTicket = eligibleAssignments.some((assignment) =>
                pendingVotes.has(`${accountId}:${assignment.roleCode}`));
              const hasAvailableTicket = eligibleAssignments.some((assignment) => {
                const ticketKey = `${accountId}:${assignment.roleCode}`;
                return voteStatuses[ticketKey]?.internalVote == null &&
                  voteStatuses[ticketKey]?.jointVote == null && !pendingVotes.has(ticketKey);
              });
              const canVote = localSigner && meta.status === 0 && hasAvailableTicket;

              return (
                <InstitutionAssignmentCard
                  key={accountId}
                  admin={admin}
                  index={i + 1}
                  balanceFen={admin.balanceFen}
                  className={hasVoted ? (voted ? 'admin-card-voted-yes' : 'admin-card-voted-no') : ''}
                  action={
                    canVote ? (
                      <button className="vote-button-inline" onClick={() => setVotingSigner({
                        ...localSigner,
                        roleAssignments: eligibleAssignments.filter((assignment) => {
                          const ticketKey = `${accountId}:${assignment.roleCode}`;
                          return voteStatuses[ticketKey]?.internalVote == null &&
                            voteStatuses[ticketKey]?.jointVote == null && !pendingVotes.has(ticketKey);
                        }),
                      })}>
                        投票
                      </button>
                    ) : hasPendingTicket && !hasVoted ? (
                      <span className="vote-result-tag vote-pending-tag">投票中</span>
                    ) : hasVoted ? (
                      <span className={`vote-result-tag ${voted ? 'vote-yes-tag' : 'vote-no-tag'}`}>
                        {voted ? '全部赞成' : `已投 ${votedCount}/${eligibleAssignments.length}`}
                      </span>
                    ) : (
                      <span className="vote-result-tag vote-none-tag">未投票</span>
                    )
                  }
                />
              );
            })}
          </div>
        </div>
      )}
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
          赞成 {yes} / 反对 {no}
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
  id: number,
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

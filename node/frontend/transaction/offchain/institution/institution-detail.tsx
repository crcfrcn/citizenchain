// 清算行机构详情页(链上 organization-manage::Institutions[cid_number] 已存在时展示)。
//
// 风格参考 governance/InstitutionDetailPage 的卡片栅格 + 折叠子页入口,
// 数据源全部走链上 organization-manage,通过 institutionReadApi.fetchInstitutionDetail 获取。
//
// 顶部按钮根据本机是否已声明清算行节点切换:
//   - 未声明 → "声明本机为清算行节点" → declare-node
//   - 已声明 → 内联展示节点对外端点信息

import { useEffect, useState } from 'react';
import { sanitizeError } from '../../../tauri';
import { offchainApi } from '../api';
import type { ClearingBankNodeOnChainInfo } from '../types';
import { institutionReadApi } from './api';
import type { InstitutionDetail, InstitutionProposalItem } from './types';

type Props = {
  cidNumber: string;
  onBack: () => void;
  onOpenOtherAccounts: (detail: InstitutionDetail) => void;
  onOpenAdminList: (detail: InstitutionDetail) => void;
  onDeclareNode: (cidNumber: string, cidFullName: string) => void;
};

const PROPOSAL_PAGE_SIZE = 10;

export function ClearingBankInstitutionDetailPage({
  cidNumber,
  onBack,
  onOpenOtherAccounts,
  onOpenAdminList,
  onDeclareNode,
}: Props) {
  const [detail, setDetail] = useState<InstitutionDetail | null>(null);
  const [nodeInfo, setNodeInfo] = useState<ClearingBankNodeOnChainInfo | null>(null);
  const [proposals, setProposals] = useState<InstitutionProposalItem[]>([]);
  const [proposalHasMore, setProposalHasMore] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [loading, setLoading] = useState(true);

  useEffect(() => {
    let cancelled = false;
    setLoading(true);
    setError(null);
    Promise.all([
      institutionReadApi.fetchInstitutionDetail(cidNumber),
      offchainApi.queryClearingBankNodeInfo(cidNumber).catch(() => null),
      institutionReadApi
        .fetchInstitutionProposals(cidNumber, 0, PROPOSAL_PAGE_SIZE)
        .catch(() => ({ items: [], hasMore: false })),
    ])
      .then(([d, n, page]) => {
        if (cancelled) return;
        setDetail(d);
        setNodeInfo(n);
        setProposals(page.items);
        setProposalHasMore(page.hasMore);
      })
      .catch((e) => {
        if (!cancelled) setError(sanitizeError(e));
      })
      .finally(() => {
        if (!cancelled) setLoading(false);
      });
    return () => {
      cancelled = true;
    };
  }, [cidNumber]);

  if (loading) {
    return (
      <>
        <button className="back-button" onClick={onBack}>← 返回</button>
        <p>加载中…</p>
      </>
    );
  }

  if (error) {
    return (
      <>
        <button className="back-button" onClick={onBack}>← 返回</button>
        <div className="error">{error}</div>
      </>
    );
  }

  if (!detail) {
    return (
      <>
        <button className="back-button" onClick={onBack}>← 返回</button>
        <p className="no-data">未找到机构详情</p>
      </>
    );
  }

  return (
    <>
      <button className="back-button" onClick={onBack}>← 返回</button>

      <div className="institution-title-row">
        <h2>{detail.cidFullName}</h2>
        {!nodeInfo && (
          <button
            className="primary-button"
            style={{ marginLeft: 'auto' }}
            onClick={() => onDeclareNode(cidNumber, detail.cidFullName)}
          >
            声明本机为清算行节点 →
          </button>
        )}
      </div>

      {/* 已声明节点的对外端点信息(只读展示) */}
      {nodeInfo && (
        <div className="node-info-panel metric-card">
          <h3>清算行节点(本机已声明)</h3>
          <dl>
            <dt>PeerId</dt>
            <dd><code>{nodeInfo.peerId}</code></dd>
            <dt>RPC 端点</dt>
            <dd>{nodeInfo.rpcDomain}:{nodeInfo.rpcPort}</dd>
            <dt>注册区块</dt>
            <dd>#{nodeInfo.registeredAt}</dd>
            <dt>声明账户</dt>
            <dd><code>{nodeInfo.registered_by_ss58_address}</code></dd>
          </dl>
        </div>
      )}

      {/* 机构信息卡片栅格 */}
      <div className="institution-detail-grid">
        <div className="metric-card">
          <div className="metric-label">
            机构身份CID号 <code className="metric-label-id">{detail.cidNumber}</code>
          </div>
          <div className="metric-value">{detail.cidNumber}</div>
        </div>

        <div className="metric-card">
          <div className="metric-label">
            主账户 <code className="metric-label-id">{detail.main_account_info.ss58_address}</code>
          </div>
          <div className="metric-value">{detail.main_account_info.balanceText} 元</div>
        </div>

        <div className="metric-card">
          <div className="metric-label">内部投票阈值</div>
          <div className="metric-value">
            {detail.threshold} / {detail.adminsLen} 票
          </div>
        </div>

        <div className="metric-card">
          <div className="metric-label">
            费用账户 <code className="metric-label-id">{detail.fee_account_info.ss58_address}</code>
          </div>
          <div className="metric-value">{detail.fee_account_info.balanceText} 元</div>
        </div>
      </div>

      {/* 其他账户列表(折叠卡片入口) */}
      <div className="institution-info-section">
        <div
          className="metric-card admin-entry-card"
          role="button"
          tabIndex={0}
          onClick={() => onOpenOtherAccounts(detail)}
          onKeyDown={(e) => e.key === 'Enter' && onOpenOtherAccounts(detail)}
        >
          <div className="admin-entry-left">
            <div className="admin-entry-title">
              其他账户列表（{detail.otherAccounts.length} 个）
            </div>
          </div>
          <span className="admin-entry-arrow">→</span>
        </div>
      </div>

      {/* 管理员列表(折叠卡片入口) */}
      <div className="institution-info-section">
        <div
          className="metric-card admin-entry-card"
          role="button"
          tabIndex={0}
          onClick={() => onOpenAdminList(detail)}
          onKeyDown={(e) => e.key === 'Enter' && onOpenAdminList(detail)}
        >
          <div className="admin-entry-left">
            <div className="admin-entry-title">
              管理员列表（{detail.admins.length} 人）
            </div>
          </div>
          <span className="admin-entry-arrow">→</span>
        </div>
      </div>

      {/* 这里只保留已接入业务；岗位任职变更必须由对应业务模块生成结果。 */}
      <div className="institution-info-section">
        <h3>发起提案</h3>
        <div className="proposal-type-grid">
          <button className="proposal-type-button" disabled title="即将上线">转账</button>
          <button className="proposal-type-button" disabled title="即将上线">关闭多签</button>
          <button className="proposal-type-button" disabled title="即将上线">手续费划转</button>
        </div>
        <p className="no-data">转账、关闭多签、手续费划转后续接入。</p>
      </div>

      {/* 提案列表(分页占位,full scan 留 follow-up) */}
      <div className="institution-info-section">
        <h3>
          提案列表
          {proposals.length > 0 ? `（${proposals.length}${proposalHasMore ? '+' : ''}）` : ''}
        </h3>
        {proposals.length === 0 ? (
          <p className="no-data">暂无提案</p>
        ) : (
          <div className="proposal-list">
            {proposals.map((item) => (
              <div key={item.proposalId} className="proposal-card">
                <div className="proposal-card-header">
                  <span className="proposal-id">#{item.proposalId}</span>
                  <span className="proposal-status">{item.statusLabel}</span>
                </div>
                <div className="proposal-card-body">
                  <span className="proposal-tag">{item.kindLabel}</span>
                  <div className="proposal-summary">{item.summary}</div>
                </div>
              </div>
            ))}
          </div>
        )}
      </div>
    </>
  );
}

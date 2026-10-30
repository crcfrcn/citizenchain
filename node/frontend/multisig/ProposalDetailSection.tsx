import { formatBalance } from "../client/format";
import { accountIdToSs58 } from "../client/ss58";
import type { MultisigTransferProposalDetails } from "./types";


type Props = {
  info: MultisigTransferProposalDetails;
};

// 多签转账详情展示归 multisig 模块维护，治理详情页只负责挂载。
export function MultisigTransferProposalDetailSection({ info }: Props) {
  return (
    <>
      {info.transferDetail && (
        <div className="institution-info-section">
          <h3>转账详情</h3>
          <div className="proposal-detail-table">
            <div className="detail-row">
              <span className="detail-label">发起主体</span>
              <span className="detail-value">
                {info.transferDetail.actorCidNumber || '个人多签'}
              </span>
            </div>
            <div className="detail-row">
              <span className="detail-label">转出账户</span>
              <code className="detail-value">
                {accountIdToSs58(info.transferDetail.fundingAccountId)}
              </code>
            </div>
            <div className="detail-row">
              <span className="detail-label">金额</span>
              <span className="detail-value">
                {formatBalance(info.transferDetail.amountFen)}
              </span>
            </div>
            <div className="detail-row">
              <span className="detail-label">收款人</span>
              <code className="detail-value">{accountIdToSs58(info.transferDetail.beneficiaryAccountId)}</code>
            </div>
            <div className="detail-row">
              <span className="detail-label">备注</span>
              <span className="detail-value">{info.transferDetail.remark || '—'}</span>
            </div>
            <div className="detail-row">
              <span className="detail-label">提案人</span>
              <code className="detail-value">{accountIdToSs58(info.transferDetail.proposerAccountId)}</code>
            </div>
          </div>
        </div>
      )}

      {info.safetyFundDetail && (
        <div className="institution-info-section">
          <h3>安全基金转账详情</h3>
          <div className="proposal-detail-table">
            <div className="detail-row">
              <span className="detail-label">机构 CID</span>
              <span className="detail-value">{info.safetyFundDetail.actorCidNumber}</span>
            </div>
            <div className="detail-row">
              <span className="detail-label">转出账户</span>
              <code className="detail-value">
                {accountIdToSs58(info.safetyFundDetail.institutionAccountId)}
              </code>
            </div>
            <div className="detail-row">
              <span className="detail-label">收款地址</span>
              <code className="detail-value">{accountIdToSs58(info.safetyFundDetail.beneficiaryAccountId)}</code>
            </div>
            <div className="detail-row">
              <span className="detail-label">金额</span>
              <span className="detail-value">{formatBalance(info.safetyFundDetail.amountFen)}</span>
            </div>
            {info.safetyFundDetail.remark && (
              <div className="detail-row">
                <span className="detail-label">备注</span>
                <span className="detail-value">{info.safetyFundDetail.remark}</span>
              </div>
            )}
          </div>
        </div>
      )}

      {info.sweepDetail && (
        <div className="institution-info-section">
          <h3>手续费划转详情</h3>
          <div className="proposal-detail-table">
            <div className="detail-row">
              <span className="detail-label">机构 CID</span>
              <span className="detail-value">{info.sweepDetail.actorCidNumber}</span>
            </div>
            <div className="detail-row">
              <span className="detail-label">转出账户</span>
              <code className="detail-value">
                {accountIdToSs58(info.sweepDetail.institutionAccountId)}
              </code>
            </div>
            <div className="detail-row">
              <span className="detail-label">金额</span>
              <span className="detail-value">{formatBalance(info.sweepDetail.amountFen)}</span>
            </div>
          </div>
        </div>
      )}
    </>
  );
}

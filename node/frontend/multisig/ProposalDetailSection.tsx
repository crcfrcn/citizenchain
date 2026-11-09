import { formatBalance } from "../client/format";
import { accountIdToSs58 } from "../client/ss58";
import type { MultisigTransferProposalDetails } from "./types";

type TransferDetailCardProps = {
  title: string;
  fundingAccountId: string;
  amountFen: string;
  beneficiaryAccountId?: string;
  remark?: string;
  proposerAccountId?: string;
};

function TransferDetailCard({
  title, fundingAccountId, amountFen, beneficiaryAccountId, remark, proposerAccountId,
}: TransferDetailCardProps) {
  return (
    <section className="institution-info-section proposal-transfer-card">
      <header className="proposal-transfer-card-heading">
        <h3>{title}</h3>
        <p className="proposal-transfer-amount">
          <span className="proposal-transfer-label">转账金额</span>
          <strong>{formatBalance(amountFen)}</strong>
        </p>
      </header>
      <div className="proposal-transfer-route">
        <div className="proposal-transfer-field proposal-transfer-source">
          <span className="proposal-transfer-label">转出账户</span>
          <code>{accountIdToSs58(fundingAccountId)}</code>
        </div>
        <span className="proposal-transfer-arrow" aria-hidden="true">→</span>
        <div className="proposal-transfer-field proposal-transfer-destination">
          <span className="proposal-transfer-label">收款账户</span>
          {beneficiaryAccountId
            ? <code>{accountIdToSs58(beneficiaryAccountId)}</code>
            : <span>本机构主账户</span>}
        </div>
        {remark != null && (
          <div className="proposal-transfer-field proposal-transfer-remark">
            <span className="proposal-transfer-label">备注</span>
            <span>{remark || '—'}</span>
          </div>
        )}
        {proposerAccountId && (
          <div className="proposal-transfer-field proposal-transfer-proposer">
            <span className="proposal-transfer-label">发起人</span>
            <code>{accountIdToSs58(proposerAccountId)}</code>
          </div>
        )}
      </div>
    </section>
  );
}

// 多签转账详情展示归 multisig 模块维护，治理详情页只负责挂载。
export function MultisigTransferProposalDetailSection({ info }: { info: MultisigTransferProposalDetails }) {
  const { transferDetail, safetyFundDetail, sweepDetail } = info;

  return (
    <>
      {transferDetail && (
        <TransferDetailCard
          title="转账详情"
          fundingAccountId={transferDetail.fundingAccountId}
          amountFen={transferDetail.amountFen}
          beneficiaryAccountId={transferDetail.beneficiaryAccountId}
          remark={transferDetail.remark}
          proposerAccountId={transferDetail.proposerAccountId}
        />
      )}
      {safetyFundDetail && (
        <TransferDetailCard
          title="安全基金转账详情"
          fundingAccountId={safetyFundDetail.institutionAccountId}
          amountFen={safetyFundDetail.amountFen}
          beneficiaryAccountId={safetyFundDetail.beneficiaryAccountId}
          remark={safetyFundDetail.remark}
        />
      )}
      {sweepDetail && (
        <TransferDetailCard
          title="手续费划转详情"
          fundingAccountId={sweepDetail.institutionAccountId}
          amountFen={sweepDetail.amountFen}
        />
      )}
    </>
  );
}

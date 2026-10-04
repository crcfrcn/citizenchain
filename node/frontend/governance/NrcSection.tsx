// 国家储委会顶级 Section：直接渲染国家储委会机构详情页（单机构，无列表层）。
// 特权动作：协议升级、安全基金转账提案 — 仅国家储委会管理员可发起。
import { useState } from 'react';
import { AdminListPage } from '../admins';
import { InstitutionDetailPage } from './InstitutionDetailPage';
import { ProposalDetailPage } from './ProposalDetailPage';
import { CreateMultisigTransferPage } from '../transaction/multisig/CreateProposalPage';
import { SafetyFundProposalPage } from '../transaction/multisig/SafetyFundProposalPage';
import { SweepProposalPage } from '../transaction/multisig/SweepProposalPage';
import { ProtocolUpgradeProposalPage } from './runtime-upgrade';
import { GrandpaKeyChangePage } from './grandpa-key/GrandpaKeyChangePage';
import type { AdminSignerMatch } from './types';

// 国家储委会 cidNumber（全链唯一，直接进入详情）。
const NRC_CID_NUMBER = 'LN001-NRC0G-944805165-2026';

type NrcView =
  | { page: 'detail' }
  | { page: 'admin-list' }
  | { page: 'proposal-detail'; proposalId: number; adminSigners: AdminSignerMatch[]; cidNumber?: string }
  | { page: 'create-proposal'; orgType: number; cidFullName: string; institution_account_id: string; adminSigners: AdminSignerMatch[] }
  | { page: 'protocol-upgrade'; adminSigners: AdminSignerMatch[] }
  | { page: 'grandpa-key'; adminSigners: AdminSignerMatch[] }
  | { page: 'propose-safety-fund'; actorCidNumber: string; institution_account_id: string; adminSigners: AdminSignerMatch[] }
  | { page: 'propose-sweep'; actorCidNumber: string; institution_account_id: string; cidFullName: string; adminSigners: AdminSignerMatch[] };

export function NrcSection() {
  const [view, setView] = useState<NrcView>({ page: 'detail' });

  const backToDetail = () => setView({ page: 'detail' });

  if (view.page === 'admin-list') {
    return (
      <AdminListPage
        cidNumber={NRC_CID_NUMBER}
        accountRef={{ cidNumber: NRC_CID_NUMBER, institutionCode: 'NRC' }}
        onBack={backToDetail}
      />
    );
  }

  if (view.page === 'proposal-detail') {
    return (
      <ProposalDetailPage
        proposalId={view.proposalId}
        adminSigners={view.adminSigners}
        cidNumber={view.cidNumber}
        onBack={backToDetail}
      />
    );
  }

  if (view.page === 'create-proposal') {
    return (
      <CreateMultisigTransferPage
        cidNumber={NRC_CID_NUMBER}
        cidFullName={view.cidFullName}
        institution_account_id={view.institution_account_id}
        adminSigners={view.adminSigners}
        onBack={backToDetail}
        onSuccess={backToDetail}
      />
    );
  }

  if (view.page === 'protocol-upgrade') {
    return (
      <ProtocolUpgradeProposalPage
        actorCidNumber={NRC_CID_NUMBER}
        adminSigners={view.adminSigners}
        onBack={backToDetail}
        onSuccess={backToDetail}
      />
    );
  }

  if (view.page === 'grandpa-key') {
    // 国家储委会换钥入口复用统一页面，机构 CID 与当前管理员签名集合由本层固定传入。
    return (
      <GrandpaKeyChangePage
        actorCidNumber={NRC_CID_NUMBER}
        adminSigners={view.adminSigners}
        onBack={backToDetail}
        onSuccess={backToDetail}
      />
    );
  }

  if (view.page === 'propose-safety-fund') {
    return (
      <SafetyFundProposalPage
        actorCidNumber={view.actorCidNumber}
        institution_account_id={view.institution_account_id}
        adminSigners={view.adminSigners}
        onBack={backToDetail}
        onSuccess={backToDetail}
      />
    );
  }

  if (view.page === 'propose-sweep') {
    return (
      <SweepProposalPage
        actorCidNumber={view.actorCidNumber}
        institution_account_id={view.institution_account_id}
        cidFullName={view.cidFullName}
        adminSigners={view.adminSigners}
        onBack={backToDetail}
        onSuccess={backToDetail}
      />
    );
  }

  // 默认直接渲染国家储委会机构详情（hideBackButton 以保持 tab 语义）。
  return (
    <InstitutionDetailPage
      cidNumber={NRC_CID_NUMBER}
      onBack={backToDetail}
      hideBackButton
      onOpenAdminList={() => setView({ page: 'admin-list' })}
      onSelectProposal={(proposalId, adminSigners, sid) =>
        setView({ page: 'proposal-detail', proposalId, adminSigners, cidNumber: sid })
      }
      onCreateProposal={(_sid, orgType, cidFullName, institution_account_id, aw) =>
        setView({ page: 'create-proposal', orgType, cidFullName, institution_account_id, adminSigners: aw })
      }
      onCreateProtocolUpgrade={(aw) =>
        setView({ page: 'protocol-upgrade', adminSigners: aw })
      }
      onChangeGrandpaKey={(aw) =>
        setView({ page: 'grandpa-key', adminSigners: aw })
      }
      onCreateSafetyFund={(actorCidNumber, institution_account_id, aw) =>
        setView({ page: 'propose-safety-fund', actorCidNumber, institution_account_id, adminSigners: aw })
      }
      onCreateSweep={(actorCidNumber, institution_account_id, cidFullName, aw) =>
        setView({ page: 'propose-sweep', actorCidNumber, institution_account_id, cidFullName, adminSigners: aw })
      }
    />
  );
}

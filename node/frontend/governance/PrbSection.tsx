// 省储行顶级 Section：列表（orgType=2）→ 机构详情 两级导航。
// 与 PrcSection 同构；唯一差异是 orgTypeFilter=2。省储行同样支持手续费划转提案。
import { useState } from 'react';
import { AdminListPage } from '../admins';
import { InstitutionListView } from './InstitutionListView';
import { InstitutionDetailPage } from './InstitutionDetailPage';
import { ProposalDetailPage } from './ProposalDetailPage';
import { CreateMultisigTransferPage } from '../transaction/multisig/CreateProposalPage';
import { SweepProposalPage } from '../transaction/multisig/SweepProposalPage';
import type { AdminSignerMatch } from './types';

type PrbView =
  | { page: 'list' }
  | { page: 'detail'; cidNumber: string }
  | { page: 'admin-list'; cidNumber: string; orgType: number }
  | { page: 'proposal-detail'; proposalId: number; adminSigners: AdminSignerMatch[]; cidNumber?: string; originCidNumber: string }
  | { page: 'create-proposal'; cidNumber: string; orgType: number; cidFullName: string; institution_account_id: string; adminSigners: AdminSignerMatch[] }
  | { page: 'propose-sweep'; cidNumber: string; institution_account_id: string; cidFullName: string; adminSigners: AdminSignerMatch[] };

export function PrbSection() {
  const [view, setView] = useState<PrbView>({ page: 'list' });

  const backToList = () => setView({ page: 'list' });
  const backToDetail = (cidNumber: string) => setView({ page: 'detail', cidNumber });

  if (view.page === 'admin-list') {
    return (
      <AdminListPage
        cidNumber={view.cidNumber}
        accountRef={{ cidNumber: view.cidNumber }}
        onBack={() => backToDetail(view.cidNumber)}
      />
    );
  }

  if (view.page === 'proposal-detail') {
    return (
      <ProposalDetailPage
        proposalId={view.proposalId}
        adminSigners={view.adminSigners}
        cidNumber={view.cidNumber}
        onBack={() => backToDetail(view.originCidNumber)}
      />
    );
  }

  if (view.page === 'create-proposal') {
    return (
      <CreateMultisigTransferPage
        cidNumber={view.cidNumber}
        cidFullName={view.cidFullName}
        institution_account_id={view.institution_account_id}
        adminSigners={view.adminSigners}
        onBack={() => backToDetail(view.cidNumber)}
        onSuccess={() => backToDetail(view.cidNumber)}
      />
    );
  }

  if (view.page === 'propose-sweep') {
    return (
      <SweepProposalPage
        actorCidNumber={view.cidNumber}
        institution_account_id={view.institution_account_id}
        cidFullName={view.cidFullName}
        adminSigners={view.adminSigners}
        onBack={() => backToDetail(view.cidNumber)}
        onSuccess={() => backToDetail(view.cidNumber)}
      />
    );
  }

  if (view.page === 'detail') {
    const cidNumber = view.cidNumber;
    return (
      <InstitutionDetailPage
        cidNumber={cidNumber}
        onBack={backToList}
        onOpenAdminList={(sid, orgType) => setView({ page: 'admin-list', cidNumber: sid, orgType })}
        onSelectProposal={(proposalId, adminSigners, sid) =>
          setView({ page: 'proposal-detail', proposalId, adminSigners, cidNumber: sid, originCidNumber: cidNumber })
        }
        onCreateProposal={(sid, orgType, cidFullName, institution_account_id, aw) =>
          setView({ page: 'create-proposal', cidNumber: sid, orgType, cidFullName, institution_account_id, adminSigners: aw })
        }
        onCreateSweep={(sid, institution_account_id, cidFullName, aw) =>
          setView({ page: 'propose-sweep', cidNumber: sid, institution_account_id, cidFullName, adminSigners: aw })
        }
      />
    );
  }

  // 默认：省储行机构列表（orgTypeFilter=2）。
  return (
    <InstitutionListView
      orgTypeFilter={2}
      onSelect={(cidNumber) => setView({ page: 'detail', cidNumber })}
    />
  );
}

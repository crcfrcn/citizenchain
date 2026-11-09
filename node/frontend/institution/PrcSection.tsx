import { AdminListPage } from "./index";
import { InstitutionListView } from "./InstitutionListView";
import { InstitutionDetailPage } from "./InstitutionDetailPage";
import { ProposalDetailPage } from "../proposal/ProposalDetailPage";
import { ProtocolUpgradeProposalPage } from "../upgrade/index";
import { GrandpaKeyChangePage } from "../grandpakey/GrandpaKeyChangePage";
import type { AdminSignerMatch } from "./council";
// 省储委会顶级 Section：列表（orgType=1）→ 机构详情 两级导航。
import { useState } from 'react';

type PrcView =
  | { page: 'list' }
  | { page: 'detail'; cidNumber: string }
  | { page: 'admin-list'; cidNumber: string; orgType: number }
  | { page: 'proposal-detail'; proposalId: number; adminSigners: AdminSignerMatch[]; cidNumber?: string; originCidNumber: string }
  | { page: 'protocol-upgrade'; cidNumber: string; adminSigners: AdminSignerMatch[] }
  | { page: 'grandpa-key'; cidNumber: string; adminSigners: AdminSignerMatch[] };

export function PrcSection() {
  const [view, setView] = useState<PrcView>({ page: 'list' });

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

  if (view.page === 'protocol-upgrade') {
    return (
      <ProtocolUpgradeProposalPage
        actorCidNumber={view.cidNumber}
        adminSigners={view.adminSigners}
        onBack={() => backToDetail(view.cidNumber)}
        onSuccess={() => backToDetail(view.cidNumber)}
      />
    );
  }

  if (view.page === 'grandpa-key') {
    // 省储委会换钥返回当前机构详情，禁止跨机构复用管理员签名集合。
    return (
      <GrandpaKeyChangePage
        actorCidNumber={view.cidNumber}
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
        onCreateProtocolUpgrade={(aw) =>
          setView({ page: 'protocol-upgrade', cidNumber, adminSigners: aw })
        }
        onChangeGrandpaKey={(aw) =>
          setView({ page: 'grandpa-key', cidNumber, adminSigners: aw })
        }
      />
    );
  }

  // 默认：省储委会机构列表（orgTypeFilter=1）。
  return (
    <InstitutionListView
      orgTypeFilter={1}
      onSelect={(cidNumber) => setView({ page: 'detail', cidNumber })}
    />
  );
}

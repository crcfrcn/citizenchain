import { AdminListPage } from "./index";
import { InstitutionDetailPage } from "./InstitutionDetailPage";
import { ProposalDetailPage } from "../proposal/ProposalDetailPage";
import { ProtocolUpgradeProposalPage } from "../upgrade/index";
import { GrandpaKeyChangePage } from "../grandpakey/GrandpaKeyChangePage";
import type { AdminSignerMatch } from "./council";
// 国家储委会顶级 Section：直接渲染国家储委会机构详情页（单机构，无列表层）。
// 机构详情负责发起转账类提案，本层保留其他子页导航。
import { useState } from 'react';

// 国家储委会 cidNumber（全链唯一，直接进入详情）。
const NRC_CID_NUMBER = 'LN001-NRC0G-944805165-2026';

type NrcView =
  | { page: 'detail' }
  | { page: 'admin-list' }
  | { page: 'proposal-detail'; proposalId: number; adminSigners: AdminSignerMatch[]; cidNumber?: string }
  | { page: 'protocol-upgrade'; adminSigners: AdminSignerMatch[] }
  | { page: 'grandpa-key'; adminSigners: AdminSignerMatch[] };

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
      onCreateProtocolUpgrade={(aw) =>
        setView({ page: 'protocol-upgrade', adminSigners: aw })
      }
      onChangeGrandpaKey={(aw) =>
        setView({ page: 'grandpa-key', adminSigners: aw })
      }
    />
  );
}

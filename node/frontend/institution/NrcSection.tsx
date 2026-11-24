// 组合国家储委会机构列表、岗位任职及详情页面。

import { AssignmentsPage } from "./index";
import { InstitutionDetailPage } from "./InstitutionDetailPage";
import { ProposalDetailPage } from "../proposal/ProposalDetailPage";
import type { RoleSigner } from "./council";
// 国家储委会顶级 Section：直接渲染国家储委会机构详情页（单机构，无列表层）。
// 机构详情负责发起提案，本层保留其他子页导航。
import { useState } from 'react';

// 国家储委会 cidNumber（全链唯一，直接进入详情）。
const NRC_CID_NUMBER = 'LN001-NRC0G-944805165-2026';

type NrcView =
  | { page: 'detail' }
  | { page: 'assignments' }
  | { page: 'proposal-detail'; proposalId: string; roleSigners: RoleSigner[]; cidNumber?: string };

export function NrcSection() {
  const [view, setView] = useState<NrcView>({ page: 'detail' });

  const backToDetail = () => setView({ page: 'detail' });

  if (view.page === 'assignments') {
    return (
      <AssignmentsPage
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
        roleSigners={view.roleSigners}
        cidNumber={view.cidNumber}
        onBack={backToDetail}
      />
    );
  }

  // 默认直接渲染国家储委会机构详情（hideBackButton 以保持 tab 语义）。
  return (
    <InstitutionDetailPage
      cidNumber={NRC_CID_NUMBER}
      onBack={backToDetail}
      hideBackButton
      onOpenAssignments={() => setView({ page: 'assignments' })}
      onSelectProposal={(proposalId, roleSigners, sid) =>
        setView({ page: 'proposal-detail', proposalId, roleSigners, cidNumber: sid })
      }
    />
  );
}

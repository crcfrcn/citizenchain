// 组合省储委会机构列表、岗位任职及详情页面。

import { AssignmentsPage } from "./index";
import { InstitutionListView } from "./InstitutionListView";
import { InstitutionDetailPage } from "./InstitutionDetailPage";
import { ProposalDetailPage } from "../proposal/ProposalDetailPage";
import type { RoleSigner } from "./council";
// 省储委会顶级 Section：列表（orgType=1）→ 机构详情 两级导航。
import { useState } from 'react';

type PrcView =
  | { page: 'list' }
  | { page: 'detail'; cidNumber: string }
  | { page: 'assignments'; cidNumber: string; orgType: number }
  | { page: 'proposal-detail'; proposalId: string; roleSigners: RoleSigner[]; cidNumber?: string; originCidNumber: string };

export function PrcSection() {
  const [view, setView] = useState<PrcView>({ page: 'list' });

  const backToList = () => setView({ page: 'list' });
  const backToDetail = (cidNumber: string) => setView({ page: 'detail', cidNumber });

  if (view.page === 'assignments') {
    return (
      <AssignmentsPage
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
        roleSigners={view.roleSigners}
        cidNumber={view.cidNumber}
        onBack={() => backToDetail(view.originCidNumber)}
      />
    );
  }

  if (view.page === 'detail') {
    const cidNumber = view.cidNumber;
    return (
      <InstitutionDetailPage
        cidNumber={cidNumber}
        onBack={backToList}
        onOpenAssignments={(sid, orgType) => setView({ page: 'assignments', cidNumber: sid, orgType })}
        onSelectProposal={(proposalId, roleSigners, sid) =>
          setView({ page: 'proposal-detail', proposalId, roleSigners, cidNumber: sid, originCidNumber: cidNumber })
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

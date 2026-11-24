// 组合省储行机构列表、岗位任职及详情页面。

import { AssignmentsPage } from "./index";
import { InstitutionListView } from "./InstitutionListView";
import { InstitutionDetailPage } from "./InstitutionDetailPage";
import { ProposalDetailPage } from "../proposal/ProposalDetailPage";
import type { RoleSigner } from "./council";
// 省储行顶级 Section：列表（orgType=2）→ 机构详情 两级导航。
// 与 PrcSection 同构；唯一差异是 orgTypeFilter=2。省储行同样支持手续费划转提案。
import { useState } from 'react';

type PrbView =
  | { page: 'list' }
  | { page: 'detail'; cidNumber: string }
  | { page: 'assignments'; cidNumber: string; orgType: number }
  | { page: 'proposal-detail'; proposalId: string; roleSigners: RoleSigner[]; cidNumber?: string; originCidNumber: string };

export function PrbSection() {
  const [view, setView] = useState<PrbView>({ page: 'list' });

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

  // 默认：省储行机构列表（orgTypeFilter=2）。
  return (
    <InstitutionListView
      orgTypeFilter={2}
      onSelect={(cidNumber) => setView({ page: 'detail', cidNumber })}
    />
  );
}

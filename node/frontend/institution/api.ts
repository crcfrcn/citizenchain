// 封装机构资料、账户及管理员状态的只读原生命令。

import { invoke } from "../client/tauri";
import type { EligibleClearingBankCandidate, InstitutionDetail, InstitutionProposalPage } from "./types";


// 清算行机构身份只读 Tauri API。机构创建归 onchina 控制台,节点不承接。
export const institutionReadApi = {
  searchEligibleClearingBanks: (query: string, limit?: number) =>
    invoke<EligibleClearingBankCandidate[]>('search_eligible_clearing_banks', { query, limit }),

  fetchInstitutionDetail: (cidNumber: string) =>
    invoke<InstitutionDetail | null>('fetch_clearing_bank_institution_detail', { cidNumber }),

  fetchInstitutionProposals: (cidNumber: string, startId: string, pageSize: number) =>
    invoke<InstitutionProposalPage>('fetch_clearing_bank_institution_proposals', {
      cidNumber,
      startId,
      pageSize,
    }),
};

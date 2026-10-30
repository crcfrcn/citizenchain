import type { ActivateRequestResult, ActivatedAdmin, InstitutionAdminInfo, InstitutionRoleAssignmentInfo, InstitutionDetail } from "./council";


export type {
  ActivateRequestResult,
  ActivatedAdmin,
  InstitutionAdminInfo,
  InstitutionRoleAssignmentInfo,
  InstitutionDetail,
};

export type InstitutionAdminsState = {
  cidNumber: string;
  institutionCode: number[];
  institutionCodeLabel: string;
  kind: number;
  kindLabel: string;
  admins: InstitutionAdminInfo[];
};

export type InstitutionAdminsRef = {
  cidNumber: string;
  institutionCode?: string | null;
};

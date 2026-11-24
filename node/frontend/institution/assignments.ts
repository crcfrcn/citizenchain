// 实体岗位及人员CID任职；钱包只表达identity当前绑定。
export type RolePermission={moduleTag:string;actionCode:number;operation:number;operationLabel:string};
export type InstitutionAssignmentInfo={personCid:string;accountId:string|null;bindingRevision:string|null;familyName:string;givenName:string;termStart:number;termEnd:number;assignmentSource:number;assignmentSourceLabel:string;assignmentSourceRef:string;active:boolean;effective:boolean};
export type InstitutionRoleInfo={roleCode:string;roleName:string;termRequired:boolean;seats:number;active:boolean;permissions:RolePermission[];assignments:InstitutionAssignmentInfo[]};
export type InstitutionRolesState={cidNumber:string;institutionCode:number[];kind:number;blockHash:string;roles:InstitutionRoleInfo[]};
export type InstitutionRef={cidNumber:string;institutionCode?:string|null};
export type RoleAccess={cidNumber:string;roleCode:string;personCid:string;accountId:string;bindingRevision:string;activatedAtMs:number};
export type RoleAccessRequest={requestJson:string;requestId:string;expectedPayloadHash:string;payloadHex:string};
export type RoleSigner={accountId:string;personCid:string;roleCode:string;bindingRevision:string;roleName:string;permissions:RolePermission[]};
export type AssignmentRow=InstitutionAssignmentInfo&{roleCode:string;roleName:string;termRequired:boolean;seats:number;permissions:RolePermission[]};
export function assignmentRows(roles:InstitutionRoleInfo[]):AssignmentRow[]{return roles.flatMap(r=>r.assignments.map(a=>({...a,roleCode:r.roleCode,roleName:r.roleName,termRequired:r.termRequired,seats:r.seats,permissions:r.permissions})));}
export function roleSigners(roles:InstitutionRoleInfo[],accesses:RoleAccess[]):RoleSigner[]{return accesses.flatMap(access=>{const role=roles.find(r=>r.active&&r.roleCode===access.roleCode);const a=role?.assignments.find(a=>a.effective&&a.personCid===access.personCid&&a.accountId===access.accountId&&a.bindingRevision===access.bindingRevision);return role&&a?[{...access,roleName:role.roleName,permissions:role.permissions}]:[];});}
export function canPropose(signer:RoleSigner,module:string,action:number){return signer.permissions.some(p=>p.moduleTag===module&&p.actionCode===action&&p.operation===0);}

// 岗位读取、本机授权及账户余额的唯一原生调用。
import {invoke} from '../client/tauri';
import type {RoleAccess,RoleAccessRequest,InstitutionRolesState,InstitutionRef} from './assignments';
export const rolesApi={
 getInstitutionRoles:(ref:InstitutionRef)=>invoke<InstitutionRolesState|null>('get_institution_roles',{cidNumber:ref.cidNumber,expectedInstitutionCode:ref.institutionCode??null}),
 buildRoleAccessRequest:(signerPublicKey:string,cidNumber:string,personCid:string,roleCode:string,ref?:InstitutionRef)=>invoke<RoleAccessRequest>('build_role_access_request',{signerPublicKey,cidNumber,personCid,roleCode,expectedInstitutionCode:ref?.institutionCode??null}),
 verifyRoleAccess:(requestId:string,signerPublicKey:string,expectedPayloadHash:string,payloadHex:string,responseJson:string)=>invoke<RoleAccess>('verify_role_access',{requestId,signerPublicKey,expectedPayloadHash,payloadHex,responseJson}),
 getRoleAccess:(cidNumber:string,ref?:InstitutionRef)=>invoke<RoleAccess[]>('get_role_access',{cidNumber,expectedInstitutionCode:ref?.institutionCode??null}),
 revokeRoleAccess:(cidNumber:string,personCid:string,roleCode:string,unlockPassword:string)=>invoke<void>('revoke_role_access',{cidNumber,personCid,roleCode,unlockPassword}),
 hasRoleAccess:()=>invoke<boolean>('has_role_access'),
 getAccountBalances:(accountIds:string[])=>invoke<Record<string,string|null>>('get_account_balances',{accountIds}),
};

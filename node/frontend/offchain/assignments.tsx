// 清算行以岗位组织人员任职；余额仅展示当前有效绑定账户。
import {useEffect,useState} from 'react';
import {rolesApi} from '../institution/roles';
import {assignmentRows,type InstitutionRoleInfo} from '../institution/assignments';
import {InstitutionAssignmentCard} from '../institution/InstitutionAssignmentCard';
import {IdentityCidText} from '../client/identityCid';
export function ClearingBankAssignmentsPage({cidNumber,roles,threshold,onBack}:{cidNumber:string;roles:InstitutionRoleInfo[];threshold:number;onBack:()=>void}){
 const rows=assignmentRows(roles);const accounts=[...new Set(rows.flatMap(a=>a.accountId?[a.accountId]:[]))];
 const [balances,setBalances]=useState<Record<string,string|null>>({}),[error,setError]=useState<string|null>(null);
 useEffect(()=>{let active=true;setBalances({});setError(null);if(accounts.length)rolesApi.getAccountBalances(accounts).then(b=>{if(active)setBalances(b);}).catch(()=>{if(active)setError('余额读取失败');});return()=>{active=false;};},[accounts.join('|')]);
 return <><button className="back-button" onClick={onBack}>← 返回</button><h2>岗位任职（治理阈值{threshold}票）</h2><IdentityCidText value={cidNumber}/>{error&&<p className="error">{error}</p>}{roles.map(role=><section key={role.roleCode}><h3>{role.roleName}（{role.seats}席）</h3>{!role.assignments.length&&<p>岗位空缺</p>}{assignmentRows([role]).map((a,i)=><InstitutionAssignmentCard key={a.personCid} assignment={a} index={i+1} balanceFen={a.accountId?balances[a.accountId]:null}/>)}</section>)}</>;
}

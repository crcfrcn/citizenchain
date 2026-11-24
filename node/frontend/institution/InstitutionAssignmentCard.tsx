// 一张卡片展示一个岗位中的一个人员CID任职，不合并为独立人员名单。
import type {ReactNode} from 'react';
import type {AssignmentRow} from './assignments';
import {IdentityCidText} from '../client/identityCid';
import {accountIdToSs58} from '../client/ss58';
import {formatBalance} from '../client/format';
export function InstitutionAssignmentCard({assignment:a,index,balanceFen,action,status,className}:{assignment:AssignmentRow;index?:number;balanceFen?:string|null;action?:ReactNode;status?:ReactNode;className?:string}){
 const day=(n:number)=>n?new Date(n*86400000).toISOString().slice(0,10):'—';
 return <div className={`institution-assignment-card ${className??''}`}>
  <div className="institution-assignment-header"><strong>{index?`${index}. `:''}{a.roleName} · {`${a.familyName}${a.givenName}`.trim()||'姓名未填写'}</strong>{action??status}</div>
  <p>人员CID：<IdentityCidText value={a.personCid}/></p>
  <p>当前绑定：{a.accountId?<code>{accountIdToSs58(a.accountId)}</code>:'无有效绑定'}</p>
  <p>席位容量：{a.seats}；任职状态：{a.effective?'有效':a.active?'未生效或已到期':'已结束'}</p>
  <p>任期：{a.termRequired?`${day(a.termStart)} 至 ${day(a.termEnd)}`:'无固定任期'}</p>
  <p>来源：{a.assignmentSourceLabel}</p>
  {balanceFen!=null&&<p>余额：{formatBalance(balanceFen)}</p>}
 </div>;
}

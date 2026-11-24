// 展示真实岗位及任职，并以当前绑定钱包证明所选任职的本机使用。
import {useState,useEffect,useRef} from 'react';
import {rolesApi} from './roles';
import {assignmentRows,type RoleAccess,type RoleAccessRequest,type InstitutionRef,type InstitutionRolesState,type AssignmentRow} from './assignments';
import {InstitutionAssignmentCard} from './InstitutionAssignmentCard';
import {CitizenSignatureModal} from '../desktop/signing';
import {sanitizeError} from '../client/tauri';
import './styles.css';
export function AssignmentsPage({cidNumber,accountRef,onBack}:{cidNumber:string;accountRef:InstitutionRef;onBack:()=>void}){
 const [state,setState]=useState<InstitutionRolesState|null>(null),[access,setAccess]=useState<RoleAccess[]>([]),[error,setError]=useState<string|null>(null),[loading,setLoading]=useState(true);
 const [selected,setSelected]=useState<AssignmentRow|null>(null),[request,setRequest]=useState<RoleAccessRequest|null>(null),[busy,setBusy]=useState(false);
 const epoch=useRef(0);
 const [revision,setRevision]=useState(0);
 useEffect(()=>{let active=true;++epoch.current;setState(null);setAccess([]);setError(null);setLoading(true);setRequest(null);setSelected(null);setBusy(false);
  if(accountRef.cidNumber!==cidNumber){setError('机构目标不一致');setLoading(false);return()=>{active=false;++epoch.current;};}
  Promise.all([rolesApi.getInstitutionRoles(accountRef),rolesApi.getRoleAccess(cidNumber,accountRef)]).then(([s,a])=>{if(active){if(s&&s.cidNumber!==cidNumber)throw new Error('机构响应目标不一致');setState(s);setAccess(a);}}).catch(e=>{if(active)setError(sanitizeError(e));}).finally(()=>{if(active)setLoading(false);});
  return()=>{active=false;++epoch.current;};
 },[cidNumber,accountRef.cidNumber,accountRef.institutionCode,revision]);
 const start=async(a:AssignmentRow)=>{if(!a.effective||!a.accountId||busy)return;const id=++epoch.current;setBusy(true);setError(null);setSelected(a);
  try{const value=await rolesApi.buildRoleAccessRequest(a.accountId,cidNumber,a.personCid,a.roleCode,accountRef);if(id===epoch.current)setRequest(value);}catch(e){if(id===epoch.current)setError(sanitizeError(e));}finally{if(id===epoch.current)setBusy(false);}};
 const verify=async(response:string)=>{if(!request||!selected?.accountId||busy)return;const id=epoch.current;setBusy(true);
  try{const a=await rolesApi.verifyRoleAccess(request.requestId,selected.accountId,request.expectedPayloadHash,request.payloadHex,response);if(id!==epoch.current)return;setAccess(old=>[...old.filter(x=>!(x.personCid===a.personCid&&x.roleCode===a.roleCode)),a]);setRequest(null);setSelected(null);}catch(e){if(id===epoch.current)setError(sanitizeError(e));}finally{if(id===epoch.current)setBusy(false);}};
 const cancel=()=>{if(busy)return;++epoch.current;setRequest(null);setSelected(null);setBusy(false);};
 return <><button className="back-button" onClick={onBack}>← 返回</button><h2>岗位任职与本机授权</h2>{loading&&<p>读取中…</p>}{error&&<div role="alert" className="error">{error}<button onClick={()=>setRevision(r=>r+1)}>重读</button></div>}
  {state?.roles.map(role=><section key={role.roleCode}><h3>{role.roleName}（{role.seats}席）</h3>{role.assignments.length===0&&<p>岗位空缺</p>}
   {assignmentRows([role]).map((a,index)=>{const granted=a.effective&&role.active&&access.some(x=>x.personCid===a.personCid&&x.roleCode===a.roleCode&&x.accountId===a.accountId&&x.bindingRevision===a.bindingRevision);return <InstitutionAssignmentCard key={a.personCid} assignment={a} index={index+1} action={granted?<span>本机已授权</span>:<button disabled={busy||!a.effective||!a.accountId} onClick={()=>void start(a)}>授权本机</button>}/>;})}</section>)}
  {request&&selected&&<CitizenSignatureModal open title={`本机授权 · ${selected.roleName}`} qrValue={request.requestJson} onScan={response=>void verify(response)} onScanError={setError} onCancel={cancel}/>}
 </>;
}

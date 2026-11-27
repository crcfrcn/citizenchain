// 储委会仅展示委员、储行仅展示董事；恢复原任职卡片，保留当前绑定钱包的激活流程。
import {useState,useEffect,useRef} from 'react';
import {rolesApi} from './roles';
import {assignmentRows,type RoleAccess,type RoleAccessRequest,type InstitutionRef,type InstitutionRolesState,type AssignmentRow} from './assignments';
import {accountIdToSs58} from '../client/ss58';
import {formatBalance} from '../client/format';
import {IdentityCidText} from '../client/identityCid';
import {CitizenSignatureModal} from '../desktop/signing';
import {sanitizeError} from '../client/tauri';
import './styles.css';

function formatDay(day: number): string {
  if (!day) return '';
  const date = new Date(day * 86400 * 1000);
  const year = date.getUTCFullYear();
  const month = `${date.getUTCMonth() + 1}`.padStart(2, '0');
  const dayOfMonth = `${date.getUTCDate()}`.padStart(2, '0');
  return `${year}/${month}/${dayOfMonth}`;
}


export function AssignmentsPage({cidNumber,orgType,accountRef,onBack}:{cidNumber:string;orgType:number;accountRef:InstitutionRef;onBack:()=>void}){
 const [state,setState]=useState<InstitutionRolesState|null>(null),[access,setAccess]=useState<RoleAccess[]>([]),[error,setError]=useState<string|null>(null),[loading,setLoading]=useState(true);
 const [selected,setSelected]=useState<AssignmentRow|null>(null),[request,setRequest]=useState<RoleAccessRequest|null>(null),[busy,setBusy]=useState(false);
 const [step,setStep]=useState<'idle'|'qr'|'verifying'|'done'|'error'>('idle');
 const [activationError,setActivationError]=useState<string|null>(null),[countdown,setCountdown]=useState(0);
 const pending=useRef(false);
 const epoch=useRef(0);
 const generation=epoch.current;
 const [revision,setRevision]=useState(0);
 const [balances,setBalances]=useState<Record<string,string|null>>({}),[balanceError,setBalanceError]=useState<string|null>(null);
 const officeName=orgType===2?'董事':'委员';
 const roleCode=orgType===2?'DIRECTOR':'COMMITTEE_MEMBER';
 const role=state?.roles.find(r=>r.roleCode===roleCode);
 const rows=role?assignmentRows([role]):[];
 const isActivated=(a:AssignmentRow)=>!!role?.active&&a.effective&&access.some(x=>x.cidNumber===cidNumber&&x.personCid===a.personCid&&x.roleCode===a.roleCode&&x.accountId===a.accountId&&x.bindingRevision===a.bindingRevision);
 useEffect(()=>{let active=true;++epoch.current;pending.current=false;setState(null);setAccess([]);setError(null);setLoading(true);setRequest(null);setSelected(null);setBusy(false);setStep('idle');setActivationError(null);setBalances({});setBalanceError(null);
  if(accountRef.cidNumber!==cidNumber){setError('机构目标不一致');setLoading(false);return()=>{active=false;++epoch.current;};}
  Promise.all([rolesApi.getInstitutionRoles(accountRef),rolesApi.getRoleAccess(cidNumber,accountRef)]).then(([s,a])=>{if(active){if(s&&s.cidNumber!==cidNumber)throw new Error('机构响应目标不一致');const office=s?.roles.find(r=>r.roleCode===roleCode);if(!office||!Number.isSafeInteger(office.seats)||office.seats<=0)throw new Error(`${officeName}席位数据不可用`);setState(s);setAccess(a);
   const accounts=[...new Set(office.assignments.flatMap(person=>person.accountId?[person.accountId]:[]))];
   if(accounts.length)void rolesApi.getAccountBalances(accounts).then(value=>{if(active)setBalances(value);}).catch(e=>{if(active)setBalanceError(sanitizeError(e));});
  }}).catch(e=>{if(active)setError(sanitizeError(e));}).finally(()=>{if(active)setLoading(false);});
  return()=>{active=false;++epoch.current;pending.current=false;};
 },[cidNumber,orgType,accountRef.cidNumber,accountRef.institutionCode,revision]);
 // 由签名请求的实际到期时间驱动倒计时，不另设与原生合同不一致的有效期。
 useEffect(()=>{
  if(step==='done'){const timer=setTimeout(()=>{setStep('idle');setRequest(null);setSelected(null);},2000);return()=>clearTimeout(timer);}
  if(step!=='qr'||!request)return;
  const expiresAt=JSON.parse(request.requestJson).e as number;
  const tick=()=>{if(generation!==epoch.current)return;const seconds=Math.max(0,Math.ceil(expiresAt-Date.now()/1000));setCountdown(seconds);if(seconds===0){++epoch.current;setActivationError('签名请求已过期，请重新激活');setStep('error');}};
  tick();const timer=setInterval(tick,1000);return()=>clearInterval(timer);
 },[step,request]);
 const start=async(a:AssignmentRow)=>{if(!a.effective||!a.accountId||pending.current||step!=='idle'||!state?.roles.some(r=>r.roleCode===a.roleCode&&r.active))return;const id=++epoch.current;pending.current=true;setBusy(true);setError(null);setActivationError(null);setSelected(a);
  try{const value=await rolesApi.buildRoleAccessRequest(a.accountId,cidNumber,a.personCid,a.roleCode,accountRef);if(id!==epoch.current)return;const expiresAt=JSON.parse(value.requestJson).e;if(!Number.isSafeInteger(expiresAt)||expiresAt<=Date.now()/1000)throw new Error('激活签名请求无效或已过期，请重新激活');setCountdown(Math.ceil(expiresAt-Date.now()/1000));setRequest(value);setStep('qr');}catch(e){if(id===epoch.current){setSelected(null);setError(sanitizeError(e));}}finally{if(id===epoch.current){pending.current=false;setBusy(false);}}};
 const verify=async(response:string)=>{if(generation!==epoch.current||!request||!selected?.accountId||pending.current||step!=='qr')return;if(JSON.parse(request.requestJson).e<=Date.now()/1000){++epoch.current;setActivationError('签名请求已过期，请重新激活');setStep('error');return;}const id=++epoch.current;pending.current=true;setBusy(true);setStep('verifying');
  try{const a=await rolesApi.verifyRoleAccess(request.requestId,selected.accountId,request.expectedPayloadHash,request.payloadHex,response);if(id!==epoch.current)return;setAccess(old=>[...old.filter(x=>!(x.personCid===a.personCid&&x.roleCode===a.roleCode)),a]);setStep('done');}catch(e){if(id===epoch.current){setActivationError(sanitizeError(e));setStep('error');}}finally{if(id===epoch.current){pending.current=false;setBusy(false);}}};
 const cancel=()=>{if(pending.current)return;++epoch.current;setRequest(null);setSelected(null);setBusy(false);setStep('idle');setActivationError(null);};
 return <div className="governance-section institution-roster">
  <button className="back-button" onClick={onBack}>← 返回机构详情</button>
  <div className="admin-list-header"><h2>{officeName}（{role?.seats??'—'}席）</h2>{rows.length>0&&<span className="admin-list-summary">共 {rows.length} 人{rows.some(isActivated)&&`，已激活 ${rows.filter(isActivated).length}`}</span>}</div>
  {loading&&<p>读取中…</p>}{error&&<div role="alert" className="error">{error}<button onClick={()=>setRevision(r=>r+1)}>重读</button></div>}
  {balanceError&&<div role="alert" className="error">余额读取失败：{balanceError}<button onClick={()=>setRevision(r=>r+1)}>重读余额</button></div>}
  <div className="institution-admin-list-grid">{Array.from({length:role?.seats??0},(_,index)=>rows[index]??null).map((a,index)=>{
   if(!a)return <div key={`seat-${index}`} className="metric-card institution-assignment-card">
    <div className="institution-assignment-top"><span className="admin-card-index">{index+1}</span><div className="institution-assignment-person"><span className="institution-assignment-role">{officeName}</span></div></div>
    <div className="institution-assignment-account">
     <div className="institution-assignment-list-row"><div className="institution-assignment-field"><span className="institution-assignment-label">来源:</span><span className="institution-assignment-value"/></div><div className="institution-assignment-field"><span className="institution-assignment-label">任期:</span><span className="institution-assignment-value"/></div></div>
     {['账户余额:','身份CID号:','绑定账户:'].map(label=><div className="institution-assignment-field" key={label}><span className="institution-assignment-label">{label}</span><span className="institution-assignment-value"/></div>)}
    </div>
   </div>;
   const activated=isActivated(a);
   const startDay=formatDay(a.termStart),endDay=formatDay(a.termEnd);
   const term=a.termRequired?(startDay||endDay?`${startDay||'—'}-${endDay||'—'}`:'待写入'):'无固定任期';
   const balance=a.accountId?balances[a.accountId]:null;
   return <div key={a.personCid} className={`metric-card institution-assignment-card ${activated?'admin-card-activated':''}`}>
    <div className="institution-assignment-top">
     <span className="admin-card-index">{index+1}</span>
     <div className="institution-assignment-person"><span className="institution-assignment-role">{officeName}</span><span className="institution-assignment-dot" aria-hidden="true"/><span className="institution-assignment-name">{`${a.familyName}${a.givenName}`.trim()||'未填写'}</span></div>
     <div className="institution-assignment-top-action">{activated?<span className="activated-tag">已激活</span>:<button className="activate-button" disabled={busy||step!=='idle'||!role?.active||!a.effective||!a.accountId} onClick={()=>void start(a)}>激活</button>}</div>
    </div>
    <div className="institution-assignment-account">
     <div className="institution-assignment-list-row">
      <div className="institution-assignment-field"><span className="institution-assignment-label">来源:</span><span className="institution-assignment-value">{a.assignmentSourceLabel}</span></div>
      <div className="institution-assignment-field"><span className="institution-assignment-label">任期:</span><span className="institution-assignment-value">{term}</span></div>
     </div>
     <div className="institution-assignment-field"><span className="institution-assignment-label">账户余额:</span><span className="institution-assignment-value">{balance!=null?formatBalance(balance):'—'}</span></div>
     <div className="institution-assignment-field"><span className="institution-assignment-label">身份CID号:</span><span className="institution-assignment-value"><IdentityCidText value={a.personCid}/></span></div>
     <div className="institution-assignment-field"><span className="institution-assignment-label">绑定账户:</span><span className="institution-assignment-value">{a.accountId?accountIdToSs58(a.accountId):'无有效绑定'}</span></div>
    </div>
   </div>;
  })}</div>
  {request&&selected&&<CitizenSignatureModal open title={`激活 · ${selected.roleName}`} qrValue={request.requestJson} countdownSeconds={countdown} status={step==='verifying'?'submitting':step==='done'?'success':step==='error'?'error':'ready'} statusTitle={step==='verifying'?'正在验证激活签名':step==='done'?'激活成功':step==='error'?'激活失败':undefined} error={activationError} onScan={response=>void verify(response)} onScanError={e=>{if(generation===epoch.current&&!pending.current&&step==='qr'){++epoch.current;setActivationError(e);setStep('error');}}} onCancel={cancel}/>}
 </div>;
}

#!/usr/bin/env node
import {readFileSync as tataGateRead, realpathSync as tataGateReal} from 'node:fs';
import {execFileSync as tataGateExec} from 'node:child_process';
export const tataGateOwner = "crcfrcn/citizenchain";
// 本仓塔塔门禁只读核对仓库、目录、声明和流程边界；产品测试由所属流程执行。
import {execFileSync,spawnSync} from 'node:child_process';
import {existsSync,lstatSync,readFileSync,readdirSync,realpathSync} from 'node:fs';
import {join,resolve,sep,isAbsolute} from 'node:path';
import {fileURLToPath} from 'node:url';
const root=resolve(fileURLToPath(new URL('../..',import.meta.url)));
const repository="crcfrcn/citizenchain";
const scripts=Object.freeze(["build.mjs", "publish.mjs"]);
const required=Object.freeze(["CitizenChainNode.md", "CitizenChainRuntime.md", "Cargo.lock"]);
const fail=message=>{throw Error('本仓只读门禁：'+message);};
const git=(...args)=>execFileSync('git',['-C',root,...args],{encoding:'utf8',maxBuffer:1024*1024}).trim();
function file(relative){
 if(typeof relative!=='string'||!relative||isAbsolute(relative)||relative.split('/').some(part=>!part||part==='.'||part==='..'))fail('登记路径无效');
 const path=join(root,relative),info=lstatSync(path);
 if(!info.isFile()||info.isSymbolicLink()||!info.size||realpathSync(path)!==path)fail('登记文件缺失或经过链接：'+relative);
 return readFileSync(path,'utf8');
}
function exact(relative,names){
 const path=join(root,relative),info=lstatSync(path);
 if(!info.isDirectory()||info.isSymbolicLink()||realpathSync(path)!==path)fail('目录身份无效：'+relative);
 if(JSON.stringify(readdirSync(path).sort())!==JSON.stringify([...names].sort()))fail('目录闭集无效：'+relative);
}
function syntax(relative){
 const result=spawnSync(process.execPath,['--check',join(root,relative)],{encoding:'utf8',maxBuffer:1024*1024});
 if(result.error||result.signal||result.status!==0)fail('Node语法无效：'+relative);
}
function sourceInventory(contract){
 if(!Array.isArray(contract.node_tests)||!Array.isArray(contract.functions))fail('门禁登记不是完整列表');
 for(const name of contract.node_tests){file(name);if(name.endsWith('.mjs'))syntax(name);}
 for(const entry of contract.functions){if(!entry||typeof entry.path!=='string')fail('功能登记无路径');file(entry.path);}
}
export function validateVectorGroup(canonical, mirror, { keys, values, top, complete = false }) {
  const normalize = value => typeof value === 'string' ? value.toLowerCase() : value;
  function index(document) {
    if (!document || !Array.isArray(document.vectors) || document.vectors.length === 0) fail('金标缺少非空向量');
    const map = new Map();
    for (const vector of document.vectors) {
      const fields = Array.isArray(keys) ? keys : keys?.[vector?.kind];
      if (!vector || !Array.isArray(fields) || !fields.length || fields.some(key => vector[key] === undefined) || values.some(key => vector[key] === undefined)) fail('金标向量字段缺失');
      const key = JSON.stringify(fields.map(field => normalize(vector[field])));
      if (map.has(key)) fail('金标存在重复语义键');
      map.set(key, vector);
    }
    return map;
  }
  const expected = index(canonical), actual = index(mirror);
  if (top.some(field => canonical[field] === undefined || normalize(canonical[field]) !== normalize(mirror[field]))) fail('金标顶层参数漂移');
  for (const [key, vector] of actual) {
    const source = expected.get(key);
    if (!source || values.some(field => normalize(source[field]) !== normalize(vector[field]))) fail('金标密码学值漂移');
  }
  if (complete && expected.size !== actual.size) fail('金标签名域必须完整覆盖');
  return actual.size;
}

// 上游链索引与本端Dart注册表必须具有真实内容，重复索引不能静默覆盖。
export function validatePalletRegistry(chain, dart = null) {
  const indices = new Map(), names = new Set();
  for (const match of chain.matchAll(/#\[runtime::pallet_index\((\d+)\)\]\s*\n\s*pub type (\w+)\s*=/gu)) {
    const index = Number(match[1]), name = match[2];
    if (indices.has(index) || names.has(name)) fail('金标链Pallet索引或名称重复');
    indices.set(index, name); names.add(name);
  }
  if (!indices.size) fail('金标链Pallet真源为空');
  if (dart === null) return indices.size;
  const constants = new Set();
  for (const match of dart.matchAll(/static const (?:int\s+)?(\w+Pallet)\s*=\s*(\d+);/gu)) {
    if (constants.has(match[1])) fail('金标DartPallet常量重复');
    constants.add(match[1]);
    const base = match[1].replace(/Pallet$/u, '');
    if (indices.get(Number(match[2])) !== base[0].toUpperCase() + base.slice(1)) fail('金标DartPallet索引漂移');
  }
  if (!constants.size) fail('金标DartPallet注册表为空');
  return constants.size;
}

// 金标和Pallet注册表只核本仓现有源码与夹具，不执行构建。
function checkPublicSource(){
 const forbidden=Buffer.from('f09f87a8f09f87b3','hex');
 for(const name of git('ls-files','-z','--cached','--others','--exclude-standard').split('\0').filter(Boolean)){
  if(name.split('/').some(part=>!part||part==='.'||part==='..'))fail('受检文件路径越界');
  const path=join(root,name);if(!existsSync(path))continue;
  const info=lstatSync(path);if(!info.isFile()||info.isSymbolicLink())fail('受检文件类型无效：'+name);
  const bytes=readFileSync(path);if(bytes.includes(forbidden))fail('文件含禁用字符：'+name);
  if(bytes.includes(0))continue;
  const source=bytes.toString('utf8');
  if(/AKIA[0-9A-Z]{16}|github_pat_[A-Za-z0-9_]{20,}|gh[pousr]_[A-Za-z0-9]{30,}|sk_live_[A-Za-z0-9]{16,}/u.test(source)
   ||/-----BEGIN (?:RSA |EC |OPENSSH )?PRIVATE KEY-----\s+([A-Za-z0-9+/=\s]{32,})/u.test(source))fail('文件疑似含机密：'+name);
  if(/\.(?:mjs|js|ts|tsx|py|sh)$/u.test(name)&&!/(?:^|\/)(?:test|tests|vendor)\//u.test(name)
   &&name!=='.github/tatagate/tatagate.mjs'){
   const boundary=name==='node/frontend/metamask.mjs'?source.indexOf('\nif(inlineTestEntry){'):-1;
   const formal=boundary>=0?source.slice(0,boundary):source;
   for(const match of formal.matchAll(/(?:http|ws):\/\/[^\s'"`]+/gu))
    if(!/\.(?:invalid|example)(?:[/?#]|$)/u.test(match[0]))fail('第一方源码含明文网络地址：'+name);
  }
 }
}
function checkChainContracts(){
 const fixture=name=>JSON.parse(file('runtime/primitives/tests/fixtures/'+name+'.json'));
 validateVectorGroup(fixture('signing_domain_vectors'),fixture('signing_domain_vectors'),{keys:['op_tag','scale_payload_hex'],values:['message_hex'],top:['domain'],complete:true});
 validateVectorGroup(fixture('binary_prefix_domain_vectors'),fixture('binary_prefix_domain_vectors'),{keys:['name'],values:['op_tag','prefix_hex','payload_hex','total_len'],top:['domain']});
 validateVectorGroup(fixture('account_derive_vectors'),fixture('account_derive_vectors'),{keys:{InstitutionMain:['kind','cid_number'],InstitutionFee:['kind','cid_number'],InstitutionSafetyFund:['kind','cid_number'],InstitutionHe:['kind','cid_number'],InstitutionStake:['kind','cid_number'],InstitutionClearing:['kind','cid_number'],InstitutionNamed:['kind','cid_number','account_name'],Personal:['kind','creator_account_id','account_name']},values:['account_id'],top:['domain','ss58_format']});
 validatePalletRegistry(file('runtime/src/lib.rs'));
}

export function checkRepository(){
  tataGateValidateWorkflow(tataGateRead(root+'/.github/workflows/tatagate.yml','utf8'));
 if(realpathSync(root)!==root||git('rev-parse','--show-toplevel')!==root||!tataGateBranch(root)
  ||!['https://github.com/'+repository,'https://github.com/'+repository+'.git'].includes(git('remote','get-url','origin')))fail('正式主检出或HTTPS来源不符');
 const contract=JSON.parse(file('.github/tatagate/tatagate.json'));
 if(contract.schema!==1||contract.repository!==repository.split('/')[1]
  ||contract.github_repository&&contract.github_repository!==repository
  ||JSON.stringify(contract.checks)!==JSON.stringify(['repository-contracts','cross-platform-contracts','flow-isolation','syntax'])
  ||!Array.isArray(contract.workflows)||!contract.workflows.length||new Set(contract.workflows).size!==contract.workflows.length)fail('本仓门禁声明无效');
 exact('scripts',scripts);
 exact('.github/tatagate',['tatagate.json','tatagate.mjs']);
 exact('.github/workflows',contract.workflows.flatMap(name=>[name,name.replace(/\.yml$/u,'.mjs')]).concat('tatagate.yml'));
 for(const name of required)file(name);
 const build=file('scripts/build.mjs'),publish=file('scripts/publish.mjs');
 if(/\.github\/tatagate\//u.test(build)||/(?:from|import\()\s*['"][^'"]*(?:publish\.mjs|\.github\/workflows)/u.test(build))fail('Build读取其它流程');
 if(/(?:from|import\()\s*['"][^'"]*(?:build\.mjs|\.github\/tatagate)/u.test(publish))fail('Publish调用其它流程');
 for(const name of contract.workflows){
  if(typeof name!=='string'||!/^release-[a-z0-9-]+\.yml$/u.test(name))fail('Workflow身份无效');
  const yaml=file('.github/workflows/'+name),entry='.github/workflows/'+name.replace(/\.yml$/u,'.mjs'),workflow=file(entry);
  if(!yaml.includes('workflow_dispatch:')||/^\s*push\s*:/mu.test(yaml))fail('自动化不得随推送派发');
  if(/scripts\/(?:build|publish)\.mjs|\.github\/tatagate\//u.test(workflow))fail('自动化调用其它流程');
  syntax(entry);
 }
 sourceInventory(contract);
 checkPublicSource();
 syntax('scripts/build.mjs');syntax('scripts/publish.mjs');syntax('.github/tatagate/tatagate.mjs');
 checkChainContracts();
 return {schema:1,product_id:contract.repository,checks:contract.checks,status:'passed'};
}
function main(args){
 const mode=args[0];
 if(mode==='physical'&&args.length===2){if(resolve(args[1])!==root)fail('门禁物理根无效');return checkRepository();}
 if(mode==='local'&&args.length===5){
  const [_,source,base,head,work]=args;
  if(source!==root||!isAbsolute(work)||resolve(work)!==work||!work.startsWith(join(root,'target')+sep)
   ||!/^[a-f0-9]{40}$/u.test(base)||!/^[a-f0-9]{40}$/u.test(head)||git('rev-parse','HEAD')!==head)fail('只读门禁任务坐标无效');
  const ancestor=spawnSync('git',['-C',root,'merge-base','--is-ancestor',base,head]);
  if(ancestor.error||ancestor.status!==0)fail('受检提交范围无效');
  return checkRepository();
 }
 if(mode==='check'&&args.length===1)return checkRepository();
 fail('只读门禁命令无效');
}
if(!['github','cleanup'].includes(process.argv[2]) && !(process.env.NODE_TEST_CONTEXT && process.argv.length===2) && process.argv[1]&&resolve(process.argv[1])===fileURLToPath(import.meta.url)){
 try{process.stdout.write(JSON.stringify(main(process.argv.slice(2)))+'\n');}
 catch(error){process.stderr.write(String(error?.message||error)+'\n');process.exitCode=1;}
}

// GitHub入口和清理只处理本仓tatagate.yml；产品检查仍由本仓原有实现执行。
export function tataGateBranch(repositoryRoot) {
  if (process.env.GITHUB_ACTIONS === 'true') { tataGateContext(repositoryRoot); return true; }
  return tataGateExec(process.env.PRODUCT_GIT_BIN || '/usr/bin/git', ['-C',repositoryRoot,'branch','--show-current'], {encoding:'utf8'}).trim() === 'main';
}
export function tataGateContext(repositoryRoot, input=process.env, event=JSON.parse(tataGateRead(input.GITHUB_EVENT_PATH,'utf8'))) {
  if (input.GITHUB_ACTIONS !== 'true' || input.GITHUB_EVENT_NAME !== 'push'
    || input.GITHUB_REPOSITORY !== tataGateOwner || input.GITHUB_REF !== 'refs/heads/main'
    || input.GITHUB_WORKSPACE !== repositoryRoot || tataGateReal(repositoryRoot) !== repositoryRoot
    || event.repository?.full_name !== tataGateOwner || event.ref !== input.GITHUB_REF
    || event.deleted === true || event.after !== input.GITHUB_SHA
    || !/^[a-f0-9]{40}$/u.test(event.after || '') || !/^[a-f0-9]{40}$/u.test(event.before || '')
    || event.before === event.after || input.GITHUB_WORKFLOW_REF!==tataGateOwner+'/.github/workflows/tatagate.yml@refs/heads/main') {
    throw Error('本仓塔塔门禁GitHub事件身份无效');
  }
  const git=input.PRODUCT_GIT_BIN || '/usr/bin/git';
  const read=args=>tataGateExec(git,['-c','core.hooksPath=/dev/null','-C',repositoryRoot,...args],{encoding:'utf8'}).trim();
  if (read(['rev-parse','HEAD']) !== event.after || read(['rev-parse','--show-toplevel']) !== repositoryRoot
    || !['https://github.com/'+tataGateOwner,'https://github.com/'+tataGateOwner+'.git'].includes(read(['remote','get-url','--all','origin']))) {
    throw Error('本仓塔塔门禁GitHub提交或来源无效');
  }
  if(read(['status','--porcelain=v1','--untracked-files=all']))throw Error('本仓塔塔门禁GitHub检出存在未提交改动');
  if(input.GITHUB_JOB==='gate'&&event.before!=='0'.repeat(40)){
    try{tataGateExec(git,['-C',repositoryRoot,'merge-base','--is-ancestor',event.before,event.after],{encoding:'utf8',stdio:'pipe'});}catch{throw Error('本仓塔塔门禁GitHub提交范围不是快进祖先');}
  }
  return {...event,before:event.before === '0'.repeat(40) ? '4b825dc642cb6eb9a060e54bf8d69288fbee4904' : event.before};
}
export function tataGateValidateWorkflow(source) {
  const jobs=source?.slice(source.indexOf('\njobs:\n')).match(/^  [a-z][a-z0-9_]*:$/gmu);
  const entry=new URL(import.meta.url).pathname.split('/').at(-1);
  const gate=source?.split('  gate:\n')[1]?.split('\n  cleanup:')[0];
  if(!gate||/^    continue-on-error:/mu.test(gate)||!source.includes('permissions:\n  contents: read\n'))throw Error('本仓塔塔门禁检查权限或结果处理无效');
  if(JSON.stringify(jobs)!==JSON.stringify(['  gate:','  cleanup:'])||!source.includes('run: node .github/tatagate/'+entry+' github\n')||!source.includes('run: node .github/tatagate/'+entry+' cleanup\n'))throw Error('本仓塔塔门禁Job或执行入口无效');
  if (typeof source !== 'string' || !source.startsWith('name: '+tataGateOwner.split('/')[1]+'.tatagate\n')
    || !/^  push:\n    branches: \[main\]$/mu.test(source)
    || /^\s*(?:workflow_run|workflow_dispatch|schedule|pull_request):/mu.test(source)
    || !source.includes('group: "${{ github.repository }}-tatagate"')
    || !/^  cancel-in-progress: false$/mu.test(source) || !/^  queue: max$/mu.test(source)
    || !/^  gate:$/mu.test(source) || !/^  cleanup:$/mu.test(source)
    || !/^    needs: \[gate\]$/mu.test(source) || !source.includes('if: ${{ always() }}')
    || !/^    continue-on-error: true$/mu.test(source)
    || !source.includes('TATAGATE_RESULT: "${{ needs.gate.result }}"')
    || !source.includes('persist-credentials: false')
    || !source.includes(' github\n') || !source.includes(' cleanup\n')) throw Error('本仓塔塔门禁Workflow合同无效');
  return true;
}
function tataGateWorkflowRun(run) {
  return Number.isSafeInteger(run?.id) && run.id>0 && Number.isSafeInteger(run.run_number) && run.run_number>0
    && Number.isSafeInteger(run.run_attempt) && run.run_attempt>0
    && run.path === '.github/workflows/tatagate.yml' && run.event === 'push' && run.head_branch === 'main'
    && run.repository?.full_name === tataGateOwner && /^[a-f0-9]{40}$/u.test(run.head_sha || '')
    && Number.isFinite(Date.parse(run.created_at));
}
export function tataGateCleanupPlan(rows,current,result) {
  if (!['success','failed'].includes(result) || !tataGateWorkflowRun(current) || !Array.isArray(rows)) throw Error('本仓塔塔门禁清理身份无效');
  return rows.filter(run=>tataGateWorkflowRun(run) && run.status==='completed' && typeof run.conclusion==='string'
    && run.id!==current.id && run.run_number<current.run_number
    && (run.conclusion==='success'?'success':'failed')===result).sort((a,b)=>a.run_number-b.run_number);
}
export async function tataGateAPI(path,{method='GET',fetchImpl=fetch,token=process.env.GH_TOKEN}={}) {
  if (typeof token!=='string' || !token || typeof path!=='string' || path.split('?')[0].includes('..') || path.split('?')[0].includes('%') || path.startsWith('/') || /[\r\n]/u.test(path)) throw Error('本仓塔塔门禁API参数无效');
  let response;
  try {response=await fetchImpl('https://api.github.com/repos/'+tataGateOwner+'/'+path,{method,redirect:'error',
    headers:{Authorization:'Bearer '+token,Accept:'application/vnd.github+json','X-GitHub-Api-Version':'2026-03-10','User-Agent':'TataGate'},
    signal:AbortSignal.timeout(30000)});}catch{throw Error('本仓塔塔门禁API连接未确认');}
  if(response.status===404 && method==='GET')return null;
  if(!response.ok)throw Error('本仓塔塔门禁API失败：HTTP '+response.status);
  if(response.status===204)return null;
  let size=0;const parts=[];
  if(!response.body)throw Error('本仓塔塔门禁API回执缺失');
  for await(const chunk of response.body){size+=chunk.length;if(size>8*1024**2)throw Error('本仓塔塔门禁API回执超限');parts.push(chunk);}
  try{return JSON.parse(new TextDecoder('utf-8',{fatal:true}).decode(Buffer.concat(parts)));}catch{throw Error('本仓塔塔门禁API回执无效');}
}
async function tataGateHistory(current,api) {
  const read=async(start,end)=>{
    const query='actions/workflows/tatagate.yml/runs?event=push&branch=main&status=completed&created='+encodeURIComponent(new Date(start).toISOString().slice(0,19)+'Z..'+new Date(end).toISOString().slice(0,19)+'Z');
    const first=await api(query+'&per_page=100&page=1');
    if(!Number.isSafeInteger(first?.total_count)||!Array.isArray(first.workflow_runs))throw Error('本仓塔塔门禁历史清单无效');
    if(first.total_count>1000){const middle=Math.floor((start+end)/2000)*1000;if(middle<=start||middle>=end)throw Error('本仓塔塔门禁历史超过同秒上限');return [...await read(start,middle),...await read(middle+1000,end)];}
    const rows=[...first.workflow_runs];
    for(let page=2;rows.length<first.total_count;page++){const value=await api(query+'&per_page=100&page='+page);if(!Array.isArray(value?.workflow_runs)||!value.workflow_runs.length)throw Error('本仓塔塔门禁历史分页不完整');rows.push(...value.workflow_runs);}
    return rows;
  };
  const rows=await read(Date.UTC(2008,0,1),Math.floor(Date.parse(current.created_at)/1000)*1000);
  return [...new Map(rows.map(run=>[run.id,run])).values()];
}
export async function tataGateCleanup(result,identity,api=tataGateAPI) {
  const current=await api('actions/runs/'+identity.id);
  if(identity.attempt!==undefined&&current?.run_attempt!==identity.attempt)throw Error('本仓塔塔门禁当前Attempt不符');
  if(!tataGateWorkflowRun(current)||current.id!==identity.id||current.head_sha!==identity.sha)throw Error('本仓塔塔门禁当前Run回读无效');
  const plan=tataGateCleanupPlan(await tataGateHistory(current,api),current,result),removed=[];
  for(const candidate of plan){
    const path='actions/runs/'+candidate.id;
    const latest=await api('actions/runs/'+current.id);
    if(!latest||latest.head_sha!==current.head_sha||latest.run_attempt!==current.run_attempt)throw Error('本仓塔塔门禁当前Run已变化');
    const again=await api(path);
    if(again===null){removed.push(candidate.id);continue;}
    if(again.run_attempt!==candidate.run_attempt||again.conclusion!==candidate.conclusion
      ||tataGateCleanupPlan([again],current,result).length!==1)throw Error('本仓塔塔门禁旧Run已变化，停止清理');
    try{await api(path,{method:'DELETE'});}catch(error){if(await api(path)!==null)throw error;}
    if(await api(path)!==null)throw Error('本仓塔塔门禁旧Run删除回查失败');
    removed.push(candidate.id);
  }
  return removed;
}
export async function tataGateCommand(mode) {
  const {fileURLToPath}=await import('node:url'),{resolve}=await import('node:path');
  const repositoryRoot=resolve(fileURLToPath(new URL('../..',import.meta.url)));
  const event=tataGateContext(repositoryRoot);
  tataGateValidateWorkflow(tataGateRead(repositoryRoot+'/.github/workflows/tatagate.yml','utf8'));
  if(mode==='github'){if(process.env.GITHUB_JOB!=='gate')throw Error('本仓塔塔门禁Job身份无效');const receipt=await tataGateRunOwn(repositoryRoot,event);console.log(JSON.stringify({repository:tataGateOwner,source_sha:event.after,receipt}));return receipt;}
  if(process.env.GITHUB_JOB!=='cleanup')throw Error('本仓塔塔门禁清理Job身份无效');
  const result=process.env.TATAGATE_RESULT;
  if(!['success','failure','cancelled','skipped'].includes(result))throw Error('本仓塔塔门禁前置结果无效');
  const id=Number(process.env.GITHUB_RUN_ID);
  if(!Number.isSafeInteger(id)||id<=0)throw Error('本仓塔塔门禁Run编号无效');
  const jobs=await tataGateAPI('actions/runs/'+id+'/jobs?filter=latest&per_page=100');
  const gate=jobs?.jobs?.find(job=>job.name==='gate');
  if(gate?.status!=='completed'||typeof gate.conclusion!=='string'||(gate.conclusion==='success')!==(result==='success'))throw Error('本仓塔塔门禁前置结果与GitHub不一致');
  const removed=await tataGateCleanup(result==='success'?'success':'failed',{id,sha:event.after,attempt:Number(process.env.GITHUB_RUN_ATTEMPT)});
  const summary='塔塔门禁'+(result==='success'?'成功':'失败')+'；同类旧Run已清理：'+(removed.join('、')||'无')+'。\n';
  if(process.env.GITHUB_STEP_SUMMARY){const {appendFileSync}=await import('node:fs');appendFileSync(process.env.GITHUB_STEP_SUMMARY,summary);}
  console.log(summary.trim());
}
if(process.argv[1] && ['github','cleanup'].includes(process.argv[2]) && process.argv.length===3
  && new URL('file:'+process.argv[1]).href===import.meta.url){
  try{await tataGateCommand(process.argv[2]);}catch(error){console.error(('本仓塔塔门禁执行失败：'+String(error.message??error)).replace(/gh[sopru]_[^\s]+|github_pat_[^\s]+/gu,'[已隐藏]').slice(0,2000));process.exitCode=1;}
}

async function tataGateRunOwn(repositoryRoot,event) {
  const receipt=await checkRepository();
  const {spawnSync}=await import('node:child_process');
  const environment={...process.env};delete environment.NODE_TEST_CONTEXT;
  const result=spawnSync(process.execPath,['--test','--test-reporter=tap',import.meta.filename],{cwd:repositoryRoot,env:environment,encoding:'utf8',maxBuffer:8*1024**2});
  process.stdout.write(result.stdout||'');process.stderr.write(result.stderr||'');
  if(result.error||result.signal||result.status!==0||!/^# tests [1-9][0-9]*$/mu.test(result.stdout||'')||!['fail','cancelled','skipped','todo'].every(name=>new RegExp('^# '+name+' 0$','mu').test(result.stdout||'')))throw Error('本仓塔塔门禁回归没有完整通过');
  return receipt;
}

// BEGIN INLINE TESTS
if(process.env.NODE_TEST_CONTEXT && process.argv.length===2 && process.argv[1]===import.meta.filename){
  const {test}=await import('node:test'),{default:assert}=await import('node:assert/strict');
  test('塔塔门禁Workflow只允许本仓push，门禁和清理同处唯一文件',()=>{
    const source=tataGateRead(new URL('../workflows/tatagate.yml',import.meta.url),'utf8');
    assert.equal(tataGateValidateWorkflow(source),true);
    for(const invalid of [source.replace('branches: [main]','branches: [other]'),source.replace('needs: [gate]','needs: [other]'),source.replace('continue-on-error: true','continue-on-error: false')])assert.throws(()=>tataGateValidateWorkflow(invalid));
  });
  test('塔塔门禁成功清旧成功、失败清旧失败，活动、未来和其它流程均保留',()=>{
    const row=(id,conclusion='success',status='completed')=>({id,run_number:id,run_attempt:1,path:'.github/workflows/tatagate.yml',event:'push',head_branch:'main',head_sha:'a'.repeat(40),repository:{full_name:tataGateOwner},created_at:'2026-01-01T00:00:00Z',status,conclusion});
    const current=row(9,null,'in_progress'),rows=[row(1),row(2,'failure'),row(3,null,'in_progress'),row(10),{...row(4),path:'.github/workflows/release-sdk.yml'},{...row(5),repository:{full_name:'example/other'}}];
    assert.deepEqual(tataGateCleanupPlan(rows,current,'success').map(x=>x.id),[1]);
    assert.deepEqual(tataGateCleanupPlan(rows,current,'failed').map(x=>x.id),[2]);
  });
  test('塔塔门禁删除逐项回查，清理失败和重跑变化均不能伪报完成',async()=>{
    const range='actions/workflows/tatagate.yml/runs?created=2008-01-01T00%3A00%3A00Z..2026-10-11T00%3A00%3A00Z';
    const empty={total_count:0,workflow_runs:[]};
    assert.deepEqual(await tataGateAPI(range,{token:'fixture',fetchImpl:async(url,options)=>{assert.ok(url.includes('created='));assert.equal(options.method,'GET');return new Response(JSON.stringify(empty));}}),empty);
    for(const path of ['../actions/runs/1','actions/%2e%2e/runs','actions/runs/1\n'])await assert.rejects(tataGateAPI(path,{token:'fixture',fetchImpl:()=>{throw Error('越界不应请求网络');}}),/参数无效/u);

    const current={id:9,run_number:9,run_attempt:1,path:'.github/workflows/tatagate.yml',event:'push',head_branch:'main',head_sha:'a'.repeat(40),repository:{full_name:tataGateOwner},created_at:'2026-01-02T00:00:00Z',status:'in_progress',conclusion:null};
    const old={...current,id:1,run_number:1,status:'completed',conclusion:'success',created_at:'2026-01-01T00:00:00Z'};
    for(const mode of ['success','readback','rerun']){
      let deleted=false;const api=async(path,options={})=>{
        if(path==='actions/runs/9')return current;
        if(path.startsWith('actions/workflows/'))return {total_count:1,workflow_runs:[old]};
        if(options.method==='DELETE'){deleted=true;return null;}
        if(path==='actions/runs/1')return mode==='rerun'?{...old,run_attempt:2}:deleted&&mode==='success'?null:old;
        throw Error('错误清理路径');
      };
      if(mode==='success')assert.deepEqual(await tataGateCleanup('success',{id:9,sha:current.head_sha},api),[1]);
      else await assert.rejects(tataGateCleanup('success',{id:9,sha:current.head_sha},api),/回查失败|已变化/u);
      if(mode==='rerun')assert.equal(deleted,false);
    }
  });
  test('GitHub门禁接受准确提交的detached检出，错仓、错SHA和错误Workflow拒绝',async()=>{
    const {mkdtempSync,mkdirSync,writeFileSync,rmSync,realpathSync}=await import('node:fs');
    const {tmpdir}=await import('node:os'),{join}=await import('node:path');
    const directory=mkdtempSync(join(realpathSync(tmpdir()),'tata-gate-context-'));
    try{
      const git=process.env.PRODUCT_GIT_BIN||'/usr/bin/git';
      const invoke=args=>tataGateExec(git,['-c','core.hooksPath=/dev/null','-c','user.name=Tata Gate Fixture','-c','user.email=fixture@example.invalid','-C',directory,...args],{encoding:'utf8'}).trim();
      invoke(['init','--quiet','--initial-branch=main']);invoke(['remote','add','origin','https://github.com/'+tataGateOwner+'.git']);
      writeFileSync(join(directory,'file'),'first');invoke(['add','file']);invoke(['commit','--quiet','-m','first']);const before=invoke(['rev-parse','HEAD']);
      writeFileSync(join(directory,'file'),'second');invoke(['add','file']);invoke(['commit','--quiet','-m','second']);const after=invoke(['rev-parse','HEAD']);
      invoke(['checkout','--quiet','--detach',after]);
      const input={GITHUB_ACTIONS:'true',GITHUB_JOB:'gate',GITHUB_EVENT_NAME:'push',GITHUB_REPOSITORY:tataGateOwner,GITHUB_REF:'refs/heads/main',GITHUB_WORKSPACE:directory,GITHUB_SHA:after,
        GITHUB_WORKFLOW_REF:tataGateOwner+'/.github/workflows/tatagate.yml@refs/heads/main',PRODUCT_GIT_BIN:git};
      const event={repository:{full_name:tataGateOwner},ref:'refs/heads/main',before,after};
      assert.equal(tataGateContext(directory,input,event).after,after);
      invoke(['remote','set-url','origin','https://github.com/'+tataGateOwner]);assert.equal(tataGateContext(directory,input,event).after,after);
      invoke(['remote','set-url','origin','https://github.com/'+tataGateOwner+'.git']);
      assert.throws(()=>tataGateContext(directory,input,{...event,before:'a'.repeat(40)}),/祖先/u);
      writeFileSync(join(directory,'late'),'new change');assert.throws(()=>tataGateContext(directory,input,event),/未提交改动/u);rmSync(join(directory,'late'));
      for(const changed of [{...input,GITHUB_SHA:before},{...input,GITHUB_REPOSITORY:'example/other'},{...input,GITHUB_WORKFLOW_REF:tataGateOwner+'/.github/workflows/release-sdk.yml@refs/heads/main'}])assert.throws(()=>tataGateContext(directory,changed,event));
    }finally{rmSync(directory,{recursive:true,force:true});}
  });

}
// END INLINE TESTS

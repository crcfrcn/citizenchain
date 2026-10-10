#!/usr/bin/env node
// 公民链独立分发准备入口：只消费已经完成的GitHub Release公开产物，不派发或重跑自动化。
// 商店等目标尚未确定；本入口输出已核实的待分发清单，不声称完成外部发布。
import {fileURLToPath} from 'node:url';
import {resolve} from 'node:path';

const repository='crcfrcn/citizenchain';
const platforms=Object.freeze({
 macos:['*.dmg'],
 windows:['*.exe'],
 'linux-arm':['*.deb','*.AppImage','*.AppImage.sig','citizenchain-node-latest-LinuxARM.json'],
 'linux-amd':['*.deb','*.AppImage','*.AppImage.sig','citizenchain-node-latest-LinuxAMD.json'],
 wasm:['citizenchain.wasm','citizenchain.compact.wasm','citizenchain.compact.compressed.wasm'],
});
const fail=message=>{throw Error('公民链分发准备：'+message);};
const sha=/^[a-f0-9]{40}$/u;
const digest=/^sha256:[a-f0-9]{64}$/u;
const positive=value=>Number.isSafeInteger(value)&&value>0;
const escaped=value=>value.replace(/[.*+?^${}()|[\]\\]/gu,'\\$&');
const matches=(name,pattern)=>pattern.startsWith('*.')?name.endsWith(pattern.slice(1))&&name.length>pattern.length-1:name===pattern;

// 本产品从已发布的Tag读取独立分发身份；自动化仍独占版本生成和Release创建。
export function publicationIdentity(platform,tag){
 if(!Object.hasOwn(platforms,platform))fail('平台未声明');
 const match=new RegExp('^citizenchain-'+escaped(platform)+'-v(.+)-r([1-9][0-9]*)-a([1-9][0-9]*)$','u').exec(tag||'');
 if(!match)fail('正式Tag身份无效');
 const [,version,runID,attempt]=match;
 if(platform==='wasm'? !/^[1-9][0-9]*$/u.test(version)||BigInt(version)>4294967295n
  : !/^(?:0|[1-9][0-9]*)\.(?:0|[1-9][0-9]*)\.(?:0|[1-9][0-9]*)$/u.test(version))fail('版本无效');
 const run_id=Number(runID),run_attempt=Number(attempt);
 if(!positive(run_id)||!positive(run_attempt))fail('运行坐标越界');
 return {platform,tag,version,run_id,run_attempt};
}

export function publicationPlan(platform,release,reference,run){
 const identity=publicationIdentity(platform,release?.tag_name);
 if(!positive(release?.id)||release.draft!==false||release.prerelease!==false||!release.published_at)fail('Release未正式发布');
 if(reference?.ref!=='refs/tags/'+identity.tag||reference.object?.type!=='commit'||!sha.test(reference.object.sha||''))fail('正式Tag提交无效');
 const workflow='.github/workflows/release-'+platform+'.yml';
 if(run?.id!==identity.run_id||run.run_attempt!==identity.run_attempt||run.status!=='completed'||run.conclusion!=='success'
  ||run.event!=='workflow_dispatch'||run.head_branch!=='main'||run.head_sha!==reference.object.sha
  ||run.repository?.full_name!==repository||![workflow,workflow+'@main'].includes(run.path))fail('自动化成功证明不属于本次Release');
 if(!Array.isArray(release.assets)||!release.assets.length)fail('正式产物缺失');
 const names=new Set(),assets=[];
 for(const asset of release.assets){
  const name=asset?.name;
  if(typeof name!=='string'||!/^[-A-Za-z0-9_.]+$/u.test(name)||name==='.'||name==='..'||names.has(name)
   ||asset.state!=='uploaded'||!positive(asset.id)||!positive(asset.size)||!digest.test(asset.digest||''))fail('正式产物身份或摘要无效');
  const expected='https://github.com/'+repository+'/releases/download/'+encodeURIComponent(identity.tag)+'/'+encodeURIComponent(name);
  if(asset.browser_download_url!==expected)fail('正式产物下载来源无效');
  names.add(name);assets.push({name,size:asset.size,sha256:asset.digest.slice(7),url:expected});
 }
 for(const pattern of platforms[identity.platform]){
  if(assets.filter(asset=>matches(asset.name,pattern)).length!==1)fail('正式产物集合缺少唯一文件：'+pattern);
 }
 return Object.freeze({schema:1,product_id:'citizenchain',repository,...identity,source_sha:reference.object.sha,
  release_id:release.id,assets:assets.sort((a,b)=>a.name.localeCompare(b.name))});
}

// 只读GitHub API；响应有界、拒绝重定向和错误身份，凭据不会进入结果或诊断。
export async function githubRead(path,{fetcher=fetch,token=process.env.GH_TOKEN}={}){
 if(!/^[a-z0-9][a-z0-9/_?.=%-]*$/u.test(path)||path.includes('..'))fail('GitHub路径无效');
 const controller=new AbortController(),timer=setTimeout(()=>controller.abort(),15000);
 try{
  const response=await fetcher('https://api.github.com/repos/'+repository+'/'+path,{method:'GET',redirect:'error',signal:controller.signal,
   headers:{Accept:'application/vnd.github+json','X-GitHub-Api-Version':'2022-11-28',...(token?{Authorization:'Bearer '+token}:{})}});
  if(!response.ok)fail('GitHub只读请求失败');
  const length=Number(response.headers?.get?.('content-length'));
  if(Number.isFinite(length)&&length>2*1024*1024)fail('GitHub响应超限');
  let bytes;
  if(response.body?.getReader){
   const reader=response.body.getReader(),chunks=[];let size=0;
   try{for(;;){const {done,value}=await reader.read();if(done)break;size+=value.byteLength;if(size>2*1024*1024)fail('GitHub响应超限');chunks.push(value);}}
   finally{reader.releaseLock();}
   bytes=Buffer.concat(chunks,size);
  }else{const text=await response.text();bytes=Buffer.from(text);if(bytes.length>2*1024*1024)fail('GitHub响应超限');}
  return JSON.parse(new TextDecoder('utf-8',{fatal:true}).decode(bytes));
 }finally{clearTimeout(timer);}
}

export async function inspectPublication(platform,tag,{read=githubRead}={}){
 const identity=publicationIdentity(platform,tag),encoded=encodeURIComponent(tag);
 const release=await read('releases/tags/'+encoded);
 const reference=await read('git/ref/tags/'+encoded);
 const run=await read('actions/runs/'+identity.run_id);
 return publicationPlan(platform,release,reference,run);
}

const direct=Boolean(process.argv[1]&&resolve(process.argv[1])===fileURLToPath(import.meta.url));
const testing=direct&&Boolean(process.env.NODE_TEST_CONTEXT)&&process.argv.length===2;
if(direct&&!testing){
 try{
  const [command,platform,tag,...extra]=process.argv.slice(2);
  if(command!=='inspect'||extra.length||!tag)fail('入口参数无效');
  process.stdout.write(JSON.stringify(await inspectPublication(platform,tag))+'\n');
 }catch(error){console.error(error.message.startsWith('公民链分发准备：')?error.message:'公民链分发准备：输入读取失败');process.exitCode=1;}
}

if(testing){
 const {default:test}=await import('node:test'),{default:assert}=await import('node:assert/strict');
 const tag='citizenchain-macos-v1.2.3-r12-a1',shaValue='a'.repeat(40),assetDigest='sha256:'+'b'.repeat(64);
 const release={id:3,tag_name:tag,draft:false,prerelease:false,published_at:'2026-10-10T00:00:00Z',assets:[{id:4,name:'citizenchain.dmg',state:'uploaded',size:8,digest:assetDigest,
  browser_download_url:'https://github.com/'+repository+'/releases/download/'+tag+'/citizenchain.dmg'}]};
 const reference={ref:'refs/tags/'+tag,object:{type:'commit',sha:shaValue}};
 const run={id:12,run_attempt:1,status:'completed',conclusion:'success',event:'workflow_dispatch',head_branch:'main',head_sha:shaValue,
  repository:{full_name:repository},path:'.github/workflows/release-macos.yml@main'};
 test('已完成的准确Release形成独立待分发清单',()=>{const result=publicationPlan('macos',release,reference,run);assert.equal(result.assets[0].sha256,'b'.repeat(64));assert.equal(result.source_sha,shaValue);});
 test('错误运行、草稿、缺件和重复产物均拒绝',()=>{
  assert.throws(()=>publicationPlan('macos',release,reference,{...run,conclusion:'failure'}),/成功证明/);
  assert.throws(()=>publicationPlan('macos',{...release,draft:true},reference,run),/未正式发布/);
  assert.throws(()=>publicationPlan('macos',{...release,assets:[]},reference,run),/产物缺失/);
  assert.throws(()=>publicationPlan('macos',{...release,assets:[...release.assets,...release.assets]},reference,run),/重复|身份/);
 });
 test('平台版本边界和只读API路径拒绝',async()=>{
  assert.throws(()=>publicationIdentity('wasm','citizenchain-wasm-v0-r1-a1'),/版本/);
  assert.throws(()=>publicationIdentity('windows',tag),/Tag/);
  await assert.rejects(githubRead('../other',{fetcher:async()=>{throw Error('不得请求');}}),/路径/);
 });
 test('独立入口只读取已完成发布证明，不派发自动化或上传',async()=>{
  const paths=[],record=await inspectPublication('macos',tag,{read:async path=>{
   paths.push(path);
   if(path.startsWith('releases/tags/'))return release;
   if(path.startsWith('git/ref/tags/'))return reference;
   if(path==='actions/runs/12')return run;
   throw Error('读取路径超出闭集');
  }});
  assert.deepEqual(paths,['releases/tags/'+tag,'git/ref/tags/'+tag,'actions/runs/12']);
  assert.equal(record.release_id,release.id);
  let called=false;
  await githubRead('releases/tags/'+tag,{fetcher:async(_url,options)=>{
   called=true;assert.equal(options.method,'GET');assert.equal(options.redirect,'error');
   return {ok:true,headers:{get:()=>null},text:async()=>'{"id":3}'};
  }});
  assert.equal(called,true);
 });
}

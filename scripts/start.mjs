#!/usr/bin/env node
import {claimFixedWork,releaseFixedWork} from './target.mjs';
const directEntry = process.argv[1] === import.meta.filename && !process.execArgv.some(value => /^(?:-e|-p|--eval|--print)(?:=|$)/u.test(value));
const inlineTestEntry = directEntry && Boolean(process.env.NODE_TEST_CONTEXT) && process.argv.length === 2;
// 本产品唯一桌面启动入口：资源、候选验证与启动自行完成；调用方只给出产物位置。
import {createHash} from 'node:crypto';
import {lstatSync,readFileSync,realpathSync,mkdtempSync,rmSync,writeSync} from 'node:fs';
import {dirname,isAbsolute,join,resolve,sep} from 'node:path';
import { temporaryRoot } from './build.mjs';
const tmpdir=()=>temporaryRoot(undefined,'tmp');
import {fileURLToPath} from 'node:url';
import {contract,outputDigest,runBuildProcess,productTarget} from './build.mjs';
import {bootstrapNode,processResources} from './resources.mjs';
const root=resolve(dirname(fileURLToPath(import.meta.url)),'..');
const fail=message=>{throw Error('产品Start：'+message);};
export function startDeclaration(platform) {
 const value=JSON.parse(readFileSync(join(root,'scripts/flows.json'),'utf8'));
 const start=value.platforms?.[platform]?.start;
 if(!['macos'].includes(platform)||value.product_id!==contract.product_id
  ||!start||start.entry!=='scripts/start.mjs'||!start.artifact?.endsWith('.app')
  ||/[\/\x00-\x1f\x7f]/u.test(start.artifact))fail('平台或启动声明无效');
 return {product:value.product_id,...start};
}
export function startArtifact(path,declared) {
 // 成功产物归本产品真实平台target；源码和其他平台目录都不能作为候选。
 const platforms=Object.entries(contract.platforms).filter(([,value])=>value.start?.artifact===declared.artifact).map(([name])=>name);
 if(platforms.length!==1)fail('成功App平台身份无效');
 const target=productTarget(platforms[0]);
 if(typeof path!=='string'||!isAbsolute(path)||resolve(path)!==path||realpathSync(path)!==path
  ||!lstatSync(path).isDirectory()||lstatSync(path).isSymbolicLink()
  ||!path.startsWith(target+sep)||path.split(sep).at(-1)!==declared.artifact)fail('成功App目录无效');
 const plist=join(path,'Contents/Info.plist'),info=lstatSync(plist);
 if(!info.isFile()||info.isSymbolicLink()||info.nlink!==1||realpathSync(plist)!==plist)fail('App声明无效');
 return path;
}
export async function start(platform,artifact,{signal,tools,run=runBuildProcess,log=message=>process.stdout.write(message)}={}) {
 const declared=startDeclaration(platform);startArtifact(artifact,declared);signal?.throwIfAborted();
 const before=outputDigest(artifact),declaration=JSON.stringify(declared);
 await run(tools.codesign,['--verify','--deep','--strict',artifact],tools.environment,root,{signal,capture:true});
 const plist=await run(tools.plutil,['-extract','CFBundleExecutable','raw','-o','-',join(artifact,'Contents/Info.plist')],tools.environment,root,{signal,capture:true});
 const name=plist.stdout.trim();if(!name||name.length>256||/[\/\x00-\x1f\x7f]/u.test(name)
  ||declared.executable!==null&&name!==declared.executable)fail('App可执行文件身份无效');
 const executable=join(artifact,'Contents/MacOS',name),metadata=lstatSync(executable);
 if(!metadata.isFile()||metadata.isSymbolicLink()||metadata.nlink!==1||!(metadata.mode&0o111)
  ||realpathSync(executable)!==executable||outputDigest(artifact)!==before)fail('启动前候选变化或入口无效');
 signal?.throwIfAborted();
 const active=await run(tools.pgrep,['-f',executable],tools.environment,root,{signal,capture:true,accepted:[0,1]});
 if(active.code===0){await run(tools.open,[artifact],tools.environment,root,{signal,capture:true});log('[Start] CitizenChain Node macOS已经运行，已激活现有窗口\n');}
 else {
  const pg=await tools.postgres();signal?.throwIfAborted();const home=tools.environment.HOME;
  if(typeof home!=='string'||!isAbsolute(home)||resolve(home)!==home)fail('数据用户目录无效');
  await run(tools.open,['-n','--env','CITIZENCHAIN_DATA_PROFILE=dev','--env','ONCHINA_EMBEDDED_PG=1',
   '--env','ONCHINA_PG_BIN_DIR='+pg,'--env','ONCHINA_PG_PORT=5433',
   '--env','ONCHINA_PG_DATA_DIR='+join(home,'Library/Application Support/gmb.dev/onchina-pgdata'),
   '--env','ONCHINA_CHINA_DB='+join(artifact,'Contents/Resources/china.sqlite'),
   '--env','ONCHINA_FRONTEND_DIST='+join(artifact,'Contents/Resources/onchina-frontend/dist'),
   '--env','ONCHINA_ENABLE_TLS=1','--env','ONCHINA_TLS_DIR='+join(home,'Library/Application Support/gmb.dev/onchina-tls'),artifact],
   tools.environment,root,{signal,capture:true});
 }
 if(outputDigest(artifact)!==before||JSON.stringify(startDeclaration(platform))!==declaration)fail('启动期间候选或声明变化');
 return {schema:1,product_id:declared.product,platform,flow:'start',artifact,status:'started'};
}
if(!inlineTestEntry&&directEntry) {
 const [command,platform,flag,artifact,...extra]=process.argv.slice(2);
 if(command!=='start'||flag!=='--artifact'||extra.length)fail('固定入口参数无效');
 startArtifact(artifact,startDeclaration(platform));
 const session=claimFixedWork('test'),work=session.owner.work;
 const cancellation=new AbortController();for(const event of ['SIGTERM','SIGINT'])process.once(event,()=>cancellation.abort());
 let unconfirmed=false;
 try {
  const publicEnv=Object.fromEntries(['HOME','USER','LOGNAME','LANG','LC_ALL','PRODUCT_TOOL_ROOT','PRODUCT_DEPENDENCY_ROOT','PRODUCT_RESULT_FD'].filter(key=>typeof process.env[key]==='string').map(key=>[key,process.env[key]]));
  publicEnv.PRODUCT_WORK_LEASE=session.owner.nonce;const options={signal:cancellation.signal,environment:publicEnv};const node=await bootstrapNode(work,options);
  if(createHash('sha256').update(readFileSync(process.execPath)).digest('hex')!==createHash('sha256').update(readFileSync(node.path)).digest('hex')) {
   const result=await runBuildProcess(node.path,[fileURLToPath(import.meta.url),...process.argv.slice(2)],publicEnv,root,{signal:cancellation.signal,capture:true,streamError:true,passHost:publicEnv.PRODUCT_RESULT_FD==='3'});
   if(publicEnv.PRODUCT_RESULT_FD!=='3')process.stdout.write(result.stdout);
  } else {
   const result=await start(platform,artifact,{signal:cancellation.signal,tools:await processResources(work,options)});
   const bytes=JSON.stringify(result)+'\n';
   if(publicEnv.PRODUCT_RESULT_FD!==undefined){if(publicEnv.PRODUCT_RESULT_FD!=='3')fail('结果通道无效');writeSync(3,bytes);}
   else process.stdout.write(bytes);
  }
 } catch(error) { unconfirmed=String(error.message).includes('退出未确认');throw error; }
 finally { releaseFixedWork(session,{unsafe:unconfirmed}); }
}

// 内嵌回归只由node --test直接运行本文件时注册，导入和正常执行不运行测试。
if(inlineTestEntry){
void (async()=>{
const {default:assert} = await import('node:assert/strict');
const {default:test} = await import('node:test');
const {mkdtempSync,mkdirSync,writeFileSync,rmSync,realpathSync,symlinkSync} = await import('node:fs');
const {join} = await import('node:path');
const { testRoot : tmpdir } = await import('./build.mjs');
const {start,startArtifact,startDeclaration} = await import('./start.mjs');
function fixture(platform) {
 const work=realpathSync(mkdtempSync(join(tmpdir(platform),'product-start-'))),declared=startDeclaration(platform);
 const artifact=join(work,declared.artifact),name=declared.executable||'ProductClient';
 mkdirSync(join(artifact,'Contents/MacOS'),{recursive:true});writeFileSync(join(artifact,'Contents/Info.plist'),'signed-info');
 const executable=join(artifact,'Contents/MacOS',name);writeFileSync(executable,'signed-code',{mode:0o700});
 return {work,artifact,executable,name};
}
const tools={codesign:'/controlled/codesign',plutil:'/controlled/plutil',open:'/controlled/open',pgrep:'/controlled/pgrep',postgres:async()=>'/controlled/postgres/bin',environment:{PATH:'/controlled',HOME:'/Users/product'}};
// 只替换外部工具边界；真实目录、签名调用順序、摘要与启动入口由产品实现验证。
for(const platform of ['macos'])test(platform+'唯一入口按验签、声明回读、启动顺序执行',async()=>{
 const f=fixture(platform),calls=[];try {
  const result=await start(platform,f.artifact,{tools,run:async(file,args)=>{calls.push([file,args]);return {code:file===tools.pgrep?1:0,stdout:file===tools.plutil?f.name+'\n':''};}});
  assert.equal(result.product_id,startDeclaration(platform).product);assert.equal(result.platform,platform);assert.equal(result.status,'started');
  assert.deepEqual(calls.map(x=>x[0]),[tools.codesign,tools.plutil,tools.pgrep,tools.open]);assert.ok(calls.at(-1)[1].includes('ONCHINA_PG_BIN_DIR=/controlled/postgres/bin'));assert.equal(calls.at(-1)[1].at(-1),f.artifact);
 }finally{rmSync(f.work,{recursive:true});}
});
test('签名失败、候选变化及取消均不得启动',async()=>{
 for(const reason of ['signature','changed','cancelled']) {
  const f=fixture('macos'),calls=[],controller=new AbortController();try {
   if(reason==='cancelled')controller.abort();
   await assert.rejects(start('macos',f.artifact,{tools,signal:controller.signal,run:async(file)=>{
    calls.push(file);if(reason==='signature')throw Error('签名失败');if(file===tools.plutil&&reason==='changed')writeFileSync(f.executable,'changed-code');
    return {code:file===tools.pgrep?1:0,stdout:file===tools.plutil?f.name+'\n':''};
   }}));assert.ok(!calls.includes(tools.open));
  }finally{rmSync(f.work,{recursive:true});}
 }
});
test('未登记平台、错误名称和链接候选在工具执行前拒绝',()=>{
 assert.throws(()=>startDeclaration('ios'));const f=fixture('macos');try {
  const link=join(f.work,'linked.app');symlinkSync(f.artifact,link);
  assert.throws(()=>startArtifact(link,startDeclaration('macos')));
  assert.throws(()=>startArtifact(f.artifact,{...startDeclaration('macos'),artifact:'other.app'}));
 }finally{rmSync(f.work,{recursive:true});}
});

// 既有窗口激活不重复准备数据库，也不改变持久路径。
test('已有节点窗口只激活，不准备PostgreSQL',async()=>{
 const f=fixture('macos'),messages=[],calls=[];try {
  await start('macos',f.artifact,{tools:{...tools,postgres:async()=>{throw Error('已有节点不能准备数据库');}},log:message=>messages.push(message),run:async(file,args)=>{calls.push([file,args]);return {code:0,stdout:file===tools.plutil?f.name+'\n':''};}});
  assert.deepEqual(calls.at(-1),[tools.open,[f.artifact]]);assert.deepEqual(messages,['[Start] CitizenChain Node macOS已经运行，已激活现有窗口\n']);
 }finally{rmSync(f.work,{recursive:true});}
});

})();
}

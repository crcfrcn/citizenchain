#!/usr/bin/env node
const directEntry = process.argv[1] === import.meta.filename && !process.execArgv.some(value => /^(?:-e|-p|--eval|--print)(?:=|$)/u.test(value));
const inlineTestEntry = directEntry && Boolean(process.env.NODE_TEST_CONTEXT) && process.argv.length === 2;
// 本产品独立拥有资源需求、工程准备与编译；公开回执仅提供验真资源，不提供执行命令。
import {spawn,spawnSync} from 'node:child_process';
import {AsyncLocalStorage} from 'node:async_hooks';
import {rmSync,rmdirSync,chmodSync,closeSync,openSync,readlinkSync,unlinkSync,copyFileSync,cpSync,existsSync,lstatSync,mkdirSync,readFileSync,readdirSync,realpathSync,renameSync,symlinkSync,writeFileSync} from 'node:fs';
import {basename,dirname,isAbsolute,join,parse,relative,resolve,sep} from 'node:path';
import {fileURLToPath,pathToFileURL} from 'node:url';
import {createHash,randomUUID} from 'node:crypto';

const root=resolve(dirname(fileURLToPath(import.meta.url)),'..');
// 本仓测试进程领取独立会话并传给子进程；正式编译进程始终使用平台固定根。
if(process.env.NODE_TEST_CONTEXT&&!process.env.CITIZENCHAIN_TEST_SESSION)process.env.CITIZENCHAIN_TEST_SESSION=randomUUID();
const testSession=process.env.NODE_TEST_CONTEXT&&/^[0-9a-f]{8}(?:-[0-9a-f]{4}){3}-[0-9a-f]{12}$/u.test(process.env.CITIZENCHAIN_TEST_SESSION||'')?process.env.CITIZENCHAIN_TEST_SESSION:null;
const installedMacosApp='/Applications/CitizenChain.app';
export const contract=Object.freeze({"schema":1,"product_id":"citizenchain","entry":"scripts/build.mjs","platforms":{"macos":{"tools":[{"id":"git","version":"2.54.0"},{"id":"node","version":"25.2.1"},{"id":"rust","version":"1.97.1"},{"id":"python","version":"3.14.3"},{"id":"protoc","version":"35.0"},{"id":"xcode","version":"27.0"},{"id":"posix","version":"27.0"},{"id":"bash","version":"5.3.20"},{"id":"grep","version":"3.12"},{"id":"sed","version":"4.10"}],"locks":[{"ecosystem":"cargo","path":"Cargo.lock"},{"ecosystem":"npm","path":"crates/scanner/package-lock.json"},{"ecosystem":"npm","path":"node/frontend/package-lock.json"},{"ecosystem":"npm","path":"onchina/frontend/package-lock.json"}],"completion":"macos-artifact","verificationPath":"Contents/MacOS/citizenchain"},"windows":{"tools":[{"id":"git","version":"2.54.0"},{"id":"node","version":"25.2.1"},{"id":"rust","version":"1.97.1"},{"id":"python","version":"3.14.3"},{"id":"protoc","version":"35.0"}],"locks":[{"ecosystem":"cargo","path":"Cargo.lock"},{"ecosystem":"npm","path":"crates/scanner/package-lock.json"},{"ecosystem":"npm","path":"node/frontend/package-lock.json"},{"ecosystem":"npm","path":"onchina/frontend/package-lock.json"}],"completion":"node-package"},"linux-arm":{"tools":[{"id":"git","version":"2.54.0"},{"id":"node","version":"25.2.1"},{"id":"rust","version":"1.97.1"},{"id":"python","version":"3.14.3"},{"id":"protoc","version":"35.0"},{"id":"bash","version":"5.3.20"},{"id":"grep","version":"3.12"},{"id":"sed","version":"4.10"}],"locks":[{"ecosystem":"cargo","path":"Cargo.lock"},{"ecosystem":"npm","path":"crates/scanner/package-lock.json"},{"ecosystem":"npm","path":"node/frontend/package-lock.json"},{"ecosystem":"npm","path":"onchina/frontend/package-lock.json"}],"completion":"node-package"},"linux-amd":{"tools":[{"id":"git","version":"2.54.0"},{"id":"node","version":"25.2.1"},{"id":"rust","version":"1.97.1"},{"id":"python","version":"3.14.3"},{"id":"protoc","version":"35.0"},{"id":"bash","version":"5.3.20"},{"id":"grep","version":"3.12"},{"id":"sed","version":"4.10"}],"locks":[{"ecosystem":"cargo","path":"Cargo.lock"},{"ecosystem":"npm","path":"crates/scanner/package-lock.json"},{"ecosystem":"npm","path":"node/frontend/package-lock.json"},{"ecosystem":"npm","path":"onchina/frontend/package-lock.json"}],"completion":"node-package"},"wasm":{"tools":[{"id":"git","version":"2.54.0"},{"id":"rust","version":"1.97.1"},{"id":"python","version":"3.14.3"},{"id":"protoc","version":"35.0"}],"locks":[{"ecosystem":"cargo","path":"Cargo.lock"}],"completion":"wasm-artifact","files":["citizenchain.wasm","citizenchain.compact.wasm","citizenchain.compact.compressed.wasm"]}},"resource_entry":"scripts/build.mjs"});
// 编译、安装与资源使用同一本仓固定工作根。
// 节点交付文件由本仓版本真源派生，本机与CI读取同一份公开声明。
for(const platform of ['macos','windows','linux-arm','linux-amd']){
 Object.defineProperty(contract.platforms[platform],'files',{enumerable:true,get:()=>nodeDeliveryFiles(platform)});
}

const targetInternals=await (async(declaredProduct)=>{
const fs=await import('node:fs');
const {basename,dirname,join,resolve,parse,relative,sep}=await import('node:path');
const {fileURLToPath}=await import('node:url');
const {randomUUID}=await import('node:crypto');
const {AsyncLocalStorage}=await import('node:async_hooks');
// 本产品的固定工作根与占用生命周期；不访问邻仓或调用方临时目录。
const root=resolve(dirname(fileURLToPath(import.meta.url)),'..');
const product=declaredProduct;
const sessions=new AsyncLocalStorage();
const scopes=new Set(['build','test']);
const fail=message=>{throw Error(product+' target：'+message);};
function fixedWork(scope){
 const testRoot=testSession?join(root,'target/test/sessions',testSession):null;
 if(scope==='test')return testRoot||join(root,'target/test');
 if(scope==='build')return join(root,'target/build');
 if(typeof scope==='string'&&scope.startsWith('build/')){
  const platform=scope.slice(6);if(Object.hasOwn(contract.platforms,platform))return join(testRoot||join(root,'target/build'),...(testRoot?['build',platform]:[platform]));
 }
 fail('工作根用途无效');
}
function isBuildWork(work){return typeof work==='string'&&Object.keys(contract.platforms).some(platform=>work===fixedWork('build/'+platform));}
function validWork(work){return work===fixedWork('test')||isBuildWork(work);}
function directory(path,create=false,allowMissing=false){
 let at=parse(path).root;
 for(const part of relative(at,path).split(sep)){
  at=join(at,part);
  if(create&&!fs.existsSync(at))try{fs.mkdirSync(at,{mode:0o700});}catch(error){if(error.code!=='EEXIST')throw error;}
  let value;try{value=fs.lstatSync(at);}catch(error){if(allowMissing&&error.code==='ENOENT')return null;throw error;}
  if(!value.isDirectory()||value.isSymbolicLink()||fs.realpathSync(at)!==at)fail('工作目录经过链接或非目录');
 }
 return fs.lstatSync(path);
}
function checkFixedWork(work,{create=false}={}){
 if(typeof work!=='string'||!validWork(work))fail('工作根只允许本产品target/build或target/test固定目录');
 directory(work,create);return work;
}
function checkScratchPath(path){
 if(typeof path!=='string'||resolve(path)!==path||![fixedWork('test'),...Object.keys(contract.platforms).map(platform=>fixedWork('build/'+platform))].some(work=>path===work||path.startsWith(work+sep)))fail('内部物化目录越出本产品固定工作根');
 directory(path);return path;
}
function fixedScratch(prefix){
 const path=resolve(prefix.replace(/-$/,''));checkScratchPath(dirname(path));
 fs.mkdirSync(path,{mode:0o700});return directory(path)&&path;
}
function assertTargetTopology(){
 const target=join(root,'target');if(!fs.existsSync(target))return;
 directory(target);
 for(const name of fs.readdirSync(target))if(!scopes.has(name)&&name!=='cache'&&name!=='node_modules')fail('target含非固定目录或根部生成文件：'+name);
 for(const name of fs.readdirSync(target))directory(join(target,name));
}
function regular(path){const value=fs.lstatSync(path);if(!value.isFile()||value.isSymbolicLink()||value.nlink!==1||value.size>65536)fail('任务标记不是准确普通文件');return value;}
function readOwner(work){const path=join(work,'.active.json');if(!fs.existsSync(path))return null;regular(path);let value;try{value=JSON.parse(fs.readFileSync(path,'utf8'));}catch{fail('任务标记损坏，禁止清场');}
 if(value.schema!==1||value.product_id!==product||value.work!==work||!Number.isSafeInteger(value.pid)||value.pid<1||typeof value.nonce!=='string'||!Array.isArray(value.groups)||!value.groups.every(pid=>Number.isSafeInteger(pid)&&pid>1))fail('任务标记身份无效');return value;
}
function alive(pid,group=false){try{process.kill(group&&process.platform!=='win32'?-pid:pid,0);return true;}catch(error){if(error.code==='ESRCH')return false;return true;}}
function writeOwner(owner){regular(join(owner.work,'.active.json'));fs.writeFileSync(join(owner.work,'.active.json'),JSON.stringify(owner)+'\n',{mode:0o600});}
function writable(path){const value=fs.lstatSync(path);if(value.isDirectory()&&!value.isSymbolicLink()){if(fs.realpathSync(path)!==path)fail('清理路径漂移');fs.chmodSync(path,value.mode|0o700);for(const name of fs.readdirSync(path))writable(join(path,name));}}
function removeTree(path){
 const state=fs.lstatSync(path);
 if(state.isSymbolicLink()){fs.unlinkSync(path);return;}
 if(state.isDirectory()){if(fs.realpathSync(path)!==path)fail('清理目录漂移');fs.chmodSync(path,state.mode|0o700);for(const name of fs.readdirSync(path))removeTree(join(path,name));fs.rmdirSync(path);return;}
 fs.unlinkSync(path);
}
// 清场由本产品确认资源供给与配方后代已经退出。
function assertSupplyExited(work){
 for(const name of ['.supply-active.json','.resource-active.json']){
  const file=join(work,name);if(!fs.existsSync(file))continue;regular(file);const record=JSON.parse(fs.readFileSync(file,'utf8'));
  if(!Number.isSafeInteger(record.pid)||record.pid<2||!Array.isArray(record.groups)||record.groups.some(pid=>!Number.isSafeInteger(pid)||pid<2))fail('资源退出记录无效');
  if((record.pid!==process.pid&&alive(record.pid))||record.groups.some(pid=>alive(pid,true)))fail('资源工具退出未确认');
 }
}
function empty(work,keep=[]){
 assertSupplyExited(work);
 const before=directory(work);
 for(const name of fs.readdirSync(work)){if(keep.includes(name))continue;const path=join(work,name);removeTree(path);}
 const after=directory(work);if(before.dev!==after.dev||before.ino!==after.ino||fs.readdirSync(work).some(name=>!keep.includes(name)))fail('固定工作目录未完全清空或被替换');
}
function short(work,action){const path=isBuildWork(work)?join(dirname(work),'.claim-'+basename(work)):join(work,'.claim.lock');try{fs.mkdirSync(path,{mode:0o700});}catch(error){if(error.code!=='EEXIST')throw error;
 const record=join(path,'owner.json');let holder=null;
 if(fs.existsSync(record)){regular(record);try{holder=JSON.parse(fs.readFileSync(record,'utf8'));}catch{fail('领取锁损坏');}}
 const active=readOwner(work);
 if(holder?(holder.work!==work||!Number.isSafeInteger(holder.pid)||alive(holder.pid)):(Date.now()-fs.lstatSync(path).mtimeMs<30000))fail('固定工作目录正在领取或收尾');
 if(active&&(alive(active.pid)||active.groups.some(pid=>alive(pid,true))))fail('固定工作目录仍有活跃进程');
 writable(path);fs.rmSync(path,{recursive:true});fs.mkdirSync(path,{mode:0o700});}
 fs.writeFileSync(join(path,'owner.json'),JSON.stringify({pid:process.pid,work})+'\n',{flag:'wx',mode:0o600});
 const before=directory(path);try{return action();}finally{const after=directory(path);if(before.dev!==after.dev||before.ino!==after.ino)fail('领取锁漂移');fs.unlinkSync(join(path,'owner.json'));fs.rmdirSync(path);}}
function clearFixedWork(work){
 checkFixedWork(work);const session=sessions.getStore(),owner=readOwner(work);
 if(owner&&!(owner.state==='retained'&&owner.pid===process.pid)&&(!session||session.owner.work!==work||session.owner.nonce!==owner.nonce))fail('固定工作目录属于其他活跃任务');
 if(owner&&owner.groups.some(pid=>alive(pid,true)))fail('工具后代退出未确认，禁止清场');
 if(fs.existsSync(join(work,'.product-build.lock')))fail('产品编译进程仍持有守卫，禁止清场');
 const value=short(work,()=>{empty(work,owner&&!(owner.state==='retained'&&owner.pid===process.pid)?['.active.json','.claim.lock']:['.claim.lock']);if(isBuildWork(work)&&fs.readdirSync(work).length===0)fs.rmdirSync(work);});return value;
}
function remoteIdentity(env){return env.GITHUB_ACTIONS==='true'&&env.GITHUB_REPOSITORY?.split('/')[1]===product&&env.GITHUB_RUN_ID&&env.GITHUB_RUN_ATTEMPT?env.GITHUB_RUN_ID+':'+env.GITHUB_RUN_ATTEMPT+':'+env.GITHUB_JOB:null;}
function claimFixedWork(scope,{environment=process.env,retain=false,run_id}={}){
 const work=checkFixedWork(fixedWork(scope),{create:true}),remote=remoteIdentity(environment),current=sessions.getStore();
 if(current?.owner.work===work){if(run_id){const owner=readOwner(work);if(owner?.nonce!==current.owner.nonce||owner.run_id&&owner.run_id!==run_id)fail('编译任务编号不一致');owner.run_id=run_id;writeOwner(owner);current.owner=owner;}return {...current,nested:true};}
 const token=environment.PRODUCT_WORK_LEASE;
 return short(work,()=>{
  const previous=readOwner(work);
  if(previous){
   if(token===previous.nonce&&alive(previous.pid))return {owner:previous,nested:true,retain};
   if(previous.groups.some(pid=>alive(pid,true)))fail('上轮工具进程仍运行，禁止领取');
   if(previous.state==='retained')fail('结果尚未由调用方消费，禁止覆盖');
   if(remote&&previous.remote===remote&&!alive(previous.pid)){
    const owner={...previous,pid:process.pid,state:'running',groups:[],nonce:randomUUID()};writeOwner(owner);return {owner,retain:true};
   }
   if(alive(previous.pid))fail('固定工作目录已有活跃任务');
  }
  if(fs.existsSync(join(work,'.product-build.lock')))fail('产品守卫尚未释放，禁止覆盖');
  empty(work,['.claim.lock']);
  const owner={schema:1,product_id:product,work,pid:process.pid,nonce:randomUUID(),groups:[],state:'running',remote,...(run_id?{run_id}:{})};
  fs.writeFileSync(join(work,'.active.json'),JSON.stringify(owner)+'\n',{flag:'wx',mode:0o600});return {owner,retain};
 });
}
function trackFixedProcess(work,pid){
 if(!pid||!validWork(work))return;
 const owner=readOwner(work);if(!owner)return;
 if(owner.pid!==process.pid&&!(alive(owner.pid)&&process.env.PRODUCT_WORK_LEASE===owner.nonce))fail('工具进程不能写入其他任务');
 if(!owner.groups.includes(pid)){owner.groups.push(pid);writeOwner(owner);}
}
function trackWorkProcess(pid){
 const session=sessions.getStore();if(!session||!pid)return;
 const owner=readOwner(session.owner.work);if(owner?.nonce!==session.owner.nonce)fail('任务所有权漂移');
 if(!owner.groups.includes(pid)){owner.groups.push(pid);writeOwner(owner);}
}
function workEnvironment(environment=process.env){
 const session=sessions.getStore();if(!session)return environment;
 const work=session.owner.work,result={...environment,PRODUCT_WORK_LEASE:session.owner.nonce};
 for(const [key,name]of Object.entries({TMPDIR:'tmp',TMP:'tmp',TEMP:'tmp',CARGO_TARGET_DIR:'cargo',CARGO_HOME:'dependencies/cargo-home',npm_config_cache:'dependencies/npm',PUB_CACHE:'dependencies/pub',GRADLE_USER_HOME:'dependencies/gradle',XDG_CACHE_HOME:'cache',XDG_CONFIG_HOME:'config',CLANG_MODULE_CACHE_PATH:'cache/clang',SWIFT_MODULECACHE_PATH:'cache/swift'})){
  const supplied=result[key];
  if(supplied!==undefined&&typeof supplied!=='string')fail('可写环境目录无效：'+key);
  const local=supplied&&(resolve(supplied)===work||resolve(supplied).startsWith(work+sep));
  result[key]=local?supplied:join(work,name);directory(resolve(result[key]),true);
 }
 return result;
}
function prepareSourceView(){
 const session=sessions.getStore();if(!session)fail('工程视图缺少固定任务');
 const project=join(session.owner.work,'source');
 if(fs.existsSync(project)){directory(project);return project;}
 const omitted=new Set(['.git','target','node_modules','build','dist','.dart_tool','.gradle','.symlinks','Pods','ephemeral','.cache','cache','tasks','tsconfig.tsbuildinfo']);
 // Node 禁止把整个根复制到其子目录；逐项复制仍沿用同一过滤规则和只读原件。
 fs.mkdirSync(project,{mode:0o700});
 for(const name of fs.readdirSync(root))fs.cpSync(join(root,name),join(project,name),{recursive:true,verbatimSymlinks:true,filter:path=>!omitted.has(path.slice(path.lastIndexOf(sep)+1))&&!['tools/shared','tools/archives','rely/objects'].some(prefix=>relative(root,path).split(sep).join('/')===prefix)});
 return directory(project)&&project;
}
function remoteStep(source,environment=process.env,shell='bash'){
 const session=sessions.getStore();if(!session)fail('远端步骤缺少固定任务');
 const project=prepareSourceView(),work=session.owner.work,prefix=product.toUpperCase();
 // 远端沿用已启动本产品入口的官方 Node，显式交付给 Rust 页面生成，不另寻工具。
 const env=workEnvironment({...environment,NODE:fs.realpathSync(process.execPath),GITHUB_WORKSPACE:project,PRODUCT_PROJECT_ROOT:project,PRODUCT_SOURCE_ROOT:root,[prefix+'_SOURCE_ROOT']:root,[prefix+'_WORK_DIR']:work,[prefix+'_PROJECT_ROOT']:project});
 const quote=value=>shell==='pwsh'?"'"+value.replaceAll("'","''")+"'":"'"+value.replaceAll("'","'\\''")+"'";
 // 包安装和前端生成在本产品工程视图进行，Rust读取原始源码并显式使用固定输出。
 let code=source.replace(/npm --prefix ([a-zA-Z0-9_./-]+)/g,(_,path)=>'npm --prefix '+quote(join(project,path)));
 code=code.replace(/node (frontend\/node_modules\/[^ \n]+)/g,(_,path)=>'node '+quote(join(project,'node',path)));
 code=code.replace(/node scripts\/([^ \n]+)/g,(_,path)=>'node '+quote(join(root,'scripts',path)));
 code=code.replaceAll('$GITHUB_WORKSPACE/scripts/','$PRODUCT_SOURCE_ROOT/scripts/').replaceAll('$env:GITHUB_WORKSPACE/scripts/','$env:PRODUCT_SOURCE_ROOT/scripts/');
 let cwd=project;
 if(/(?:^|\n)\s*(?:npm (?:ci|run|install)|flutter |dart )/.test(code))cwd=project;
 if(code.includes('fs.writeFileSync')||code.includes('.write_text(')||code.includes('.write_bytes('))cwd=project;
 return {source:code,env,cwd};
}
function retainWork(){const session=sessions.getStore();if(!session)fail('缺少当前任务');session.retain=true;}
function releaseFixedWork(session,{unsafe=false}={}){
 if(session.nested)return;
 const work=session.owner.work;
 const value=short(work,()=>{
  const owner=readOwner(work);if(owner?.nonce!==session.owner.nonce)fail('任务所有权漂移');
  const groups=owner.groups.filter(pid=>alive(pid,true));
  if(unsafe||groups.length){writeOwner({...owner,groups,state:'unsafe'});fail('工具后代退出未确认，保留守卫并禁止任务完成');}
  if(session.retain){writeOwner({...owner,groups:[],state:owner.remote?'remote':'retained'});return;}
  if(fs.existsSync(join(work,'.product-build.lock')))fail('产品编译守卫未释放，禁止完成');
  empty(work,['.claim.lock']);if(isBuildWork(work))fs.rmdirSync(work);
 });return value;
}
function withFixedWorkSync(scope,action,options={}){
 const session=claimFixedWork(scope,options);let unsafe=false;
 try{return sessions.run(session,()=>action(session.owner.work,session));}
 catch(error){unsafe=String(error?.message).includes('退出未确认');throw error;}
 finally{releaseFixedWork(session,{unsafe});}
}
async function withFixedWork(scope,action,options={}){
 const session=claimFixedWork(scope,options);let unsafe=false;
 try{return await sessions.run(session,()=>action(session.owner.work,session));}
 catch(error){unsafe=String(error?.message).includes('退出未确认');throw error;}
 finally{releaseFixedWork(session,{unsafe});}
}
// 调用方在消费结果且产品进程退出后，只能收尾这个产品的准确固定目录。
function finishFixedWork(work,{run_id,forceRemote=false}={}){
 if(!validWork(work))fail('工作根只允许本产品target/build或target/test固定目录');
 // 远端在正式链预检失败时尚未创建target；只把真正缺失的现场视为空，链接仍拒绝。
 if(!directory(work,false,true))return;
 const value=short(work,()=>{
  const owner=readOwner(work);if(run_id&&!owner)return;if(run_id&&!owner.remote&&owner.run_id!==run_id)fail('编译收尾任务编号不一致');if(owner){
   if((alive(owner.pid)&&!(owner.pid===process.pid&&owner.state==='retained'))||owner.groups.some(pid=>alive(pid,true)))fail('产品进程退出未确认');
   if(owner.remote&&!forceRemote&&owner.remote!==remoteIdentity(process.env))fail('远端任务身份不符');
  }
  if(fs.existsSync(join(work,'.product-build.lock')))fail('产品守卫尚未释放');
  if(run_id&&fs.existsSync(join(work,'build-result.json'))){regular(join(work,'build-result.json'));if(JSON.parse(fs.readFileSync(join(work,'build-result.json'),'utf8')).run_id!==run_id)fail('结果任务编号不符');}
  empty(work,['.claim.lock']);if(isBuildWork(work))fs.rmdirSync(work);
 });return value;
}
function taskScope(work){checkFixedWork(work);if(work===fixedWork('test'))return 'test';const platform=Object.keys(contract.platforms).find(name=>work===fixedWork('build/'+name));if(!platform)fail('编译平台现场无效');return 'build/'+platform;}


if(inlineTestEntry){
 const {default:test}=await import("node:test");
 const {default:assert}=await import("node:assert/strict");
 const {execFileSync}=await import("node:child_process");
const isEmpty=()=>assert.deepEqual(fs.readdirSync(fixedWork('test')),[]);

test('回归夹具使用本轮唯一现场且正式编译路径不变',()=>{
 assert.ok(testSession);
 const session=join(root,'target/test/sessions',testSession);
 assert.equal(fixedWork('test'),session);
 assert.equal(fixedWork('build/macos'),join(session,'build/macos'));
 assert.equal(taskScope(checkFixedWork(fixedWork('build/macos'),{create:true})),'build/macos');
 finishFixedWork(fixedWork('build/macos'));
 const module=join(import.meta.dirname,'build.mjs');
 const code='import {fixedWork} from '+JSON.stringify(module)+';process.stdout.write(JSON.stringify([fixedWork("test"),fixedWork("build/macos")]));';
 const formal=JSON.parse(execFileSync(process.execPath,['--input-type=module','-e',code],{env:{PATH:process.env.PATH},encoding:'utf8'}));
 assert.deepEqual(formal,[join(root,'target/test'),join(root,'target/build/macos')]);
});
test('远端预检前固定现场缺失可收尾，链接与越界仍拒绝',()=>{
 const work=fixedWork('build/wasm');
 assert.equal(fs.existsSync(work),false);
 finishFixedWork(work);
 assert.equal(fs.existsSync(work),false);
 fs.mkdirSync(dirname(work),{recursive:true});
 try{
  fs.symlinkSync(join(dirname(work),'missing'),work);
  assert.throws(()=>finishFixedWork(work),/工作目录经过链接或非目录/);
  fs.unlinkSync(work);
  fs.symlinkSync(fixedWork('test'),work);
  assert.throws(()=>finishFixedWork(work),/工作目录经过链接或非目录/);
 }finally{try{fs.unlinkSync(work);}catch(error){if(error.code!=='ENOENT')throw error;}}
 assert.throws(()=>finishFixedWork(join(dirname(work),'unexpected')),/工作根只允许/);
});
test('正式入口在领取工作根前拒绝合成阶段',()=>{
 const module=JSON.stringify(fileURLToPath(import.meta.url));
 const child=spawnSync(process.execPath,['--input-type=module','-e','import {execute} from '+module+';await execute("bad","/bad",{}, {stages:{}});'],{env:{PATH:process.env.PATH,HOME:process.env.HOME},encoding:'utf8'});
 assert.notEqual(child.status,0);assert.match(child.stderr,/合成阶段仅用于本仓测试/);
});

// 各平台并发领取自己的工作根，正常退出后只删除本平台目录。
test('平台编译现场独立领取且结束删除',async()=>{
 const platforms=Object.keys(contract.platforms).slice(0,2),joined=[];let release;const both=new Promise(resolve=>{release=resolve;});
 await Promise.all(platforms.map(platform=>withFixedWork('build/'+platform,async work=>{
  fs.writeFileSync(join(work,'platform'),platform);joined.push(platform);if(joined.length===platforms.length)release();
  await both;assert.equal(fs.readFileSync(join(work,'platform'),'utf8'),platform);
 })));
 assert.deepEqual(joined.sort(),platforms.sort());for(const platform of platforms)assert.equal(fs.existsSync(fixedWork('build/'+platform)),false);
});

test('远端步骤交付当前入口的规范Node路径并实际执行',async()=>{
 await withFixedWork('test',async()=>{
  const bound=remoteStep('node --version',{PATH:process.env.PATH,NODE:'/nonexistent/node'});
  assert.deepEqual(fs.readFileSync(join(bound.cwd,'node/frontend/metamask.mjs')),fs.readFileSync(join(root,'node/frontend/metamask.mjs')));
  assert.equal(fs.existsSync(join(bound.cwd,'target')),false);
  assert.equal(bound.env.NODE,fs.realpathSync(process.execPath));
  assert.ok(fs.lstatSync(bound.env.NODE).isFile());
  assert.equal(execFileSync(bound.env.NODE,['--version'],{cwd:bound.cwd,env:bound.env,encoding:'utf8'}).trim(),process.version);
 });isEmpty();
});
test('资源CLI进入本产品资源实现并在获取前拒绝错误身份',async()=>{
 await withFixedWork('test',async work=>{
  const result=spawnSync(process.execPath,[fileURLToPath(import.meta.url),'resources','macos','--work',work,'--offline'],{
   cwd:root,env:workEnvironment(process.env),input:JSON.stringify({schema:2}),encoding:'utf8',timeout:10000,
  });
  assert.equal(result.status,1);
  assert.match(result.stderr,/资源请求身份无效/);
  assert.doesNotMatch(result.stderr,/resources is not a function/);
 });isEmpty();
});
test('固定根拒绝任意任务目录、平台目录和外部临时根',()=>{
 for(const path of [join(root,'target'),join(root,'target/test/other'),join(root,'target/macos/test'),join(root,'target/build/run-123'),'/tmp/test'])assert.throws(()=>checkFixedWork(path),/固定目录/);
});
test('成功入口清空全部现场并保留固定目录',async()=>{
 await withFixedWork('test',async work=>{fs.mkdirSync(join(work,'dependencies'));fs.writeFileSync(join(work,'dependencies/fixture'),'input');fs.chmodSync(join(work,'dependencies'),0o555);});isEmpty();assertTargetTopology();
});
test('失败入口同样清空，不由测试代替被测入口清理',async()=>{
 await assert.rejects(withFixedWork('test',async work=>{fs.writeFileSync(join(work,'partial'),'partial');throw Error('synthetic failure');}),/synthetic failure/);isEmpty();
});
test('第二个真实进程不能领取活跃固定根或清理前一任务',async()=>{
 await withFixedWork('test',async work=>{
  fs.writeFileSync(join(work,'sentinel'),'owned');
  const module=join(import.meta.dirname,'build.mjs');
  assert.throws(()=>execFileSync(process.execPath,['--input-type=module','-e','import {withFixedWork} from '+JSON.stringify(module)+'; await withFixedWork("test",()=>{});'],{env:{PATH:process.env.PATH,NODE_TEST_CONTEXT:'1',CITIZENCHAIN_TEST_SESSION:testSession},stdio:['ignore','pipe','pipe']}),/活跃任务/);
  assert.equal(fs.readFileSync(join(work,'sentinel'),'utf8'),'owned');
 });isEmpty();
});
test('活跃标记损坏时拒绝覆盖和清理',async()=>{
 await withFixedWork('test',async work=>{const path=join(work,'.active.json'),bytes=fs.readFileSync(path);fs.writeFileSync(path,'{}');try{assert.throws(()=>finishFixedWork(work),/身份无效/);}finally{fs.writeFileSync(path,bytes);}});isEmpty();
});
test('嵌套内部步骤使用同一个任务，外层结束才清空',async()=>{
 await withFixedWork('test',async work=>{await withFixedWork('test',async inner=>{assert.equal(inner,work);fs.writeFileSync(join(work,'nested'),'owned');});assert.equal(fs.readFileSync(join(work,'nested'),'utf8'),'owned');});isEmpty();
});

test('真实工具超时和取消后停止进程组并清场',async()=>{
 const {runResourceProcess}=await import('./build.mjs');
 const run=(work,signal,timeout)=>runResourceProcess(process.execPath,['-e','setInterval(()=>{},1000)'],{cwd:work,env:{PATH:process.env.PATH,PRODUCT_WORK_DIR:work},signal,timeout});
 for(const kind of ['timeout','cancel']){await assert.rejects(withFixedWork('test',async work=>{fs.writeFileSync(join(work,'partial'),'partial');const abort=new AbortController();const timer=kind==='cancel'?setTimeout(()=>abort.abort(Error('synthetic cancel')),50):null;try{await run(work,abort.signal,kind==='timeout'?50:10000);}finally{clearTimeout(timer);}}),/超时|取消|synthetic cancel|失败/);isEmpty();}
});

test('清场删除断开的链接且不跟随链接删除其它固定根',async()=>{
 await withFixedWork('test',async testWork=>{const keep=join(testWork,'keep');fs.writeFileSync(keep,'protected');await withFixedWork('build/'+Object.keys(contract.platforms)[0],async buildWork=>{fs.symlinkSync(keep,join(buildWork,'external'));fs.symlinkSync(join(buildWork,'missing'),join(buildWork,'broken'));});assert.equal(fs.readFileSync(keep,'utf8'),'protected');assert.equal(fs.existsSync(fixedWork('build/'+Object.keys(contract.platforms)[0])),false);});isEmpty();
});

test('实际任务被强制终止后下一轮在同一固定根恢复并清场',async()=>{
 const {spawn}=await import('node:child_process');
 const module=join(import.meta.dirname,'build.mjs');
 const code='import {withFixedWork} from '+JSON.stringify(module)+';import fs from "node:fs";await withFixedWork("test",async work=>{fs.writeFileSync(work+"/interrupted","partial");process.stdout.write("ready");await new Promise(()=>{setInterval(()=>{},1000);});});';
 const child=spawn(process.execPath,['--input-type=module','-e',code],{env:{PATH:process.env.PATH,NODE_TEST_CONTEXT:'1',CITIZENCHAIN_TEST_SESSION:testSession},stdio:['ignore','pipe','pipe']});
 const finished=new Promise(resolve=>child.once('close',(code,signal)=>resolve({code,signal})));
 try{await new Promise((resolve,reject)=>{const timer=setTimeout(()=>reject(Error('任务领取超时')),5000);child.once('error',reject);child.stdout.once('data',()=>{clearTimeout(timer);resolve();});});child.kill('SIGKILL');assert.equal((await finished).signal,'SIGKILL');
  assert.equal(fs.readFileSync(join(fixedWork('test'),'interrupted'),'utf8'),'partial');
  await withFixedWork('test',async work=>{assert.equal(fs.existsSync(join(work,'interrupted')),false);fs.writeFileSync(join(work,'next'),'new task');});isEmpty();
 }finally{child.kill('SIGKILL');await finished;}
});

}

// 本产品收尾拒绝异任务与仍存活的资源后代。
if(inlineTestEntry){
 const {test}=await import('node:test'),{default:assert}=await import('node:assert/strict'),{spawn}=await import('node:child_process');
 test('收尾必须匹配本轮编号并等待资源后代退出',async()=>{
  const work=fixedWork('test');await withFixedWork('test',()=>withFixedWork('test',async()=>{fs.writeFileSync(join(work,'keep'),'owned');},{run_id:'owned-run'}),{retain:true});
  assert.throws(()=>finishFixedWork(work,{run_id:'other-run'}),/任务编号/);assert.equal(fs.readFileSync(join(work,'keep'),'utf8'),'owned');
  const child=spawn(process.execPath,['-e','setInterval(()=>{},1000)'],{detached:true,stdio:'ignore'}),closed=new Promise(resolve=>child.once('close',resolve));
  const marker=join(work,'.supply-active.json');fs.writeFileSync(marker,JSON.stringify({pid:process.pid,groups:[child.pid]}));
  try{assert.throws(()=>finishFixedWork(work,{run_id:'owned-run'}),/退出未确认/);assert.equal(fs.readFileSync(join(work,'keep'),'utf8'),'owned');}
  finally{process.kill(-child.pid,'SIGTERM');await closed;fs.writeFileSync(marker,JSON.stringify({pid:process.pid,groups:[]}));finishFixedWork(work,{run_id:'owned-run'});}
  assert.deepEqual(fs.readdirSync(work),[]);
 });
}

return {fixedWork,checkFixedWork,checkScratchPath,fixedScratch,assertTargetTopology,clearFixedWork,claimFixedWork,trackFixedProcess,trackWorkProcess,workEnvironment,prepareSourceView,remoteStep,retainWork,releaseFixedWork,withFixedWorkSync,withFixedWork,finishFixedWork,taskScope};
})(contract.product_id);
export const {fixedWork,checkFixedWork,checkScratchPath,fixedScratch,assertTargetTopology,clearFixedWork,claimFixedWork,trackFixedProcess,trackWorkProcess,workEnvironment,prepareSourceView,remoteStep,retainWork,releaseFixedWork,withFixedWorkSync,withFixedWork,finishFixedWork,taskScope}=targetInternals;
export const finishBuild=finishFixedWork;
if(testSession&&process.argv.length===2&&testFiles().some(file=>join(root,file)===resolve(process.argv[1]||'')))process.on('exit',()=>{
 // 回归进程只收尾自己的随机会话；守卫或后代仍在运行就保留现场并令测试失败。
 try{
  for(const platform of Object.keys(contract.platforms)){
   const work=fixedWork('build/'+platform);if(existsSync(work))finishFixedWork(work);
  }
  const session=fixedWork('test');if(existsSync(session)){finishFixedWork(session);rmdirSync(session);}
  const sessions=join(root,'target/test/sessions');if(existsSync(sessions)&&readdirSync(sessions).length===0)rmdirSync(sessions);
 }catch(error){process.stderr.write('公民链测试会话收尾失败：'+error.message+'\n');process.exitCode=1;}
});

const product=contract.product_id, prefix=product.toUpperCase();
// 节点版本与Runtime spec_version由本仓分别解释；不消费通用发布正文合同。
const inside=(base,path)=>{const r=relative(base,path);return r===''||!isAbsolute(r)&&r!=='..'&&!r.startsWith('..'+sep);};
const fail=message=>{throw Error(product+' Build：'+message);};
export function checkWork(work) { return checkFixedWork(work); }

// 产品自己拥有target工作边界；测试与独立入口也不借用调用方的全局缓存。
export function productTarget(platform) {
 platformContract(platform);
 return join(root,'target');
}
export function temporaryRoot(platform=Object.keys(contract.platforms)[0],scope='test',suppliedInput) {
 if(!['test','tmp','build','ci','release','publish'].includes(scope))fail('临时目录职责无效');
 platformContract(platform);const expected=fixedWork(scope==='test'?'test':'build/'+platform);
 if(suppliedInput!=null&&suppliedInput!==expected)fail('临时工作根必须是本产品固定目录');
 return checkFixedWork(expected,{create:true});
}
// 测试继承当前平台现场；独立执行没有任务身份时才选产品首个平台。
export const testRoot=platform=>{
 const workflow=String(process.env.GITHUB_WORKFLOW||'').split('.');
 const local=process.env.TMPDIR?relative(join(root,'target'),resolve(process.env.TMPDIR)).split(sep)[0]:undefined;
 const inherited=workflow[0]===product&&Object.hasOwn(contract.platforms,workflow[1])?workflow[1]
  :Object.hasOwn(contract.platforms,local)?local:undefined;
 return temporaryRoot(platform||inherited||Object.keys(contract.platforms)[0],'test');
};
// 远端Runner基础设施仍归GitHub；本产品步骤的可写临时目录归准确平台流程target。
export function remoteEnvironment(environment=process.env) {
 const [id,platform,...extra]=String(environment.GITHUB_WORKFLOW||'').split('.');
 if(id!==product||extra.length||!Object.hasOwn(contract.platforms,platform))fail('远端临时目录缺少准确产品平台流程身份');
 const temporary=temporaryRoot(platform,'build',null);
 return {...environment,RUNNER_TEMP:temporary,TMPDIR:temporary,TMP:temporary,TEMP:temporary};
}

// 展开来源根由本产品指定，调用者不识别任何产品来源名称。
export function resourceSourceRoot(name,work){checkWork(work);if(!/^[a-z][a-z0-9_]*$/u.test(name))fail('来源名称无效');return join(work,'git-sources',name);}
// 清理只针对当前执行拥有的工作根；工具全部退出后删除并回读，固定根本身保留。
export function clearWork(work) { return clearFixedWork(work); }

export function platformContract(platform) {
 if(!Object.hasOwn(contract.platforms,platform))fail('平台未声明');
 return contract.platforms[platform];
}
const sourceRoot=()=>root;
const nativePlatform=platform=>platform.endsWith('android')?'Android':platform.includes('linux-arm')?'LinuxARM':platform.includes('linux-amd')?'LinuxAMD':platform.endsWith('windows')?'Windows':'macOS';
const osPlatform=platform=>platform.includes('linux-')?'linux':platform.replace(/^(?:host|client)-/u,'');

// 本仓当前平台从自身原始Cargo/npm锁取得输入；没有另行声明的第一方Git源码包。
export function lockedSources() {
 return [];
}
export function requirements(platform,work) {
 checkWork(work);const declared=platformContract(platform);
 // QR 协议是本仓独立工作空间；其测试锁与平台构建锁在同一任务资源回执中验真。
 const protocolLock={ecosystem:'cargo',path:'crates/protocol/Cargo.lock'};
 const locks=[...declared.locks.map(value=>({...value})),...(platform==='wasm'?[]:[protocolLock])],sources=lockedSources(),archives=[];
 // 原生源归档坐标归本产品已有声明；准备后才提出展开源码的Cargo锁。
 for(const lock of locks){const file=join(root,lock.path);if(!existsSync(file)||!lstatSync(file).isFile()||lstatSync(file).isSymbolicLink())fail('原始锁缺失或带链接：'+lock.path);}
 // WASM平台使用Node执行本产品入口，但不编译Node包；复用本产品唯一登记的Node工具版本。
 const testNode=contract.platforms.macos.tools.find(value=>value.id==='node');
 if(!testNode)fail('产品测试Node声明缺失');
 const tools=declared.tools.some(value=>value.id==='node')?declared.tools:[...declared.tools,testNode];
 return {schema:1,product_id:product,platform,tools,locks,sources,archives};
}

export function resourceEnvironment(platform,work,receipt,base={}) {
 checkWork(work);const declared=platformContract(platform);
 if(!receipt||receipt.schema!==1||receipt.product_id!==product||receipt.platform!==platform||receipt.work!==work||receipt.offline!==true
  ||!receipt.tools||!receipt.dependencies||!receipt.archives)fail('资源回执身份无效');
 const env={HOME:base.HOME,USER:base.USER,LOGNAME:base.LOGNAME,LANG:'zh_CN.UTF-8',LC_ALL:'C',
  ...receipt.environment,TMPDIR:join(work,'tmp')+sep,TMP:join(work,'tmp'),TEMP:join(work,'tmp'),XDG_CACHE_HOME:join(work,'cache'),XDG_CONFIG_HOME:join(work,'config'),
  CARGO_TARGET_DIR:join(work,'work/cargo-target'),CARGO_NET_OFFLINE:'true',CARGO_INCREMENTAL:'1',
  npm_config_offline:'true',npm_config_audit:'false',npm_config_fund:'false'};
 if(base.CITIZENCHAIN_PG_DIST){const pg=base.CITIZENCHAIN_PG_DIST;if(!isAbsolute(pg)||resolve(pg)!==pg||!existsSync(join(pg,'bin'))||realpathSync(pg)!==pg)fail('PostgreSQL资源路径无效');env.CITIZENCHAIN_PG_DIST=pg;}
 for(const key of ['TAURI_SIGNING_PRIVATE_KEY','TAURI_SIGNING_PRIVATE_KEY_PASSWORD'])if(typeof base[key]==='string'&&base[key])env[key]=base[key];
 const allowedEnvironment=new Set([prefix+'_RESOURCE_MODE','PRODUCT_WORK_DIR','PRODUCT_BASH_BIN','PRODUCT_RSYNC_BIN','PATH','DEVELOPER_DIR','SDKROOT','DART_EXECUTABLE','XCODEBUILD','CODESIGN','SECURITY','XCRUN','XCODE_SELECT','XATTR','CC','CXX','SWIFT','OTOOL','INSTALL_NAME_TOOL','LIPO','MAKE','AR','RANLIB','NM','STRIP','LLVM_NM','LD','LDCXX','CARGO_TARGET_AARCH64_APPLE_DARWIN_LINKER','ANDROID_HOME','ANDROID_SDK_ROOT','ANDROID_NDK_HOME','ANDROID_USER_HOME','ANDROID_EMULATOR_HOME','GRADLE_INIT_SCRIPT','GRADLE_USER_HOME']);
 if(Object.keys(receipt.environment||{}).some(key=>!allowedEnvironment.has(key)))fail('资源回执包含未声明环境或注入变量');
 for(const tool of declared.tools) {
  const value=receipt.tools[tool.id];
  if(!value||value.version!==tool.version||typeof value.path!=='string'||!isAbsolute(value.path)||resolve(value.path)!==value.path)fail('缺少准确版本的工具：'+tool.id);
  const s=lstatSync(value.path);if(!s.isFile()||s.isSymbolicLink()||!(s.mode&0o111)||realpathSync(value.path)!==value.path)fail('工具入口必须是普通执行器：'+tool.id);
 }
 const aliases={node:'NODE',git:'GIT',flutter:'FLUTTER',rust:'RUSTC',python:'PYTHON',java:'JAVA',gradle:'GRADLE',
  cmake:'CMAKE',cocoapods:'POD',protoc:'PROTOC',zig:'ZIG','worker-build':'WORKER_BUILD','wasm-bindgen':'WASM_BINDGEN_BIN','wasm-opt':'WASM_OPT_BIN',esbuild:'ESBUILD_BIN',
  perl:'PERL',m4:'M4',bison:'BISON',flex:'FLEX',tcl:'TCLSH',gettext:'GETTEXT',openssl:'OPENSSL'};
 for(const [id,name]of Object.entries(aliases))if(receipt.tools[id])env[name]=receipt.tools[id].path;
 // POSIX旧Shell不进入正式PATH；基础工具只通过产品已验真的GNU投影交付。
 const paths=Object.entries(receipt.tools).filter(([id])=>id!=='posix').map(([,value])=>dirname(value.path));
 env.PATH=[...new Set([...paths,...(env.PATH||'').split(':')].filter(Boolean))].join(':');
 if(env.GIT)env.PRODUCT_GIT_BIN=env.GIT;
 if(env.RUSTC)env.CARGO=join(dirname(env.RUSTC),'cargo');
 if(env.FLUTTER){env.FLUTTER_ROOT=dirname(dirname(env.FLUTTER));env.DART_EXECUTABLE=join(env.FLUTTER_ROOT,'bin/cache/dart-sdk/bin/dart');}
 if(env.PYTHON)env.PYTHONHOME=dirname(dirname(env.PYTHON));
 if(env.JAVA)env.JAVA_HOME=dirname(dirname(env.JAVA));
 if(env.OPENSSL)env.TUYU_OPENSSL_PREFIX=dirname(dirname(env.OPENSSL));
 const own=receipt.dependencies.own||{};
 // 原始锁要求的目录必须显式交付，不能落入用户默认缓存。
 for(const lock of declared.locks){const key={npm:'npmCache',pub:'pubCache',cargo:'cargoHome'}[lock.ecosystem];if(key&&!own[key])fail('缺少原始锁依赖回执：'+lock.ecosystem);}
 for(const [key,name]of [['npmCache','npm_config_cache'],['pubCache','PUB_CACHE'],['cargoHome','CARGO_HOME']])if(own[key]){
  checkDependency(work,own[key]);env[name]=own[key];
 }
 env[prefix+'_WORK_DIR']=work;env[prefix+'_BUILD_WORK_DIR']=join(work,'work');env[prefix+'_DEPENDENCY_DIR']=join(work,'dependencies');
 env[prefix+'_BUILD_DIR']=join(work,'work/flutter');env[prefix+'_ARTIFACT_DIR']=work;env[prefix+'_OFFLINE']='true';
 env.BUILD_DIR=join(work,'work/flutter');env[prefix+'_NODE_BIN']=env.NODE;
 env[prefix+'_PROJECT_ROOT']=join(work,'source-view',sourceRoot().replace(/^\/+/u,''));
 env.PRODUCT_SOURCE_DIR=env[prefix+'_PROJECT_ROOT'];
 if(env.GRADLE)env[prefix+'_GRADLE_BIN']=env.GRADLE;
 env.GRADLE_USER_HOME=join(work,'dependencies/gradle');env.CP_HOME_DIR=join(work,'dependencies/cocoapods');
 env[prefix+'_PUB_OFFLINE']='true';env.GRADLE_OPTS='-Dorg.gradle.project.android.builder.sdkDownload=false';
 if(receipt.archives.native)env.CHATSERVER_NATIVE_ARCHIVE=receipt.archives.native[0].path;
 if(receipt.archives.protocol)env.CHATSERVER_PROTOCOL_ARCHIVE=receipt.archives.protocol[0].path;
 env["CITIZENCHAIN_RESOURCE_MODE"]??='independent';
 if(env["CITIZENCHAIN_RESOURCE_MODE"]==='provided'){env.PIP_NO_INDEX='1';env.COMPOSER_DISABLE_NETWORK='1';env.YARN_ENABLE_NETWORK='0';}
 const execution=executions.getStore();if(execution)execution.buildEnvironment=env;
 return env;
}
function checkDependency(work,path){if(!isAbsolute(path)||resolve(path)!==path||!inside(work,path)||path===work||!lstatSync(path).isDirectory()||realpathSync(path)!==path)fail('依赖回执越界或无效');}
// 工程输入复制到本轮真实目录，保证包解析与写入均不进入正式源码；内部链接映射到同轮副本。
export function createView(source,destination) {
 const ownTarget=source===root&&inside(join(root,'target'),destination)&&['build','test'].includes(relative(join(root,'target'),destination).split(sep)[0]);
 if(realpathSync(source)!==source||!lstatSync(source).isDirectory()||!isAbsolute(destination)||resolve(destination)!==destination||inside(source,destination)&&!ownTarget||inside(destination,source))fail('工程输入与输出边界无效');
 let parent=dirname(destination);while(!existsSync(parent))parent=dirname(parent);
 if(ownTarget){const work=[fixedWork('test'),...Object.keys(contract.platforms).map(platform=>fixedWork('build/'+platform))].find(value=>inside(value,destination));if(!work)fail('工程输出不属于本轮工作根');checkWork(work);}
 if(!lstatSync(parent).isDirectory()||realpathSync(parent)!==parent)fail('工程输出经过链接');
 if(lstatSync(destination,{throwIfNoEntry:false}))fail('本轮工程已存在');mkdirSync(destination,{recursive:true,mode:0o700});
 const generated=new Set(['.git','.dart_tool','.gradle','.symlinks','Pods','build','target','node_modules','ephemeral','.cache','.DS_Store','swiftpm','dist','tsconfig.tsbuildinfo']);
 function visit(from,to){for(const name of readdirSync(from).sort()){if(generated.has(name))continue;const a=join(from,name),b=join(to,name),s=lstatSync(a);
  if(s.isDirectory()){mkdirSync(b);visit(a,b);}else if(s.isFile()){copyFileSync(a,b);}
  else if(s.isSymbolicLink()){const target=realpathSync(a);if(!inside(source,target)||!lstatSync(target).isFile())fail('源码链接越界');symlinkSync(join(destination,relative(source,target)),b);}else fail('源码文件类型无效');
 }}visit(source,destination);return destination;
}
// 归档坐标只接受本产品当前锁；完整性在build前核验，prepare允许稍后展开的锁。
export async function checkArchives(platform,work,receipt,complete=false) {
 const requested=(await requirements(platform,work)).archives;
 const expected=new Map(requested.map(value=>[value.group+'@'+value.name,value]));const seen=new Set();
 for(const [group,items]of Object.entries(receipt.archives)){
  if(!Array.isArray(items))fail('归档回执类型无效');
  for(const item of items){const key=group+'@'+item.name,wanted=expected.get(key);
   if(!wanted||seen.has(key)||['url','version','sha256'].some(key=>item[key]!==wanted[key])||typeof item.path!=='string'||!isAbsolute(item.path)||resolve(item.path)!==item.path||!inside(work,item.path))fail('归档回执与产品锁不一致');
   seen.add(key);const info=lstatSync(item.path);if(!info.isFile()||info.isSymbolicLink()||realpathSync(item.path)!==item.path||!info.size||createHash('sha256').update(readFileSync(item.path)).digest('hex')!==wanted.sha256)fail('锁定归档原件无效');
  }
 }
 if(complete&&seen.size!==expected.size)fail('缺少产品锁定归档回执');
}
async function stageArchives(work,receipt) {
 // 归档都来自回执；先按本产品锁回读摘要，再交给现有原生准备器，缺失时禁止下载。
 for(const item of receipt.archives['sdk-native']||[]) {
  if(createHash('sha256').update(readFileSync(item.path)).digest('hex')!==item.sha256)fail('原生归档摘要漂移');
  const directory=join(work,'sdk-native/sources/archives');mkdirSync(directory,{recursive:true});
  const suffix=new URL(item.url).pathname.endsWith('.zip')?'.zip':'.tar.gz';
  const target=join(directory,item.sha256+suffix);if(!existsSync(target))copyFileSync(item.path,target);
 }
}
export async function prepare(platform,work,receipt,base) {
 const env=resourceEnvironment(platform,work,receipt,base),source=sourceRoot();
 for(const name of ['work','tmp','cache','config','dependencies','stage'])mkdirSync(join(work,name),{recursive:true,mode:0o700});
 await checkArchives(platform,work,receipt);await stageArchives(work,receipt);

 return {schema:1,product_id:product,platform,work};
}
// RocksDB的构建脚本需要同一Xcode工具链的libclang；路径只从已交付的Apple工具推导。
function checkedLibclangDirectory(env){
 const developer=env.DEVELOPER_DIR,clang=env.CC;
 if(typeof developer!=='string'||typeof clang!=='string'||!isAbsolute(developer)||!isAbsolute(clang)
  ||resolve(developer)!==developer||resolve(clang)!==clang||!clang.endsWith(sep+'bin'+sep+'clang')
  ||!inside(developer,clang))fail('节点编译缺少准确Xcode Clang入口');
 const library=join(dirname(dirname(clang)),'lib'),dylib=join(library,'libclang.dylib');
 if(!inside(developer,library))fail('libclang目录越出当前Xcode');
 for(const path of [developer,library]){
  const info=lstatSync(path);if(!info.isDirectory()||info.isSymbolicLink()||realpathSync(path)!==path)fail('Xcode库目录不是规范目录');
 }
 for(const path of [clang,dylib]){
  const info=lstatSync(path);if(!info.isFile()||info.isSymbolicLink()||info.nlink!==1||!info.size||realpathSync(path)!==path)fail('Xcode Clang或libclang不是准确普通文件');
 }
 return library;
}
function nodeCargoEnvironment(env){
 const libclang=checkedLibclangDirectory(env);
 // Rust 1.97.1在当前macOS上裁剪过程宏调试信息会生成dyld拒绝的LINKEDIT；只禁用宿主构建依赖裁剪。
 return {...env,LIBCLANG_PATH:libclang,DYLD_LIBRARY_PATH:libclang,CARGO_PROFILE_RELEASE_BUILD_OVERRIDE_STRIP:'none'};
}

function npmCLI(node) {
 const base=dirname(node),candidates=[join(base,'node_modules/npm/bin/npm-cli.js'),join(base,'../lib/node_modules/npm/bin/npm-cli.js')];
 const file=candidates.find(path=>existsSync(path)&&lstatSync(path).isFile());
 if(!file)fail('当前Node没有配套的npm CLI');return resolve(file);
}

function singleBundle(directory,suffix){
 const files=[];
 function visit(path){for(const name of readdirSync(path)){const child=join(path,name),info=lstatSync(child);if(info.isSymbolicLink())fail('节点安装包经过链接');if(info.isDirectory())visit(child);else if(info.isFile()&&name.endsWith(suffix)&&info.size>0)files.push(child);}}
 if(!existsSync(directory))fail('节点打包目录缺失：'+suffix);visit(directory);
 if(files.length!==1)fail('节点安装包不是唯一文件：'+suffix);return files[0];
}

// WASM正式编译只使用本产品已领取的固定现场；不能让Cargo回写默认target/debug。
function wasmCargoEnvironment(work,environment){
 if(work!==fixedWork('build/wasm'))fail('WASM校验不属于本平台固定工作根');
 if(Object.keys(environment).some(key=>key==='SKIP_WASM_BUILD'||/^SKIP_.+_WASM_BUILD$/u.test(key))
  ||environment.DOCS_RS!==undefined||environment.WASM_FILE!==undefined)
  fail('正式WASM编译环境含跳过或外部WASM入口');
 return {...environment,CARGO_TARGET_DIR:join(work,'work/cargo-target'),CITIZENCHAIN_SOURCE_ROOT:root,
  CITIZENCHAIN_WORK_DIR:work,WASM_BUILD_FROM_SOURCE:'1',CITIZENCHAIN_PRODUCTION_WASM_BUILD:'1'};
}
// 四端节点只编译原生程序；SDK内部开发Runtime的WASM构建也必须跳过。
function nativeCargoEnvironment(environment){
 const env={...environment};
 for(const key of ['WASM_FILE','WASM_BUILD_FROM_SOURCE','CITIZENCHAIN_PRODUCTION_WASM_BUILD','FORCE_WASM_BUILD'])delete env[key];
 for(const key of Object.keys(env))if(/^SKIP_.+_WASM_BUILD$/u.test(key))delete env[key];
 env.SKIP_WASM_BUILD='1';
 return env;
}
function platformCargoEnvironment(platform,work,environment){
 if(platform==='wasm')return wasmCargoEnvironment(work,environment);
 const env=nativeCargoEnvironment(environment);
 return platform==='macos'&&env.CC&&env.DEVELOPER_DIR?nodeCargoEnvironment(env):env;
}
async function buildWasmArtifacts(work,env){
 const cargo=env.CARGO||'cargo',target=env.CARGO_TARGET_DIR||join(work,'work/cargo-target');
 mkdirSync(target,{recursive:true});
 await runBuildProcess(cargo,['--config',join(root,'config.toml'),'build','--locked','--release','-p','citizenchain','--no-default-features','--features','std'],
  {...env,CARGO_TARGET_DIR:target,WASM_BUILD_FROM_SOURCE:'1',CITIZENCHAIN_PRODUCTION_WASM_BUILD:'1'},root);
 const source=join(target,'release/wbuild/citizenchain');
 for(const name of contract.platforms.wasm.files){const from=join(source,name),info=lstatSync(from);if(!info.isFile()||info.isSymbolicLink()||!info.size)fail('真实WASM候选缺失：'+name);copyFileSync(from,join(work,name));}
}

// 四端节点使用同一原生构建函数；调用环境只决定工具及是否允许联网准备依赖。
async function buildNodePackage(platform,work,environment){
 if(!['windows','linux-arm','linux-amd'].includes(platform))fail('节点打包平台无效');
 const expected={windows:['win32','x64'],'linux-arm':['linux','arm64'],'linux-amd':['linux','x64']}[platform];
 if(process.platform!==expected[0]||process.arch!==expected[1])fail('节点必须在对应原生宿主构建');
 const environmentForNode=nativeCargoEnvironment(environment);
 const project=prepareSourceView(),node=environmentForNode.NODE||process.execPath,cargo=environmentForNode.CARGO||'cargo';
 const target=environmentForNode.CARGO_TARGET_DIR||join(work,'work/cargo-target'),stage=join(work,'stage');
 const pg=environmentForNode.CITIZENCHAIN_PG_DIST||process.env.CITIZENCHAIN_PG_DIST;
 if(typeof pg!=='string'||!isAbsolute(pg)||!existsSync(join(pg,'bin'))||realpathSync(pg)!==pg)fail('正式节点包缺少当前平台PostgreSQL原件');
 mkdirSync(target,{recursive:true});mkdirSync(stage,{recursive:true});
 // 节点安装包沿用链上Runtime代码，打包时不重新构建正式Runtime WASM。
 const env={...environmentForNode,CARGO_TARGET_DIR:target,
  ONCHINA_FRONTEND_DIST:join(stage,'onchina-frontend'),CITIZENCHAIN_FRONTEND_DIST:join(stage,'node-frontend')};
 const npm=npmCLI(node),npmRun=(args,cwd,extra={})=>runBuildProcess(node,[npm,...args],{...env,...extra},cwd);
 const frontendRun=(args,cwd,clean)=>runBuildProcess(node,[npm,...args],clean,cwd);
 for(const name of ['crates/scanner','node/frontend','onchina/frontend'])await npmRun(['ci',...(env.npm_config_offline==='true'?['--offline']:[])],join(project,name));
 await runBuildProcess(cargo,['--config',join(project,'config.toml'),'build','--locked','--release','-p','onchina'],env,project);
 await buildFrontendIncrementally(platform,'onchina',work,join(project,'onchina/frontend'),env.ONCHINA_FRONTEND_DIST,env,frontendRun);
 await buildFrontendIncrementally(platform,'node',work,join(project,'node/frontend'),env.CITIZENCHAIN_FRONTEND_DIST,env,frontendRun);
 const resources=join(stage,'resources'),binary=process.platform==='win32'?'onchina.exe':'onchina';
 mkdirSync(join(resources,'onchina-bin'),{recursive:true});mkdirSync(join(resources,'onchina-frontend'),{recursive:true});mkdirSync(join(resources,'postgres'),{recursive:true});
 copyFileSync(join(target,'release',binary),join(resources,'onchina-bin',binary));
 cpSync(env.ONCHINA_FRONTEND_DIST,join(resources,'onchina-frontend/dist'),{recursive:true});
 const stagedPostgres=join(resources,'postgres',process.platform==='win32'?'windows':'linux');
 cpSync(pg,stagedPostgres,{recursive:true});
 const initdb=join(stagedPostgres,'bin',process.platform==='win32'?'initdb.exe':'initdb');
 if(!existsSync(initdb))fail('节点包PostgreSQL缺少initdb');
 await runBuildProcess(initdb,['-D',join(work,'pg-probe'),'-U','citizenchain','--auth-local=trust','--auth-host=trust','--encoding=UTF8','--no-locale'],
  {...env,LC_ALL:'C',LANG:'C'},project);
 const config={build:{beforeBuildCommand:null,frontendDist:env.CITIZENCHAIN_FRONTEND_DIST},
  bundle:{createUpdaterArtifacts:true,resources:{[resources+sep]:'',[join(project,'icons')+sep]:'icons/',[join(project,'onchina/src/codes/china.sqlite')]:'china.sqlite'}}};
 const tauri=join(project,'node/frontend/node_modules/@tauri-apps/cli/tauri.js');
 const bundles=platform==='windows'?'nsis':'deb,appimage';
 await runBuildProcess(node,[tauri,'build','--config',JSON.stringify(config),'--bundles',bundles,'--ci','--','--locked','--config',join(project,'config.toml')],env,join(project,'node'));
 const bundleRoot=join(target,'release/bundle');
 if(platform==='windows'){
  const installer=singleBundle(join(bundleRoot,'nsis'),'.exe');copyFileSync(installer,join(work,'CitizenChain.exe'));
  const signature=installer+'.sig',info=lstatSync(signature,{throwIfNoEntry:false});
  if(!info?.isFile()||!info.size)fail('Windows节点更新签名缺失');
  copyFileSync(signature,join(work,'CitizenChain.exe.sig'));
 }
 else{
  copyFileSync(singleBundle(join(bundleRoot,'deb'),'.deb'),join(work,'CitizenChain.deb'));
  const appImage=singleBundle(join(bundleRoot,'appimage'),'.AppImage');
  copyFileSync(appImage,join(work,'CitizenChain.AppImage'));
  const signature=appImage+'.sig',info=lstatSync(signature,{throwIfNoEntry:false});
  if(!info?.isFile()||!info.size)fail('Linux节点更新签名缺失');
  copyFileSync(signature,join(work,'CitizenChain.AppImage.sig'));
 }
}

// 版本只从所属节点配置读取；Workflow标题和交付文件名均是该值的展示投影。
function nodeDeliveryVersion(){
 const version=JSON.parse(readFileSync(join(root,'node/tauri.conf.json'),'utf8')).version;
 if(typeof version!=='string'||!/^(?:0|[1-9][0-9]*)\.(?:0|[1-9][0-9]*)\.(?:0|[1-9][0-9]*)$/u.test(version))fail('节点版本无效');
 return version;
}
function nodeDeliveryName(platform,version=nodeDeliveryVersion()){
 const display={macos:'macOS',windows:'Windows','linux-arm':'LinuxARM','linux-amd':'LinuxAMD'}[platform];
 if(!display||typeof version!=='string'||!/^(?:0|[1-9][0-9]*)\.(?:0|[1-9][0-9]*)\.(?:0|[1-9][0-9]*)$/u.test(version))fail('节点交付名称无效');
 return `公民链·${display}-v${version}`;
}
function nodeDeliveryFiles(platform){
 const extensions={macos:['.app'],windows:['.exe','.exe.sig'],'linux-arm':['.deb','.AppImage','.AppImage.sig'],'linux-amd':['.deb','.AppImage','.AppImage.sig']}[platform];
 if(!extensions)fail('节点交付平台无效');
 const name=nodeDeliveryName(platform);return extensions.map(extension=>name+extension);
}
function githubReleaseFiles(platform,version){
 if(platform==='wasm')return [...contract.platforms.wasm.files];
 const extensions={macos:['.dmg','.app.tar.gz','.app.tar.gz.sig','.json'],windows:['.exe','.exe.sig','.json'],
  'linux-arm':['.deb','.AppImage','.AppImage.sig','.json'],'linux-amd':['.deb','.AppImage','.AppImage.sig','.json']}[platform];
 if(!extensions)fail('Release平台无效');const name=nodeDeliveryName(platform,version);
 return extensions.map(extension=>name+extension);
}
function stageNodeDelivery(platform,work){
 if(platform==='wasm')return;
 const originals={macos:['CitizenChain.app'],windows:['CitizenChain.exe','CitizenChain.exe.sig'],'linux-arm':['CitizenChain.deb','CitizenChain.AppImage','CitizenChain.AppImage.sig'],'linux-amd':['CitizenChain.deb','CitizenChain.AppImage','CitizenChain.AppImage.sig']}[platform];
 const files=nodeDeliveryFiles(platform);
 for(let index=0;index<files.length;index++){
  const source=join(work,originals[index]),destination=join(work,files[index]);
  if(realpathSync(source)!==source||existsSync(destination))fail('节点交付候选缺失、经过链接或重复');
  renameSync(source,destination);
 }
}
function githubReleaseAssetUrl(tag,name){
 return `https://github.com/crcfrcn/citizenchain/releases/download/${encodeURIComponent(tag)}/${encodeURIComponent(name)}`;
}
function matchesGithubReleaseAssetUrl(tag,name,value){
 try{return new URL(value).href===githubReleaseAssetUrl(tag,name);}catch{return false;}
}
async function prepareGithubAssets(platform,work,result,environment){
 const run=environment.GITHUB_RUN_ID,attempt=environment.GITHUB_RUN_ATTEMPT;
 if(!/^[1-9][0-9]*$/u.test(run||'')||!/^[1-9][0-9]*$/u.test(attempt||''))fail('GitHub运行坐标无效');
 if(!Number.isSafeInteger(Number(run))||!Number.isSafeInteger(Number(attempt)))fail('GitHub运行坐标越界');
 const version=platform==='wasm'?(()=>{
  const values=[...readFileSync(join(root,'runtime/src/lib.rs'),'utf8').matchAll(/spec_version:\s*([0-9]+)\s*,/gu)];
  if(values.length!==1||!Number.isSafeInteger(Number(values[0][1]))||Number(values[0][1])<1)fail('WASM版本真源无效');return values[0][1];
 })():nodeDeliveryVersion();
 if(platform!=='wasm'&&!/^(?:0|[1-9][0-9]*)\.(?:0|[1-9][0-9]*)\.(?:0|[1-9][0-9]*)$/u.test(version))fail('节点版本无效');
 const tag=`citizenchain-${platform}-v${version}-r${run}-a${attempt}`;
 const directory=join(work,'release-assets');mkdirSync(directory,{mode:0o700});
 const owned=[];
 const add=(source,name)=>{const target=join(directory,name),info=lstatSync(source);if(!info.isFile()||info.isSymbolicLink()||!info.size||existsSync(target))fail('GitHub资产无效或重复：'+name);copyFileSync(source,target);owned.push(target);return target;};
 let update;
 if(platform==='wasm')for(const name of contract.platforms.wasm.files)add(join(work,name),name);
 else{
  const name=nodeDeliveryName(platform,version);
  let installer,updateAsset,signature,updateTarget;
  if(platform==='macos'){
   installer=join(work,name+'.app');
   if(!lstatSync(installer).isDirectory()||realpathSync(installer)!==installer)fail('macOS App候选无效');
   const dmg=join(work,name+'.dmg');
   await runBuildProcess('/usr/bin/hdiutil',['create','-volname','CitizenChain','-srcfolder',installer,'-format','UDZO',dmg],environment,root);
   add(dmg,name+'.dmg');
   const bundle=join(work,'work/cargo-target/release/bundle/macos');
   updateAsset=add(singleBundle(bundle,'.app.tar.gz'),name+'.app.tar.gz');
   const signatureFile=singleBundle(bundle,'.app.tar.gz.sig');
   signature=readFileSync(signatureFile,'utf8').trim();
   add(signatureFile,name+'.app.tar.gz.sig');
   updateTarget='darwin-aarch64';
  }else if(platform==='windows'){
   installer=add(join(work,name+'.exe'),name+'.exe');
   updateAsset=installer;signature=readFileSync(join(work,name+'.exe.sig'),'utf8').trim();updateTarget='windows-x86_64';
   add(join(work,name+'.exe.sig'),name+'.exe.sig');
  }else{
   installer=add(join(work,name+'.deb'),name+'.deb');
   updateAsset=add(join(work,name+'.AppImage'),name+'.AppImage');
   signature=readFileSync(join(work,name+'.AppImage.sig'),'utf8').trim();
   add(join(work,name+'.AppImage.sig'),name+'.AppImage.sig');
   updateTarget=platform==='linux-arm'?'linux-aarch64':'linux-x86_64';
  }
  if(!signature||signature.includes('\n')||signature.length>8192)fail('节点更新签名无效');
  const manifestName=name+'.json';
  const url=githubReleaseAssetUrl(tag,basename(updateAsset));
  update={version,platforms:{[updateTarget]:{signature,url}}};
  const manifest=join(directory,manifestName);writeFileSync(manifest,JSON.stringify(update)+'\n',{flag:'wx',mode:0o600});owned.push(manifest);
 }
 const assets=owned.map(path=>({name:basename(path),size:lstatSync(path).size,sha256:releaseAssetSha256(path)})).sort((a,b)=>a.name.localeCompare(b.name));
 if(JSON.stringify(assets.map(asset=>asset.name).sort())!==JSON.stringify(githubReleaseFiles(platform,version).sort()))fail('本平台Release交付集合不完整');
 const proof={schema:1,repository:'crcfrcn/citizenchain',product_id:product,platform,
  workflow:`.github/workflows/${{macos:'macos.yml',windows:'windows.yml','linux-arm':'linuxarm.yml','linux-amd':'linuxamd.yml',wasm:'wasm.yml'}[platform]}`,
  run_id:Number(run),run_attempt:Number(attempt),source_sha:environment.GITHUB_SHA,tag,version,assets};
 const notes=join(work,'release-notes.txt');writeFileSync(notes,`<!-- automation:${JSON.stringify(proof)} -->\n`,{flag:'wx',mode:0o600});
 const title=platform==='wasm'?`公民链·WASM-v${version}`:nodeDeliveryName(platform,version);
 const plan={schema:1,platform,tag,title,source_sha:environment.GITHUB_SHA,assets,directory,notes};
 writeFileSync(join(work,'release-plan.json'),JSON.stringify(plan)+'\n',{flag:'wx',mode:0o600});
 return plan;
}
// 两种编译入口复用同一协议测试；本机按已交付资源离线执行，CI仅按原锁领取依赖。
async function runQrProtocolTests(platform,work,environment,{controlled=false,runner=runBuildProcess}={}){
 if(platform==='wasm')return;
 if(!['macos','windows','linux-arm','linux-amd'].includes(platform)||work!==fixedWork('build/'+platform))
  fail('QR协议测试平台或现场无效');
 const cargo=environment.CARGO;
 // 调度模式维持绝对工具入口；原生独立入口沿用其已选择的Cargo命令。
 if(typeof cargo!=='string'||!cargo||!isAbsolute(cargo)&&(controlled||cargo!=='cargo')
  ||isAbsolute(cargo)&&(realpathSync(cargo)!==cargo||!lstatSync(cargo).isFile()||lstatSync(cargo).isSymbolicLink()))
  fail('QR协议测试缺少受控 Cargo 入口');
 const offline=environment.CITIZENCHAIN_OFFLINE==='true'||environment.CARGO_NET_OFFLINE==='true';
 await runner(cargo,['test','--manifest-path',join(root,'crates/protocol/Cargo.toml'),'--locked',...(offline?['--offline']:[])],
  {...environment,CARGO_TARGET_DIR:join(work,'work/qr-test-target')},root,{timeout:600000});
}
// 一个门槛失败立即拒绝，后续门槛及正式编译由调用方的await自然阻断。
async function runNodePrebuildTests(platform,work,environment,{controlled=false,qr=runQrProtocolTests,rpc=runDesktopRpcTests}={}){
 if((qr!==runQrProtocolTests||rpc!==runDesktopRpcTests)&&!inlineTestEntry)fail('测试门槛替身仅限本仓回归');
 await qr(platform,work,environment,{controlled});
 await rpc(platform,work,environment);
}
export async function build(platform,work,receipt,base) {
 const supplied=resourceEnvironment(platform,work,receipt,base),declared=platformContract(platform);
 const env=platformCargoEnvironment(platform,work,supplied);
 await checkArchives(platform,work,receipt,true);await stageArchives(work,receipt);
 restoreBuildCache(platform,work,env);
 await runNodePrebuildTests(platform,work,{...env,CARGO_NET_OFFLINE:'true'},{controlled:true});
 const shell=receipt.tools.bash?.path;
 if(platform==='macos'&&!shell)fail('缺少显式Shell资源');
 const project=env[prefix+'_PROJECT_ROOT'];

  env.CITIZENCHAIN_BUILD_WORK_DIR=join(work,'work');env.CITIZENCHAIN_DEPENDENCY_DIR=join(work,'dependencies');env.CITIZENCHAIN_ARTIFACT_DIR=work;
  if(platform==='wasm')await buildWasmArtifacts(work,env);
  else if(platform==='macos'){
   await runEmbeddedBuild('run',[],{...env,PRODUCT_BASH_BIN:shell});
  }else await buildNodePackage(platform,work,env);

 stageNodeDelivery(platform,work);
 const result=await completeBuild(platform,work,receipt,env);
 saveBuildCache(platform,work,env);
 return result;
}

// 每次调用拥有自己的取消和进程集合，导入API并发也不能共享执行状态。
const executions=new AsyncLocalStorage();
export async function runBuildProcess(file,args,env,cwd=root,{capture=false,input,accepted=[0],timeout=7200000,signal=executions.getStore()?.signal,passHost=false,streamError=false}={}) {
 signal?.throwIfAborted();
 return new Promise((ok,reject)=>{
  const child=spawn(file,args,{cwd,env:workEnvironment(env),detached:true,stdio:['pipe','pipe','pipe',...(passHost?[3]:[])]});
  trackWorkProcess(child.pid);
  let stdout=[],stderr=[],bytes=0,reason,settled=false;
  const target=process.platform==='win32'?child.pid:-child.pid;
  const stop=()=>{try{process.kill(target,'SIGTERM');}catch(error){if(error.code!=='ESRCH')reason='无法取消产品工具进程组';}};
  let killer;
  const terminate=()=>{stop();clearTimeout(killer);killer=setTimeout(()=>{try{process.kill(target,'SIGKILL');}catch{}},1500);};
  const forced=setTimeout(()=>{reason='产品工具超时';terminate();},timeout);forced.unref();
  const abort=()=>{reason='产品任务已取消';terminate();};
  signal?.addEventListener('abort',abort,{once:true});if(signal?.aborted)abort();
  const consume=(chunk,out)=>{bytes+=chunk.length;if(bytes>16*1024*1024){reason='产品工具输出超限';terminate();return;}out.push(chunk);if(!capture)process.stderr.write(chunk);};
  child.stdout.on('data',chunk=>consume(chunk,stdout));child.stderr.on('data',chunk=>{if(capture&&streamError)process.stderr.write(chunk);else consume(chunk,stderr);});
  child.stdin.on('error',()=>{reason='产品工具输入失败';stop();});
  child.once('error',()=>{reason='产品工具无法启动';});
  child.once('close',async(code,termination)=>{
   clearTimeout(forced);clearTimeout(killer);
   // 主进程close不代表后代退出；未退出的同组工具必须停止并确认，之后才能清理材料。
   const alive=()=>{if(!child.pid)return false;try{process.kill(target,0);return true;}catch(error){return error.code!=='ESRCH';}};
   if(alive()){reason??='产品工具退出后仍有后代';stop();for(let n=0;n<15&&alive();n++)await new Promise(r=>setTimeout(r,100));if(alive())try{process.kill(target,'SIGKILL');}catch{};for(let n=0;n<15&&alive();n++)await new Promise(r=>setTimeout(r,100));}
   if(alive()){reason='产品工具后代退出未确认，保留工作目录';const state=executions.getStore();if(state)state.unconfirmed=true;}
   signal?.removeEventListener('abort',abort);clearTimeout(killer);
   if(signal?.aborted)reason='产品任务已取消';
   if(settled)return;settled=true;
   if(reason||termination||!accepted.includes(code))reject(Error(reason||'产品工具执行失败'));
   else ok({stdout:Buffer.concat(stdout).toString('utf8'),stderr:Buffer.concat(stderr).toString('utf8'),code});
  });
  child.stdin.end(input);
 });
}
const run=async(file,args,env,cwd=root,capture=false)=>(await runBuildProcess(file,args,env,cwd,{capture})).stdout;

export function outputDigest(path) {
 const hash=createHash('sha256');const base=path;
 function visit(file){const info=lstatSync(file);const name=relative(base,file);
  if(info.isSymbolicLink()){const real=realpathSync(file);if(!inside(base,real))fail('输出链接越界');hash.update(JSON.stringify([name,'link',readlinkSync(file)])+'\n');}
  else if(info.isDirectory()){hash.update(JSON.stringify([name,'directory'])+'\n');for(const child of readdirSync(file).sort())visit(join(file,child));}
  else if(info.isFile()&&info.nlink===1){hash.update(JSON.stringify([name,'file',Boolean(info.mode&0o111),info.size])+'\n');hash.update(readFileSync(file));}
  else fail('输出包含特殊文件或硬链接');
 }visit(path);return hash.digest('hex');
}

// GitHub Release的digest只哈入上传文件原始字节；产品目录回执仍使用outputDigest的全树合同。
function rawFileSha256(path){return createHash('sha256').update(readFileSync(path)).digest('hex');}
function releaseAssetSha256(path){
 const info=lstatSync(path);
 if(!info.isFile()||info.isSymbolicLink()||info.nlink!==1||!info.size||realpathSync(path)!==path)fail('GitHub上传资产不是规范普通文件');
 return rawFileSha256(path);
}

// 本机签名安装只接受本产品显式请求，并由本产品核验结果。
const NODE_RELEASE_CANDIDATE_SOURCE="def candidate(value):\n    coordinate(value)\n    require(IDENTITY['product'] == 'citizenchain' and IDENTITY['platform'] == 'linux-arm'\n            and REPOSITORY == 'crcfrcn/citizenchain', '公民链只交付自身Linux ARM节点部署')\n    release = metadata('/releases/' + str(value['release_id']))\n    require(isinstance(release, dict) and release.get('draft') is False and release.get('prerelease') is False\n            and release.get('id') == value['release_id'] and release.get('url') == API + '/releases/' + str(value['release_id']), '节点正式版本身份无效')\n    tag = release.get('tag_name')\n    match = re.fullmatch(re.escape(IDENTITY['tagPrefix']) + r'([0-9]+[.][0-9]+[.][0-9]+)-r([1-9][0-9]*)-a([1-9][0-9]*)', str(tag))\n    require(match is not None, '节点版本不属于公民链Linux ARM公开Tag')\n    run_id, attempt = int(match[2]), int(match[3])\n    require(run_id <= 2**53 - 1 and attempt <= 2**53 - 1, '节点版本Run坐标越界')\n    reference = metadata('/git/ref/tags/' + quote(tag, safe=''))\n    require(isinstance(reference, dict) and reference.get('ref') == 'refs/tags/' + tag, '节点Tag引用身份无效')\n    obj = reference.get('object')\n    require(isinstance(obj, dict) and obj.get('type') == 'commit'\n            and re.fullmatch('[0-9a-f]{40}', str(obj.get('sha', ''))), '节点Tag没有准确提交')\n    successes, page = [], 1\n    while True:\n        runs = metadata('/actions/workflows/' + IDENTITY['workflow'] + '/runs?per_page=100&page=' + str(page))\n        require(isinstance(runs, dict) and isinstance(runs.get('workflow_runs'), list), '节点自动化记录无效')\n        successes.extend(run for run in runs['workflow_runs'] if isinstance(run, dict)\n                         and run.get('status') == 'completed' and run.get('conclusion') == 'success')\n        if len(runs['workflow_runs']) < 100:\n            break\n        page += 1\n    require(successes, '公民链Linux ARM缺少正式成功版本')\n    run = max(successes, key=lambda item: int(item.get('id', 0)))\n    require(run.get('id') == run_id and run.get('run_attempt') == attempt\n            and run.get('path') == '.github/workflows/' + IDENTITY['workflow'] and run.get('head_sha') == obj['sha']\n            and run.get('head_branch') == 'main' and run.get('event') == 'workflow_dispatch'\n            and isinstance(run.get('repository'), dict) and run['repository'].get('full_name') == REPOSITORY\n            and run.get('url') == API + '/actions/runs/' + str(run_id), '节点版本没有匹配的本仓成功自动化')\n    assets = release.get('assets')\n    require(isinstance(assets, list) and assets and all(isinstance(asset, dict) for asset in assets), '节点资产列表无效')\n    names = [asset.get('name') for asset in assets]\n    require(all(isinstance(name, str) and '/' not in name and name for name in names)\n            and len(names) == len(set(names)), '节点资产名称不唯一')\n    require(INSTALLER_NAME in names and 'citizenchain-node-latest-LinuxARM.json' in names\n            and sum(name.endswith('.AppImage') for name in names) == 1\n            and sum(name.endswith('.AppImage.sig') for name in names) == 1, '公民链Linux ARM完整运行包集合缺失')\n    for asset in assets:\n        require(type(asset.get('id')) is int and 0 < asset['id'] <= 2**53 - 1\n                and asset.get('url') == API + '/releases/assets/' + str(asset['id']) and asset.get('state') == 'uploaded'\n                and type(asset.get('size')) is int and 0 < asset['size'] <= LIMIT\n                and re.fullmatch('sha256:[0-9a-f]{64}', str(asset.get('digest', ''))), '节点GitHub正式资产不完整')\n    installer = next(asset for asset in assets if asset['name'] == INSTALLER_NAME)\n    require(installer['id'] == value['asset_id'], '节点安装资产坐标不属于本次请求')\n    return {key: installer[key] for key in ('id', 'url', 'name', 'state', 'size', 'digest')} | {\n        'release_id': release['id'], 'tag': tag, 'source_sha': obj['sha'], 'run_id': run_id}\n";
export function nodeDeploymentVerifier(){return NODE_RELEASE_CANDIDATE_SOURCE;}

// 本机签名安装只接受本产品显式请求，并由本产品核验结果。
// 本产品拥有测试清单、资源准备和执行顺序。
function testFiles(){return ['scripts/build.mjs'];}
async function runProductTests(platform,work,receipt,base){
 const env=resourceEnvironment(platform,work,receipt,base),expected=requirements(platform,work).tools.find(value=>value.id==='node'),node=receipt.tools.node;
 if(!node||node.version!==expected?.version||!isAbsolute(node.path)||realpathSync(node.path)!==node.path)fail('本仓测试缺少准确Node入口');
 // Node测试运行器自行设置测试子进程身份；预设NODE_TEST_CONTEXT会让它跳过全部文件。
 const testEnvironment={HOME:work,USER:base.USER,LOGNAME:base.LOGNAME,LANG:'C',LC_ALL:'C',PATH:env.PATH,
  PRODUCT_GIT_BIN:env.GIT};
 const files=testFiles().map(file=>join(root,file));
 for(const file of files){const info=lstatSync(file);if(!info.isFile()||info.isSymbolicLink()||realpathSync(file)!==file)fail('本仓测试文件无效');}
 const digests=files.map(file=>createHash('sha256').update(readFileSync(file)).digest('hex'));
 await runBuildProcess(node.path,['--test','--test-concurrency=1',...files],testEnvironment,root);
 if(files.some((file,index)=>createHash('sha256').update(readFileSync(file)).digest('hex')!==digests[index]))fail('本仓测试期间源码改变');
}

function localInstallRequested(request,platform,environment){
 if(request.local_install===undefined)return false;
 if(request.local_install!==true||platform!=='macos'||environment.GITHUB_ACTIONS==='true')
  fail('公民链本机安装只接受本机macOS编译任务');
 return true;
}

export async function execute(platform,work,request={},options={}) {
 if(options.stages&&!process.env.NODE_TEST_CONTEXT)fail('合成阶段仅用于本仓测试');
 checkFixedWork(work,{create:true});
 return withFixedWork(taskScope(work),()=>executeTask(platform,work,request,options),{run_id:request.run_id,environment:options.environment||process.env});
}
async function executeTask(platform,work,request={},options={}) {
 checkWork(work);platformContract(platform);
 if(!inside(productTarget(platform),work)||work===productTarget(platform))fail('执行工作根与当前产品平台不一致');
 options.signal?.throwIfAborted();
 if(!request||typeof request!=='object'||Array.isArray(request)||Object.keys(request).some(k=>!['run_id','program_digest','local_install'].includes(k))
  ||request.run_id!==undefined&&!/^[1-9][0-9]{8}$/u.test(request.run_id)||request.program_digest!==undefined&&!/^[a-f0-9]{64}$/u.test(request.program_digest)
  )fail('本仓Build只接受任务编号、程序输入记录及显式供给方式');
 const installRequested=localInstallRequested(request,platform,options.environment||process.env);
 const {local_install:_localInstall,...resourceRequest}=request;
 chmodSync(work,0o700);
 const lock=join(work,'.product-build.lock'),resultFile=join(work,'build-result.json');
 if(existsSync(resultFile))fail('本轮完整Build已有结果，禁止复用旧终态');
 const handle=openSync(lock,'wx',0o600);closeSync(handle);
 const cancellation=new AbortController(),abort=()=>cancellation.abort();options.signal?.addEventListener('abort',abort,{once:true});if(options.signal?.aborted)abort();
 const state={signal:cancellation.signal,cancellation,installRequested,unconfirmed:false,finished:false};
 try{return await executions.run(state,async()=>{
  const stages=options.stages||{requirements,resources:(...args)=>resourceInternals.resources(...args),prepare,build};
  const resourcesOptions={signal:state.signal,offline:Boolean(options.offline),environment:options.environment||process.env};

  await stages.requirements(platform,work);state.signal.throwIfAborted();
  let receipt=await stages.resources(platform,work,resourceRequest,resourcesOptions);state.signal.throwIfAborted();
  if(!options.stages)await runProductTests(platform,work,receipt,resourcesOptions.environment);
  state.signal.throwIfAborted();
  await stages.prepare(platform,work,receipt,resourcesOptions.environment);state.signal.throwIfAborted();
  await stages.requirements(platform,work);
  receipt=await stages.resources(platform,work,receipt,resourcesOptions);state.signal.throwIfAborted();
  const result=await stages.build(platform,work,receipt,resourcesOptions.environment);state.signal.throwIfAborted();
  checkBuildResult(result,platform,work,request.run_id);
  writeFileSync(resultFile,JSON.stringify(result)+'\n',{flag:'wx',mode:0o600});return result;
 });}catch(error){if(String(error?.message).includes('退出未确认'))state.unconfirmed=true;throw error;}finally{state.finished=true;options.signal?.removeEventListener('abort',abort);if(!state.unconfirmed){unlinkSync(lock);clearWork(work);}}
}
export function checkBuildResult(value,platform,work,runId) {
 const node=platformContract(platform),desktop=platform==='macos';
 if(!value||value.schema!==1||value.product_id!==product||value.platform!==platform||value.work!==work||value.run_id!==runId)
  fail('公民链本机Build请求与结果不一致');
 if(value.completion!==node.completion||!Array.isArray(value.files)
  ||value.files.length!==node.files.length||desktop&&(node.files.length!==1||node.files[0]!==nodeDeliveryName('macos')+'.app'))fail('公民链节点交付方式错误');
 const supplied=new Map(value.files.map(file=>[file.path,file]));
 if(supplied.size!==value.files.length)fail('公民链输出重复');
 for(const name of node.files){
  const path=join(work,name),file=supplied.get(path);
  if(!file||Object.keys(file).sort().join(',')!=='path,sha256'||!inside(work,path)||realpathSync(path)!==path
   ||typeof file.sha256!=='string'||file.sha256.length!==64||!/^[a-f0-9]+$/u.test(file.sha256)||outputDigest(path)!==file.sha256)fail('公民链节点输出身份或字节改变');
 }
 if(Object.keys(value).sort().join(',')!==(runId?'completion,files,platform,product_id,run_id,schema,work':'completion,files,platform,product_id,schema,work'))fail('公民链结果字段超出本机接口');
 return value;
}
async function completeBuild(platform,work,receipt,env) {
 const declared=platformContract(platform);
 if(declared.completion==='device-install')fail('本产品未声明设备安装实现');
 if(declared.completion==='macos-artifact')for(const name of declared.files)await run(env.CODESIGN,['--verify','--deep','--strict',join(work,name)],env);
 const result={schema:1,product_id:product,platform,work,completion:declared.completion,
  files:declared.files.map(name=>{const path=join(work,name);if(!inside(work,path)||realpathSync(path)!==path)fail('Build候选越界');return {path,sha256:outputDigest(path)};})};
 if(receipt.run_id)result.run_id=receipt.run_id;
 checkBuildResult(result,platform,work,receipt.run_id);
// 本机签名安装只接受本产品显式请求，并由本产品核验结果。
 if(platform==='macos'&&executions.getStore()?.installRequested)await installMacosApp(work,env);
 return result;
}

// 正式安装路径固定；测试只能使用本产品target/test中的独立夹具目录。
function checkMacosDestination(destination){
 const test=Boolean(process.env.NODE_TEST_CONTEXT)&&inside(join(root,'target/test'),destination);
 if(destination!==installedMacosApp&&!test)fail('公民链本机安装目标越界');
 if(!isAbsolute(destination)||resolve(destination)!==destination||!destination.endsWith('/CitizenChain.app'))fail('公民链安装路径无效');
 const parent=dirname(destination),info=lstatSync(parent);
 if(!info.isDirectory()||info.isSymbolicLink()||realpathSync(parent)!==parent)fail('公民链安装父目录不是准确目录');
 for(const path of [destination,destination+'.next',destination+'.old']){
  const state=lstatSync(path,{throwIfNoEntry:false});
  if(state&&(!state.isDirectory()||state.isSymbolicLink()||realpathSync(path)!==path))fail('公民链安装路径包含非普通目录');
 }
 if(existsSync(destination+'.next')||existsSync(destination+'.old'))fail('公民链上次安装事务未收口');
 return destination;
}

async function verifyMacosApp(path,expected,env){
 const info=lstatSync(path);
 if(!info.isDirectory()||info.isSymbolicLink()||realpathSync(path)!==path)fail('公民链App不是准确目录');
 for(const name of ['Contents/Info.plist','Contents/MacOS/citizenchain']){
  const file=join(path,name),state=lstatSync(file);
  if(!state.isFile()||state.isSymbolicLink()||state.nlink!==1||realpathSync(file)!==file||!state.size)fail('公民链App入口或声明无效');
 }
 const digest=outputDigest(path);
 if(expected!==null&&digest!==expected)fail('公民链安装候选字节漂移');
 if(typeof env.CODESIGN!=='string'||!isAbsolute(env.CODESIGN))fail('公民链安装缺少验真的签名工具');
 await runBuildProcess(env.CODESIGN,['--verify','--deep','--strict',path],env,root,{capture:true,timeout:120000});
 const details=await runBuildProcess(env.CODESIGN,['-dv','--verbose=4',path],env,root,{capture:true,timeout:120000});
 const identity=details.stdout+'\n'+details.stderr;
 if(!/^Identifier=macOS\.citizenappchain$/mu.test(identity)||!/^TeamIdentifier=MHYMVRN6FC$/mu.test(identity)
  ||!/^Authority=Developer ID Application: WEI CHENG \(MHYMVRN6FC\)$/mu.test(identity)||!/^Timestamp=/mu.test(identity))fail('公民链App签名身份无效');
 const plist=await runBuildProcess('/usr/bin/plutil',['-extract','CFBundleIdentifier','raw','-o','-',join(path,'Contents/Info.plist')],env,root,{capture:true,timeout:120000});
 if(plist.stdout.trim()!=='macOS.citizenappchain'||outputDigest(path)!==digest)fail('公民链App标识或验签后字节漂移');
 return digest;
}

async function installMacosApp(work,env,{destination=installedMacosApp,verify=verifyMacosApp}={}){
 const fixture=Boolean(process.env.NODE_TEST_CONTEXT)&&inside(join(root,'target/test'),destination);
 if(!fixture&&(process.platform!=='darwin'||process.arch!=='arm64'))fail('公民链本机安装只支持macOS ARM');
 checkMacosDestination(destination);
 if(fixture){
  if(!inside(join(root,'target/test'),work)||realpathSync(work)!==work)fail('公民链安装测试现场越界');
 }else checkWork(work);
 const candidate=join(work,nodeDeliveryName('macos')+'.app'),next=destination+'.next',old=destination+'.old';
 if(!inside(work,candidate)||realpathSync(candidate)!==candidate)fail('公民链安装候选不属于当前Build');
 const digest=await verify(candidate,null,env),previous=existsSync(destination);
 const previousDigest=previous?await verify(destination,null,env):null;
 let movedOld=false,movedNew=false;
 try{
  cpSync(candidate,next,{recursive:true,force:false,errorOnExist:true,verbatimSymlinks:true,preserveTimestamps:true});
  await verify(next,digest,env);
  executions.getStore()?.signal?.throwIfAborted();
  if(previous?outputDigest(destination)!==previousDigest:lstatSync(destination,{throwIfNoEntry:false}))fail('公民链原安装在换入前发生变化');
  if(previous){renameSync(destination,old);movedOld=true;}
  renameSync(next,destination);movedNew=true;
  await verify(destination,digest,env);
 }catch(error){
  let rollback;
  try{
   if(movedNew){if(outputDigest(destination)!==digest)fail('公民链安装回滚前候选发生变化');rmSync(destination,{recursive:true,force:true});}
   if(movedOld)renameSync(old,destination);
   if(existsSync(next))rmSync(next,{recursive:true,force:true});
  }catch(failure){rollback=failure;}
  throw rollback?new Error(error.message+'；公民链安装回滚未确认：'+rollback.message):error;
 }
 if(movedOld)rmSync(old,{recursive:true,force:true});
 if(outputDigest(destination)!==digest)fail('公民链安装完成后字节漂移');
 process.stderr.write('公民链macOS App已安装并回读：'+destination+'\n');
 return {path:destination,sha256:digest};
}

// 模块先完成初始化，资源模块才能反向导入本文件的唯一校验；异步CLI在独立Promise中执行。
async function editorCommand(values){
 let pid,objects;
 for(let at=0;at<values.length;at+=2){const[key,value]=values.slice(at,at+2);if(key==='--pid'&&/^[1-9][0-9]*$/.test(value||''))pid=Number(value);else if(key==='--objects'&&value&&isAbsolute(value)&&resolve(value)===value)objects=value;else fail('编辑器会话参数无效');}
 if(!Number.isSafeInteger(pid)||pid<2||pid===process.pid)fail('编辑器会话必须绑定真实编辑器进程');
 const alive=()=>{try{process.kill(pid,0);return true;}catch(error){if(error.code==='ESRCH')return false;throw error;}};
 if(!alive())fail('编辑器进程已退出');
 const cancellation=new AbortController(),cancel=()=>cancellation.abort(Error('编辑器会话已关闭'));
 for(const name of ['SIGTERM','SIGINT'])process.once(name,cancel);
 try{return await withFixedWork('test',async work=>{
  const {editorResources}=resourceInternals;await editorResources(work,{objects,signal:cancellation.signal});
  console.log('公民链编辑器依赖已就绪：target/node_modules；本轮 target/test 随即清理');
 });}finally{for(const name of ['SIGTERM','SIGINT'])process.removeListener(name,cancel);}
}

// 本产品在独立编译与调度编译中均清理自己的生成物。
export function cleanBuildPath(path,options={},environment=executions.getStore()?.buildEnvironment||process.env){
 const work=environment.PRODUCT_WORK_DIR||[...Object.keys(contract.platforms).map(platform=>fixedWork('build/'+platform)),fixedWork('test')].find(work=>path?.startsWith(work+sep));if(typeof work!=='string'||typeof path!=='string'||resolve(path)!==path||!path.startsWith(work+sep))fail('编译清理路径越界');
 checkFixedWork(work);let parent=dirname(path);while(!existsSync(parent))parent=dirname(parent);if(realpathSync(parent)!==parent)fail('编译清理父目录经过链接');
 rmSync(path,options);
}
export function cleanShellPaths(args,environment=process.env){
 const paths=args.filter(value=>!value.startsWith('-')),options={recursive:args.some(value=>/^-[^-]*[rR]/u.test(value)),force:args.some(value=>/^-[^-]*f/u.test(value))};if(!paths.length)fail('清理路径缺失');for(const path of paths)cleanBuildPath(resolve(path),options,environment);
}

async function assertFreshGithubVersion(platform,environment,{fetchImpl=fetch}={}){
 const current=platform==='wasm'?(()=>{const values=[...readFileSync(join(root,'runtime/src/lib.rs'),'utf8').matchAll(/^\s*spec_version:\s*([0-9]+)\s*,\s*$/gmu)];
  if(values.length!==1)return fail('WASM版本真源不唯一');return values[0][1];})()
  :JSON.parse(readFileSync(join(root,'node/tauri.conf.json'),'utf8')).version;
 const valid=platform==='wasm'?/^[1-9][0-9]*$/u:/^(?:0|[1-9][0-9]*)\.(?:0|[1-9][0-9]*)\.(?:0|[1-9][0-9]*)$/u;
 if(!valid.test(current))fail('正式版本无效');
 const rank=value=>platform==='wasm'?[BigInt(value)]:value.split('.').map(BigInt);
 const greater=(a,b)=>{const left=rank(a),right=rank(b);for(let index=0;index<left.length;index++){if(left[index]!==right[index])return left[index]>right[index];}return false;};
 const token=environment.GH_TOKEN||environment.GITHUB_TOKEN;
 if(!token)fail('缺少GitHub版本核验令牌');
 const pattern=new RegExp('^citizenchain-'+platform+'-v('+ (platform==='wasm'?'[1-9][0-9]*':'[0-9]+\\.[0-9]+\\.[0-9]+') +')-r[1-9][0-9]*-a[1-9][0-9]*$','u');
 for(let page=1;page<=100;page++){
  const response=await fetchImpl(`https://api.github.com/repos/crcfrcn/citizenchain/releases?per_page=100&page=${page}`,
   {redirect:'error',signal:AbortSignal.timeout(15000),headers:{Accept:'application/vnd.github+json',Authorization:'Bearer '+token,'X-GitHub-Api-Version':'2022-11-28'}});
  if(!response.ok)fail('正式版本历史读取失败');
  const rows=await response.json();if(!Array.isArray(rows))fail('正式版本历史格式无效');
  for(const release of rows){if(release?.draft||release?.prerelease)continue;const previous=pattern.exec(release?.tag_name||'')?.[1];
   if(previous&&!greater(current,previous))fail('当前版本没有高于本平台已发布版本');}
  if(rows.length<100)return current;
 }
 fail('正式版本历史分页超限');
}

// 节点四平台在本产品原有Cargo现场先验收桌面证书与链上中国连接，失败不进入编译。
async function runDesktopRpcTests(platform,work,environment){
 if(platform==='wasm')return;
 if(!['macos','windows','linux-arm','linux-amd'].includes(platform))fail('桌面RPC测试平台无效');
 const cargo=environment.CARGO||'cargo';
 const testTarget=join(work,'work/cargo-target');
 mkdirSync(testTarget,{recursive:true});
 let env={...environment,CARGO_TARGET_DIR:testTarget,CITIZENCHAIN_SOURCE_ROOT:root,CITIZENCHAIN_WORK_DIR:work,
  TAURI_CONFIG:JSON.stringify({build:{frontendDist:join(root,'node/frontend')}})};
 const offline=env.CITIZENCHAIN_OFFLINE==='true'?['--offline']:[];
 const checkpointed=new Set();
 for(const [name,filter] of [
  ['onchina','core::chain_url::tests::desktop_chain_certificate_is_bound_to_loopback_and_valid_hex'],
  ['node','desktop::node_runner::tests::startup_disconnect_reports_saved_thread_failure'],
  ['node','desktop::node_runner::tests::essential_task_failure_reports_rpc_reason_when_available'],
  ['node','desktop::node_runner::tests::task_manager_runtime_shutdown_releases_database_before_reopen'],
  ['node','desktop::node_runner::tests::essential_failure_keeps_original_reason_after_runtime_shutdown'],
  ['node','desktop::node_runner::tests::cleanup_panic_is_appended_without_overwriting_first_error'],
  ['node','core::service::tests::cpu_miner_shutdown_joins_idle_and_busy_threads_and_releases_resources'],
  ['node','core::service::tests::rpc_stage_preserves_original_error_chain'],
  ['node','core::rpc_tls::tests::missing_relative_invalid_and_mismatched_credentials_are_rejected'],
  ['node','core::rpc_tls::tests::https_wss_certificate_validation_safe_methods_and_plaintext_rejection'],
  ['node','shared::rpc::tests::desktop_rpc_url_uses_own_loopback_identity'],
  ['node','settings::desktop_update::update_tests::delivery_manifest_name_and_url_bind_platform_version_and_encoded_path'],
  ['node','core::service::tests::ethereum_rpc_requires_runtime_revive_after_genesis'],
  ['node','core::node_guard::runtime_policy::tests::node_fixed_fee_values_are_exact'],
  ['node','core::node_guard::runtime_policy::tests::rates_above_zero_point_one_percent_are_rejected'],
  ['node','core::node_guard::runtime_policy::tests::actual_onchain_fee_event_must_match_fixed_formula'],
  ['node','core::node_guard::runtime_policy::tests::actual_offchain_settlement_uses_recipient_rate_and_one_fen_minimum'],
  ['node','core::node_guard::runtime_policy::tests::upgrade_block_events_use_parent_runtime_schema_without_skipping_fee_checks'],
  ['node','core::node_guard::runtime_policy::tests::cid_storage_keys_use_compact_len_prefix'],
 ]){
  const result=await runBuildProcess(cargo,['--config',join(root,'config.toml'),'test','--locked','--release','--quiet',...offline,'-p',name,filter,'--','--exact'],env,root,{capture:true,streamError:true});
  if(!/test result: ok\. 1 passed; 0 failed; 0 ignored;/u.test(result.stdout))fail('桌面RPC定向测试未准确运行：'+filter);
  process.stderr.write('桌面RPC定向测试通过：'+filter+'\n');
  // 每个Cargo包首次成功编译后留下验真中间物，后续测试或打包失败也能增量复用。
  if(!checkpointed.has(name)){saveBuildCache(platform,work,env);checkpointed.add(name);}
 }
}

// 各平台的持久编译缓存与本轮固定现场分离；仅复制Cargo中间物，不缓存签名包或发布资产。
const cacheParts=Object.freeze(['.fingerprint','build','deps','incremental']);
function platformCacheRoot(platform){
 platformContract(platform);
 return testSession?join(root,'target/test/sessions',testSession,'cache',platform):join(root,'target/cache',platform);
}
function cacheIdentity(platform,environment){
 const declared=platformContract(platform),lockDigest=declared.locks.map(item=>{
  const path=join(root,item.path);
  return [item.path,createHash('sha256').update(readFileSync(path)).digest('hex')];
 });
 const values={schema:1,platform,source_root:root,host:process.platform,arch:process.arch,
  tools:declared.tools,locks:lockDigest,config:createHash('sha256').update(readFileSync(join(root,'config.toml'))).digest('hex'),
  runner_image:environment.ImageVersion||'',runner_os:environment.ImageOS||'',
  rustflags:environment.RUSTFLAGS||'',encoded_rustflags:environment.CARGO_ENCODED_RUSTFLAGS||'',
  wasm_flags:environment.WASM_BUILD_RUSTFLAGS||'',incremental:'1'};
 // 不触碰既有WASM缓存身份；原生缓存必须区分SDK是否编译了附带WASM。
 if(platform!=='wasm'){
  values.native_wasm_skip=environment.SKIP_WASM_BUILD||'';
  if(platform==='macos')values.build_script_strip=environment.CARGO_PROFILE_RELEASE_BUILD_OVERRIDE_STRIP||'';
 }
 return createHash('sha256').update(JSON.stringify(values)).digest('hex');
}
function checkedCacheDirectory(path,{create=false}={}){
 if(!isAbsolute(path)||resolve(path)!==path||!inside(join(root,'target'),path))fail('平台缓存目录越界');
 const parts=relative(root,path).split(sep);let current=root;
 for(const part of parts){
  current=join(current,part);
  if(create&&!existsSync(current))try{mkdirSync(current,{mode:0o700});}catch(error){if(error.code!=='EEXIST')throw error;}
  const state=lstatSync(current,{throwIfNoEntry:false});
  if(!state)return false;
  if(!state.isDirectory()||state.isSymbolicLink()||realpathSync(current)!==current)fail('平台缓存目录含链接或非目录');
 }
 return true;
}
function validateCacheSnapshot(snapshot){
 if(!checkedCacheDirectory(snapshot))fail('平台缓存快照目录缺失');
 const profiles=new Set(['debug','release']),parts=new Set(cacheParts);
 function visit(path,depth){
  for(const name of readdirSync(path)){
   if(depth===0&&!profiles.has(name)||depth===1&&!parts.has(name))fail('平台缓存包含未声明路径');
   const child=join(path,name),state=lstatSync(child);
   if(state.isSymbolicLink()||realpathSync(child)!==child||state.isFile()&&state.nlink!==1)fail('平台缓存包含链接');
   if(state.isDirectory())visit(child,depth+1);
   else if(depth<2)fail('平台缓存层级不是目录');
   else if(!state.isFile())fail('平台缓存包含特殊文件');
  }
 }
 visit(snapshot,0);
}
function verifiedBuildCacheSnapshot(platform,environment){
 const cache=platformCacheRoot(platform),current=join(cache,'current');
 if(!checkedCacheDirectory(cache)||!checkedCacheDirectory(current))return null;
 if(readdirSync(current).sort().join(',')!=='manifest.json,snapshot')fail('平台缓存包含未验真条目');
 const metadata=join(current,'manifest.json'),snapshot=join(current,'snapshot');
 if(!existsSync(metadata)||!checkedCacheDirectory(snapshot))fail('平台缓存快照不完整');
 const state=lstatSync(metadata);
 if(!state.isFile()||state.isSymbolicLink()||state.nlink!==1||state.size>65536)fail('平台缓存清单无效');
 const manifest=JSON.parse(readFileSync(metadata,'utf8'));
 if(manifest.schema!==1||manifest.platform!==platform||typeof manifest.identity!=='string'
  ||!/^[a-f0-9]{64}$/u.test(manifest.digest||''))fail('平台缓存身份无效');
 if(manifest.identity!==cacheIdentity(platform,environment))return null;
 validateCacheSnapshot(snapshot);
 if(outputDigest(snapshot)!==manifest.digest)fail('平台缓存字节漂移');
 return snapshot;
}
function verifiedPlatformCacheForUpload(platform,environment){
 if(!verifiedBuildCacheSnapshot(platform,environment))return false;
 const cache=platformCacheRoot(platform);
 for(const name of readdirSync(cache)){
  if(name==='current')continue;
  if(name!=='frontend')fail('平台缓存包含未验真目录');
  const frontend=join(cache,name);
  if(!checkedCacheDirectory(frontend))fail('前端缓存目录无效');
  for(const component of readdirSync(frontend)){
   if(!['node','onchina'].includes(component))fail('前端缓存组件无效');
   const base=join(frontend,component);
   if(!checkedCacheDirectory(base))fail('前端缓存组件目录无效');
   const entries=readdirSync(base);
   if(entries.length!==1||entries[0]!=='current')fail('前端缓存包含未验真候选');
   const current=join(base,'current'),manifestPath=join(current,'manifest.json'),dist=join(current,'dist');
   if(!checkedCacheDirectory(current))fail('前端缓存当前目录无效');
   if(readdirSync(current).sort().join(',')!=='dist,manifest.json')fail('前端缓存包含未验真条目');
   if(!checkedCacheDirectory(dist))fail('前端缓存快照缺失');
   const state=lstatSync(manifestPath,{throwIfNoEntry:false});
   if(!state?.isFile()||state.isSymbolicLink()||state.nlink!==1||state.size>65536)
    fail('前端缓存清单无效');
   const manifest=JSON.parse(readFileSync(manifestPath,'utf8'));
   if(manifest.schema!==1||manifest.platform!==platform||manifest.name!==component
    ||!/^[a-f0-9]{64}$/u.test(manifest.identity||'')
    ||!/^[a-f0-9]{64}$/u.test(manifest.digest||''))fail('前端缓存身份无效');
   validateFrontendTree(dist);
   if(outputDigest(dist)!==manifest.digest)fail('前端缓存字节漂移');
  }
 }
 return true;
}
function restoreBuildCache(platform,work,environment){
 if(work!==fixedWork('build/'+platform))fail('平台缓存与任务现场不匹配');
 const snapshot=verifiedBuildCacheSnapshot(platform,environment);
 if(!snapshot)return false;
 const target=join(work,'work/cargo-target');
 if(existsSync(target))fail('平台缓存导入目标已存在');
 mkdirSync(dirname(target),{recursive:true});
 cpSync(snapshot,target,{recursive:true,errorOnExist:true,force:false});
 process.stderr.write('公民链'+platform+'命中已验真增量缓存\n');
 return true;
}
function saveBuildCache(platform,work,environment){
 if(work!==fixedWork('build/'+platform))fail('平台缓存与任务现场不匹配');
 const target=join(work,'work/cargo-target'),cache=platformCacheRoot(platform);
 if(!checkedCacheDirectory(target))fail('平台Cargo目标目录无效');
 checkedCacheDirectory(cache,{create:true});
 const candidate=join(cache,'candidate'),current=join(cache,'current');
 if(existsSync(candidate)){
  checkedCacheDirectory(candidate);
  rmSync(candidate,{recursive:true});
 }
 checkedCacheDirectory(candidate,{create:true});
 const snapshot=join(candidate,'snapshot');checkedCacheDirectory(snapshot,{create:true});
 for(const profile of ['debug','release'])for(const part of cacheParts){
  const source=join(target,profile,part);
  if(!existsSync(source))continue;
  if(!checkedCacheDirectory(source))fail('平台Cargo缓存来源无效');
  const destination=join(snapshot,profile,part);mkdirSync(dirname(destination),{recursive:true});
  cpSync(source,destination,{recursive:true,errorOnExist:true,force:false,filter:path=>{
   const state=lstatSync(path);
   // 正式WASM与所属Runtime本体每轮从当前源码重建；只复用其依赖的编译中间物。
   if(platform==='wasm'&&path!==source){
    const rel=relative(source,path).split(sep);
    if(rel.length===1&&part==='build'&&/^citizenchain-/u.test(rel[0]))return false;
    if(rel.length===1&&part==='.fingerprint'&&/^citizenchain-/u.test(rel[0]))return false;
    if(rel.length===1&&part==='deps'&&/^(?:lib)?citizenchain[-.]/u.test(rel[0]))return false;
   }
   return state.isDirectory()&&!state.isSymbolicLink()||state.isFile()&&!state.isSymbolicLink()&&state.nlink===1;
  }});
 }
 validateCacheSnapshot(snapshot);
 const manifest={schema:1,platform,identity:cacheIdentity(platform,environment),digest:outputDigest(snapshot)};
 writeFileSync(join(candidate,'manifest.json'),JSON.stringify(manifest)+'\n',{flag:'wx',mode:0o600});
 if(lstatSync(current,{throwIfNoEntry:false})){checkedCacheDirectory(current);rmSync(current,{recursive:true});}
 renameSync(candidate,current);
 process.stderr.write('公民链'+platform+'增量缓存已验真保存\n');
}

// 节点前端只在准确源码、共享扫码组件、图标及原始锁均未变化时复用公开静态产物。
function frontendInputDigest(name){
 if(!['node','onchina'].includes(name))fail('前端缓存名称无效');
 const paths=[join(root,name,'frontend'),join(root,'crates/scanner'),join(root,'icons')];
 const hash=createHash('sha256'),ignored=new Set(['node_modules','dist','build','target','.cache','.DS_Store','tsconfig.tsbuildinfo']);
 function visit(path,base){
  const state=lstatSync(path);
  if(state.isSymbolicLink()||realpathSync(path)!==path||state.isFile()&&state.nlink!==1)fail('前端缓存输入包含链接');
  const rel=relative(base,path);
  if(state.isDirectory()){
   hash.update(JSON.stringify([rel,'directory'])+'\n');
   for(const child of readdirSync(path).sort()){
    if(ignored.has(child))continue;
    if(child.startsWith('.env'))fail('前端缓存输入包含环境机密文件');
    visit(join(path,child),base);
   }
  }else if(state.isFile()){
   hash.update(JSON.stringify([rel,'file',state.size])+'\n');hash.update(readFileSync(path));
  }else fail('前端缓存输入包含特殊文件');
 }
 for(const path of paths){hash.update(relative(root,path)+'\n');visit(path,path);}
 return hash.digest('hex');
}
function frontendCacheIdentity(platform,name,environment){
 const node=contract.platforms.macos.tools.find(value=>value.id==='node');
 if(!node||Object.keys(environment).some(key=>key.startsWith('VITE_')))fail('前端构建环境含未声明变量');
 return createHash('sha256').update(JSON.stringify({schema:1,platform,name,node:node.version,
  source:frontendInputDigest(name),runner:environment.ImageVersion||'',node_env:environment.NODE_ENV||'',
  onchina_url:environment.ONCHINA_BASE_URL||''})).digest('hex');
}
function validateFrontendTree(path){
 if(!checkedCacheDirectory(path))fail('前端静态产物目录无效');
 function visit(at){for(const name of readdirSync(at)){
  const child=join(at,name),state=lstatSync(child);
  if(state.isSymbolicLink()||realpathSync(child)!==child||state.isFile()&&state.nlink!==1)
   fail('前端静态产物包含链接');
  if(state.isDirectory())visit(child);
  else if(!state.isFile())fail('前端静态产物包含特殊文件');
 }}visit(path);
}
function frontendBuildEnvironment(environment,output){
 const result={...environment,...output};
 for(const key of Object.keys(result))if(/(?:^|_)(?:TOKEN|SECRET|PASSWORD|PRIVATE_KEY|CERTIFICATE_BASE64|PROFILE_BASE64)$/iu.test(key))delete result[key];
 return result;
}
async function buildFrontendIncrementally(platform,name,work,source,destination,environment,runner){
 if(platform==='wasm'||!['node','onchina'].includes(name)||work!==fixedWork('build/'+platform)
  ||!inside(work,source)||!inside(work,destination)||resolve(source)!==source||resolve(destination)!==destination)
  fail('前端构建不属于本平台现场');
 if(!checkedCacheDirectory(source))fail('前端工程目录无效');
 const cache=join(platformCacheRoot(platform),'frontend',name),current=join(cache,'current');
 const identity=frontendCacheIdentity(platform,name,environment);
 if(checkedCacheDirectory(current)){
  const metadata=join(current,'manifest.json'),dist=join(current,'dist');
  if(!existsSync(metadata)||!checkedCacheDirectory(dist))fail('前端缓存快照不完整');
  const state=lstatSync(metadata);
  if(!state.isFile()||state.isSymbolicLink()||state.nlink!==1||state.size>65536)fail('前端缓存清单无效');
  const manifest=JSON.parse(readFileSync(metadata,'utf8'));
  if(manifest.schema!==1||manifest.platform!==platform||manifest.name!==name
   ||!/^[a-f0-9]{64}$/u.test(manifest.digest||''))fail('前端缓存身份无效');
  if(manifest.identity===identity){
   validateFrontendTree(dist);
   if(outputDigest(dist)!==manifest.digest)fail('前端缓存字节漂移');
   if(existsSync(destination))fail('前端缓存导入目标已存在');
   mkdirSync(dirname(destination),{recursive:true});cpSync(dist,destination,{recursive:true,errorOnExist:true,force:false});
   process.stderr.write('公民链'+platform+' '+name+'前端命中增量缓存\n');
   return;
  }
 }
 const output=name==='node'?{CITIZENCHAIN_FRONTEND_DIST:destination}:{ONCHINA_FRONTEND_DIST:destination};
 await runner(['run','build'],source,frontendBuildEnvironment(environment,output));
 if(!checkedCacheDirectory(destination))fail('前端编译产物缺失');
 validateFrontendTree(destination);
 checkedCacheDirectory(cache,{create:true});
 const candidate=join(cache,'candidate');
 if(lstatSync(candidate,{throwIfNoEntry:false})){checkedCacheDirectory(candidate);rmSync(candidate,{recursive:true});}
 checkedCacheDirectory(candidate,{create:true});
 const dist=join(candidate,'dist');cpSync(destination,dist,{recursive:true,errorOnExist:true,force:false});
 const manifest={schema:1,platform,name,identity,digest:outputDigest(dist)};
 writeFileSync(join(candidate,'manifest.json'),JSON.stringify(manifest)+'\n',{flag:'wx',mode:0o600});
 if(lstatSync(current,{throwIfNoEntry:false})){checkedCacheDirectory(current);rmSync(current,{recursive:true});}
 renameSync(candidate,current);
}

async function runNativePlatform(platform,work,environment){
 const base={...environment,NODE:process.execPath,CARGO:environment.CARGO||'cargo',CARGO_INCREMENTAL:'1',
  CARGO_TARGET_DIR:join(work,'work/cargo-target'),CITIZENCHAIN_WORK_DIR:work,PRODUCT_WORK_DIR:work};
 const env=platformCargoEnvironment(platform,work,base);
 restoreBuildCache(platform,work,env);
 mkdirSync(env.CARGO_TARGET_DIR,{recursive:true});
 await runNodePrebuildTests(platform,work,env);
 if(platform==='wasm'){
  await buildWasmArtifacts(work,env);
 }
 else if(platform==='macos')await runEmbeddedBuild('run',[],{...env,PRODUCT_BASH_BIN:environment.PRODUCT_BASH_BIN||'/bin/bash'});
 else await buildNodePackage(platform,work,env);
 stageNodeDelivery(platform,work);
 const declared=platformContract(platform);
 const result={schema:1,product_id:product,platform,work,completion:declared.completion,
  files:declared.files.map(name=>{const path=join(work,name);if(!existsSync(path)||realpathSync(path)!==path)fail('平台编译产物缺失');return {path,sha256:outputDigest(path)};})};
 checkBuildResult(result,platform,work);
 saveBuildCache(platform,work,env);
 return {env,result};
}

// 所属回归位于正式WASM实现之后，避免测试注册早于产品固定根导出初始化。
if(inlineTestEntry){
 const {test}=await import('node:test'),{default:assert}=await import('node:assert/strict');
 test('WASM正式编译只使用本平台固定现场且不注入Node编译配置',()=>{
  const work=fixedWork('build/wasm'),env=wasmCargoEnvironment(work,{CARGO:'/verified/cargo',CARGO_TARGET_DIR:'/outside'});
  assert.equal(env.CARGO_TARGET_DIR,join(work,'work/cargo-target'));
  assert.equal(env.CITIZENCHAIN_SOURCE_ROOT,root);
  assert.equal(env.CITIZENCHAIN_WORK_DIR,work);
  assert.equal(env.TAURI_CONFIG,undefined);
  assert.equal(env.WASM_BUILD_FROM_SOURCE,'1');
  assert.equal(env.CITIZENCHAIN_PRODUCTION_WASM_BUILD,'1');
  assert.throws(()=>wasmCargoEnvironment(fixedWork('test'),env),/固定工作根/);
  for(const injected of [{SKIP_WASM_BUILD:'0'},{SKIP_CITIZENCHAIN_WASM_BUILD:'1'},
   {WASM_FILE:'/untrusted.wasm'},{DOCS_RS:'1'}])
   assert.throws(()=>wasmCargoEnvironment(work,injected),/跳过或外部WASM入口/);
 });
 test('四端原生编译统一跳过SDK依赖WASM且清除源码构建环境',()=>{
  const injected={SKIP_REVIVE_DEV_RUNTIME_WASM_BUILD:'0',WASM_FILE:'/outside.wasm',
   WASM_BUILD_FROM_SOURCE:'1',CITIZENCHAIN_PRODUCTION_WASM_BUILD:'1',FORCE_WASM_BUILD:'1'};
  for(const platform of ['macos','windows','linux-arm','linux-amd']){
   const env=platformCargoEnvironment(platform,fixedWork('build/'+platform),injected);
   assert.equal(env.SKIP_WASM_BUILD,'1');
   for(const key of Object.keys(injected))assert.equal(env[key],undefined);
   assert.notEqual(cacheIdentity(platform,env),cacheIdentity(platform,{...env,SKIP_WASM_BUILD:''}));
  }
 });
 test('五平台增量缓存按平台隔离，命中复用并拒绝字节漂移',async()=>{
  const platform='wasm',cache=platformCacheRoot(platform),other=platformCacheRoot('windows'),
   failed=platformCacheRoot('linux-arm'),env={CARGO_INCREMENTAL:'1'};
  try{
   await withFixedWork('build/'+platform,async work=>{
    const source=join(work,'work/cargo-target/release/deps');
    mkdirSync(source,{recursive:true});writeFileSync(join(source,'runtime.rlib'),'first');
    writeFileSync(join(source,'libcitizenchain-001.rlib'),'runtime-owned');
    const wasmOutput=join(work,'work/cargo-target/release/wbuild/citizenchain');
    mkdirSync(wasmOutput,{recursive:true});writeFileSync(join(wasmOutput,'citizenchain.wasm'),'candidate');
    saveBuildCache(platform,work,env);
    assert.ok(verifiedBuildCacheSnapshot(platform,env));
    assert.equal(existsSync(join(cache,'current/snapshot/release/deps/libcitizenchain-001.rlib')),false);
    assert.equal(existsSync(join(cache,'current/snapshot/release/wbuild')),false);
    rmSync(join(work,'work/cargo-target'),{recursive:true});
    assert.equal(restoreBuildCache(platform,work,env),true);
    assert.equal(readFileSync(join(source,'runtime.rlib'),'utf8'),'first');
    assert.equal(restoreBuildCache('windows',fixedWork('build/windows'),env),false);
    assert.equal(restoreBuildCache(platform,work,{...env,RUSTFLAGS:'changed'}),false);
    assert.equal(verifiedBuildCacheSnapshot(platform,{...env,RUSTFLAGS:'changed'}),null);
    assert.equal(restoreBuildCache(platform,work,{...env,ImageVersion:'changed'}),false);
    rmSync(join(work,'work/cargo-target'),{recursive:true});
    writeFileSync(join(cache,'current/snapshot/release/deps/runtime.rlib'),'drift');
    assert.throws(()=>restoreBuildCache(platform,work,env),/字节漂移/);
    assert.throws(()=>verifiedBuildCacheSnapshot(platform,env),/字节漂移/);
   });
   await withFixedWork('build/windows',async work=>{
    const source=join(work,'work/cargo-target/release/deps');
    mkdirSync(source,{recursive:true});writeFileSync(join(source,'node.rlib'),'independent');
    const nodeEnv=nativeCargoEnvironment(env);
    saveBuildCache('windows',work,nodeEnv);
    rmSync(join(work,'work/cargo-target'),{recursive:true});
    assert.equal(restoreBuildCache('windows',work,nodeEnv),true);
    assert.equal(readFileSync(join(source,'node.rlib'),'utf8'),'independent');
    const head=spawnSync('git',['rev-parse','HEAD'],{cwd:root,encoding:'utf8'}).stdout.trim();
    const github={GITHUB_ACTIONS:'true',GITHUB_REPOSITORY:'crcfrcn/citizenchain',
     GITHUB_REF:'refs/heads/main',GITHUB_EVENT_NAME:'workflow_dispatch',
     GITHUB_WORKFLOW:'citizenchain.windows',GITHUB_SHA:head};
    assert.equal(githubCacheReady('windows',github),true);
    assert.equal(githubCacheReady('windows',{...github,RUSTFLAGS:'different'}),false);
    assert.throws(()=>githubCacheReady('linux-arm',github),/GitHub编译身份无效/);
    mkdirSync(join(other,'candidate'));
    assert.throws(()=>githubCacheReady('windows',github),/未验真目录/);
    rmSync(join(other,'candidate'),{recursive:true});
    assert.equal(githubCacheReady('windows',github),true);
   });
   const armEnv=nativeCargoEnvironment(env);
   await assert.rejects(withFixedWork('build/linux-arm',async work=>{
    const source=join(work,'work/cargo-target/release/deps');
    mkdirSync(source,{recursive:true});writeFileSync(join(source,'compiled.rlib'),'verified');
    saveBuildCache('linux-arm',work,armEnv);
    throw Error('后续打包失败');
   }),/后续打包失败/);
   assert.equal(existsSync(fixedWork('build/linux-arm')),false);
   assert.ok(verifiedBuildCacheSnapshot('linux-arm',armEnv));
  }finally{for(const path of [cache,other,failed])if(existsSync(path))rmSync(path,{recursive:true});}
 });
 test('节点前端仅在准确输入不变时复用，缓存漂移与机密环境失败关闭',async()=>{
  const platform='windows',cache=platformCacheRoot(platform);
  try{await withFixedWork('build/'+platform,async work=>{
   const source=join(work,'source/node/frontend'),destination=join(work,'stage/node-frontend');
   mkdirSync(source,{recursive:true});let calls=0;
   const environment={NODE_ENV:'production',GH_TOKEN:'fixture-private-token'};
   const runner=async(args,cwd,clean)=>{
    calls++;assert.deepEqual(args,['run','build']);assert.equal(cwd,source);assert.equal(clean.GH_TOKEN,undefined);
    mkdirSync(destination,{recursive:true});writeFileSync(join(destination,'index.html'),'public');
   };
   await buildFrontendIncrementally(platform,'node',work,source,destination,environment,runner);
   rmSync(destination,{recursive:true});
   await buildFrontendIncrementally(platform,'node',work,source,destination,environment,runner);
   assert.equal(calls,1);assert.equal(readFileSync(join(destination,'index.html'),'utf8'),'public');
   rmSync(destination,{recursive:true});
   writeFileSync(join(cache,'frontend/node/current/dist/index.html'),'drift');
   await assert.rejects(buildFrontendIncrementally(platform,'node',work,source,destination,environment,runner),/字节漂移/);
   await assert.rejects(buildFrontendIncrementally(platform,'node',work,source,destination,{...environment,VITE_SECRET:'bad'},runner),/未声明变量/);
  });}finally{if(existsSync(cache))rmSync(cache,{recursive:true});}
 });
}

export async function buildDirect(platform,environment=process.env){
 platformContract(platform);
 if(environment.GITHUB_ACTIONS==='true')fail('直接编译入口不接受远端身份');
 return withFixedWork('build/'+platform,async work=>{
  const {result}=await runNativePlatform(platform,work,environment);
  writeFileSync(join(work,'build-result.json'),JSON.stringify(result)+'\n',{flag:'wx',mode:0o600});
  retainWork();
  return result;
 },{environment});
}

function assertGithubBuildIdentity(platform,environment){
 const workflow={macos:'macos.yml',windows:'windows.yml','linux-arm':'linuxarm.yml','linux-amd':'linuxamd.yml',wasm:'wasm.yml'}[platform];
 if(!workflow||environment.GITHUB_ACTIONS!=='true'||environment.GITHUB_REPOSITORY!=='crcfrcn/citizenchain'
  ||environment.GITHUB_REF!=='refs/heads/main'||environment.GITHUB_EVENT_NAME!=='workflow_dispatch'
  ||environment.GITHUB_WORKFLOW!=='citizenchain.'+platform||!/^[0-9a-f]{40}$/u.test(environment.GITHUB_SHA||''))fail('GitHub编译身份无效');
 const head=spawnSync('git',['rev-parse','HEAD'],{cwd:root,encoding:'utf8'});
 if(head.status!==0||head.stdout.trim()!==environment.GITHUB_SHA)fail('GitHub源码提交不匹配');
 if(platform!=='wasm'){
  const title=nodeDeliveryName(platform),source=readFileSync(join(root,'.github/workflows',workflow),'utf8');
  if(!source.split('\n').includes('run-name: "'+title+'"')||!source.split('\n').includes('    name: "'+title+'"'))
   fail('节点自动化名称与正式版本不一致');
 }
}
export async function buildForGithub(platform,environment=process.env){
 assertGithubBuildIdentity(platform,environment);
 await assertFreshGithubVersion(platform,environment);
 return withFixedWork('build/'+platform,async work=>{
  const {env,result}=await runNativePlatform(platform,work,environment);
  await prepareGithubAssets(platform,work,result,env);
  writeFileSync(join(work,'build-result.json'),JSON.stringify(result)+'\n',{flag:'wx',mode:0o600});
  return result;
 },{environment,retain:true});
}
// 失败后只允许上传已通过产品身份、锁与全树摘要复验的增量快照。
function githubCacheReady(platform,environment=process.env){
 assertGithubBuildIdentity(platform,environment);
 const work=fixedWork('build/'+platform);
 const base={...environment,NODE:process.execPath,CARGO:environment.CARGO||'cargo',CARGO_INCREMENTAL:'1',
  CARGO_TARGET_DIR:join(work,'work/cargo-target'),CITIZENCHAIN_WORK_DIR:work,PRODUCT_WORK_DIR:work};
 return verifiedPlatformCacheForUpload(platform,platformCargoEnvironment(platform,work,base));
}

export async function verifyGithubRelease(platform,phase,environment=process.env,{fetchImpl=fetch}={}){
 if(!['draft','published'].includes(phase)||environment.GITHUB_ACTIONS!=='true'||environment.GITHUB_REPOSITORY!=='crcfrcn/citizenchain'
  ||environment.GITHUB_WORKFLOW!=='citizenchain.'+platform)fail('GitHub Release核验身份无效');
 const work=fixedWork('build/'+platform);checkFixedWork(work);
 const plan=JSON.parse(readFileSync(join(work,'release-plan.json'),'utf8'));
 if(plan.platform!==platform||plan.source_sha!==environment.GITHUB_SHA||!Array.isArray(plan.assets)
  ||plan.directory!==join(work,'release-assets')||plan.notes!==join(work,'release-notes.txt'))fail('本轮Release计划无效');
 const token=environment.GH_TOKEN||environment.GITHUB_TOKEN;
 if(typeof token!=='string'||!token)fail('缺少GitHub核验令牌');
 const base='https://api.github.com/repos/crcfrcn/citizenchain/releases';
 const read=async url=>{
  const response=await fetchImpl(url,{redirect:'error',headers:{Accept:'application/vnd.github+json',Authorization:'Bearer '+token,'X-GitHub-Api-Version':'2022-11-28'}});
  if(!response.ok)fail('GitHub Release回读失败');
  return response.json();
 };
 let release;
 if(phase==='draft'){
  // GitHub按tag查询只返回已发布版本；草稿先从有权限的完整列表定位唯一数字ID。
  let candidate,finished=false;
  for(let page=1;page<=100;page++){
   const rows=await read(`${base}?per_page=100&page=${page}`);
   if(!Array.isArray(rows)||rows.length>100)fail('GitHub草稿列表格式无效');
   for(const row of rows)if(row?.tag_name===plan.tag){if(candidate)fail('GitHub草稿Tag不唯一');candidate=row;}
   if(rows.length<100){finished=true;break;}
  }
  if(!finished)fail('GitHub草稿列表分页超限');
  if(!candidate)fail('GitHub草稿Release缺失');
  if(!Number.isSafeInteger(candidate.id)||candidate.id<=0||candidate.url!==`${base}/${candidate.id}`)fail('GitHub草稿Release身份无效');
  release=await read(`${base}/${candidate.id}`);
  if(release.id!==candidate.id)fail('GitHub草稿Release身份漂移');
 }else release=await read(`${base}/tags/${encodeURIComponent(plan.tag)}`);
 if(!Number.isSafeInteger(release?.id)||release.id<=0||release.url!==`${base}/${release.id}`
  ||release.tag_name!==plan.tag||release.target_commitish!==plan.source_sha||release.prerelease!==false
  ||release.name!==plan.title||release.draft!==(phase==='draft')||phase==='published'&&!release.published_at
  ||release.body?.trim()!==readFileSync(plan.notes,'utf8').trim())fail('GitHub Release身份或状态不符');
 if(!Array.isArray(release.assets)||release.assets.length!==plan.assets.length)fail('GitHub Release资产数量不符');
 for(const expected of plan.assets){
  const path=join(plan.directory,expected.name),local=lstatSync(path);
  if(!local.isFile()||local.isSymbolicLink()||local.size!==expected.size||releaseAssetSha256(path)!==expected.sha256)fail('本轮资产字节已改变');
  const matches=release.assets.filter(value=>value.name===expected.name);
  if(matches.length!==1||matches[0].state!=='uploaded'||matches[0].size!==expected.size
   ||matches[0].digest!=='sha256:'+expected.sha256||!matchesGithubReleaseAssetUrl(plan.tag,expected.name,matches[0].browser_download_url))
   fail('GitHub Release资产回读不符：'+expected.name);
 }
 return {platform,tag:plan.tag,phase};
}

// 本平台末尾Job调用产品收尾；回读已完成业务Job，业务结果与整个Run最终结论分开记录。
// 本产品公开GitHub接口只使用普通参数与返回值；平台流程及终态处理归本仓。
export async function finalizeGithubHistory(platform,request,environment=process.env,{fetchImpl=fetch}={}){
 const workflow={macos:'macos.yml',windows:'windows.yml','linux-arm':'linuxarm.yml','linux-amd':'linuxamd.yml',wasm:'wasm.yml'}[platform];
 if(!workflow||!request||Object.keys(request).sort().join(',')!=='business_result,run_attempt,run_id,source_sha'
  ||!Number.isSafeInteger(request.run_id)||request.run_id<1||!Number.isSafeInteger(request.run_attempt)||request.run_attempt<1
  ||!/^[0-9a-f]{40}$/u.test(request.source_sha||'')||!['success','failure','cancelled','skipped'].includes(request.business_result))fail('历史收尾坐标无效');
 if(environment.GITHUB_ACTIONS!=='true'||environment.GITHUB_REPOSITORY!=='crcfrcn/citizenchain'
  ||environment.GITHUB_WORKFLOW!=='citizenchain.'+platform||environment.GITHUB_EVENT_NAME!=='workflow_dispatch'
  ||environment.GITHUB_REF!=='refs/heads/main'||environment.GITHUB_SHA!==request.source_sha
  ||environment.GITHUB_RUN_ID!==String(request.run_id)||environment.GITHUB_RUN_ATTEMPT!==String(request.run_attempt))
  fail('历史收尾必须属于本仓本平台原Workflow');
 const token=environment.GH_TOKEN||environment.GITHUB_TOKEN;
 if(typeof token!=='string'||!token)fail('历史收尾缺少本仓授权');
 const api='https://api.github.com/repos/crcfrcn/citizenchain',workflowPath='.github/workflows/'+workflow;
 const call=async(path,{method='GET',missing=false}={})=>{
  if(!path.startsWith('/')||path.includes('..')||path.includes('#'))fail('历史收尾请求越界');
  const response=await fetchImpl(api+path,{method,redirect:'error',signal:AbortSignal.timeout(15000),
   headers:{Accept:'application/vnd.github+json',Authorization:'Bearer '+token,'X-GitHub-Api-Version':'2022-11-28'}});
  if(missing&&response.status===404)return null;
  if(!response.ok)fail('历史收尾GitHub请求失败：'+method+' '+path+' HTTP '+response.status);
  if(method==='DELETE'){if(response.status!==204)fail('历史删除缺少准确回执');return null;}
  return response.json();
 };
 const pages=async(path,field)=>{
  const result=[];
  for(let page=1;page<=100;page++){
   const data=await call(path+(path.includes('?')?'&':'?')+'per_page=100&page='+page),rows=field?data?.[field]:data;
   if(!Array.isArray(rows)||rows.length>100)fail('历史收尾分页无效');result.push(...rows);
   if(rows.length<100)return result;
  }
  fail('历史收尾分页超限');
 };
 const checkedRun=run=>{
  if(!Number.isSafeInteger(run?.id)||run.id<1||!Number.isSafeInteger(run.run_attempt)||run.run_attempt<1
   ||run.path!==workflowPath||run.event!=='workflow_dispatch'||run.head_branch!=='main'
   ||run.repository?.full_name!=='crcfrcn/citizenchain'||!/^[0-9a-f]{40}$/u.test(run.head_sha||''))fail('历史Run身份不符');
  if(run.status==='completed'&&!['success','failure','cancelled','timed_out','action_required','neutral','skipped','stale','startup_failure'].includes(run.conclusion))
   fail('历史Run终态无效');
  return run;
 };
 const source=checkedRun(await call('/actions/runs/'+request.run_id));
 if(source.id!==request.run_id||source.run_attempt!==request.run_attempt||source.head_sha!==request.source_sha)fail('历史收尾Run坐标已变化');
 if(source.status!=='in_progress')fail('本平台收尾必须在原Workflow执行中完成');
 const businessName=platform==='wasm'?'release':nodeDeliveryName(platform);
 const jobs=await pages('/actions/runs/'+source.id+'/attempts/'+source.run_attempt+'/jobs','jobs');
 const business=jobs.filter(job=>job.name===businessName);
 if(business.length!==1||business[0].run_id!==source.id||business[0].head_sha!==source.head_sha
  ||business[0].status!=='completed'||business[0].conclusion!==request.business_result)
  fail('本平台前序业务Job尚未完成或结果不符');
 const finalizers=jobs.filter(job=>job.name==='finalize');
 if(finalizers.length!==1||finalizers[0].status!=='in_progress'
  ||jobs.some(job=>![businessName,'finalize'].includes(job.name)||job.run_id!==source.id||job.head_sha!==source.head_sha))
  fail('本平台收尾Job身份或执行集合不符');
 const resultOf=run=>run.id===source.id?request.business_result:run.conclusion;
 const settled=run=>run.id===source.id||run.status==='completed';
 const runPath='/actions/workflows/'+workflow+'/runs?branch=main&event=workflow_dispatch';
 const runs=(await pages(runPath,'workflow_runs')).map(checkedRun),byId=new Map(runs.map(run=>[run.id,run]));
 const listedSource=byId.get(source.id);
 if(byId.size!==runs.length||!listedSource||listedSource.status!==source.status||listedSource.conclusion!==source.conclusion
  ||listedSource.run_attempt!==request.run_attempt||listedSource.head_sha!==request.source_sha)fail('历史Run列表重复或原Run坐标漂移');
 const latest=new Map();
 for(const run of [...runs].sort((left,right)=>right.id-left.id||right.run_attempt-left.run_attempt)){
  if(!settled(run))continue;
  const state=resultOf(run)==='success'?'success':'failed';if(!latest.has(state))latest.set(state,run);
 }
 if(latest.get(request.business_result==='success'?'success':'failed')?.id!==source.id)fail('已有更新业务结果，拒绝旧请求覆盖新结果');
 const versionPattern=platform==='wasm'?'[1-9][0-9]*':'(?:0|[1-9][0-9]*)\\.(?:0|[1-9][0-9]*)\\.(?:0|[1-9][0-9]*)';
 const pattern=new RegExp('^citizenchain-'+platform+'-v('+versionPattern+')-r([1-9][0-9]*)-a([1-9][0-9]*)$','u');
 const owned=[];
 for(const release of await pages('/releases')){
  const match=pattern.exec(release?.tag_name||'');if(!match)continue;
  const run=byId.get(Number(match[2]));
  if(!run)fail('本平台Release缺少可核实的原Run');
  if(!settled(run))continue;
  if(run.run_attempt!==Number(match[3]))fail('Release与原Run重试坐标不一致');
  const lines=String(release.body||'').split('\n').filter(line=>line.startsWith('<!-- automation:')&&line.endsWith(' -->'));
  if(lines.length!==1)fail('历史Release证明不唯一');
  const proof=JSON.parse(lines[0].slice('<!-- automation:'.length,-' -->'.length));
  if(proof.schema!==1||proof.repository!=='crcfrcn/citizenchain'||proof.product_id!==product||proof.platform!==platform
   ||proof.workflow!==workflowPath||proof.run_id!==run.id||proof.run_attempt!==run.run_attempt||proof.source_sha!==run.head_sha
   ||proof.tag!==release.tag_name||String(proof.version)!==match[1]||release.target_commitish!==run.head_sha
   ||!Number.isSafeInteger(release.id)||release.id<1||!Array.isArray(proof.assets)||!Array.isArray(release.assets)
   ||!proof.assets.length)fail('历史Release来源证明不符');
  let complete=proof.assets.length===release.assets.length;
  const names=new Set();
  for(const asset of proof.assets){
   if(typeof asset.name!=='string'||!asset.name||asset.name.includes('/')||asset.name.includes('\\')||names.has(asset.name)
    ||!Number.isSafeInteger(asset.size)||asset.size<1||!/^[0-9a-f]{64}$/u.test(asset.sha256||''))fail('历史资产证明无效');
   names.add(asset.name);
   const matches=release.assets.filter(actual=>actual.name===asset.name);
   if(matches.length!==1||matches[0].state!=='uploaded'||matches[0].size!==asset.size||matches[0].digest!=='sha256:'+asset.sha256)complete=false;
  }
  // 上传中断可留下不完整草稿；仅凭完整来源坐标确认归属后撤销，不能把残包当作有效版本。
  if(resultOf(run)==='success'&&!complete)fail('历史成功资产回读不符');
  if(release.assets.some(asset=>!names.has(asset.name))||new Set(release.assets.map(asset=>asset.name)).size!==release.assets.length)
   fail('失败Release包含不属于本次证明的资产');
  const reference=await call('/git/ref/tags/'+encodeURIComponent(release.tag_name),{missing:release.draft===true});
  if(reference&&(reference.ref!=='refs/tags/'+release.tag_name||reference.object?.type!=='commit'||reference.object.sha!==run.head_sha))
   fail('历史Tag身份或提交不符');
  owned.push({release,run,reference,version:match[1]});
 }
 const successful=latest.get('success'),keepers=owned.filter(item=>item.run.id===successful?.id&&item.run.run_attempt===successful?.run_attempt
  &&item.release.draft===false&&item.release.prerelease===false&&Boolean(item.release.published_at));
 if(successful&&keepers.length!==1)fail('最新成功Run没有唯一有效正式Release，保留旧版本并停止');
 const keep=keepers[0],keepRuns=new Set([...latest.values()].map(run=>run.id));
 if(keep&&JSON.stringify(keep.release.assets.map(asset=>asset.name).sort())!==JSON.stringify(githubReleaseFiles(platform,keep.version).sort()))
  fail('最新有效Release缺少本平台完整交付集合，保留旧版本并停止');
 const deletingReleases=owned.filter(item=>item.release.id!==keep?.release.id&&!item.release.prerelease);
 const deletingRuns=runs.filter(run=>run.status==='completed'&&!keepRuns.has(run.id));
 const stableRelease=release=>JSON.stringify({tag:release.tag_name,source:release.target_commitish,body:release.body,
  draft:release.draft,prerelease:release.prerelease,assets:release.assets.map(asset=>({id:asset.id,name:asset.name,state:asset.state,size:asset.size,digest:asset.digest}))});
 for(const item of deletingReleases){
  const current=await call('/releases/'+item.release.id);
  if(stableRelease(current)!==stableRelease(item.release))fail('历史Release在删除前已变化');
  await call('/releases/'+item.release.id,{method:'DELETE'});
  if(await call('/releases/'+item.release.id,{missing:true})!==null)fail('历史Release删除未确认');
  if(item.reference){
   const path='/git/ref/tags/'+encodeURIComponent(item.release.tag_name),currentTag=await call(path);
   if(currentTag.object?.type!=='commit'||currentTag.object.sha!==item.run.head_sha)fail('历史Tag在删除前已变化');
   await call('/git/refs/tags/'+encodeURIComponent(item.release.tag_name),{method:'DELETE'});
   if(await call(path,{missing:true})!==null)fail('历史Tag删除未确认');
  }
 }
 for(const run of deletingRuns){
  const current=checkedRun(await call('/actions/runs/'+run.id));
  if(current.status!=='completed'||current.run_attempt!==run.run_attempt||current.head_sha!==run.head_sha||current.conclusion!==run.conclusion)
   fail('历史Run在删除前已变化');
  await call('/actions/runs/'+run.id,{method:'DELETE'});
  if(await call('/actions/runs/'+run.id,{missing:true})!==null)fail('历史Run删除未确认');
 }
 const remaining=(await pages(runPath,'workflow_runs')).map(checkedRun).filter(settled);
 if(remaining.length!==keepRuns.size||remaining.some(run=>{
  const expected=latest.get(resultOf(run)==='success'?'success':'failed');
  if(!expected||!keepRuns.has(run.id)||run.id!==expected.id||run.run_attempt!==expected.run_attempt||run.head_sha!==expected.head_sha)return true;
  return run.id===source.id?run.status!=='in_progress':run.conclusion!==expected.conclusion;
 }))fail('业务收尾Run保留集合回读不符');
 const remainingReleases=(await pages('/releases')).filter(release=>pattern.test(release?.tag_name||'')&&release.draft===false&&release.prerelease===false);
 if(remainingReleases.length!==(keep?1:0)||keep&&(remainingReleases[0].id!==keep.release.id||stableRelease(remainingReleases[0])!==stableRelease(keep.release)))
  fail('正式Release保留集合回读不符');
 if(keep){
  const reference=await call('/git/ref/tags/'+encodeURIComponent(keep.release.tag_name));
  if(reference.ref!=='refs/tags/'+keep.release.tag_name||reference.object?.type!=='commit'||reference.object.sha!==keep.run.head_sha)
   fail('保留版本Tag回读不符');
 }
 return {schema:1,platform,...request,run_status:source.status,final_run_verified:false,kept_runs:[...keepRuns],kept_release:keep?.release.id??null,
  deleted_runs:deletingRuns.map(run=>run.id),deleted_releases:deletingReleases.map(item=>item.release.id)};
}


async function runCLI(){
 if(process.argv[2]==='ci-finalize'){
  if(process.argv.length!==4)fail('历史收尾入口参数无效');
  let input='';for await(const chunk of process.stdin){input+=chunk;if(Buffer.byteLength(input)>16384)fail('历史收尾坐标超限');}
  process.stdout.write(JSON.stringify(await finalizeGithubHistory(process.argv[3],JSON.parse(input)))+'\n');return;
 }
 if(process.argv[2]==='editor')return editorCommand(process.argv.slice(3));
 if(process.argv[2]==='frontend-build'){
  const platform=process.argv[3],name=process.argv[4];
  if(process.argv.length!==5||platform!=='macos'||!['node','onchina'].includes(name)
   ||process.env.CITIZENCHAIN_WORK_DIR!==fixedWork('build/macos'))fail('前端构建入口身份无效');
  await withFixedWork('build/macos',async work=>{
   const source=join(work,'source/citizenchain',name==='node'?'node/frontend':'onchina/frontend');
   const destination=join(work,name==='node'?'node-frontend':'onchina-frontend/dist');
   const environment={...process.env,NODE:process.execPath};
   const runner=(args,cwd,clean)=>runBuildProcess(process.execPath,[npmCLI(process.execPath),...args],clean,cwd);
   await buildFrontendIncrementally(platform,name,work,source,destination,environment,runner);
  },{environment:process.env});return;
 }
 if(process.argv[2]==='native-build'){
  if(process.argv.length!==4)fail('直接编译入口参数无效');
  process.stdout.write(JSON.stringify(await buildDirect(process.argv[3]))+'\n');return;
 }
 if(process.argv[2]==='ci-build'){
  if(process.argv.length!==4)fail('GitHub编译入口参数无效');
  process.stdout.write(JSON.stringify(await buildForGithub(process.argv[3]))+'\n');return;
 }
 if(process.argv[2]==='ci-cache-ready'){
  if(process.argv.length!==4)fail('GitHub缓存验真入口参数无效');
  process.stdout.write(String(githubCacheReady(process.argv[3]))+'\n');return;
 }
 if(process.argv[2]==='ci-verify-release'){
  if(process.argv.length!==5)fail('GitHub Release核验入口参数无效');
  process.stdout.write(JSON.stringify(await verifyGithubRelease(process.argv[3],process.argv[4]))+'\n');return;
 }
 const [operation,platform,flag,work]=process.argv.slice(2);
 if(operation==='execute'){if(flag!=='--work'||work!==fixedWork('build/'+platform))fail('平台编译现场不符');return withFixedWork('build/'+platform,()=>runCommand(),{environment:process.env});}
 if(operation==='execute')return runCommand();
 if(['resources','prepare','build'].includes(operation)&&flag==='--work'){
  checkWork(work);
  return withFixedWork(taskScope(work),()=>runCommand(),{environment:process.env});
 }
 return runCommand();
}
async function runCommand(){
 const [command,platform,option,work,...extra]=process.argv.slice(2);
 if(command==='clean'){cleanShellPaths(process.argv.slice(3));return;}
 if(command==='describe'){
  if(process.argv.length!==3)fail('编译声明入口参数无效');
  process.stdout.write(JSON.stringify(contract)+'\n');return;
 }
 if(command==='finish'){
  if(extra.length||work!==undefined||platform!=='test'&&!Object.hasOwn(contract.platforms,platform))fail('平台收尾入口参数无效');
  finishFixedWork(fixedWork(platform==='test'?'test':'build/'+platform),{run_id:option});return;
 }
 if(command==='rsync')return resourceInternals.runResourceCLI(command,process.argv.slice(3));

 if(command==='temporary-root') {
  if(work!==undefined||extra.length)fail('临时入口参数无效');
  const host=process.platform==='darwin'?'macos':process.platform==='win32'?'windows':process.platform==='linux'?(process.arch==='arm64'?'linux-arm':process.arch==='x64'?'linux-amd':undefined):undefined;
  const fallback=option?.endsWith('macos')?option.slice(0,-5)+host:option;
  const chosen=Object.hasOwn(contract.platforms,platform)?platform
   :platform&&option?.endsWith('-'+platform)&&Object.hasOwn(contract.platforms,option)?option
   :Object.hasOwn(contract.platforms,'host-'+platform)?'host-'+platform:!platform?(Object.hasOwn(contract.platforms,fallback)?fallback:option):platform;
  platformContract(chosen);process.stdout.write(temporaryRoot(chosen,'tmp')+'\n');
 } else {

 if(!['requirements','resources','prepare','build','execute'].includes(command)||option!=='--work'||extra.some(x=>x!=='--offline')||extra.length>1||extra.length&&!['resources','execute'].includes(command))fail('固定入口参数无效');
 if(command==='execute'){platformContract(platform);if(work!==fixedWork('build/'+platform))fail('平台编译现场不符');}else checkWork(work);
 if(command==='requirements')process.stdout.write(JSON.stringify(requirements(platform,work))+'\n');
 else{
  const cancellation=new AbortController();for(const name of ['SIGTERM','SIGINT'])process.once(name,()=>cancellation.abort());
  let input='';for await(const chunk of process.stdin){input+=chunk;if(Buffer.byteLength(input)>2*1024*1024)fail('公开输入超限');}
  const request=input?JSON.parse(input):{},options={environment:process.env,signal:cancellation.signal,offline:extra.includes('--offline')};
  let result;
  if(command==='execute'){
   const {bootstrapNode}=resourceInternals;const node=await bootstrapNode(work,options);
   if(createHash('sha256').update(readFileSync(process.execPath)).digest('hex')!==createHash('sha256').update(readFileSync(node.path)).digest('hex')){
    const environment=Object.fromEntries(['HOME','USER','LOGNAME','LANG','LC_ALL','PRODUCT_WORK_LEASE'].filter(k=>typeof process.env[k]==='string').map(k=>[k,process.env[k]]));
    result=JSON.parse((await runBuildProcess(node.path,[fileURLToPath(import.meta.url),command,platform,option,work,...extra],workEnvironment(environment),root,{capture:true,streamError:true,input:JSON.stringify(request),signal:cancellation.signal})).stdout);
   }else result=await execute(platform,work,request,options);
  }else if(command==='resources')result=await resourceInternals.resources(platform,work,request,options);
  else result=await executions.run({signal:cancellation.signal},()=>command==='prepare'?prepare(platform,work,request,process.env):build(platform,work,request,process.env));
  process.stdout.write(JSON.stringify(result)+'\n');
 }
}
}

// CLI拒绝必须真实失败，不能留成未完成顶层await或输出成功回执。

// 产品工具、锁定依赖、配方与门禁资源由本仓编译入口直接拥有。
const resourceInternals=await (async()=>{
const {fixedScratch,trackWorkProcess,workEnvironment,checkScratchPath}=targetInternals;
const {AsyncLocalStorage}=await import('node:async_hooks');
const {writeFileSync:writeGroupRecord,existsSync,readFileSync,constants}=await import('node:fs');
const supplyGroups=new Map();
const {createHash,randomUUID}=await import('node:crypto');
const {lstat,realpath,readdir,readlink,symlink,copyFile,readFile,writeFile,mkdir,mkdtemp,rename,rm:removeResourcePath,chmod,open}=await import('node:fs/promises');
const {dirname,join,resolve,relative,isAbsolute,sep,parse,win32,posix}=await import('node:path');
const {fileURLToPath,pathToFileURL}=await import('node:url');
const {homedir}=await import('node:os');
const {gunzipSync,inflateRawSync}=await import('node:zlib');
const {spawn}=await import('node:child_process');
const {createRequire}=await import('node:module');
// 产品资源阶段：声明、取得、验真和物化均属于本仓；可选原件目录不参与版本决策。
const resourceSupplies=new AsyncLocalStorage();
const exec=runResourceProcess,execute=exec;
// 工具返回只在主进程和整组后代退出后完成；未确认的输入目录禁止后续清理或改权限。
const retainedResourceRoots=new Set();
function retainedResourcePath(path) {
 const value=resolve(String(path));
 return [...retainedResourceRoots].some(root=>value===root||value.startsWith(root+sep)||root.startsWith(value+sep));
}
async function rm(path,options) {
 if(retainedResourcePath(path))throw Error('资源工具退出未确认，保留工作目录');

 if(retainedResourcePath(path))throw Error('资源工具退出未确认，保留工作目录');
 return removeResourcePath(path,options);
}
function runResourceProcess(command,args,{signal,maxBuffer=8*1024**2,timeout=3600000,encoding='utf8',quietOutput=false,...options}={}) {


 signal?.throwIfAborted();
 if(!Number.isSafeInteger(maxBuffer)||maxBuffer<=0||!Number.isSafeInteger(timeout)||timeout<=0)throw Error('资源进程边界参数无效');
 return new Promise((ok,reject)=>{
  const child=spawn(command,args,{...options,env:workEnvironment(options.env),detached:process.platform!=='win32',stdio:['ignore','pipe','pipe']});
  trackWorkProcess(child.pid);
  const supply=resourceSupplies.getStore(),groups=supply?.work?(supplyGroups.get(supply.work)||new Set()):null;
  const record=()=>{if(groups)writeGroupRecord(join(supply.work,'.resource-active.json'),JSON.stringify({pid:process.pid,groups:[...groups]})+'\n');};
  if(groups&&Number.isSafeInteger(child.pid)){supplyGroups.set(supply.work,groups);groups.add(child.pid);record();}
  const output=[],errors=[];let bytes=0,done=false,closed=false,failure=null,probe=null,force=null,limit=null;
  const groupExists=()=>{
   if(process.platform==='win32')return !closed;
   if(!child.pid)return false;
   try{process.kill(-child.pid,0);return true;}catch(error){if(error.code==='ESRCH')return false;return true;}
  };
  const stop=hard=>{
   if(!child.pid)return;
   // 进程组已接收信号时立即返回，禁止同轮再次向组内主进程发送相同信号。
   if(process.platform!=='win32')try{process.kill(-child.pid,hard?'SIGKILL':'SIGTERM');return;}catch(error){if(error.code!=='ESRCH'){failure=Error('资源工具取消无法确认');return;}}
   if(!closed)try{child.kill(hard?'SIGKILL':'SIGTERM');}catch{failure=Error('资源工具取消无法确认');}
  };
  const finish=(error,result)=>{
   if(done)return;done=true;if(groups&&closed&&!groupExists()){groups.delete(child.pid);record();}clearTimeout(timer);clearTimeout(force);clearTimeout(limit);clearTimeout(probe);signal?.removeEventListener('abort',cancel);
   error?reject(error):ok(result);
  };
  const retain=()=>{
   for(const path of [options.cwd,options.env?.PRODUCT_WORK_DIR])if(typeof path==='string'&&isAbsolute(path))retainedResourceRoots.add(resolve(path));
   finish(Error('资源工具退出未确认，保留工作目录'));
  };
  const confirm=()=>{
   if(done)return;
   if(closed&&!groupExists()) {
    const stdout=Buffer.concat(output),stderr=Buffer.concat(errors);
    finish(failure,{stdout:encoding==='buffer'?stdout:stdout.toString(encoding),stderr:encoding==='buffer'?stderr:stderr.toString(encoding)});return;
   }
   probe=setTimeout(confirm,50);
  };
  const requestStop=error=>{
   if(done||force!==null)return;failure=error;stop(false);
   force=setTimeout(()=>stop(true),8000);limit=setTimeout(retain,12000);
   if(probe===null)confirm();
  };
  const cancel=()=>requestStop(signal.reason instanceof Error?signal.reason:Error('资源进程已取消'));
  const timer=setTimeout(()=>requestStop(Error('资源进程超时')),timeout);
  signal?.addEventListener('abort',cancel,{once:true});if(signal?.aborted)cancel();
  for(const [stream,parts]of [[child.stdout,output],[child.stderr,errors]])stream.on('data',chunk=>{
   if(done)return;bytes+=chunk.length;
   if(bytes>maxBuffer){requestStop(Error('资源进程输出超限'));return;}
   parts.push(chunk);if(!quietOutput)process.stderr.write(chunk);
  });
  child.once('error',()=>{if(!child.pid){closed=true;finish(Error('资源工具无法启动'));}else requestStop(Error('资源工具进程错误'));});
  child.once('close',(code,termination)=>{
   closed=true;
   if(!failure&&(code!==0||termination))failure=Error('资源工具失败');
   if(groupExists())requestStop(failure||Error('资源工具后代未结束'));
   if(probe===null)confirm();
  });
 });
}
const root=resolve(dirname(fileURLToPath(import.meta.url)),'..');
const hash=bytes=>createHash('sha256').update(bytes).digest('hex');
const fail=message=>{throw Error('产品资源：'+message);};
const safePath=value=>typeof value==='string'&&value.length>0&&!isAbsolute(value)&&!/[\\\x00-\x1f]/u.test(value)&&value.split('/').every(x=>x&&x!=='.'&&x!=='..');
const inside=(base,path)=>path.startsWith(base+sep);
const stat=async path=>lstat(path).catch(e=>{if(e.code==='ENOENT')return null;throw e;});
async function regular(path){const s=await lstat(path);if(!s.isFile()||s.isSymbolicLink()||s.nlink!==1||await realpath(path)!==path)fail('非独占普通文件：'+path);return s;}
async function directory(path,create=false){if(!isAbsolute(path)||resolve(path)!==path||path===parse(path).root)fail('目录不是准确绝对路径');let at=parse(path).root;for(const name of relative(at,path).split(sep)){at=join(at,name);if(create&&!await stat(at))await mkdir(at,{mode:0o700}).catch(e=>{if(e.code!=='EEXIST')throw e;});const s=await lstat(at);if(!s.isDirectory()||s.isSymbolicLink()||await realpath(at)!==at)fail('目录经过链接或特殊项：'+at);}return path;}
// 普通资源清单保持独占文件要求；工具内部硬链接只由同一扫描器的私有验真现场核对。
async function inventory(base,path=base){return inventoryFiles(base,path);}
function inventoryStatMatches(before,after){
 return ['dev','ino','nlink','mode','uid','gid','size','mtimeMs','ctimeMs','birthtimeMs'].every(key=>before[key]===after[key]);
}
async function inventoryFiles(base,path,toolScan){
 if(toolScan){
  const info=await lstat(path);
  if(!info.isDirectory()||info.isSymbolicLink()||await realpath(path)!==path)fail('工具原件目录边界无效');
  toolScan.entries.push({path,info});
 }
 const files=[];
 for(const name of (await readdir(path)).sort()){
  const file=join(path,name),s=await lstat(file),key=relative(base,file),entry={path:file,info:s};
  if(s.isSymbolicLink()){
   const target=relative(base,await realpath(file));if(!safePath(target))fail('链接越界');
   entry.target=target;files.push({path:key,target});
  }else if(s.isDirectory())files.push({path:key,directory:true},...await inventoryFiles(base,file,toolScan));
  else if(s.isFile()){
   let bytes;
   if(toolScan){
    if(!safePath(key)||await realpath(file)!==file)fail('工具原件文件边界无效');
    const id=s.dev+':'+s.ino;let group=toolScan.hardlinks.get(id);
    if(!group)toolScan.hardlinks.set(id,group={nlink:s.nlink,paths:[]});
    if(group.nlink!==s.nlink)fail('工具清单读取期间硬链接计数变化');
    group.paths.push(file);
    // 不跟随末级链接；打开和读取后均核对同一文件身份，禁止替换或改权限后继续验真。
    const handle=await open(file,constants.O_RDONLY|constants.O_NOFOLLOW);
    try{
     if(!inventoryStatMatches(s,await handle.stat()))fail('工具清单读取期间文件变化');
     bytes=await handle.readFile();
     if(!inventoryStatMatches(s,await handle.stat()))fail('工具清单读取期间文件变化');
    }finally{await handle.close();}
   }else{
    if(s.nlink!==1)fail('共享硬链接');bytes=await readFile(file);
   }
   files.push({path:key,sha256:hash(bytes),executable:Boolean(s.mode&0o111)});
  }else fail('特殊文件');
  if(toolScan&&!s.isDirectory())toolScan.entries.push(entry);
 }
 return files;
}
async function permissions(path,writable){
 if(retainedResourcePath(path))throw Error('资源工具退出未确认，保留工作目录');const s=await lstat(path);if(s.isSymbolicLink())return;if(s.isDirectory()){if(writable)await chmod(path,0o700);for(const name of await readdir(path))await permissions(join(path,name),writable);if(!writable)await chmod(path,0o555);}else await chmod(path,writable?0o600:s.mode&0o111?0o555:0o444);}
function checkedURL(input){const url=new URL(input);if(url.protocol!=='https:'||url.username||url.password||url.hash)fail('来源必须是无凭据HTTPS');return url.href;}
function digestSpec(entry){if(entry.sha256&&/^[a-f0-9]{64}$/u.test(entry.sha256))return ['sha256',Buffer.from(entry.sha256,'hex')];const m=/^(sha256|sha512)-([A-Za-z0-9+/]+={0,2})$/u.exec(entry.integrity||'');if(!m)fail('来源缺少锁定摘要');const b=Buffer.from(m[2],'base64');if(b.toString('base64')!==m[2]||b.length!==({sha256:32,sha512:64}[m[1]]))fail('完整性不是规范摘要');return [m[1],b];}
function verifyBytes(bytes,entry){const [algorithm,digest]=digestSpec(entry);if(!createHash(algorithm).update(bytes).digest().equals(digest))fail('锁定来源摘要不符');return hash(bytes);}
// 不持下载锁。每个候选独占生成，提交使用同对象短锁与排他重命名，竞争者核验同一字节。
// 候选下载、解包和编译归本产品当前target现场；永久原件只在验真提交后接收。
async function resourceWork(work) {
 const owner=await import('./build.mjs');const path=work||owner.temporaryRoot(undefined,'tmp');
 checkScratchPath(path);return directory(join(path,'resource-pending'),true);
}
async function acquireArchive(entry,{store,work,optional,offline=false,fetcher=fetch,signal,maxBytes=4*1024**3}={}){

 const url=checkedURL(entry.url);digestSpec(entry);await directory(store,true);const coordinate=hash(JSON.stringify([url,entry.sha256||entry.integrity]));const target=join(store,coordinate+'.blob');
 const check=async path=>{await regular(path);const b=await readFile(path);if(!b.length||b.length>maxBytes)fail('原件大小超限');verifyBytes(b,entry);return path;};
 if(await stat(target))return check(target);
 // 可选目录只按准确内容摘要读取，绝不读取它的产品白名单或版本登记。
 if(optional&&await stat(optional)){await directory(optional);let digest=entry.sha256;if(!digest&&entry.integrity){const index=join(dirname(optional),'index.json');if(await stat(index)){await regular(index);if((await lstat(index)).size>32*1024**2)fail('可选原件索引超限');const data=await readDependencySupply(optional);digest=data.packages?.flatMap(x=>x.archives||[]).find(x=>x.url===entry.url&&x.integrity===entry.integrity)?.sha256;}}if(digest&&!/^[a-f0-9]{64}$/u.test(digest))fail('可选供给摘要无效');const supplied=digest?join(optional,digest+'.blob'):null;if(supplied&&await stat(supplied)){await check(supplied);const candidate=join(await resourceWork(work),'.'+coordinate+'.'+randomUUID()+'.pending');try{await copyFile(supplied,candidate,constants.COPYFILE_EXCL);await chmod(candidate,0o444);await commitCandidate(candidate,target,{signal,verify:check});return await check(target);}finally{await rm(candidate,{force:true});}}}
 if(offline)fail('离线缺少锁定资源：'+url);signal?.throwIfAborted();let response,current=url;
 for(let i=0;i<=5;i++){response=await fetcher(current,{redirect:'manual',signal});if([301,302,303,307,308].includes(response.status)){await response.body?.cancel();if(i===5)fail('来源重定向超限');current=checkedURL(new URL(response.headers.get('location'),current).href);continue;}break;}
 if(!response?.ok||!response.body)fail('来源获取失败：'+url);const length=Number(response.headers.get('content-length'));if(length>maxBytes)fail('来源声明超限');
 const temporary=join(await resourceWork(work),'.'+coordinate+'.'+randomUUID()+'.pending'),h=await open(temporary,'wx',0o600);let bytes=0;const [algorithm,digest]=digestSpec(entry),checksum=createHash(algorithm);
 try{for await(const chunk of response.body){signal?.throwIfAborted();bytes+=chunk.length;if(bytes>maxBytes)fail('来源数据超限');checksum.update(chunk);let offset=0;while(offset<chunk.length){const n=await h.write(chunk,offset,chunk.length-offset);if(!n.bytesWritten)fail('原件写入中断');offset+=n.bytesWritten;}}if(!bytes||!checksum.digest().equals(digest))fail('锁定来源摘要不符');if(length&&length!==bytes)fail('来源数据不完整');await h.sync();await h.close();signal?.throwIfAborted();await chmod(temporary,0o444);await commitCandidate(temporary,target,{signal,verify:check});return await check(target);}finally{await h.close().catch(()=>{});await rm(temporary,{force:true});await response.body?.cancel().catch(()=>{});}
}
async function downloadTool(entry,target,options){const archive=await acquireArchive(entry,{...options,store:options.store||dirname(target)});await copyFile(archive,target,constants.COPYFILE_EXCL);}
// 解包先解析全部成员并验证闭包，之后才写入；链接不得指向归档外部或成为文件父目录。
async function extractArchive(input,destination,{prefix='',stripRoot=false,signal,tar,maxBytes=8*1024**3}={}){
 await regular(input);if(await stat(destination))fail('解包目标已存在');let bytes=await readFile(input),entries=[];
 const add=(path,type,data,mode=0o644,target)=>{
  path=path.replace(/\/$/u,'').replace(/^\.\//u,'');if(path==='.'||!path)return;const original=path;
  // npm上游存在package/./file与package/file的同字节重复项；只接受普通文件的准确别名。
  if(stripRoot)path=path.split('/').filter(x=>x!=='.').join('/');
  if(!safePath(path))fail('归档成员越界');const previous=entries.find(x=>x.path===path);
  if(previous){if(stripRoot&&type==='file'&&previous.type==='file'&&previous.original!==original&&(original.includes('/./')||previous.original.includes('/./'))&&mode===previous.mode&&data.equals(previous.data))return;fail('归档成员重复或超限');}
  if(entries.length>=400000)fail('归档成员重复或超限');entries.push({path,type,data,mode,target,original});
 };
 if(bytes[0]===0x50&&bytes[1]===0x4b){let end=-1;for(let i=bytes.length-22;i>=Math.max(0,bytes.length-65557);i--)if(bytes.readUInt32LE(i)===0x06054b50){end=i;break;}if(end<0)fail('ZIP目录缺失');const count=bytes.readUInt16LE(end+10);let cursor=bytes.readUInt32LE(end+16),total=0;for(let n=0;n<count;n++){signal?.throwIfAborted();if(bytes.readUInt32LE(cursor)!==0x02014b50)fail('ZIP成员无效');const method=bytes.readUInt16LE(cursor+10),size=bytes.readUInt32LE(cursor+24),compressed=bytes.readUInt32LE(cursor+20),nameLength=bytes.readUInt16LE(cursor+28),extra=bytes.readUInt16LE(cursor+30),comment=bytes.readUInt16LE(cursor+32),mode=bytes.readUInt32LE(cursor+38)>>>16,offset=bytes.readUInt32LE(cursor+42),name=bytes.subarray(cursor+46,cursor+46+nameLength).toString('utf8');if(bytes.readUInt16LE(cursor+8)&1||size===0xffffffff||offset===0xffffffff)fail('ZIP加密或Zip64未声明');total+=size;if(total>maxBytes)fail('ZIP解压超限');if(bytes.readUInt32LE(offset)!==0x04034b50)fail('ZIP本地记录无效');const start=offset+30+bytes.readUInt16LE(offset+26)+bytes.readUInt16LE(offset+28),data=bytes.subarray(start,start+compressed),output=method===0?data:method===8?inflateRawSync(data,{maxOutputLength:Math.max(1,size)}):fail('ZIP压缩方式未声明');if(output.length!==size)fail('ZIP长度不符');add(name,name.endsWith('/')?'directory':(mode&0o170000)===0o120000?'symlink':'file',output,mode||0o644,output.toString());cursor+=46+nameLength+extra+comment;}}
 else{if(bytes[0]===0x1f&&bytes[1]===0x8b)bytes=gunzipSync(bytes,{maxOutputLength:maxBytes});else if(bytes[0]===0xfd&&bytes[1]===0x37){if(!tar)fail('XZ需要已验真基础归档工具');fail('XZ应通过受控tar清单提取');}
  const number=b=>{const v=b.toString().replace(/\0.*$/su,'').trim();if(!/^[0-7]*$/u.test(v))fail('TAR数字无效');return parseInt(v||'0',8);};let pax={},global={};
  for(let cursor=0;cursor+512<=bytes.length;){signal?.throwIfAborted();const block=bytes.subarray(cursor,cursor+512);if(block.every(x=>x===0))break;let sum=0;for(let i=0;i<512;i++)sum+=(i>=148&&i<156)?32:block[i];if(sum!==number(block.subarray(148,156)))fail('TAR头摘要不符');const size=number(block.subarray(124,136)),type=String.fromCharCode(block[156]||48),str=(a,b)=>block.subarray(a,b).toString().replace(/\0.*$/su,''),data=bytes.subarray(cursor+512,cursor+512+size);if(data.length!==size||size>maxBytes)fail('TAR内容超限');cursor+=512+Math.ceil(size/512)*512;
   if(type==='x'||type==='g'){const values={};let at=0;while(at<data.length){const space=data.indexOf(32,at),length=Number(data.subarray(at,space).toString());if(!Number.isInteger(length)||length<=space-at+1||at+length>data.length)fail('PAX长度无效');const record=data.subarray(space+1,at+length-1).toString(),eq=record.indexOf('=');if(eq<1)fail('PAX字段无效');values[record.slice(0,eq)]=record.slice(eq+1);at+=length;}if(type==='g')global={...global,...values};else pax=values;continue;}
   if(type==='L'){pax.path=data.toString().replace(/\0.*$/su,'');continue;}if(type==='K'){pax.linkpath=data.toString().replace(/\0.*$/su,'');continue;}
   const attrs={...global,...pax};pax={};if(Object.keys(attrs).some(x=>x.startsWith('GNU.sparse')))fail('TAR稀疏文件未声明');const name=attrs.path||[str(345,500),str(0,100)].filter(Boolean).join('/'),target=attrs.linkpath||str(157,257);if(!['0','5','2','1'].includes(type))fail('TAR特殊成员未声明');add(name,{'0':'file','5':'directory','2':'symlink','1':'hardlink'}[type],data,number(block.subarray(100,108)),target);
  }
 }
 // npm发行件的顶层名称由上游决定；只接受唯一真实顶层，不能静默丢弃其它成员。
 if(stripRoot){if(prefix)fail('归档根参数冲突');const roots=new Set(entries.map(e=>e.path.split('/')[0]));if(roots.size!==1)fail('归档不是唯一顶层');prefix=[...roots][0];}
 const selected=entries.filter(e=>!prefix||e.path===prefix||e.path.startsWith(prefix+'/')).map(e=>({...e,path:prefix?e.path.slice(prefix.length).replace(/^\//u,''):e.path})).filter(e=>e.path);if(!selected.length)fail('归档根缺失');const table=new Map(selected.map(e=>[e.path,e]));
 for(const entry of selected){let parent=dirname(entry.path);while(parent!=='.'){if(table.has(parent)&&table.get(parent).type!=='directory')fail('归档父目录不是目录');parent=dirname(parent);}if(['symlink','hardlink'].includes(entry.type)){const target=entry.type==='symlink'?posix.normalize(posix.join(posix.dirname(entry.path),entry.target)):prefix?entry.target.replace(new RegExp('^'+prefix+'/'),''):entry.target;if(!safePath(target)||!table.has(target))fail('归档链接越界或缺失');entry.resolved=target;}}
 await mkdir(destination,{mode:0o700});try{for(const entry of selected.filter(x=>['file','directory'].includes(x.type))){signal?.throwIfAborted();const file=join(destination,entry.path);await mkdir(dirname(file),{recursive:true,mode:0o700});if(entry.type==='directory')await mkdir(file,{recursive:true,mode:0o700});else await writeFile(file,entry.data,{flag:'wx',mode:entry.mode&0o111?0o755:0o644});}
 for(const entry of selected.filter(x=>['symlink','hardlink'].includes(x.type))){signal?.throwIfAborted();const file=join(destination,entry.path);await mkdir(dirname(file),{recursive:true});if(entry.type==='hardlink'){const source=table.get(entry.resolved);if(source.type!=='file')fail('硬链接目标不是普通文件');await copyFile(join(destination,entry.resolved),file,constants.COPYFILE_EXCL);}else await symlink(entry.target,file);}await inventory(destination);return destination;}catch(e){await permissions(destination,true);await rm(destination,{recursive:true});throw e;}
}
// XZ由验真POSIX tar处理；双遍清单、无链接父目录与候选物化后的清单一起保护边界。
async function unpack(input,target,{prefix='',signal,foundation}={}){const b=await readFile(input);if(!(b[0]===0xfd&&b[1]===0x37))return extractArchive(input,target,{prefix,signal});const tar=foundation?.tools.tar;if(!tar)fail('XZ缺少已验真tar');const list=await exec(tar,['-tvf',input],{signal,maxBuffer:32*1024**2});if(list.stdout.split('\n').filter(Boolean).some(x=>!/^[-d]/u.test(x)))fail('XZ成员含链接或特殊项');const names=(await exec(tar,['-tf',input],{signal,maxBuffer:32*1024**2})).stdout.split('\n').filter(Boolean);if(names.some(x=>!safePath(x.replace(/\/$/u,'').replace(/^\.\//u,''))))fail('XZ成员越界');await mkdir(target);await exec(tar,['-xkf',input,'--no-same-owner','-C',target],{signal,maxBuffer:2*1024**2});await inventory(target);if(prefix){const child=join(target,prefix),temporary=target+'.root';await directory(child);await rename(child,temporary);await permissions(target,true);await rm(target,{recursive:true});await rename(temporary,target);}return target;}
// 本仓准确工具配方；外部供给登记不能改变这些版本和来源。
const posixRecipe=(()=>{

// 只采集官方macOS发行件中的基础入口；不纳入Git/Python/Ruby等独立登记工具。
const posixNames = Object.freeze([
  'sh', 'bash', 'tar', 'awk', 'sed', 'grep', 'cat', 'chmod', 'cp', 'cut', 'dirname',
  'echo', 'env', 'expr', 'false', 'find', 'head', 'install', 'ln', 'ls', 'mkdir',
  'mktemp', 'mv', 'od', 'paste', 'pwd', 'readlink', 'rm', 'rmdir', 'sleep', 'sort',
  'tail', 'tee', 'test', 'touch', 'tr', 'true', 'uname', 'uniq', 'wc', 'xargs',
  'basename', 'printf', 'date', 'cmp', 'comm', 'dd', 'df', 'du', 'hostname',
  'whoami', 'file', 'stat', 'zip', 'unzip', 'plutil', 'ditto', 'rsync', 'patch',
  'sw_vers', 'chflags', 'cpio', 'gzip', 'gunzip', 'bzip2', 'egrep', 'fgrep',
  'open', 'pgrep', 'pkill', 'lsof', 'zsh', 'ps', 'kill', 'diff', 'yes', 'realpath',
  'which', 'sysctl',
]);
const digest = bytes => createHash('sha256').update(bytes).digest('hex');
const fail = message => { throw new Error('受控POSIX工具：' + message); };

function validatePosixTool(tool) {
  if (tool?.id !== 'posix' || tool.command !== 'bash' || !tool.managed
    || !/^\d+\.\d+(?:\.\d+)?$/u.test(tool.version)
    || tool.source !== 'https://opensource.apple.com/'
    || JSON.stringify(tool.requires) !== JSON.stringify(['node', 'xcode'])
    || tool.archive?.kind !== 'apple-posix'
    || tool.archive.url !== tool.source || tool.archive.root !== 'macos-posix-' + tool.version
    || tool.archive.executable !== 'bin/bash' || !/^[a-f0-9]{64}$/u.test(tool.archive.sha256)
    || tool.dependencies || tool.components || tool.archives) fail('官方发行件登记不完整');
}

// 本机基础工具从已声明的系统位置取得实际入口。
async function locatePosixSources({signal,platform=process.platform}={}){
 if(platform!=='darwin')fail('仅限已授权macOS自举');
 const files=[];
 for(const name of posixNames){signal?.throwIfAborted();const entry=['/bin/','/usr/bin/','/usr/sbin/'].map(prefix=>prefix+name).find(existsSync);if(!entry)fail('缺少官方基础入口：'+name);const path=await realpath(entry),info=await lstat(path);if(!info.isFile()||!(info.mode&0o111))fail('系统输入不是普通可执行文件');files.push({name,path});}
 return files;
}

// 采集不是常态回退：只有本次明确授权的bootstrap调用可建立候选，发布仍由工具事务完成。
async function buildPosixTool({ tool, payload, bootstrap = false, run, signal }) {
  validatePosixTool(tool);
  if (bootstrap !== true) fail('缺少本次首次自举授权');
  const files = await locatePosixSources({signal});
  await mkdir(payload); await mkdir(join(payload, 'bin'));
  for (const file of files) {
    const target = join(payload, 'bin', file.name);
    await copyFile(file.path, target);
    // 系统原签名带平台限制；候选建立本机运行签名。
    await run('/usr/bin/codesign', ['--force', '--sign', '-', '--timestamp=none', target],
      { signal, timeout: 60000, env: { PATH: '', LANG: 'C' } });
  }
  // 发布前真实执行文件、归档及Flutter宿主探测；只接受同对象内的sysctl。
  const bin = join(payload, 'bin'), probe = join(payload, '.probe');
  await mkdir(probe);
  try {
    const result = await run(join(bin, 'bash'), ['--noprofile', '--norc', '-e', '-o', 'pipefail', '-c',
      'printf "controlled-posix-ok\\n" > input; cp input copy; cmp input copy; awk "{print}" copy | sed -n "1p"; tar -cf check.tar input; tar -tf check.tar; which sysctl; sysctl -n hw.optional.arm64; if which citizen-tool-not-installed > /dev/null 2>&1; then exit 1; fi'],
      { cwd: probe, signal, timeout: 60000, env: { PATH: bin, LANG: 'C' } });
    if (result.stdout !== 'controlled-posix-ok\ninput\n' + join(bin, 'sysctl') + '\n1\n') fail('基础工具真实执行回读不符');
  } finally { await rm(probe, {recursive:true,force:true}); }
}

// 调用方只取得同一只读对象内入口；缺失立即失败，绝不从系统或PATH补齐。
async function controlledPosixTools(library, verify) {
  const tool = library.tools.find(entry => entry.id === 'posix');
  if (!tool) fail('基础工具未登记');
  validatePosixTool(tool);
  const installed = await verify(library, tool);
  if (!installed) fail('请先安装已登记基础工具原件');
  const bin = dirname(installed.path), tools = {};
  for (const name of posixNames) {
    const path = join(bin, name), info = await lstat(path);
    if (!info.isFile() || !(info.mode & 0o111) || await realpath(path) !== path) fail('受控入口失效：' + name);
    tools[name] = path;
  }
  return { bin, tools };
}
return {posixNames,buildPosixTool,controlledPosixTools};})();
const sourceRecipe=(()=>{
const controlledPosixTools=(...args)=>productFoundation(...args);

const fail = message => { throw new Error('官方源码工具：' + message); };
const plain = (value, fields) => value && typeof value === 'object' && !Array.isArray(value)
  && Object.keys(value).sort().join(',') === [...fields].sort().join(',');
const hash = value => createHash('sha256').update(value).digest('hex');
const sources = Object.freeze({
  bash: version => { const base=version.split('.').slice(0,2).join('.');return ['https://ftp.gnu.org/gnu/bash/bash-'+base+'.tar.gz','bash-'+base,'bin/bash']; },
  grep: version => ['https://ftp.gnu.org/gnu/grep/grep-' + version + '.tar.xz', 'grep-' + version, 'bin/grep'],
  sed: version => ['https://ftp.gnu.org/gnu/sed/sed-' + version + '.tar.xz', 'sed-' + version, 'bin/sed'],
  m4: version => ['https://ftp.gnu.org/gnu/m4/m4-' + version + '.tar.xz', 'm4-' + version, 'bin/m4'],
  bison: version => ['https://ftp.gnu.org/gnu/bison/bison-' + version + '.tar.xz', 'bison-' + version, 'bin/bison'],
  flex: version => ['https://github.com/westes/flex/releases/download/v' + version + '/flex-' + version + '.tar.gz', 'flex-' + version, 'bin/flex'],
  gettext: version => ['https://ftp.gnu.org/gnu/gettext/gettext-' + version + '.tar.gz', 'gettext-' + version, 'bin/msgfmt'],
  tcl: version => ['https://github.com/tcltk/tcl/releases/download/core-' + version.replaceAll('.', '-') + '/tcl' + version + '-src.tar.gz', 'tcl' + version, 'bin/tclsh' + version.split('.').slice(0, 2).join('.')],
  postgres: version => ['https://ftp.postgresql.org/pub/source/v'+version+'/postgresql-'+version+'.tar.bz2','postgresql-'+version,'bin/postgres'],
  git: version => ['https://www.kernel.org/pub/software/scm/git/git-' + version + '.tar.xz', 'git-' + version, 'bin/git'],
  python: version => ['https://www.python.org/ftp/python/' + version + '/Python-' + version + '.tar.xz', 'Python-' + version, 'bin/python' + version.split('.').slice(0, 2).join('.')],
  perl: version => ['https://www.cpan.org/src/5.0/perl-' + version + '.tar.xz', 'perl-' + version, 'bin/perl'],
  ruby: version => ['https://cache.ruby-lang.org/pub/ruby/' + version.split('.').slice(0, 2).join('.') + '/ruby-' + version + '.tar.gz', 'ruby-' + version, 'bin/ruby'],
  openssl: version => ['https://github.com/openssl/openssl/releases/download/openssl-' + version + '/openssl-' + version + '.tar.gz', 'openssl-' + version, 'bin/openssl'],
  cocoapods: version => ['https://rubygems.org/downloads/cocoapods-' + version + '.gem', '.', 'bin/pod'],
});
const requirements = Object.freeze({
  bash: ['node', 'xcode', 'posix'], grep: ['node', 'xcode', 'posix'], sed: ['node', 'xcode', 'posix'],
  postgres: ['node', 'xcode', 'perl', 'openssl', 'm4', 'bison', 'flex'],
  git: ['node', 'xcode', 'perl', 'python', 'gettext'], python: ['node', 'xcode', 'openssl'],
  m4: ['node', 'xcode'], bison: ['node', 'xcode', 'm4'], flex: ['node', 'xcode', 'm4', 'bison'],
  gettext: ['node', 'xcode', 'perl', 'm4', 'bison', 'flex'], tcl: ['node', 'xcode'],
  perl: ['node', 'xcode'], openssl: ['node', 'xcode', 'perl'],
  ruby: ['node', 'xcode', 'openssl'], cocoapods: ['node', 'xcode', 'ruby', 'git'],
});

// 来源、归档根、执行入口和前置对象形成闭集；运行时绝不解析latest或系统同名命令。
function validateSourceTool(tool) {
  const expected = sources[tool?.id]?.(tool.version);
  if (!expected || !tool.managed || !/^\d+\.\d+(?:\.\d+)?$/u.test(tool.version)
    || JSON.stringify(tool.requires) !== JSON.stringify(requirements[tool.id])
    || tool.archive?.kind !== (tool.id === 'cocoapods' ? 'gem' : 'native-source')
    || JSON.stringify([tool.archive.url, tool.archive.root, tool.archive.executable]) !== JSON.stringify(expected)
    || !/^[a-f0-9]{64}$/u.test(tool.archive.sha256)) fail('官方固定归档或工具前置关系不符');
  const patches = tool.upstream_patches ?? [];
  if (!Array.isArray(patches) || tool.id === 'bash' && patches.length !== Number(tool.version.split('.')[2]||0)
    || tool.id !== 'bash' && patches.length) fail('官方源码补丁闭包不符');
  for (const [i, patch] of patches.entries()) {
    if (!plain(patch, ['url', 'sha256']) || !/^[a-f0-9]{64}$/u.test(patch.sha256)
      || patch.url !== 'https://ftp.gnu.org/gnu/bash/bash-'+tool.version.split('.').slice(0,2).join('.')+'-patches/bash'+tool.version.split('.').slice(0,2).join('')+'-' + String(i + 1).padStart(3, '0')) fail('官方Bash补丁顺序或坐标不符');
  }
  const dependencies = tool.dependencies ?? [];
  if (!Array.isArray(dependencies) || new Set(dependencies.map(entry => entry.name)).size !== dependencies.length) fail('依赖身份重复');
  for (const entry of dependencies) {
    if (tool.id === 'cocoapods') {
      if (!plain(entry, ['name', 'version', 'url', 'sha256']) || entry.name === 'cocoapods'
        || !/^[A-Za-z][A-Za-z0-9_-]*$/u.test(entry.name) || !/^\d+(?:\.\d+){1,3}$/u.test(entry.version)
        || entry.url !== 'https://rubygems.org/downloads/' + entry.name + '-' + entry.version + '.gem') fail('CocoaPods依赖必须是固定官方Gem');
    } else if (tool.id === 'ruby') {
      if (!plain(entry, ['name', 'version', 'url', 'sha256', 'root']) || entry.name !== 'libyaml'
        || entry.url !== 'https://pyyaml.org/download/libyaml/yaml-' + entry.version + '.tar.gz'
        || entry.root !== 'yaml-' + entry.version) fail('Ruby YAML依赖来源不符');
    } else if (tool.id === 'python') {
      if (!plain(entry, ['name', 'version', 'url', 'sha256', 'root']) || entry.name !== 'xz'
        || !/^\d+\.\d+\.\d+$/u.test(entry.version)
        || entry.url !== 'https://github.com/tukaani-project/xz/releases/download/v'+entry.version+'/xz-'+entry.version+'.tar.xz'
        || entry.root !== 'xz-'+entry.version) fail('Python LZMA依赖来源不符');
    } else fail('该工具没有独立外部源码依赖');
    if (!/^[a-f0-9]{64}$/u.test(entry.sha256)) fail('源码依赖摘要缺失');
  }
  if (['ruby', 'python'].includes(tool.id) && dependencies.length !== 1
    || tool.id === 'cocoapods' && (!dependencies.length
      || !dependencies.some(entry => entry.name === 'cocoapods-core' && entry.version === tool.version))) {
    fail('工具运行依赖闭包不完整');
  }
  return true;
}

// Perl实际安装目录只来自本轮官方Configure输出，不猜版本目录或架构名称。
function perlRuntimeLibraries(config, finalPayload, payload) {
  const paths=['installprivlib','installarchlib'].map(name=>{
    const values=[...config.matchAll(new RegExp('^'+name+"='([^']*)'$",'gm'))];
    const value=values.length===1?values[0][1]:null;
    if(!value || !value.startsWith(finalPayload+'/') || value!==resolve(value)
      || /[\x00-\x1f]/u.test(value)) fail('Perl官方安装目录声明无效：'+name);
    return join(payload,relative(finalPayload,value));
  });
  if(new Set(paths).size!==2) fail('Perl普通与架构运行库必须准确隔离');
  return paths;
}

async function directory(path) {
  if (!isAbsolute(path) || path !== resolve(path) || await realpath(path) !== path
    || !(await lstat(path)).isDirectory()) fail('候选目录必须是规范真实目录');
}
async function regular(path, executable = false) {
  const info = await lstat(path);
  if (!info.isFile() || info.isSymbolicLink() || !info.size
    || await realpath(path) !== path || executable && !(info.mode & 0o111)) fail('输入或输出必须是准确普通文件');
}

// 只有此工具对象的候选目录可写；最终前缀固定到已登记摘要，DESTDIR收集后才原子发布。
async function buildSourceTool({ library, tool, source, archive, pending, payload,
  finalPayload, signal, fetcher, exec, verify, apple, bootstrap = false, environment = process.env,
  prepare = prepareSourceDependencies, download = downloadTool }) {
  validateSourceTool(tool);
  const expected = library.pending;
  if (pending !== expected || payload !== join(pending, 'payload')
    || finalPayload !== library.finalPayload
    || archive !== join(pending, 'archive')
    || source !== (tool.archive.kind === 'gem' ? archive : join(pending, 'unpack', tool.archive.root))) fail('候选对象身份不符');
  await directory(pending); await regular(archive);
  if (hash(await readFile(archive)) !== tool.archive.sha256) fail('完整官方归档摘要不符');
  if (tool.archive.kind !== 'gem') await directory(source);
  const installed = {};
  for (const id of tool.requires) {
    const registered = library.tools.find(entry => entry.id === id);
    const result = registered && await verify(library, registered);
    if (!result) fail('缺少已验真前置工具：' + id);
    installed[id] = result.path;
  }
  const foundation = await controlledPosixTools(library, verify, { bootstrap, id: tool.id });
  const selected = await apple(library, { names: ['clang', 'clang++', 'ar', 'make', 'ld', 'nm', 'ranlib', 'strip', 'xcrun', 'otool', 'install_name_tool', 'codesign'], signal });
  const sdkResult = await exec(selected.tools.xcrun, ['--sdk', 'macosx', '--show-sdk-path'], {
    env: { PATH: '', DEVELOPER_DIR: selected.developerDirectory }, signal, timeout: 60_000,
  });
  // xcrun返回包内官方SDK链接；固定到同一包内真实目标，不接纳包外SDK。
  const sdkInput = sdkResult.stdout.trim();
  if (!isAbsolute(sdkInput) || sdkInput !== resolve(sdkInput)) fail('SDK返回路径无效');
  const sdk = await realpath(sdkInput); await directory(sdk);
  if (!sdk.startsWith(selected.developerDirectory + '/')) fail('SDK不属于同一Xcode');
  const work = join(pending, 'probe'), stage = join(work, 'stage');
  await mkdir(work); await mkdir(stage);
  const env = { ...environment, HOME: work, TMPDIR: work, DEVELOPER_DIR: selected.developerDirectory,
    SDKROOT: sdk, MACOSX_DEPLOYMENT_TARGET: library.tools.find(entry=>entry.id==='posix').version,
    PATH: [...new Set([...Object.values(installed).map(dirname), foundation.path,
      dirname(selected.tools.clang), dirname(selected.tools.make)])].join(':'),
    // Clang自带汇编器，避免调用带系统解释器shebang的Xcode as脚本。
    CC: selected.tools.clang, CXX: selected.tools['clang++'], AR: selected.tools.ar,
    CPP: selected.tools.clang + ' -E', LD: selected.tools.ld, AS: selected.tools.clang,
    NM: selected.tools.nm, RANLIB: selected.tools.ranlib, STRIP: selected.tools.strip,
    MAKE: selected.tools.make, PERL: installed.perl ?? '', PYTHON: installed.python ?? '',
    RUBY: installed.ruby ?? '', MAKEINFO: 'true', HELP2MAN: 'true', M4: installed.m4 ?? 'false', BISON: installed.bison ?? 'false',
    YACC: installed.bison ? installed.bison + ' -y' : 'false', FLEX: installed.flex ?? 'false',
    // 官方AC_PROG_LEX用冒号表示未安装Lex，false会误入必须生成扫描器的探测。
    LEX: installed.flex ?? ':', COCOAPODS_DISABLE_STATS: 'true',
  };
  for (const key of Object.keys(env)) if (key.startsWith('DYLD_') || ['NODE_OPTIONS', 'NODE_PATH', 'BASH_ENV', 'ENV', 'SHELLOPTS', 'BASHOPTS', 'CDPATH', 'GLOBIGNORE',
    'PYTHONHOME', 'PYTHONPATH', 'RUBYOPT', 'RUBYLIB', 'GEM_HOME', 'GEM_PATH', 'PERL5OPT', 'PERL5LIB',
    'ARCHFLAGS', 'ARCH', 'CC_FOR_BUILD', 'CXX_FOR_BUILD', 'CROSS_COMPILE', 'LD_PRELOAD', 'LD_LIBRARY_PATH',
    'CFLAGS', 'CXXFLAGS', 'CPPFLAGS', 'LDFLAGS', 'CPATH', 'LIBRARY_PATH', 'PKG_CONFIG_PATH', 'CONFIG_SITE', 'GNUMAKEFLAGS', 'MAKEFLAGS', 'MFLAGS',
    'DESTDIR', 'LD_RUN_PATH', 'CMAKE_TOOLCHAIN_FILE', 'npm_execpath', 'npm_node_execpath', 'NVM_BIN', 'NVM_DIR'].includes(key)) delete env[key];
  // 已验真SDK通过SDKROOT传给Clang，避免Configure把嵌入引号当作路径字节。
  env.CFLAGS = '-O2';
  env.CXXFLAGS = env.CFLAGS;
  env.LDFLAGS = '-Wl,-headerpad_max_install_names';
  env.ARCHFLAGS = '-arch arm64';
  env.SHELL = foundation.tools.sh;
  env.CONFIG_SHELL = foundation.tools.sh;
  env.M4PATH = '';
  env.BISON_PKGDATADIR = installed.bison ? join(dirname(dirname(installed.bison)), 'share/bison') : '';
  env.PKG_CONFIG = 'false';
  env.PKG_CONFIG_LIBDIR = '';
  env.CONFIG_SITE = '';
  const run = (command, args, cwd = tool.id === 'tcl' ? join(source, 'unix') : source, extra = {}) => exec(command, args, {
    cwd, env: { ...env, ...extra }, signal, timeout: 3_600_000, maxBuffer: 8 * 1024 * 1024,
  });
  const originals = await prepare({ library, tool, pending, environment: env, signal, fetcher });
  const upstream = [];
  for (const [i, patch] of (tool.upstream_patches ?? []).entries()) {
    const file = join(pending, 'bash53-' + String(i + 1).padStart(3, '0'));

    await download(patch, file, { fetcher, signal });
    await regular(file);
    if (hash(await readFile(file)) !== patch.sha256) fail('Bash官方补丁原件摘要不符');
    await run(foundation.tools.patch, ['--batch', '--forward', '--fuzz=0', '-p0', '-i', file]);
    upstream.push(file);
  }
  if (tool.id === 'cocoapods') {
    await mkdir(payload); await mkdir(join(payload, 'bin'));
    const manifest = join(work, 'gems.json');
    await writeFile(manifest, JSON.stringify([{ name: 'cocoapods', version: tool.version, file: archive },
      ...tool.dependencies.map(entry => ({ name: entry.name, version: entry.version, file: originals.get(entry.name) }))]), { flag: 'wx' });
    // Ruby读取验真Gem内的真实spec并核对整个运行闭包；无在线解析、系统Gem或忽略版本要求。
    const code = [
      "require 'rubygems'; require 'rubygems/package'; require 'rubygems/installer'; require 'json'; require 'fileutils'",
      "entries = JSON.parse(File.read(ARGV.fetch(0))); home = ARGV.fetch(1)",
      "specs = entries.to_h { |e| s = Gem::Package.new(e.fetch('file')).spec; raise 'Gem identity' unless s.name == e.fetch('name') && s.version.to_s == e.fetch('version') && s.platform == Gem::Platform::RUBY; raise 'Ruby requirement' unless s.required_ruby_version.satisfied_by?(Gem::Version.new(RUBY_VERSION)); [s.name, s] }",
      "specs.each_value { |s| s.runtime_dependencies.each { |d| v = specs[d.name]; raise 'Gem closure' unless v && d.requirement.satisfied_by?(v.version) } }",
      "Gem.use_paths(home, [home]); RbConfig::CONFIG['CC'] = ENV.fetch('CC'); RbConfig::CONFIG['CXX'] = ENV.fetch('CXX'); RbConfig::CONFIG['AR'] = ENV.fetch('AR'); RbConfig::CONFIG['MAKE'] = ENV.fetch('MAKE')",
      "entries.each { |e| Gem::Installer.at(e.fetch('file'), install_dir: home, ignore_dependencies: true, wrappers: false, env_shebang: false, document: [], build_args: ['--disable-system-libffi']).install }",
      // 显式install_dir不更新本进程规格缓存；离线安装后刷新，再逐包回读准确版本。
      "Gem::Specification.reset",
      // 只保留Gem原始脚本与唯一pod包装入口，清除换位后会断开的自动命令链接。
      "specs.each_value { |s| raise 'Gem installed version' unless Gem::Specification.find_by_name(s.name, s.version).version == s.version }; FileUtils.rm_rf(File.join(home, 'cache')); FileUtils.rm_rf(File.join(home, 'bin'))",
    ].join('\n');
    await run(installed.ruby, ['--disable-gems', '-e', code, manifest, join(payload, 'gems')], work,
      { GEM_HOME: join(payload, 'gems'), GEM_PATH: join(payload, 'gems') });
    // 只开放当前CocoaPods与同一受控Ruby自带Gem；不继承外部或用户Gem路径。
    const program = "ENV['GEM_HOME'] = File.expand_path('../gems', File.dirname(ARGV.shift)); ENV['GEM_PATH'] = ENV['GEM_HOME']; ENV['COCOAPODS_DISABLE_STATS'] = 'true'; require 'rubygems'; ENV['GEM_PATH'] = [ENV['GEM_HOME'], Gem.default_dir].join(File::PATH_SEPARATOR); Gem.clear_paths; require 'logger'; load Gem.bin_path('cocoapods', 'pod', " + JSON.stringify(tool.version) + ")";
    const quote = value => "'" + value.replaceAll("'", "'\\''") + "'";
    // 运行入口固定安装时已验真的工具路径，确保Git可用并排除调用方PATH。
    const wrapper = '#!' + foundation.tools.sh + '\nunset RUBYOPT RUBYLIB GEM_HOME GEM_PATH DYLD_LIBRARY_PATH DYLD_INSERT_LIBRARIES\n'
      + 'PATH=' + quote(env.PATH) + '\n'
      + 'exec ' + quote(installed.ruby) + ' --disable-gems -e ' + quote(program) + ' "$0" "$@"\n';
    await writeFile(join(payload, 'bin/pod'), wrapper, { flag: 'wx', mode: 0o555 });
  } else {
    let flags = ['--prefix=' + finalPayload];
    if (tool.id === 'ruby') {
      const yaml = join(work, 'yaml'); await mkdir(yaml);
      await run(foundation.tools.tar, ['-xkf', originals.get('libyaml'), '--no-same-owner', '-C', yaml], work);
      const root = join(yaml, tool.dependencies[0].root); await directory(root);
      const prefix = join(work, 'libyaml');
      await run(foundation.tools.sh, [join(root, 'configure'), '--prefix=' + prefix, '--disable-shared'], root);
      await run(selected.tools.make, ['-j8', 'SHELL=' + foundation.tools.sh], root);
      await run(selected.tools.make, ['install', 'SHELL=' + foundation.tools.sh], root);
      flags.push('--with-baseruby=no', '--with-libyaml-dir=' + prefix,
        '--with-openssl-dir=' + dirname(dirname(installed.openssl)), '--disable-install-doc');
    }
    if (tool.id === 'python') {
      // Xcode SDK不提供lzma头文件；仅编译已锁官方liblzma静态库，不借用户或系统缓存。
      const archiveDirectory = join(work, 'xz'); await mkdir(archiveDirectory);
      await run(foundation.tools.tar, ['-xkf', originals.get('xz'), '--no-same-owner', '-C', archiveDirectory], work);
      const root = join(archiveDirectory, tool.dependencies[0].root); await directory(root);
      const prefix = join(work, 'liblzma');
      await run(foundation.tools.sh, [join(root, 'configure'), '--prefix=' + prefix,
        '--disable-shared', '--enable-static', '--with-pic', '--disable-xz', '--disable-xzdec',
        '--disable-lzmadec', '--disable-lzmainfo', '--disable-scripts', '--disable-doc', '--disable-nls'], root);
      await run(selected.tools.make, ['-j8', 'SHELL=' + foundation.tools.sh], root);
      await run(selected.tools.make, ['install', 'SHELL=' + foundation.tools.sh], root);
      await regular(join(prefix, 'include/lzma.h')); await regular(join(prefix, 'lib/liblzma.a'));
      // Python官方configure支持这两个边界变量；静态链接不携带候选运行库路径。
      env.LIBLZMA_CFLAGS = '-I' + join(prefix, 'include');
      env.LIBLZMA_LIBS = join(prefix, 'lib/liblzma.a');
    }
    if (['bison', 'flex', 'bash', 'grep', 'sed'].includes(tool.id)) flags.push('--disable-nls');
    if (tool.id === 'postgres') {
      // 节点内嵌数据库只启用所需TLS；禁止configure探测包管理器的可选库。
      const openssl=dirname(dirname(installed.openssl));
      env.CPPFLAGS='-I'+join(openssl,'include');env.LDFLAGS+=' -L'+join(openssl,'lib');
      env.PG_SYSROOT=sdk;flags.push('--without-readline','--without-icu','--without-zlib','--with-openssl');
    }
    if (tool.id === 'bash') flags.push('--without-bash-malloc');
    if (tool.id === 'grep') flags.push('--disable-perl-regexp');
    // libfl的yylex由消费者扫描器提供；macOS交付静态库，避免共享库链接未定义符号。
    if (tool.id === 'flex') flags.push('--disable-shared');
    if (tool.id === 'gettext') flags.push('--disable-shared', '--disable-java', '--disable-csharp', '--without-emacs');
    if (tool.id === 'tcl') flags.push('--enable-threads', '--enable-shared');
    if (tool.id === 'python') flags.push('--with-openssl=' + dirname(dirname(installed.openssl)), '--with-openssl-rpath=auto');
    if (tool.id === 'perl') {
      await run(foundation.tools.sh, [join(source, 'Configure'), '-des', '-Dprefix=' + finalPayload,
        '-Dcc=' + selected.tools.clang, '-Dld=' + selected.tools.clang, '-Dar=' + selected.tools.ar,
        '-Duseshrplib', '-Dinstallusrbinperl=n', '-Dccflags=' + env.CFLAGS,
        '-Dldflags=' + env.LDFLAGS, '-Dman1dir=none', '-Dman3dir=none']);
    } else if (tool.id === 'openssl') {
      await run(installed.perl, [join(source, 'Configure'), 'darwin64-arm64-cc', '--prefix=' + finalPayload,
        '--openssldir=/private/etc/ssl', 'no-shared']);
    } else if (tool.id !== 'git') await run(foundation.tools.sh, [join(tool.id === 'tcl' ? join(source, 'unix') : source, 'configure'), ...flags]);
    let curlLibrary;
    if (tool.id === 'git') {
      // 官方Makefile允许显式交付CURL输入；固定同一Xcode SDK，不执行未登记curl-config。
      await regular(join(sdk, 'usr/include/curl/curl.h'));
      // Apple SDK的libcurl.tbd是官方链接；只接受同一SDK目录内的真实普通目标。
      curlLibrary = await realpath(join(sdk, 'usr/lib/libcurl.tbd'));
      if (!curlLibrary.startsWith(sdk + '/usr/lib/')) fail('CURL链接输入不属于同一SDK');
      await regular(curlLibrary);
    }
    const makeArgs = tool.id === 'git' ? ['prefix=' + finalPayload,
      'CURL_CFLAGS=-I' + join(sdk, 'usr/include'), 'CURL_LDFLAGS=' + curlLibrary,
      'NO_FINK=YesPlease', 'NO_DARWIN_PORTS=YesPlease', 'NO_HOMEBREW=YesPlease',
      'GETTEXT_PATH=' + join(dirname(installed.gettext), 'gettext'), 'CPPFLAGS=-I' + dirname(dirname(installed.gettext)) + '/include',
      'LDFLAGS=' + env.LDFLAGS + ' -L' + dirname(dirname(installed.gettext)) + '/lib', 'NO_TCLTK=YesPlease', 'PERL_PATH=' + installed.perl, 'PYTHON_PATH=' + installed.python,
      'CC=' + selected.tools.clang, 'AR=' + selected.tools.ar, 'SHELL_PATH=' + foundation.tools.sh, 'SHELL=' + foundation.tools.sh] : ['SHELL=' + foundation.tools.sh];
    await run(selected.tools.make, ['-j8', ...makeArgs]);
    await run(selected.tools.make, ['DESTDIR=' + stage, ...makeArgs,
      tool.id === 'openssl' ? 'install_sw' : 'install']);
    const staged = join(stage, finalPayload.slice(1)); await directory(staged);
    await rename(staged, payload);
  }
  await regular(join(payload, tool.archive.executable), true);
  if (tool.id === 'grep') for (const name of ['egrep', 'fgrep']) {
    const alias = join(payload, 'bin', name);
    const info = await lstat(alias).catch(error => {if (error.code !== 'ENOENT') throw error; return null;});
    if (info) {if (!info.isFile() || info.isSymbolicLink()) fail('上游grep别名不是普通脚本');await rm(alias);}
  }
  if (tool.id === 'bash') {
    // sh是同一个Bash产物的准确普通副本，版本与回执同属唯一工具对象。
    await writeFile(join(payload, 'bin/sh'), await readFile(join(payload, 'bin/bash')), { flag: 'wx', mode: 0o555 });
  }
  if (tool.archive.kind === 'native-source') {
    const walk = async path => {
      const result = [];
      for (const entry of await readdir(path, { withFileTypes: true })) {
        const file = join(path, entry.name);
        if (entry.isDirectory()) result.push(...await walk(file));
        else if (entry.isFile()) result.push(file);
        else if (!entry.isSymbolicLink()) fail('工具输出包含特殊文件');
      }
      return result;
    };
    const binaries = [];
    for (const file of await walk(payload)) {
      const bytes = await readFile(file);
      if (bytes.length >= 32 && bytes.readUInt32LE(0) === 0xfeedfacf) {
        if (bytes.readUInt32LE(4) !== 0x0100000c) fail('源码工具Mach-O架构不是ARM64');
        binaries.push(file);
      }
    }
    if (!binaries.includes(join(payload, tool.archive.executable))) fail('编译没有生成ARM64工具入口');
    const relocated = new Set();
    for (const file of binaries) {
      const identity = await lstat(file), key = identity.dev + ':' + identity.ino;
      if (relocated.has(key)) continue;
      relocated.add(key);
      const listing = await run(selected.tools.otool, ['-L', file], work);
      let ownRuntime = false;
      for (const line of listing.stdout.split('\n').slice(1)) {
        const dependency = line.trim().split(' (')[0];
        if (!dependency) continue;
        if (dependency.startsWith(finalPayload + '/')) {
          ownRuntime = true;
          await run(selected.tools.install_name_tool, ['-change', dependency,
            '@rpath/' + relative(finalPayload, dependency), file], work);
        } else if (isAbsolute(dependency) && !dependency.startsWith('/usr/lib/')
          && !dependency.startsWith('/System/Library/')
          && !Object.values(installed).some(path => dependency.startsWith(dirname(dirname(path)) + '/'))) {
          fail('工具链接到未验真的外部库');
        }
      }
      // 相对工具对象根定位同一候选与最终对象，避免Perl共享库在原子发布前指向不存在的前缀。
      const loader = '@loader_path' + (relative(dirname(file), payload) ? '/' + relative(dirname(file), payload) : '');
      // 仅链接同对象运行库的入口需要新rpath；系统库模块不能平白扩大Mach-O加载命令。
      if (ownRuntime) await run(selected.tools.install_name_tool, ['-add_rpath', loader, file], work);
      if (file.endsWith('.dylib')) await run(selected.tools.install_name_tool, ['-id',
        '@rpath/' + relative(payload, file), file], work);
      await run(selected.tools.codesign, ['--force', '--sign', '-', '--timestamp=none', file], work);
    }
  }

  // 入口版本不能代替运行闭包验收；解释器必须在原子发布前真实加载所需核心与加密模块。
  const executable = join(payload, tool.archive.executable);
  const probe = async (args, extra = {}) => {
    const result = await run(executable, args, work, extra);
    if (result.stdout.trim() !== 'controlled-' + tool.id + '-ok') fail('解释器真实模块探测未返回准确结果');
  };
  if (tool.id === 'python') {
    await probe(['-I', '-c', 'import ssl,zlib,bz2,lzma,sqlite3,ctypes,json; '
      + 'assert ssl.create_default_context().verify_mode == ssl.CERT_REQUIRED; '
      + 'assert ssl.OPENSSL_VERSION.startswith('+JSON.stringify('OpenSSL '+library.tools.find(value=>value.id==='openssl').version+' ')+'); print("controlled-python-ok")'],
    { PYTHONHOME: payload });
  } else if (tool.id === 'perl') {
    const cores=perlRuntimeLibraries(await readFile(join(source,'config.sh'),'utf8'),finalPayload,payload);
    for(const path of cores) await directory(path);
    await regular(join(cores[1],'Config.pm'));
    await probe(['-MConfig', '-MJSON::PP', '-MEncode', '-MFile::Find', '-e',
      'die "Perl version" unless "$^V" eq '+JSON.stringify('v'+tool.version)+'; print "controlled-perl-ok\\n"'],
      {PERL5LIB:cores.join(':')});
  } else if (tool.id === 'ruby') {
    const versions=(await readdir(join(payload,'lib/ruby'))).filter(value=>/^\d+\.\d+\.\d+$/u.test(value));
    if(versions.length!==1)fail('Ruby核心模块版本目录缺失或不唯一');
    const base=join(payload,'lib/ruby',versions[0]); await directory(base);
    const cores=[base];
    for(const entry of await readdir(base,{withFileTypes:true})) {
      if(!entry.isDirectory()) continue;
      const marker=join(base,entry.name,'rbconfig.rb');
      try {await regular(marker);cores.push(join(base,entry.name));}
      catch(error){if(error.code!=='ENOENT')throw error;}
    }
    if(cores.length!==2) fail('Ruby ARM64核心模块目录缺失或不唯一');
    await probe(['--disable-gems', '-rrubygems', '-rpsych', '-ropenssl', '-rjson', '-e',
      'raise "Ruby version" unless RUBY_VERSION == '+JSON.stringify(tool.version)+'; '
      + 'raise "OpenSSL version" unless OpenSSL::OPENSSL_VERSION.start_with?('+JSON.stringify('OpenSSL '+library.tools.find(value=>value.id==='openssl').version+' ')+'); '
      + 'raise "libyaml missing" if Psych.libyaml_version.empty?; puts "controlled-ruby-ok"'],
      {RUBYLIB:cores.join(':'),GEM_HOME:join(payload,'lib/ruby/gems',versions[0]),GEM_PATH:join(payload,'lib/ruby/gems',versions[0])});
  }
  // 保留官方法律全文与原始源码归档；产品需要解释器运行库时可连同其原始许可一起打包。
  if (tool.archive.kind === 'native-source') {
    const legalNames = (await readdir(source)).filter(name => /^(?:COPYING|LICENSE|LICENCE|NOTICE|Artistic|COPYRIGHT|BSDL|GPL|LEGAL)(?:[._-].*)?$/iu.test(name));
    if (!legalNames.length) fail('官方工具源码缺少根许可全文');
    const directory = join(payload, 'licenses'); await mkdir(directory);
    for (const name of legalNames) {
      const file = join(source, name);
      if ((await lstat(file)).isFile()) await writeFile(join(directory, name), await readFile(file), { flag: 'wx', mode: 0o444 });
    }
    if (!(await readdir(directory)).length) fail('工具根许可必须包含真实法律文件');
  }

  if (tool.id === 'tcl') await writeFile(join(payload, 'version.tcl'), 'puts [info patchlevel]\n', { flag: 'wx', mode: 0o444 });
  // 原件与编译输入回执留在唯一工具对象；运行依赖原件仍归rely，不保留第二份Gem缓存。
  await writeFile(join(payload, tool.archive.kind === 'gem' ? 'source.gem' : 'source.archive'),
    await readFile(archive), { flag: 'wx', mode: 0o444 });
  if (upstream.length) {
    const directory = join(payload, 'upstream-patches'); await mkdir(directory);
    for (const [i, file] of upstream.entries()) await writeFile(join(directory, 'bash53-' + String(i + 1).padStart(3, '0')),
      await readFile(file), { flag: 'wx', mode: 0o444 });
    // 原件只保留在回执覆盖的payload内，清除同一安装事务产生的临时重复文件。
    for (const [i, file] of upstream.entries()) {
      await rm(file);
    }
  }
  }
return {buildSourceTool,validateSourceTool};})();
const flutterRecipe=(()=>{

const execute = exec;

const digest = value => createHash('sha256').update(value).digest('hex');
const fail = message => { throw new Error('Flutter受控修订：' + message); };
const safePath = value => typeof value === 'string' && value.length > 0 && !isAbsolute(value)
  && !/[\\\x00-\x1f]/u.test(value) && value.split('/').every(part => part && part !== '.' && part !== '..');

// 产品资源阶段先验真共享工具并取得任务目录；这里只生成该任务的配置，不改共享SDK。
async function prepareFlutterTaskTools(root, work, platform, { signal, environment = {} } = {}) {
  signal?.throwIfAborted();
  if (!['android', 'ios', 'macos', 'sdk'].includes(platform)) return {};
  if (await realpath(root) !== root || await realpath(work) !== work
    || work === root || work.startsWith(root + '/') || root.startsWith(work + '/')) fail('工具任务目录不安全');
  const directory = join(work, 'flutter-tools');
  const prepareJava = async () => {
    if (!environment.JAVA_HOME) return {};
    if (!isAbsolute(environment.JAVA_HOME) || /[\r\n\x00]/u.test(environment.JAVA_HOME)) fail('受控Java路径无效');
    // Flutter的jdk-dir优先于Android Studio；配置仅写入当前任务HOME，不影响用户设置。
    await writeFile(join(work, '.flutter_settings'), JSON.stringify({ 'jdk-dir': environment.JAVA_HOME }), { flag: 'wx', mode: 0o600 });
    return { HOME: work };
  };
  if (platform === 'sdk') return prepareJava();
  if (platform !== 'android') {
    signal?.throwIfAborted();
    await mkdir(directory);
    const quote = value => "'" + value.replaceAll("'", "'\\''") + "'";
    // 使用验真rsync，额外处理其未落实的副本权限；入口不覆盖或替换系统工具。
    await writeFile(join(directory, 'rsync'), '#!'+environment.PRODUCT_BASH_BIN+'\n'
      // 验真rsync的本机接收端会经PATH再次调用rsync；协议端必须继承原始stdio，不能进入Node缓冲执行。
      + 'if [ "${1:-}" = "--server" ]; then exec '+quote(environment.PRODUCT_RSYNC_BIN)+' "$@"; fi\n'
      + 'exec ' + quote(process.execPath) + ' '
      + quote(fileURLToPath(import.meta.url)) + ' rsync "$@"\n', { flag: 'wx', mode: 0o700 });
    signal?.throwIfAborted();
    return { ...await prepareJava(), PATH: directory };
  }
  const gradleHome = environment.GRADLE_HOME;
  if (!gradleHome || !isAbsolute(gradleHome) || /[\r\n\x00]/u.test(gradleHome)) fail('Android任务缺少产品资源阶段受控Gradle');
  if (!environment.JAVA_HOME || !isAbsolute(environment.JAVA_HOME) || /[\r\n\x00]/u.test(environment.JAVA_HOME)) fail('Android任务缺少产品资源阶段受控Java');
  // 先校验唯一配置再生成插件目录；链接、双配置与错误入口都不得留下半成品。
  const candidates = [];
  for (const name of ['settings.gradle.kts', 'settings.gradle']) {
    const path = join(work, 'android', name);
    const info = await lstat(path).catch(error => { if (error.code === 'ENOENT') return null; throw error; });
    if (info) candidates.push({ path, info });
  }
  if (candidates.length !== 1) fail('Android任务必须只有一份settings配置');
  const { path: settings, info } = candidates[0];
  if (!info.isFile() || info.nlink !== 1 || info.size > 1024 * 1024 || await realpath(settings) !== settings) fail('Android任务配置不是独占普通文件');
  const handle = await open(settings, constants.O_RDWR | constants.O_NOFOLLOW);
  try {
    const opened = await handle.stat();
    if (opened.dev !== info.dev || opened.ino !== info.ino) fail('Android任务配置已被替换');
    const input = await handle.readFile('utf8');
    const pattern = /includeBuild\((["'])\$flutterSdkPath\/packages\/flutter_tools\/gradle\1\)/gu;
    if ([...input.matchAll(pattern)].length !== 1) fail('Android配置缺少唯一Flutter插件入口');
    signal?.throwIfAborted();
    const target = await prepareGradle(root, work, { signal });
    const current = await lstat(settings);
    if (await realpath(settings) !== settings || current.dev !== info.dev || current.ino !== info.ino
      || current.nlink !== 1 || await readFile(settings, 'utf8') !== input) fail('Android任务配置已被替换或修改');
    signal?.throwIfAborted();
    // 使用已核对的文件描述符写入，不能重新打开后来替换的路径。
    const content = Buffer.from(input.replace(pattern, 'includeBuild(' + JSON.stringify(target) + ')'));
    let offset = 0;
    while (offset < content.length) {
      const { bytesWritten } = await handle.write(content, offset, content.length - offset, offset);
      if (!bytesWritten) fail('Android任务配置写入未完成');
      offset += bytesWritten;
    }
    await handle.truncate(content.length);
    signal?.throwIfAborted();
  } finally { await handle.close(); }
  // Flutter固定调用工程gradlew；仅替换本任务的入口链接，不触碰源工程或共享SDK。
  const wrapper = join(work, 'android/gradlew');
  const existing = await lstat(wrapper).catch(error => { if (error.code === 'ENOENT') return null; throw error; });
  if (existing && !existing.isSymbolicLink() && (!existing.isFile() || existing.nlink !== 1)) fail('Gradle任务入口不是独占文件');
  signal?.throwIfAborted();
  if (existing) {
    const current = await lstat(wrapper);
    if (current.dev !== existing.dev || current.ino !== existing.ino) fail('Gradle任务入口已被替换');
    await rm(wrapper);
  }
  const quote = value => "'" + value.replaceAll("'", "'\\''") + "'";
  await writeFile(wrapper, '#!'+environment.PRODUCT_BASH_BIN+'\n# 只执行本产品验真的工具，不下载Wrapper分发。\nexport JAVA_HOME='
    + quote(environment.JAVA_HOME) + '\nexec '
    + quote(join(gradleHome, 'bin/gradle')) + ' "$@"\n', { flag: 'wx', mode: 0o700 });
  signal?.throwIfAborted();
  return prepareJava();
}

// 只修正受控SDK复制到本任务的framework/dSYM；不跟随链接chmod共享原件。
async function copyFlutterArtifact(args, environment = process.env, run = execute) {
  const root = environment.FLUTTER_ROOT, work = environment.PRODUCT_WORK_DIR;
  if (!root || !work || await realpath(root) !== root || await realpath(work) !== work
    || work === root || work.startsWith(root + '/') || root.startsWith(work + '/')) fail('引擎复制缺少安全任务目录');
  const source = args.at(-2), destination = args.at(-1);
  let copied;
  if (source && isAbsolute(source) && source.startsWith(root + '/')
    && /\.(?:framework|dSYM)\/?$/u.test(source)) {
    const output = resolve(destination || '.');
    if (!output.startsWith(work + '/') || await realpath(output) !== output) fail('引擎副本不属于当前任务');
    if (!safePath(relative(root, await realpath(source)))) fail('引擎原件越界');
    copied = source.endsWith('/') ? output : join(output, source.split('/').at(-1));
    const existing = await lstat(copied).catch(error => { if (error.code === 'ENOENT') return null; throw error; });
    if (existing?.isSymbolicLink()) fail('引擎目标不能是符号链接');
    // 复用目标可能含硬链接，必须在rsync写入前拒绝，不能等复制后才保护原件。
    const inspect = async path => {
      const info = await lstat(path);
      if (info.isSymbolicLink()) return;
      if (await realpath(path) !== path || (!info.isDirectory() && (!info.isFile() || info.nlink !== 1))) fail('引擎目标不是独占生成物');
      if (info.isDirectory()) for (const name of await readdir(path)) await inspect(join(path, name));
    };
    if (existing) await inspect(copied);
    if (args.some(value => ['--inplace', '--keep-dirlinks', '--copy-dirlinks', '--copy-links', '-K', '-k', '-L'].includes(value))) fail('引擎复制禁止跟随目标链接');
  }
  if(!environment.PRODUCT_RSYNC_BIN||!environment.PRODUCT_BASH_BIN)fail('缺少验真同步和Shell入口');
  const result = await run(environment.PRODUCT_RSYNC_BIN, args, { env: environment, maxBuffer: 8 * 1024 * 1024 });
  if (copied) {
    const writable = async path => {
      const info = await lstat(path);
      if (info.isSymbolicLink()) {
        if (path === copied) fail('引擎目标已被替换为链接');
        return;
      }
      if (await realpath(path) !== path || (!info.isDirectory() && (!info.isFile() || info.nlink !== 1))) fail('引擎副本不是独占生成物');
      // 持有无跟随描述符并核对inode，chmod不重新打开可能已被替换的路径。
      const handle = await open(path, constants.O_RDONLY | constants.O_NOFOLLOW);
      try {
        const current = await handle.stat();
        if (current.dev !== info.dev || current.ino !== info.ino || await realpath(path) !== path) fail('引擎副本路径已被替换');
        await handle.chmod((current.mode & 0o777) | 0o200);
      } finally { await handle.close(); }
      if (info.isDirectory()) for (const name of await readdir(path)) await writable(join(path, name));
    };
    await writable(copied);
  }
  return result;
}

function verifyFlutterVersion(actual, tool) {
  if (actual?.frameworkVersion !== tool.version || actual?.frameworkRevision !== tool.patch.source.split('/').at(-1)) fail('实际Flutter版本不符或官方提交不符');
}

// 调用方先验真SDK并占有本端工作目录；这里只生成Gradle配置，不复制SDK或业务源码。
async function prepareGradle(root, work, { signal } = {}) {
  signal?.throwIfAborted();
  if (!isAbsolute(root) || !isAbsolute(work) || await realpath(root) !== root || await realpath(work) !== work
    || work === root || work.startsWith(root + '/') || root.startsWith(work + '/')) fail('Gradle工作目录必须独立于SDK原件');
  const sdk = join(root, 'packages/flutter_tools/gradle');
  const source = join(sdk, 'src');
  if (await realpath(source) !== source || !(await lstat(source)).isDirectory()) fail('Gradle源码必须位于SDK原件内');
  const files = [];
  for (const name of ['settings.gradle.kts', 'build.gradle.kts']) {
    const path = join(sdk, name), info = await lstat(path);
    if (!info.isFile() || info.size > 1024 * 1024 || await realpath(path) !== path) fail('Gradle配置不是SDK内受限普通文件');
    files.push([name, await readFile(path)]);
  }
  files.push(['gradle.properties', Buffer.from('# Kotlin只在本任务Gradle进程编译，不创建用户目录守护进程状态。\n'
    + 'kotlin.compiler.execution.strategy=in-process\nkotlin.daemon.useFallbackStrategy=false\n')]);
  const directory = join(work, 'flutter-gradle');
  // mkdir排他创建，已有目录不属于本次准备，禁止接管或清理。
  signal?.throwIfAborted();
  await mkdir(directory, { mode: 0o700 });
  const identity = await lstat(directory);
  const assertDirectory = async () => {
    const current = await lstat(directory);
    if (await realpath(directory) !== directory || current.dev !== identity.dev || current.ino !== identity.ino) fail('Gradle配置目录已被替换，保留现场');
  };
  try {
    for (const [name, content] of files) {
      signal?.throwIfAborted();
      await assertDirectory();
      const handle = await open(join(directory, name), 'wx', 0o600);
      try { await handle.writeFile(content); } finally { await handle.close(); }
    }
    await assertDirectory();
    signal?.throwIfAborted();
    await symlink(source, join(directory, 'src'), 'dir');
    signal?.throwIfAborted();
    return directory;
  } catch (error) {
    await assertDirectory();
    await rm(directory, { recursive: true });
    throw error;
  }
}

function flutterEnvironment(root, env) {
  const work = env.PRODUCT_WORK_DIR;
  const result = { ...env, HOME: work, USERPROFILE: work, TMPDIR: work,
    XDG_CONFIG_HOME: join(work, 'config'), XDG_CACHE_HOME: join(work, 'cache'),
    PUB_CACHE: join(root, 'bin/cache/pub'), BOT: 'true', FLUTTER_ROOT: root,
    // 官方开关在AI环境也返回NoOpAnalytics，防止首次提示混入机器JSON。
    FLUTTER_SUPPRESS_ANALYTICS: 'true' };
  for (const name of ['FLUTTER_ALREADY_LOCKED', 'FLUTTER_TOOL_ARGS', 'FLUTTER_HOST', 'PUB_HOSTED_URL', 'FLUTTER_STORAGE_BASE_URL']) delete result[name];
  return result;
}

// Windows直接执行SDK自带Dart与快照，避免Node把批处理文件当作原生可执行文件。
function flutterCommand(root, args, platform = process.platform) {
  const path = platform === 'win32' ? win32 : posix;
  if (!['win32', 'darwin', 'linux'].includes(platform)) fail('不支持的Flutter宿主');
  return platform === 'win32'
    ? [path.join(root, 'bin/cache/dart-sdk/bin/dart.exe'),
      [path.join(root, 'bin/cache/flutter_tools.snapshot'), ...args]]
    : [path.join(root, 'bin/flutter'), args];
}

async function checkFlutter(root, tool, env, { signal, run = execute } = {}) {
  const work = env.PRODUCT_WORK_DIR;
  if (!work || await realpath(work) !== work || work === root || work.startsWith(root + sep)) fail('运行状态不能写入SDK原件');
  const environment = flutterEnvironment(root, env);
  const [command, args] = flutterCommand(root, ['--suppress-analytics', '--version', '--machine']);
  const result = await run(command, args, {
    cwd: work, env: environment, signal, timeout: 60_000, maxBuffer: 1024 * 1024,
  });
  const actual = JSON.parse(result.stdout);
  verifyFlutterVersion(actual, tool);
}

// 补丁只接受完整基线与结果摘要；行号、上下文任一不符都拒绝，不做模糊匹配。
function parsePatch(text) {
  if (!text) fail('补丁为空');
  if (typeof text !== 'string' || !text.endsWith('\n')) fail('补丁必须完整换行结束');
  const lines = text.split('\n');
  const files = []; let at = 0;
  while (at < lines.length && !lines[at].startsWith('diff --git ')) {
    if (lines[at] && !lines[at].startsWith('#')) fail('补丁头部存在未登记内容');
    at++;
  }
  while (at < lines.length - 1) {
    const header = /^diff --git a\/(\S+) b\/(\S+)$/u.exec(lines[at++]);
    if (!header || header[1] !== header[2] || !safePath(header[1])) fail('补丁文件路径无效');
    const path = header[1];
    if (files.some(file => file.path === path)) fail('补丁文件重复');
    const hashes = /^index ([a-f0-9]{64})\.\.([a-f0-9]{64})$/u.exec(lines[at++]);
    if (!hashes || lines[at++] !== '--- a/' + path || lines[at++] !== '+++ b/' + path) fail('缺少完整文件摘要');
    const hunks = [];
    while (at < lines.length - 1 && !lines[at].startsWith('diff --git ')) {
      const hunk = /^@@ -(\d+),(\d+) \+(\d+),(\d+) @@$/u.exec(lines[at++]);
      if (!hunk) fail('补丁区块格式错误');
      const old = [], next = [];
      while (at < lines.length - 1 && /^[ +\-]/u.test(lines[at])) {
        const line = lines[at++];
        if (line[0] !== '+') old.push(line.slice(1));
        if (line[0] !== '-') next.push(line.slice(1));
      }
      if (old.length !== Number(hunk[2]) || next.length !== Number(hunk[4])) fail('补丁区块行数错误');
      hunks.push({ oldLine: Number(hunk[1]), nextLine: Number(hunk[3]), old, next });
    }
    if (!hunks.length) fail('补丁文件没有改动区块');
    files.push({ path, before: hashes[1], after: hashes[2], hunks });
  }
  if (!files.length) fail('补丁为空');
  return files;
}

function transformFile(input, file) {
  if (digest(input) !== file.before) fail('源码基线摘要不符：' + file.path);
  const lines = input.toString('utf8').split('\n');
  if (lines.pop() !== '') fail('源码必须使用末尾换行');
  const result = []; let cursor = 0;
  for (const hunk of file.hunks) {
    const offset = hunk.oldLine === 0 ? 0 : hunk.oldLine - 1;
    if (offset < cursor || offset > lines.length) fail('补丁区块重叠或越界');
    result.push(...lines.slice(cursor, offset));
    if ((hunk.nextLine === 0 ? 0 : hunk.nextLine - 1) !== result.length
      || JSON.stringify(lines.slice(offset, offset + hunk.old.length)) !== JSON.stringify(hunk.old)) fail('补丁上下文不符');
    result.push(...hunk.next); cursor = offset + hunk.old.length;
  }
  result.push(...lines.slice(cursor));
  const output = Buffer.from(result.join('\n') + '\n');
  if (digest(output) !== file.after) fail('修订结果摘要不符：' + file.path);
  return output;
}

async function readPatch(root, patch) {
  if (!patch || !safePath(patch.path) || !/^[a-f0-9]{64}$/u.test(patch.sha256)
    || !/^https:\/\/github\.com\/flutter\/flutter\/commit\/[a-f0-9]{40}$/u.test(patch.source)) fail('修订登记不完整');
  const path = join(root, patch.path);
  if (!(await lstat(path)).isFile() || await realpath(path) !== path) fail('补丁必须为库内普通文件');
  const input = await readFile(path);
  if (input.length > 4 * 1024 * 1024 || digest(input) !== patch.sha256) fail('补丁摘要不符');
  return parsePatch(input.toString('utf8'));
}

// 只在受控安装器独占的候选对象中准备；应用阶段不允许产品重新生成SDK快照。
async function prepareFlutter(root, { tool, files, env, signal, run = execute }) {
  root = resolve(root);
  if (await realpath(root) !== root || !env?.PRODUCT_WORK_DIR) fail('准备环境不完整');
  const work = resolve(env.PRODUCT_WORK_DIR);
  if (await realpath(work) !== work || work === root || work.startsWith(root + sep)) fail('准备状态不能写入SDK原件');
  const cache = join(root, 'bin/cache');
  const version = JSON.parse(await readFile(join(cache, 'flutter.version.json'), 'utf8'));
  if (version.frameworkVersion !== tool.version || version.frameworkRevision !== tool.patch.source.split('/').at(-1)) fail('SDK版本或官方提交不符');
  await applyPatch(root, files);
  await prepareFlutterSnapshot(root, { tool, files, env, signal, run });
}

// 安装候选的所有源码必须达到唯一目标后才重建快照；已安装原件不得原位修订。
async function prepareFlutterSnapshot(root, { tool, files, env, signal, run = execute, offline = false }) {
  root = resolve(root);
  const work = env?.PRODUCT_WORK_DIR && resolve(env.PRODUCT_WORK_DIR);
  if (await realpath(root) !== root || !work || await realpath(work) !== work
    || work === root || work.startsWith(root + sep)) fail('准备状态不能写入SDK原件');
  const cache = join(root, 'bin/cache');
  const version = JSON.parse(await readFile(join(cache, 'flutter.version.json'), 'utf8'));
  if (version.frameworkVersion !== tool.version || version.frameworkRevision !== tool.patch.source.split('/').at(-1)) fail('SDK版本或官方提交不符');
  if (!Array.isArray(files) || !files.length) fail('快照缺少完整修订输入');
  for (const file of files) {
    const path = join(root, file.path);
    if (!safePath(file.path) || !(await lstat(path)).isFile() || await realpath(path) !== path
      || digest(await readFile(path)) !== file.after) fail('快照源码未达到修订结果：' + file.path);
  }
  const tools = join(root, 'packages/flutter_tools');
  const dart = join(cache, 'dart-sdk/bin', process.platform === 'win32' ? 'dart.exe' : 'dart');
  const lock = await readFile(join(tools, 'pubspec.lock'));
  const environment = flutterEnvironment(root, env);
  // 现有验真对象维护使用离线缓存；首次Runner准备仍由正式Pub按锁获取工具依赖。
  await run(dart, ['pub', 'get', '--enforce-lockfile', '--no-precompile', ...(offline ? ['--offline'] : [])], {
    cwd: tools, env: environment, signal, timeout: 600_000, maxBuffer: 2 * 1024 * 1024,
  });
  if (!(await readFile(join(tools, 'pubspec.lock'))).equals(lock)) fail('工具依赖锁文件被改变');
  const configPath = join(tools, '.dart_tool/package_config.json');
  // Pub生成配置后再校验真实位置，不能沿着符号链接改写当前候选之外的文件。
  if (!(await lstat(configPath)).isFile() || await realpath(configPath) !== configPath) fail('Dart包配置不是候选内普通文件');
  const config = JSON.parse(await readFile(configPath, 'utf8'));
  if (!Array.isArray(config.packages) || !config.packages.length) fail('Dart包配置不完整');
  // 候选对象最终会原位改名；包地址全部改为对象内部相对地址，不保留准备目录绝对路径。
  for (const item of config.packages) {
    const path = fileURLToPath(new URL(item.rootUri, pathToFileURL(configPath)));
    const canonical = await realpath(path);
    if (!canonical.startsWith(root + sep)) fail('工具依赖指向共享对象之外');
    item.rootUri = relative(dirname(configPath), canonical).split(sep).map(encodeURIComponent).join('/') + '/';
  }
  const handle = await open(configPath, 'w');
  try { await handle.writeFile(JSON.stringify(config)); } finally { await handle.close(); }
  const snapshot = join(cache, 'flutter_tools.snapshot');
  // 旧快照属于尚未发布的当前候选，必须删除后由修订源码重新产生，失败不保留可运行旧工具。
  await rm(snapshot, { force: true });
  await run(dart, ['--snapshot=' + snapshot, '--snapshot-kind=app-jit', '--packages=' + configPath,
    join(tools, 'bin/flutter_tools.dart'), '--version', '--machine'], {
    cwd: work, env: environment, signal, timeout: 300_000, maxBuffer: 2 * 1024 * 1024,
  });
  if (!(await lstat(snapshot)).isFile() || !(await lstat(snapshot)).size) fail('修订快照没有生成');
  await checkFlutter(root, tool, environment, { signal, run });
}

async function applyPatch(root, files) {
  root = resolve(root);
  if (await realpath(root) !== root || !(await lstat(root)).isDirectory()) fail('准备目录不安全');
  const lockPath = join(root, 'flutter.lock');
  const lock = await open(lockPath, 'wx', 0o600);
  try {
    const prepared = [];
    for (const file of files) {
      if (!safePath(file.path)) fail('源码路径越界');
      const path = join(root, file.path);
      if (!(await lstat(path)).isFile() || await realpath(path) !== path) fail('源码不是准备目录内的普通文件');
      prepared.push({ path, output: transformFile(await readFile(path), file) });
    }
    for (const { path, output } of prepared) {
      const temporary = path + '.pending'; let owned = false;
      try {
        const handle = await open(temporary, 'wx', 0o600); owned = true;
        try { await handle.writeFile(output); await handle.sync(); } finally { await handle.close(); }
        await rename(temporary, path); owned = false;
      } finally { if (owned) await rm(temporary); }
    }
  } finally { await lock.close(); await rm(lockPath); }
}

return {prepareFlutterTaskTools,copyFlutterArtifact,parsePatch,prepareFlutter,checkFlutter};})();
const appleSystemTools = Object.freeze({
  codesign: '/usr/bin/codesign', security: '/usr/bin/security',
  xcrun: '/usr/bin/xcrun', 'xcode-select': '/usr/bin/xcode-select', xattr: '/usr/bin/xattr',
});
const appleBundleTools = new Set([
  'xcodebuild', 'make', 'clang', 'clang++', 'swift', 'swiftc', 'ar', 'ld', 'as', 'nm', 'ranlib', 'strip', 'lipo', 'libtool',
  'otool', 'install_name_tool', 'codesign_allocate', 'devicectl', 'xctrace', 'actool', 'ibtool', 'notarytool', 'llvm-nm',
]);
async function verifyAppleTools(library,{names=['xcodebuild'],signal,run=exec,environment=process.env}={}){
  const wanted=library.tools.find(tool=>tool.id==='xcode');if(!wanted||!Array.isArray(names)||names.some(name=>!appleBundleTools.has(name)&&!Object.hasOwn(appleSystemTools,name)))fail('Apple工具需求无效');

  const env=cleanEnvironment(environment),developerDirectory=(await run('/usr/bin/xcode-select',['-p'],{env,signal})).stdout.trim();await directory(developerDirectory);
  const tools={};for(const name of names){
    const path=appleSystemTools[name]||(name==='xcodebuild'?join(developerDirectory,'usr/bin/xcodebuild'):(await run('/usr/bin/xcrun',['--find',name],{env:{...env,DEVELOPER_DIR:developerDirectory},signal})).stdout.trim());
    if(!isAbsolute(path)||resolve(path)!==path)fail('Apple工具入口不是规范绝对路径');
    if(Object.hasOwn(appleSystemTools,name)){await regular(path);tools[name]=path;continue;}
    // Xcode自带的clang++等入口可为包内链接；必须核对入口与最终普通文件均归当前Xcode。
    if(!inside(developerDirectory,path))fail('Apple工具入口越出当前Xcode');
    const entry=await lstat(path),target=await realpath(path);
    if((!entry.isFile()&&!entry.isSymbolicLink())||!inside(developerDirectory,target))fail('Apple工具目标越出当前Xcode');
    await regular(target);
    tools[name]=path;
  }
  return {developerDirectory,version:wanted.version,tools};
 }

const toolDefinitions=[{"id":"git","title":"Git","version":"2.54.0","source":"https://git-scm.com/download/mac","command":"git","archive":{"url":"https://www.kernel.org/pub/software/scm/git/git-2.54.0.tar.xz","sha256":"f689162364c10de79ef89aa8dbf48731eb057e34edbbd20aca510ce0154681a3","root":"git-2.54.0","executable":"bin/git","kind":"native-source"},"managed":true,"requires":["node","xcode","perl","python","gettext"]},{"id":"node","title":"Node.js","version":"25.2.1","source":"https://nodejs.org/dist/v25.2.1/SHASUMS256.txt","command":"node","archives":{"linux-arm":{"url":"https://nodejs.org/dist/v25.2.1/node-v25.2.1-linux-arm64.tar.xz","sha256":"75f910b5234d3ee324ceebcf41e2c3c221c4c2225463a02ecd685b884155e0f6","root":"node-v25.2.1-linux-arm64"},"linux-amd":{"url":"https://nodejs.org/dist/v25.2.1/node-v25.2.1-linux-x64.tar.xz","sha256":"b9f6a97e81c89a9df45526b4f86dafdccaf12b82295f7bf35bdb2b0f5e68744f","root":"node-v25.2.1-linux-x64"},"windows":{"url":"https://nodejs.org/dist/v25.2.1/node-v25.2.1-win-x64.zip","sha256":"f97ba75ead7720652f3925d9cf8661e083a28c6b98ea77acc83903d77a9dd688","root":"node-v25.2.1-win-x64"}},"archive":{"url":"https://nodejs.org/dist/v25.2.1/node-v25.2.1-darwin-arm64.tar.gz","sha256":"be87e21bd235a451fad02c89e5bf7cb17e206e4cd89dd5664f20d19e7dfde6f9","root":"node-v25.2.1-darwin-arm64","executable":"bin/node","kind":"extract"},"managed":true,"requires":[]},{"id":"python","title":"Python","version":"3.14.3","source":"https://www.python.org/downloads/macos/","command":"python3","archive":{"url":"https://www.python.org/ftp/python/3.14.3/Python-3.14.3.tar.xz","sha256":"a97d5549e9ad81fe17159ed02c68774ad5d266c72f8d9a0b5a9c371fe85d902b","root":"Python-3.14.3","executable":"bin/python3.14","kind":"native-source"},"managed":true,"requires":["node","xcode","openssl"],"dependencies":[{"name":"xz","version":"5.8.2","url":"https://github.com/tukaani-project/xz/releases/download/v5.8.2/xz-5.8.2.tar.xz","sha256":"890966ec3f5d5cc151077879e157c0593500a522f413ac50ba26d22a9a145214","root":"xz-5.8.2"}]},{"id":"rust","title":"Rust","version":"1.97.1","source":"https://static.rust-lang.org/dist/channel-rust-1.97.1.toml","command":"rustc","archive":{"url":"https://static.rust-lang.org/dist/2026-07-16/rust-1.97.1-aarch64-apple-darwin.tar.xz","sha256":"c9748cc86107734a2a024069908a895de7caa2d37062fb641eef9f756938ace2","root":"rust-1.97.1-aarch64-apple-darwin","executable":"bin/rustc","kind":"rust"},"components":[{"target":"aarch64-linux-android","url":"https://static.rust-lang.org/dist/2026-07-16/rust-std-1.97.1-aarch64-linux-android.tar.xz","sha256":"d664a49fb80d125d68f779112aa97d2a3f5def5f807a35540aa77fce0b350c4f","root":"rust-std-1.97.1-aarch64-linux-android"},{"target":"aarch64-apple-ios","url":"https://static.rust-lang.org/dist/2026-07-16/rust-std-1.97.1-aarch64-apple-ios.tar.xz","sha256":"1d58e856a295852a419f92445fe6b3db268049eb6222a672d52c52de52f36631","root":"rust-std-1.97.1-aarch64-apple-ios"},{"target":"aarch64-apple-ios-sim","url":"https://static.rust-lang.org/dist/2026-07-16/rust-std-1.97.1-aarch64-apple-ios-sim.tar.xz","sha256":"3deb094abb0f7382aad761b8e1e89cebd68bec0591d39e313330ef709b99f6e4","root":"rust-std-1.97.1-aarch64-apple-ios-sim"},{"target":"aarch64-unknown-linux-gnu","url":"https://static.rust-lang.org/dist/2026-07-16/rust-std-1.97.1-aarch64-unknown-linux-gnu.tar.xz","sha256":"46aed8e63186350004d8ec6afca798811e6530b514352e5a8a26f3dc4939b3be","root":"rust-std-1.97.1-aarch64-unknown-linux-gnu"},{"target":"wasm32-unknown-unknown","url":"https://static.rust-lang.org/dist/2026-07-16/rust-std-1.97.1-wasm32-unknown-unknown.tar.xz","sha256":"fa0edb6e9f34faae5735554d62d50875eded839dc707d0f1c01467a918d8453b","root":"rust-std-1.97.1-wasm32-unknown-unknown"}],"managed":true,"requires":[]},{"id":"xcode","title":"Xcode","version":"27.0","source":"https://developer.apple.com/xcode/","command":"xcodebuild","archive":null,"managed":false,"requires":[]},{"id":"protoc","title":"Protocol Buffers Compiler","version":"35.0","source":"https://github.com/protocolbuffers/protobuf/releases/tag/v35.0","command":"protoc","archive":{"url":"https://github.com/protocolbuffers/protobuf/releases/download/v35.0/protoc-35.0-osx-aarch_64.zip","sha256":"45444963204757fd3e2fbe304bc1fdadfb488d8556ff099c4cc06575eab88976","root":".","executable":"bin/protoc","kind":"extract"},"managed":true,"requires":["node"]},{"id":"perl","title":"Perl","version":"5.42.3","source":"https://www.cpan.org/src/5.0/","command":"perl","archive":{"url":"https://www.cpan.org/src/5.0/perl-5.42.3.tar.xz","sha256":"c9387e1473a1866935cb047ece7c2e0a80767a3acdecb79d4a375f8a95970ddc","root":"perl-5.42.3","executable":"bin/perl","kind":"native-source"},"managed":true,"requires":["node","xcode"]},{"id":"openssl","title":"OpenSSL","version":"3.6.3","source":"https://github.com/openssl/openssl/releases/download/openssl-3.6.3/","command":"openssl","archive":{"url":"https://github.com/openssl/openssl/releases/download/openssl-3.6.3/openssl-3.6.3.tar.gz","sha256":"243a86649cf6f23eeb6a2ff2456e09e5d77dd9018a54d3d96b0c6bdd6ba6c7f1","root":"openssl-3.6.3","executable":"bin/openssl","kind":"native-source"},"managed":true,"requires":["node","xcode","perl"]},{"id":"m4","title":"GNU M4","version":"1.4.21","source":"https://ftp.gnu.org/gnu/m4/","command":"m4","archive":{"url":"https://ftp.gnu.org/gnu/m4/m4-1.4.21.tar.xz","sha256":"f25c6ab51548a73a75558742fb031e0625d6485fe5f9155949d6486a2408ab66","root":"m4-1.4.21","executable":"bin/m4","kind":"native-source"},"managed":true,"requires":["node","xcode"]},{"id":"bison","title":"GNU Bison","version":"3.8.2","source":"https://ftp.gnu.org/gnu/bison/","command":"bison","archive":{"url":"https://ftp.gnu.org/gnu/bison/bison-3.8.2.tar.xz","sha256":"9bba0214ccf7f1079c5d59210045227bcf619519840ebfa80cd3849cff5a5bf2","root":"bison-3.8.2","executable":"bin/bison","kind":"native-source"},"managed":true,"requires":["node","xcode","m4"]},{"id":"flex","title":"Flex","version":"2.6.4","source":"https://github.com/westes/flex/releases/download/v2.6.4/","command":"flex","archive":{"url":"https://github.com/westes/flex/releases/download/v2.6.4/flex-2.6.4.tar.gz","sha256":"e87aae032bf07c26f85ac0ed3250998c37621d95f8bd748b31f15b33c45ee995","root":"flex-2.6.4","executable":"bin/flex","kind":"native-source"},"managed":true,"requires":["node","xcode","m4","bison"]},{"id":"gettext","title":"GNU Gettext","version":"1.0","source":"https://ftp.gnu.org/gnu/gettext/","command":"msgfmt","archive":{"url":"https://ftp.gnu.org/gnu/gettext/gettext-1.0.tar.gz","sha256":"85d99b79c981a404874c02e0342176cf75c7698e2b51fe41031cf6526d974f1a","root":"gettext-1.0","executable":"bin/msgfmt","kind":"native-source"},"managed":true,"requires":["node","xcode","perl","m4","bison","flex"]},{"id":"posix","title":"macOS POSIX 基础工具","version":"27.0","source":"https://opensource.apple.com/","command":"bash","archive":{"url":"https://opensource.apple.com/","sha256":"8ca7560842b9606bcbe9628248866cc52675775a956574199716515dadf020fe","root":"macos-posix-27.0","executable":"bin/bash","kind":"apple-posix"},"managed":true,"requires":["node","xcode"]},{"id":"bash","title":"GNU Bash","version":"5.3.20","source":"https://www.gnu.org/software/bash/","command":"bash","archive":{"url":"https://ftp.gnu.org/gnu/bash/bash-5.3.tar.gz","sha256":"0d5cd86965f869a26cf64f4b71be7b96f90a3ba8b3d74e27e8e9d9d5550f31ba","root":"bash-5.3","executable":"bin/bash","kind":"native-source"},"upstream_patches":[{"url":"https://ftp.gnu.org/gnu/bash/bash-5.3-patches/bash53-001","sha256":"1f608434364af86b9b45c8b0ea3fb3b165fb830d27697e6cdfc7ac17dee3287f"},{"url":"https://ftp.gnu.org/gnu/bash/bash-5.3-patches/bash53-002","sha256":"e385548a00130765ec7938a56fbdca52447ab41fabc95a25f19ade527e282001"},{"url":"https://ftp.gnu.org/gnu/bash/bash-5.3-patches/bash53-003","sha256":"f245d9c7dc3f5a20d84b53d249334747940936f09dc97e1dcb89fc3ab37d60ed"},{"url":"https://ftp.gnu.org/gnu/bash/bash-5.3-patches/bash53-004","sha256":"9591d245045529f32f0812f94180b9d9ce9023f5a765c039b852e5dfc99747d0"},{"url":"https://ftp.gnu.org/gnu/bash/bash-5.3-patches/bash53-005","sha256":"cca1ef52dbbf433bc98e33269b64b2c814028efe2538be1e2c9a377da90bc99d"},{"url":"https://ftp.gnu.org/gnu/bash/bash-5.3-patches/bash53-006","sha256":"29119addefed8eff91ae37fd51822c31780ee30d4a28376e96002706c995ff10"},{"url":"https://ftp.gnu.org/gnu/bash/bash-5.3-patches/bash53-007","sha256":"c0976bbfffa1453c7cfdd62058f206a318568ff2d690f5d4fa048793fa3eb299"},{"url":"https://ftp.gnu.org/gnu/bash/bash-5.3-patches/bash53-008","sha256":"097cd723cbfb8907674ac32214063a3fd85282657ec5b4e544d2c0f719653fb4"},{"url":"https://ftp.gnu.org/gnu/bash/bash-5.3-patches/bash53-009","sha256":"eee30fe78a4b0cb2fe20e010e00308899cfc613e0774ebb3c8557a1552f24f8c"},{"url":"https://ftp.gnu.org/gnu/bash/bash-5.3-patches/bash53-010","sha256":"cf76f1cce2ea300c18bff9f002d21f280cc931acd17c28518110b93fe6e72569"},{"url":"https://ftp.gnu.org/gnu/bash/bash-5.3-patches/bash53-011","sha256":"0298df8f5ea2a31d3be43ed7d269c5b3c7c342dd5b570bea7f64d66dcbbe7531"},{"url":"https://ftp.gnu.org/gnu/bash/bash-5.3-patches/bash53-012","sha256":"d71379b39bebaedaf123414414e77fb458a0a43b9ad3116594c6df7ca6754573"},{"url":"https://ftp.gnu.org/gnu/bash/bash-5.3-patches/bash53-013","sha256":"042f9cda967e24bf4211944697441e93d06ff42b4b998629a98a1b249279f200"},{"url":"https://ftp.gnu.org/gnu/bash/bash-5.3-patches/bash53-014","sha256":"bd4360b401d38507e358783dcad8536a99c6789f0d3a5bd0cfb8c4a34144696c"},{"url":"https://ftp.gnu.org/gnu/bash/bash-5.3-patches/bash53-015","sha256":"55b79ceee2fc27f6767eed697e939a7eb2fe2a28c01556bd75f18d581014f46e"},{"url":"https://ftp.gnu.org/gnu/bash/bash-5.3-patches/bash53-016","sha256":"9ea29b266b7d24cb34d0ff3f1c4631e4d527bfe2d1ef15d17cdb924bf31ef767"},{"url":"https://ftp.gnu.org/gnu/bash/bash-5.3-patches/bash53-017","sha256":"443b927b45c1558ca72052410f8b8f6e5152b617ed707061a2781d4375b0d1c3"},{"url":"https://ftp.gnu.org/gnu/bash/bash-5.3-patches/bash53-018","sha256":"ae715d76c50341d7d7095e9a8d2eeed1ca9546152c2ac7289206f90cf30ac697"},{"url":"https://ftp.gnu.org/gnu/bash/bash-5.3-patches/bash53-019","sha256":"a25c581e4d0057dea3833918438a930e2e86ee4c6dc17fe15267b7f04cbc4e3d"},{"url":"https://ftp.gnu.org/gnu/bash/bash-5.3-patches/bash53-020","sha256":"df217ed3a9122aa2286d9b67bbe348661b6a9db262b580c29150dae55d532896"}],"managed":true,"requires":["node","xcode","posix"]},{"id":"grep","title":"GNU grep","version":"3.12","source":"https://www.gnu.org/software/grep/","command":"grep","archive":{"url":"https://ftp.gnu.org/gnu/grep/grep-3.12.tar.xz","sha256":"2649b27c0e90e632eadcd757be06c6e9a4f48d941de51e7c0f83ff76408a07b9","root":"grep-3.12","executable":"bin/grep","kind":"native-source"},"managed":true,"requires":["node","xcode","posix"]},{"id":"sed","title":"GNU sed","version":"4.10","source":"https://www.gnu.org/software/sed/","command":"sed","archive":{"url":"https://ftp.gnu.org/gnu/sed/sed-4.10.tar.xz","sha256":"b8e72182b2ec96a3574e2998c47b7aaa64cc20ce000d8e9ac313cc07cecf28c7","root":"sed-4.10","executable":"bin/sed","kind":"native-source"},"managed":true,"requires":["node","xcode","posix"]},{"id":"postgres","title":"PostgreSQL","version":"17.11","source":"https://ftp.postgresql.org/pub/source/v17.11/postgresql-17.11.tar.bz2.sha256","command":"postgres","archive":{"url":"https://ftp.postgresql.org/pub/source/v17.11/postgresql-17.11.tar.bz2","sha256":"dd27f2b3c59e73ed14aa3324901242bf69a032a6347805f274e6260322d42979","root":"postgresql-17.11","executable":"bin/postgres","kind":"native-source"},"managed":true,"requires":["node","xcode","perl","openssl","m4","bison","flex"]}];
const androidPlatformDefinitions=[{"path":"platforms;android-35","version":"2","source":"https://dl.google.com/android/repository/platform-35_r02.zip","sha256":"14c793e5c50d69bd3a5b15e42bf763b39ea90d8d3fb3a4a690b5a7b05c299d6f"}];
const androidDefinitions=[{"path":"platforms;android-36","version":"2","url":"https://dl.google.com/android/repository/platform-36_r02.zip","sha256":"37607369a28c5b640b3a7998868d45898ebcb777565a0e85f9acf36f29631d2e","root":"android-36"},{"path":"build-tools;36.0.0","version":"36.0.0","url":"https://dl.google.com/android/repository/build-tools_r36_macosx.zip","sha256":"04e7f3a72044de4926fa038fa0e251a37bba1e1c3fb8beab6f8401bfd9eb4bf3","root":"android-16"},{"path":"platform-tools","version":"37.0.1","url":"https://dl.google.com/android/repository/platform-tools_r37.0.1-darwin.zip","sha256":"ee39ad5967e95c2a07f04dbcbde96b1a0c916ba376096db5d2f498b7727a5d1d","root":"platform-tools"},{"path":"cmdline-tools;22.0","version":"22.0","url":"https://dl.google.com/android/repository/commandlinetools-mac_arm64-15859902_latest.zip","sha256":"835b62a26162b229b441d1f6d4680383815a270809eb33522c0d480fa5002c4e","root":"cmdline-tools"},{"tool":"cmake"},{"path":"ndk;28.2.13676358","version":"28.2.13676358","url":"https://dl.google.com/android/repository/android-ndk-r28c-darwin.zip","sha256":"0d4599e8bbf1a1668a0d51a541729b2246360f350018a2081d0b302dbb594f2a","root":"android-ndk-r28c"}];
const parserDefinitions={"yaml":{"name":"yaml","version":"2.8.3","url":"https://registry.npmjs.org/yaml/-/yaml-2.8.3.tgz","integrity":"sha512-AvbaCLOO2Otw/lW5bmh9d/WEdcDFdQp2Z2ZUH3pX9U2ihyUY0nvLv7J6TrWowklRGPYbB/IuIMfYgxaCPg5Bpg=="},"toml":{"name":"smol-toml","version":"1.4.2","url":"https://registry.npmjs.org/smol-toml/-/smol-toml-1.4.2.tgz","integrity":"sha512-rInDH6lCNiEyn3+hH8KVGFdbjc099j47+OSgbMrfDYX1CmXLfdKd7qi6IfcWj2wFxvSVkuI46M+wPGYfEOEj6g=="}};
const cleanEnvironment=environment=>Object.fromEntries(['HOME','USER','LOGNAME','LANG','LC_ALL'].filter(k=>typeof environment[k]==='string').map(k=>[k,environment[k]]));
const objectRecipe=tool=>hash(JSON.stringify([tool,tool.id==='posix'?posixRecipe.buildPosixTool.toString():['native-source','gem'].includes(tool.archive?.kind)?sourceRecipe.buildSourceTool.toString():'locked-extract-v1']));
function toolArchive(tool){if(tool.archive)return tool.archive;if(tool.id==='cmake')return {...tool.archives.macos,kind:'extract',executable:'bin/cmake'};return null;}
async function verifyToolObject(directory,tool){
  if(!await stat(directory))return null;await directoryCheck(directory);const payload=join(directory,'payload'),archive=toolArchive(tool),path=join(payload,archive.executable);
  await directoryCheck(payload);const info=await regular(path);if(!(info.mode&0o111))fail('工具入口不可执行');return {path,version:tool.version};
 }
const directoryCheck=path=>directory(path);
// 基础工具的正式PATH投影排除发行件旧Shell/grep/sed；自举仅限本产品已声明GNU三工具。
async function productFoundation(library,verify,{bootstrap=false,id}={}){
 const base=await posixRecipe.controlledPosixTools(library,verify);if(bootstrap){if(!['bash','grep','sed'].includes(id))fail('自举仅限GNU三工具');return {...base,path:base.bin};}
 const tools={...base.tools},paths=[];for(const name of ['bash','grep','sed']){const tool=library.tools.find(x=>x.id===name),value=tool&&await verify(library,tool);if(!value)fail('GNU闭包缺失：'+name);tools[name]=value.path;paths.push(dirname(value.path));}tools.sh=tools.bash;delete tools.egrep;delete tools.fgrep;
 const view=join(library.work,'resource-tools');await directory(view,true);const shell=join(view,'sh');if(await stat(shell)){if(!((await lstat(shell)).isSymbolicLink())||await realpath(shell)!==tools.sh)fail('GNU sh交付漂移');}else await symlink(tools.sh,shell);for(const [name,path]of Object.entries(base.tools)){if(['sh','bash','grep','sed','egrep','fgrep'].includes(name))continue;const link=join(view,name);if(await stat(link)){if(!((await lstat(link)).isSymbolicLink())||await realpath(link)!==path)fail('基础交付漂移');}else await symlink(path,link);}return {tools,bin:view,path:[...paths,view].join(':')};
}
async function prepareSourceDependencies({library,tool,pending,signal,fetcher,options={}}){const result=new Map();for(const entry of tool.dependencies||[])result.set(entry.name,await acquireArchive(entry,{work:library.work,store:join(library.root,'archives'),optional:options.optionalDependencies,offline:options.offline,signal,fetcher}));return result;}
async function commitCandidate(pending,target,{signal,verify}={}){


 // 下载与编译已完成后才取得短锁；等待可取消，已有对象永不覆盖。
 const lock=target+'.lock';let handle;for(let n=0;n<500;n++){signal?.throwIfAborted();try{handle=await open(lock,'wx',0o600);break;}catch(e){if(e.code!=='EEXIST')throw e;await new Promise(r=>setTimeout(r,20));}}if(!handle)fail('原件提交锁等待超限');
 try{signal?.throwIfAborted();if(await stat(target)){if(verify)await verify(target);}else {const mode=(await lstat(pending)).mode&0o777;await chmod(pending,mode|0o700);try{await rename(pending,target);await chmod(target,mode);if(verify)await verify(target);}catch(error){if(await stat(pending))await chmod(pending,mode);throw error;}}}finally{await handle.close();await rm(lock);}
}
// 准备产物身份与使用验收由本产品定义，供应端不解释组件布局。
function toolSupplyKey(tool){const archive=toolArchive(tool);if(tool.id==='node')return archive.sha256;return archive.sha256+'-'+objectRecipe(tool);}



async function installTool(library,tool,options,visiting=new Set()){


 if(library.installed.has(tool.id))return library.installed.get(tool.id);if(visiting.has(tool.id))fail('工具声明循环：'+tool.id);visiting=new Set([...visiting,tool.id]);const archive=toolArchive(tool);
 if(tool.id==='xcode'){const apple=await verifyAppleTools(library,{environment:cleanEnvironment(options.environment),signal:options.signal});const value={path:apple.tools.xcodebuild,version:tool.version};library.installed.set(tool.id,value);return value;}
 if(!archive)fail('工具归档未声明：'+tool.id);const shared=join(library.root,'shared');await directory(shared,true);const target=join(shared,archive.sha256+'-'+objectRecipe(tool));const verify=p=>verifyToolObject(p,tool,{produced:true});let value=await verify(target);
 if(!value&&options.optionalTools&&await stat(options.optionalTools)){await directory(options.optionalTools);value=await verifyToolObject(join(options.optionalTools,'shared',archive.sha256),tool);}
 for(const id of tool.requires||[]){const entry=library.tools.find(x=>x.id===id);if(!entry)fail('前置工具未声明：'+id);await installTool(library,entry,options,visiting);}
 if(value){library.installed.set(tool.id,value);return value;}if(options.offline)fail('离线缺少工具：'+tool.id);
 // Node用内置解包形成最小宿主，POSIX用固定签名输入；其余工具只能使用完成GNU接管的基础工具。
 let foundation;if(!['node','posix'].includes(tool.id)){for(const id of ['posix',...(['bash','grep','sed'].includes(tool.id)?[]:['bash','grep','sed'])]){if(visiting.has(id))fail('工具自举循环');await installTool(library,library.tools.find(x=>x.id===id),options,visiting);}foundation=await productFoundation(library,async(_,t)=>library.installed.get(t.id),{bootstrap:['bash','grep','sed'].includes(tool.id),id:tool.id});}
 const pending=await fixedScratch(join(await resourceWork(library.work),'.'+archive.sha256+'-'));const canonical=join(pending,'library/shared',archive.sha256+'.pending'),payload=join(canonical,'payload');const localLibrary={...library,pending:canonical,finalPayload:options.finalPayload||join(target,'payload')};await directory(canonical,true);const original=join(canonical,'archive');
 try{
  let source;if(archive.kind==='apple-posix'){await verifyAppleTools(library,{names:['codesign'],environment:cleanEnvironment(options.environment),signal:options.signal});await posixRecipe.buildPosixTool({tool,payload,bootstrap:true,run:exec,signal:options.signal});source=payload;}
  else{const file=await acquireArchive(archive,{work:library.work,store:join(library.root,'archives'),optional:options.optionalDependencies,offline:options.offline,fetcher:options.fetcher,signal:options.signal});await copyFile(file,original);if(['gem','binary','phar'].includes(archive.kind))source=original;else {const unpacked=join(canonical,'unpack');await unpack(original,unpacked,{foundation,signal:options.signal});source=archive.root==='.'?unpacked:join(unpacked,archive.root);await directory(source);}}
  const environment={...cleanEnvironment(options.environment),HOME:canonical,TMPDIR:canonical,PATH:foundation?.path||'',PRODUCT_WORK_DIR:canonical};
  const verifyInstalled=async(_,t)=>library.installed.get(t.id)||null;
  if(['native-source','gem'].includes(archive.kind))await sourceRecipe.buildSourceTool({library:localLibrary,tool,source,archive:original,pending:canonical,payload,finalPayload:localLibrary.finalPayload,signal:options.signal,fetcher:options.fetcher,exec,verify:verifyInstalled,apple:verifyAppleTools,bootstrap:['bash','grep','sed'].includes(tool.id),environment,prepare:input=>prepareSourceDependencies({...input,options}),download:(entry,target,context)=>downloadTool(entry,target,{...context,...options,work:library.work,store:join(library.root,'archives'),optional:options.optionalDependencies})});
  else if(['binary','phar'].includes(archive.kind)){await mkdir(dirname(join(payload,archive.executable)),{recursive:true});await copyFile(original,join(payload,archive.executable));await chmod(join(payload,archive.executable),0o555);}
  else if(archive.kind==='rust'){
   await exec(foundation.tools.bash,[join(source,'install.sh'),'--prefix='+payload,'--disable-ldconfig','--components=rustc,cargo,rust-std-aarch64-apple-darwin,rust-src,rustfmt-preview,clippy-preview'],{signal:options.signal,env:environment,maxBuffer:2*1024**2,timeout:300000});
   for(const component of tool.components||[]){const file=await acquireArchive(component,{work:library.work,store:join(library.root,'archives'),offline:options.offline,fetcher:options.fetcher,signal:options.signal}),dir=join(canonical,component.target);await unpack(file,dir,{foundation,signal:options.signal});await exec(foundation.tools.bash,[join(dir,component.root,'install.sh'),'--prefix='+payload,'--disable-ldconfig'],{signal:options.signal,env:environment,maxBuffer:2*1024**2,timeout:300000});}
  }else if(archive.kind==='cargo-source'){
   await directory(payload,true);const rust=library.installed.get('rust').path,apple=await verifyAppleTools(library,{names:['clang','ar'],environment:cleanEnvironment(options.environment),signal:options.signal}),work=join(canonical,'cargo');await directory(work,true);
   const deps=await prepareCargo([join(source,'Cargo.lock')],work,{...options,library});await exec(join(dirname(rust),'cargo'),['build','--manifest-path',join(source,'Cargo.toml'),'--release','--locked','--offline','--bin',tool.command],{cwd:work,signal:options.signal,timeout:1800000,maxBuffer:8*1024**2,env:{...environment,PATH:dirname(rust)+':'+environment.PATH,RUSTC:rust,CC:apple.tools.clang,AR:apple.tools.ar,DEVELOPER_DIR:apple.developerDirectory,CARGO_HOME:deps.cargoHome,CARGO_TARGET_DIR:join(work,'target')}});await mkdir(join(payload,'bin'));await copyFile(join(work,'target/release',tool.command),join(payload,archive.executable));await copyFile(original,join(payload,'source.crate'));
  }else if(archive.kind!=='apple-posix')await rename(source,payload);
  if(tool.id==='pnpm')await symlink('pnpm.cjs',join(payload,'bin/pnpm'));
  if(tool.id==='java')environment.JAVA_HOME=dirname(dirname(join(payload,archive.executable)));if(tool.id==='gradle')environment.JAVA_HOME=dirname(dirname(library.installed.get('java').path));if(tool.id==='python')environment.PYTHONHOME=payload;

  await permissions(payload,false);
  options.signal?.throwIfAborted();await commitCandidate(canonical,target,{signal:options.signal,verify});value=await verify(target);library.installed.set(tool.id,value);return value;
 }finally{if(await stat(pending)){await permissions(pending,true);await rm(pending,{recursive:true});}}
}
async function parser(kind,options){
 const entry=parserDefinitions[kind],work=await resourceWork(options.library.work),parserRoot=join(work,'parsers',hash(JSON.stringify(entry))),payload=join(parserRoot,'payload');
 await directoryCheck(options.library.root);await directoryCheck(options.library.work);
 if(!await stat(payload)){
  if(await stat(parserRoot))fail('本轮解析器现场不完整');
  // 解析器是锁定npm依赖，复用依赖原件目录及其URL/SRI映射，不走工具归档通道。
  const file=await packageOriginal(entry,options);
  await directory(parserRoot,true);
  try{await extractArchive(file,payload,{prefix:'package',signal:options.signal});}
  catch(error){await rm(parserRoot,{recursive:true,force:true});throw error;}
 }
 const mod=createRequire(import.meta.url)(payload);if(kind==='yaml')return text=>mod.parse(text,{uniqueKeys:true});const parse=text=>mod.parse(text);parse.stringify=mod.stringify;return parse;
}
async function checkedLock(path){await regular(path);const s=await lstat(path);if(s.size>32*1024**2)fail('锁文件超限');return readFile(path,'utf8');}
async function packageOriginal(entry,options){return acquireArchive(entry,{work:options.library.work,store:join(options.dependencyRoot||join(options.library.root,'..','rely'),'archives'),optional:options.optionalDependencies,offline:options.offline,fetcher:options.fetcher,signal:options.signal});}
// 第一方file依赖消费本仓真实源码；远端归档才要求SRI，不能把本地包误判为离线缺件。
async function validateNpmLocal(lock,key,entry,sourceBase=root){
 const location=entry.resolved,local=entry.link===true||typeof location==='string'&&location.startsWith('file:')||!key.startsWith('node_modules/');if(!local)return false;
 if(sourceBase!==root&&!inside(root,sourceBase)||await realpath(sourceBase)!==sourceBase||!inside(sourceBase,lock))fail('npm本地来源源码边界无效');
 const raw=entry.link?location:typeof location==='string'&&location.startsWith('file:')?location.slice(5):key;
 if(typeof raw!=='string'||!raw||isAbsolute(raw)||/[\\\x00-\x1f]/u.test(raw)||/^[a-z]+:/iu.test(raw))fail('npm本地来源路径无效');
 const directoryPath=resolve(dirname(lock),raw);if(!inside(sourceBase,directoryPath)||directoryPath===sourceBase)fail('npm本地来源越界');await directory(directoryPath);if(await realpath(directoryPath)!==directoryPath)fail('npm本地来源经过链接');
 const packageFile=join(directoryPath,'package.json');await regular(packageFile);const own=JSON.parse(await readFile(packageFile,'utf8')),host=JSON.parse(await readFile(join(dirname(lock),'package.json'),'utf8'));
 if(typeof own.name!=='string'||typeof own.version!=='string'||entry.name!==undefined&&entry.name!==own.name||entry.version!==undefined&&entry.version!==own.version||key.startsWith('node_modules/')&&key!=='node_modules/'+own.name)fail('npm本地包名称或版本与原件不一致');
 const declared=[host.dependencies,host.devDependencies,host.optionalDependencies].flatMap(values=>values?.[own.name]?[values[own.name]]:[]);
 if(declared.length!==1||!declared[0].startsWith('file:')||resolve(dirname(lock),declared[0].slice(5))!==directoryPath)fail('npm本地来源未由本仓工程准确声明');return true;
}
async function prepareNpm(locks,work,options){const cache=join(work,'npm');await directory(cache,true);const node=options.library.installed.get('node').path,require=createRequire(join(dirname(node),'../lib/node_modules/npm/bin/npm-cli.js')),cacache=require('cacache');for(const lock of locks){const document=JSON.parse(await checkedLock(lock));if(![2,3].includes(document.lockfileVersion)||!document.packages)fail('npm原始锁格式无效');for(const [path,entry]of Object.entries(document.packages)){if(!path)continue;if(await validateNpmLocal(lock,path,entry))continue;if(!entry.resolved||!entry.integrity||!entry.version)fail('npm包未锁定来源');const file=await packageOriginal({url:entry.resolved,integrity:entry.integrity},options);await cacache.put(join(cache,'_cacache'),'make-fetch-happen:request-cache:'+entry.resolved,await readFile(file),{integrity:entry.integrity,metadata:{time:Date.now(),url:entry.resolved,reqHeaders:{},resHeaders:{'content-type':'application/octet-stream'}}});}}return {npmCache:cache};}
async function preparePub(locks,cache,options){await directory(cache,true);const parse=await parser('yaml',options),files=[];for(const lock of locks){const d=parse(await checkedLock(lock));if(!d.packages)fail('Pub锁格式无效');for(const [name,entry]of Object.entries(d.packages)){if(['sdk','path'].includes(entry.source))continue;if(entry.source==='git'){const d=entry.description;if(!d||d.ref!==d['resolved-ref']||!options.sources?.some(x=>x.name===name&&x.url===d.url&&x.ref===d.ref))fail('Pub Git来源不属于产品固定闭包：'+name);continue;}if(entry.source!=='hosted'||entry.description?.name!==name||!['https://pub.dev','https://pub.dev/'].includes(entry.description.url)||!entry.description.sha256)fail('Pub来源未锁定');const coordinate={url:'https://pub.dev/api/archives/'+name+'-'+entry.version+'.tar.gz',sha256:entry.description.sha256};files.push({name:name+'-'+entry.version,sha256:coordinate.sha256,file:await packageOriginal(coordinate,options)});}}
 for(const entry of files){const target=join(cache,'hosted/pub.dev',entry.name),proof=join(cache,'hosted-hashes/pub.dev',entry.name+'.sha256');await directory(dirname(target),true);await directory(dirname(proof),true);if(await stat(target)){if(await readFile(proof,'utf8')!==entry.sha256+'\n')fail('Pub缓存摘要漂移');}else{await extractArchive(entry.file,target,{signal:options.signal});await writeFile(proof,entry.sha256+'\n',{flag:'wx'});}}
 await directory(join(cache,'_temp'),true);return {pubCache:cache};}
function gitCoordinate(source){const u=new URL(source.replace(/^git\+/u,''));const ref=u.searchParams.get('rev');if(u.protocol!=='https:'||u.hostname!=='github.com'||u.username||u.password||!u.pathname.endsWith('.git')||!/^[a-f0-9]{40}$/u.test(ref||'')||u.hash!=='#'+ref||[...u.searchParams.keys()].length!==1)fail('Git来源不是唯一锁定提交');return {url:u.origin+u.pathname,ref};}
function gitCommandTimeout(remoteFetch=false){if(typeof remoteFetch!=='boolean')fail('Git获取超时类型无效');return remoteFetch?1800000:600000;}
// 固定提交可能以refs/tata承载；初始化后显式导入该对象，避免clone忽略非heads引用。
async function importLockedGitBundle(run,bundle,source,checkout){await run(['init','--quiet','--template=',checkout]);await run(['-C',checkout,'fetch','--quiet','--no-tags','--no-recurse-submodules',bundle,source.ref]);await run(['-C',checkout,'remote','add','origin',source.url]);}
// 供给模式只领取公开原件；独立模式将原锁提交的完整bundle持久保存在本产品原件库。
async function gitOriginal(source,options,run){
 const coordinate='git+'+source.url+'?rev='+source.ref+'#'+source.ref;
 const packages=[...new Map((source.packages||[]).map(value=>[value.name+'@'+value.version,{name:value.name,version:value.version}])).values()];


 if(options.optionalDependencies){
  const index=join(dirname(options.optionalDependencies),'index.json');
  if(await stat(index)){const catalog=await readDependencySupply(options.optionalDependencies),entry=catalog.git_sources.find(value=>value.source===coordinate);
   if(entry){if(!/^[a-f0-9]{64}$/u.test(entry.sha256||''))fail('Git供给原件坐标无效');const path=join(options.optionalDependencies,entry.sha256+'.blob');await regular(path);return path;}
  }
 }
 if(!options.dependencyRoot){if(options.offline)fail('离线缺少Git提交');fail('独立Git准备缺少本产品永久原件库');}
 const objects=join(options.dependencyRoot,'objects');await directory(objects,true);
 const target=join(objects,hash(coordinate)+'.blob'),lock=target+'.lock';
 if(await stat(target)){await regular(target);return target;}
 if(options.offline)fail('离线缺少Git提交');
 let handle;const deadline=Date.now()+2100000;
 while(!handle){options.signal?.throwIfAborted();try{handle=await open(lock,'wx',0o600);}catch(error){if(error.code!=='EEXIST')throw error;
   if(await stat(target)){await regular(target);return target;}
   if(Date.now()>=deadline)fail('独立Git原件等待超时');await new Promise(resolve=>setTimeout(resolve,50));}}
 const owner=await handle.stat();let candidate;
 try{
  if(await stat(target)){await regular(target);return target;}
  candidate=await fixedScratch(join(await resourceWork(options.library.work),'.git-'));
  const checkout=join(candidate,'repository'),file=join(candidate,'source.bundle');await mkdir(checkout);
  await run(['init','--quiet','--template=',checkout]);
  await run(['-C',checkout,'fetch','--quiet','--no-tags','--no-recurse-submodules',source.url,source.ref],{remoteFetch:true});
  if((await run(['-C',checkout,'rev-parse','FETCH_HEAD'])).stdout.trim()!==source.ref)fail('Git取得提交不符');
  await run(['-C',checkout,'update-ref','refs/heads/locked',source.ref]);
  await run(['-C',checkout,'fsck','--full','--strict']);
  await run(['-C',checkout,'bundle','create',file,'refs/heads/locked']);
  options.signal?.throwIfAborted();await chmod(file,0o444);await rename(file,target);return target;
 }finally{
  if(candidate)await rm(candidate,{recursive:true,force:true});await handle.close();
  const current=await lstat(lock);if(current.dev!==owner.dev||current.ino!==owner.ino)fail('独立Git原件锁身份变化');await rm(lock);
 }
}
async function gitCheckout(source,target,options){
 const git=options.library.installed.get('git')?.path;if(!git||!/^[a-f0-9]{40}$/u.test(source.ref||''))fail('Git未验真或来源没有固定提交');checkedURL(source.url);
 const environment={...cleanEnvironment(options.environment),PATH:(await productFoundation(options.library,async(_,t)=>options.library.installed.get(t.id))).path,GIT_CONFIG_NOSYSTEM:'1',GIT_CONFIG_GLOBAL:'/dev/null',GIT_TERMINAL_PROMPT:'0',HOME:options.library.work};
 // 仅锁定提交的GitHub HTTPS获取允许较长时间；本地导入和身份复验仍限十分钟。
 const run=(args,{remoteFetch=false}={})=>exec(git,['-c','credential.helper=','-c','core.hooksPath=/dev/null','-c','protocol.file.allow=always',...args],{signal:options.signal,env:environment,maxBuffer:16*1024**2,timeout:gitCommandTimeout(remoteFetch)});
 if(await stat(target)){await directory(target);await directory(join(target,'.git'));if((await run(['-C',target,'rev-parse','HEAD'])).stdout.trim()!==source.ref||(await run(['-C',target,'status','--porcelain=v1','--untracked-files=all'])).stdout||(await run(['-C',target,'remote','get-url','origin'])).stdout.trim()!==source.url)fail('Git检出身份漂移');return target;}
 // 本轮只复制已入库或已交付的原件，清场不再删除唯一的Git来源。
 const bundle=join(await resourceWork(options.library.work),'git-bundles',hash(JSON.stringify({url:source.url,ref:source.ref}))+'.bundle');await directory(dirname(bundle),true);
 if(!await stat(bundle))await copyFile(await gitOriginal(source,options,run),bundle,constants.COPYFILE_EXCL);
 await directory(dirname(target),true);const pending=await fixedScratch(join(dirname(target),'.checkout-'));try{const checkout=join(pending,'source');await importLockedGitBundle(run,bundle,source,checkout);await run(['-C',checkout,'checkout','--quiet','--detach',source.ref]);await run(['-C',checkout,'fsck','--full','--strict']);options.signal?.throwIfAborted();await rename(checkout,target);}finally{await rm(pending,{recursive:true});}return gitCheckout(source,target,options);
}
// Git工作区包转为目录源时展开workspace继承，并把相对path依赖固定到同一锁中的准确版本。
function normalizeCargoManifest(document,workspace,locked,selectedSource=null) {
 const d=structuredClone(document),w=workspace?.workspace||{};
 for(const [key,value]of Object.entries(d.package||{}))if(value&&typeof value==='object'&&value.workspace===true){if(w.package?.[key]===undefined)fail('Git包workspace字段缺失：'+key);d.package[key]=w.package[key];}
 const section=(values,development=false)=>{for(const [name,value]of Object.entries(values||{})){let dep=typeof value==='string'?{version:value}:{...value};if(dep.workspace){const inherited=w.dependencies?.[name];if(!inherited)fail('Git包workspace依赖缺失：'+name);const source=typeof inherited==='string'?{version:inherited}:inherited;dep={...source,...dep,features:[...(source.features||[]),...(dep.features||[])]};delete dep.workspace;}
  if(dep.path){delete dep.path;const matches=locked.filter(x=>x.name===(dep.package||name)&&(!selectedSource||x.source===selectedSource));if(!dep.version){const versions=[...new Set(matches.map(x=>x.version))];if(!versions.length&&selectedSource&&(dep.optional===true||development)){}else{if(versions.length!==1||!selectedSource&&matches.length!==1)fail('相对依赖没有唯一锁定版本：'+name);dep.version='='+versions[0];}}if(selectedSource){const coordinate=gitCoordinate(selectedSource),url=new URL(selectedSource.slice(4));delete dep.registry;delete dep.branch;delete dep.tag;delete dep.rev;dep.git=coordinate.url;for(const key of ['rev','branch','tag'])if(url.searchParams.has(key))dep[key]=url.searchParams.get(key);}}values[name]=dep;}};
 for(const key of ['dependencies','build-dependencies','dev-dependencies'])section(d[key],key==='dev-dependencies');for(const target of Object.values(d.target||{}))for(const key of ['dependencies','build-dependencies','dev-dependencies'])section(target[key],key==='dev-dependencies');if(d.lints?.workspace){if(!w.lints)fail('Git包workspace lints缺失');d.lints=w.lints;}delete d.workspace;return d;
}
async function prepareCargo(locks,work,options){const rust=options.library.installed.get('rust');if(rust){const standard=join(dirname(dirname(rust.path)),'lib/rustlib/src/rust/library/Cargo.lock');await regular(standard);locks=[...new Set([...locks,standard])];}await directory(work,true);const parse=await parser('toml',options),packages=new Map(),gitSources=new Map(),allPackages=[],vendor=join(work,'cargo-vendor');await directory(vendor,true);
 for(const lock of locks){const doc=parse(await checkedLock(lock));if(!Array.isArray(doc.package))fail('Cargo锁格式无效');allPackages.push(...doc.package);for(const pkg of doc.package){if(!pkg.source)continue;if(pkg.source==='registry+https://github.com/rust-lang/crates.io-index'){if(!/^[a-f0-9]{64}$/u.test(pkg.checksum||''))fail('Cargo包缺少摘要');const key=pkg.name+'-'+pkg.version;if(packages.has(key)&&packages.get(key)!==pkg.checksum)fail('Cargo包版本冲突');packages.set(key,pkg.checksum);const target=join(vendor,key),file=await packageOriginal({url:'https://static.crates.io/crates/'+pkg.name+'/'+key+'.crate',sha256:pkg.checksum},options);if(!await stat(target)){await extractArchive(file,target,{prefix:key,signal:options.signal});const files=Object.fromEntries((await inventory(target)).filter(x=>x.sha256).map(x=>[x.path,x.sha256]));await writeFile(join(target,'.cargo-checksum.json'),JSON.stringify({files,package:pkg.checksum}),{flag:'wx'});}else {const proof=JSON.parse(await readFile(join(target,'.cargo-checksum.json'),'utf8'));if(proof.package!==pkg.checksum)fail('Cargo目录源摘要漂移');for(const [name,digest]of Object.entries(proof.files)){if(!safePath(name)||hash(await readFile(join(target,name)))!==digest)fail('Cargo目录源被篡改');}}}
 else if(pkg.source.startsWith('git+')){const coordinate=gitCoordinate(pkg.source);if(!gitSources.has(pkg.source))gitSources.set(pkg.source,{coordinate,packages:[]});gitSources.get(pkg.source).packages.push(pkg);}else fail('Cargo来源未声明');}}
 let config='[net]\noffline = true\n[source.crates-io]\nreplace-with = "product-verified"\n[source.product-verified]\ndirectory = '+JSON.stringify(vendor)+'\n';
 for(const [source,entry]of gitSources){const sourceVendor=join(work,'cargo-git-vendor',hash(source));await directory(sourceVendor,true);const checkout=await gitCheckout({...entry.coordinate,packages:entry.packages},join(work,'cargo-git',hash(source)),options);const manifests=[];async function walk(path){for(const name of await readdir(path)){if(['.git','target'].includes(name))continue;const file=join(path,name),s=await lstat(file);if(s.isDirectory())await walk(file);else if(name==='Cargo.toml'&&s.isFile())manifests.push(file);}}await walk(checkout);for(const pkg of entry.packages){let found;for(const manifest of manifests){const doc=parse(await readFile(manifest,'utf8'));if(doc.package?.name===pkg.name){let version=doc.package.version;if(typeof version==='object'&&version.workspace)version=parse(await readFile(join(checkout,'Cargo.toml'),'utf8')).workspace?.package?.version;if(version===pkg.version){if(found)fail('Git包路径不唯一');found=dirname(manifest);}}}if(!found)fail('Git包名称版本与锁不一致');const target=join(sourceVendor,pkg.name+'-'+pkg.version);const old=join(vendor,pkg.name+'-'+pkg.version+'-'+hash(source).slice(0,12));if(await stat(old))await rm(old,{recursive:true});if(await stat(target)&&!await stat(join(target,'.cargo-checksum.json')))await rm(target,{recursive:true});if(!await stat(target)){await copyTree(found,target,checkout);let workspace={};for(let at=found;inside(checkout,at)||at===checkout;at=dirname(at)){const file=join(at,'Cargo.toml');if(await stat(file)){const candidate=parse(await readFile(file,'utf8'));if(candidate.workspace){workspace=candidate;break;}}if(at===checkout)break;}const manifest=normalizeCargoManifest(parse(await readFile(join(found,'Cargo.toml'),'utf8')),workspace,allPackages,source);await writeFile(join(target,'Cargo.toml'),parse.stringify(manifest));const files=Object.fromEntries((await inventory(target)).filter(x=>x.sha256).map(x=>[x.path,x.sha256]));await writeFile(join(target,'.cargo-checksum.json'),JSON.stringify({files,package:null}));}}
 const key='product-git-'+hash(source).slice(0,12),url=new URL(source.slice(4));config+='[source.'+key+']\ngit = '+JSON.stringify(entry.coordinate.url)+'\n';for(const selector of ['rev','branch','tag'])if(url.searchParams.has(selector))config+=selector+' = '+JSON.stringify(url.searchParams.get(selector))+'\n';config+='replace-with = '+JSON.stringify(key+'-verified')+'\n[source.'+key+'-verified]\ndirectory = '+JSON.stringify(sourceVendor)+'\n';}
 const cargoHome=join(work,'cargo-home');await directory(cargoHome,true);await writeFile(join(cargoHome,'config.toml'),config);return {cargoHome};
}
// 只物化当前固定Git闭包内已有目标；外部链接、目录循环和特殊项均拒绝。
async function copyTree(source,target,closure=source,ancestors=new Set()){
 const canonical=await realpath(source),boundary=await realpath(closure);if(canonical!==boundary&&!inside(boundary,canonical)||ancestors.has(canonical))fail('目录源链接越界或循环');await directory(canonical);const lineage=new Set([...ancestors,canonical]);await mkdir(target);
 for(const name of await readdir(canonical)){if(['.git','target'].includes(name))continue;const a=join(canonical,name),b=join(target,name),s=await lstat(a);if(s.isDirectory())await copyTree(a,b,boundary,lineage);else if(s.isFile())await copyFile(a,b);else if(s.isSymbolicLink()){const actual=await realpath(a);if(actual!==boundary&&!inside(boundary,actual))fail('目录源链接越界');const state=await lstat(actual);if(state.isDirectory())await copyTree(actual,b,boundary,lineage);else if(state.isFile())await copyFile(actual,b);else fail('目录源链接特殊项');}else fail('目录源特殊项未声明');}
}

// 2026-10-06只读核对官方GitHub tag/Release资产元数据；未下载或安装这些原件。
const podSourceDefinitions=[];
// 官方tag仅用于核对声明；产品预先锁定其40位提交，运行时不解析浮动tag。
function podSourceCoordinate(spec, definitions = podSourceDefinitions) {
 const source=spec.source,entry=definitions.find(x=>x.name===spec.name&&x.version===spec.version);
 if(!source||typeof source!=='object')fail('Pod缺少官方来源');
 if(entry){if(source.git!==entry.url&&source.http!==entry.url||source.tag!==entry.tag&&entry.tag!==undefined)fail('Pod官方来源与产品固定坐标不一致');
  if(entry.ref){if(source.commit&&source.commit!==entry.ref)fail('Pod提交漂移');return {url:checkedURL(entry.url),ref:entry.ref};}
  if(source.sha256&&source.sha256!==entry.sha256)fail('Pod发行摘要漂移');return {url:checkedURL(entry.url),sha256:entry.sha256};}
 if(source.git&&/^[a-f0-9]{40}$/u.test(source.commit||''))return {url:checkedURL(source.git),ref:source.commit};
 if(source.http&&/^[a-f0-9]{64}$/u.test(source.sha256||''))return {url:checkedURL(source.http),sha256:source.sha256};
 fail('Pod来源没有产品锁定提交或SHA256：'+spec.name);
}
async function responseBytes(response,limit,signal){
 if(!response.ok||!response.body)fail('官方来源响应失败');if(Number(response.headers.get('content-length'))>limit)fail('官方响应声明超限');
 const chunks=[];let size=0;try{for await(const chunk of response.body){signal?.throwIfAborted();size+=chunk.length;if(size>limit)fail('官方响应数据超限');chunks.push(chunk);}if(!size)fail('官方响应为空');return Buffer.concat(chunks);}finally{await response.body.cancel().catch(()=>{});}
}
async function verifyPodSpec(file,name,version,checksum,options){
 await regular(file);if((await lstat(file)).size>2*1024**2)fail('Pod spec超限');const spec=JSON.parse(await readFile(file,'utf8'));
 if(spec.name!==name||String(spec.version)!==version)fail('Pod spec身份不符');
 const tool=options.library.installed.get('cocoapods'),ruby=options.library.installed.get('ruby')?.path;if(!tool||!ruby)fail('Pod验真缺少Ruby/CocoaPods');const gems=join(dirname(dirname(tool.path)),'gems');
 // 同一Ruby的自带Gem与产品交付Gem是唯一搜索路径；每次复用均回读规范spec锁摘要。
 const program="require 'rubygems'; ENV['GEM_PATH'] = [ENV.fetch('GEM_HOME'), Gem.default_dir].join(File::PATH_SEPARATOR); Gem.clear_paths; require 'logger'; require 'cocoapods'; print Pod::Specification.from_file(ARGV.fetch(0)).checksum";
 const result=await exec(ruby,['-e',program,file],{signal:options.signal,env:{...cleanEnvironment(options.environment),GEM_HOME:gems,GEM_PATH:gems},maxBuffer:1024**2});if(result.stdout!==checksum)fail('Pod spec与锁摘要不符');return spec;
}
async function copyPodSource(source,target,base=source){
 await directory(source);await directory(target,true);for(const name of await readdir(source)){if(name==='.git')continue;const input=join(source,name),output=join(target,name),s=await lstat(input);
  if(s.isDirectory())await copyPodSource(input,output,base);else if(s.isFile()){await regular(input);await copyFile(input,output,constants.COPYFILE_EXCL);await chmod(output,s.mode&0o111?0o755:0o644);}else if(s.isSymbolicLink()){const resolved=await realpath(input);if(!inside(base,resolved))fail('Pod源码链接越界');await symlink(await readlink(input),output);}else fail('Pod源码含特殊项');}
}
async function preparePods(lockfile,work,options){const parse=await parser('yaml',options),text=await checkedLock(lockfile),lock=parse(text),podHome=join(work,'cocoapods');await directory(podHome,true);
 // 可选供给按单个Pod坐标匹配，与整锁、宿主和其它Pod变化无关。
 let restored=false;const supplied=await readDependencySupply(options.optionalDependencies);

 const local=new Set();for(const [name,source]of Object.entries(lock['EXTERNAL SOURCES']||{})){if(typeof source[':path']!=='string'||Object.keys(source).some(x=>x!==':path'))fail('Pod外部来源必须另有产品固定锁：'+name);local.add(name);}
 const handled=new Set();for(const item of lock.PODS||[]){const record=typeof item==='string'?item:Object.keys(item)[0],m=/^([^/( ]+)(?:\/[^ (]+)? \(([^)]+)\)$/u.exec(record);if(!m)fail('Pod锁记录无效');const [,name,version]=m;if(local.has(name)||handled.has(name))continue;handled.add(name);
  const checksum=lock['SPEC CHECKSUMS']?.[name];if(!/^[a-f0-9]{40}$/u.test(checksum||''))fail('Pod缺少锁定spec摘要');const key=version+'-'+checksum.slice(0,5),specPath=join(podHome,'cache/Pods/Specs/Release',name,key+'.podspec.json'),release=join(podHome,'cache/Pods/Release',name,key);
  const candidates=(supplied?.pods||[]).filter(x=>x.name===name&&x.version===version&&x.checksum===checksum);if(candidates.length>1)fail('Pod供给坐标重复');if(candidates.length){await materializePodSupply(candidates[0],options.optionalDependencies,podHome,{signal:options.signal});restored=true;}
  // Pod spec与源码只进入当前任务的CocoaPods视图，不另存共享派生对象。
  if(!await stat(specPath)||!await stat(release)){const candidate=await fixedScratch(join(await resourceWork(options.library.work),'.pod-'));try{const payload=join(candidate,'payload');await mkdir(payload);const specFile=join(payload,'spec.json');
    if(await stat(specPath))await copyFile(specPath,specFile,constants.COPYFILE_EXCL);else{if(options.offline)fail('离线缺少Pod spec');const md5=createHash('md5').update(name).digest('hex'),url='https://cdn.cocoapods.org/Specs/'+md5[0]+'/'+md5[1]+'/'+md5[2]+'/'+name+'/'+version+'/'+name+'.podspec.json';await writeFile(specFile,await responseBytes(await options.fetcher(url,{signal:options.signal,redirect:'error'}),2*1024**2,options.signal),{flag:'wx'});}
    const spec=await verifyPodSpec(specFile,name,version,checksum,options),coordinate=podSourceCoordinate(spec),source=join(payload,'source');
    if(await stat(release)){await inventory(release);await copyPodSource(release,source);}else if(coordinate.ref){const checkout=join(candidate,'checkout');await gitCheckout(coordinate,checkout,options);await copyPodSource(checkout,source);await rm(checkout,{recursive:true});}else{const file=await packageOriginal(coordinate,options);await extractArchive(file,source,{signal:options.signal});}
    if(!await stat(release)&&spec.prepare_command){if(typeof spec.prepare_command!=='string')fail('Pod准备命令不是锁定文本');const foundation=await productFoundation(options.library,async(_,t)=>options.library.installed.get(t.id));await exec(foundation.tools.bash,['-ec',spec.prepare_command],{cwd:source,signal:options.signal,env:{...cleanEnvironment(options.environment),PATH:foundation.path,HOME:options.library.work,COCOAPODS_VERSION:options.library.tools.find(x=>x.id==='cocoapods').version}});}
    if(!await stat(specPath)){await directory(dirname(specPath),true);await copyFile(specFile,specPath,constants.COPYFILE_EXCL);}await verifyPodSpec(specPath,name,version,checksum,options);
    if(!await stat(release))await copyPodSource(source,release);if(JSON.stringify(await inventory(release))!==JSON.stringify(await inventory(source)))fail('Pod任务源码漂移');
   }finally{await rm(candidate,{recursive:true,force:true});}}
  await verifyPodSpec(specPath,name,version,checksum,options);
 }
 const version=options.library.tools.find(x=>x.id==='cocoapods')?.version,file=join(podHome,'cache/Pods/VERSION');await directory(dirname(file),true);if(await stat(file)){await regular(file);if((await readFile(file,'utf8')).trim()!==version)fail('Pod缓存工具版本漂移');}else await writeFile(file,version,{flag:'wx'});
 checkCocoaPodsResources(lockfile,podHome);return {restored};
}
async function platformTreeDigest(directory){const digest=createHash('sha256');async function visit(path=''){for(const name of (await readdir(join(directory,path))).sort()){if(name==='.DS_Store')continue;const relative=join(path,name),file=join(directory,relative),s=await lstat(file);if(s.isDirectory())await visit(relative);else{await regular(file);digest.update(JSON.stringify([relative,s.size])+'\n');digest.update(await readFile(file));}}}await visit();return digest.digest('hex');}
async function acquireOfficialPlatform(item,options){
 // 固定官方发行树先有界下载，再以产品登记的整树摘要验真；候选归入同一取消清理范围。
 if(options.offline)fail('离线缺少额外Android发行件');const data=await responseBytes(await options.fetcher(checkedURL(item.source),{signal:options.signal,redirect:'error'}),512*1024**2,options.signal);const file=join(options.library.work,'.platform-'+randomUUID()+'.zip');await writeFile(file,data,{flag:'wx'});return file;
}
async function installAndroidResources(options){const library=options.library,cmake=library.tools.find(x=>x.id==='cmake'),packages=androidDefinitions.map(x=>x.tool?{path:'cmake;'+cmake.version,version:cmake.version,...cmake.archives.macos}:x),wanted=library.requested.flatMap(x=>x.packages||[]);for(const item of wanted){const match=library.androidPlatforms?.find(x=>x.path===item.path&&x.version===item.version);if(!match)fail('SDK平台没有产品准确登记');if(!packages.some(x=>x.path===match.path))packages.push(match);}
 const sha256=hash(JSON.stringify(packages)),store=join(library.root,'shared');await directory(store,true);const target=join(store,'android-'+sha256);const verify=async directory=>{if(!await stat(directory))return null;await directoryCheck(directory);const payload=join(directory,'payload'),receipt=JSON.parse(await readFile(join(directory,'receipt.json'),'utf8'));if(receipt.sha256!==sha256||JSON.stringify(receipt.files)!==JSON.stringify(await inventory(payload)))fail('SDK原件回执不符');for(const item of packages){const text=await readFile(join(payload,...item.path.split(';'),'source.properties'),'utf8');if([...text.matchAll(/^Pkg\.Revision\s*=\s*(\S+)\s*$/gmu)].length!==1||!text.includes('Pkg.Revision='+item.version)&&!new RegExp('^Pkg\\.Revision\\s*=\\s*'+item.version.replaceAll('.','\\.')+'\\s*$','mu').test(text))fail('SDK组件版本不符：'+item.path);}return payload;};
 let payload=await verify(target);if(!payload){if(options.offline)fail('离线缺少SDK闭包');const pending=await fixedScratch(join(await resourceWork(options.library.work),'.android-'));try{payload=join(pending,'payload');await mkdir(payload);for(const item of packages){const at=join(payload,...item.path.split(';'));await directory(dirname(at),true);if(item.source){const file=await acquireOfficialPlatform(item,options),unpacked=join(pending,'unpack');try{await extractArchive(file,unpacked,{signal:options.signal});const names=await readdir(unpacked);if(names.length!==1)fail('额外平台归档根不唯一');await rename(join(unpacked,names[0]),at);await rm(unpacked,{recursive:true});if(await platformTreeDigest(at)!==item.sha256)fail('额外平台发行件树摘要不符');}finally{await rm(file,{force:true});}}else{const file=await packageOriginal(item,options),unpacked=join(pending,'unpack');await extractArchive(file,unpacked,{signal:options.signal});await rename(item.root==='.'?unpacked:join(unpacked,item.root),at);if(await stat(unpacked))await rm(unpacked,{recursive:true});}}await permissions(payload,false);await writeFile(join(pending,'receipt.json'),JSON.stringify({sha256,files:await inventory(payload)}),{flag:'wx',mode:0o444});await commitCandidate(pending,target,{signal:options.signal,verify});payload=await verify(target);}finally{if(await stat(pending)){await permissions(pending,true);await rm(pending,{recursive:true});}}}
 const versions=id=>library.tools.find(x=>x.id===id)?.version;for(const [id,file]of [['android','platform-tools/adb'],['android-sdk','cmdline-tools/'+versions('android-sdk')+'/bin/sdkmanager'],['android-ndk','ndk/'+versions('android-ndk')+'/ndk-build'],['cmake','cmake/'+versions('cmake')+'/bin/cmake']])if(library.requested.some(x=>x.id===id))library.installed.set(id,{path:join(payload,file),version:versions(id)});
 return {ANDROID_HOME:payload,ANDROID_SDK_ROOT:payload,ANDROID_NDK_HOME:join(payload,'ndk',versions('android-ndk')),ANDROID_USER_HOME:join(library.work,'android-user'),ANDROID_EMULATOR_HOME:join(library.work,'android-user')};
}
async function appleEnvironment(library,options){if(!library.installed.has('xcode'))return {};const mapping={xcodebuild:'XCODEBUILD',codesign:'CODESIGN',security:'SECURITY',xcrun:'XCRUN','xcode-select':'XCODE_SELECT',xattr:'XATTR',clang:'CC','clang++':'CXX',swift:'SWIFT',otool:'OTOOL',install_name_tool:'INSTALL_NAME_TOOL',lipo:'LIPO',make:'MAKE',ar:'AR',ranlib:'RANLIB',nm:'NM',strip:'STRIP','llvm-nm':'LLVM_NM'};const apple=await verifyAppleTools(library,{names:Object.keys(mapping),signal:options.signal,environment:cleanEnvironment(options.environment)}),environment={DEVELOPER_DIR:apple.developerDirectory};const bin=join(library.work,'apple-tools');await directory(bin,true);for(const [name,key]of Object.entries(mapping)){environment[key]=apple.tools[name];const target=join(bin,name);if(await stat(target)){if(!((await lstat(target)).isSymbolicLink())||await realpath(target)!==await realpath(apple.tools[name]))fail('Apple任务入口漂移');}else await symlink(apple.tools[name],target);}environment.PATH=bin;environment.LD=apple.tools.clang;environment.LDCXX=apple.tools['clang++'];environment.CARGO_TARGET_AARCH64_APPLE_DARWIN_LINKER=apple.tools.clang;const sdk=(await exec(apple.tools.xcrun,['--sdk','macosx','--show-sdk-path'],{signal:options.signal,env:{PATH:'',DEVELOPER_DIR:apple.developerDirectory},timeout:60000})).stdout.trim();environment.SDKROOT=await realpath(sdk);if(!inside(apple.developerDirectory,environment.SDKROOT))fail('SDK越出Xcode');return environment;}
// 可选供给遵循唯一原件协议，产品独立解析本仓锁，拒绝旧快照和状态库回退。
async function readDependencySupply(objects) {
 if(!objects||!await stat(objects))return null;await directory(objects);const file=join(dirname(objects),'index.json');await regular(file);
 if((await lstat(file)).size>32*1024**2)fail('可选原件索引超限');const value=JSON.parse(await readFile(file,'utf8'));
 if(!value||typeof value!=='object'||Array.isArray(value)||JSON.stringify(Object.keys(value).sort())!==JSON.stringify(['git_sources','packages','pods','schema_version'])||value.schema_version!==2||!Array.isArray(value.packages)||!Array.isArray(value.git_sources)||!Array.isArray(value.pods))fail('可选原件索引协议无效');return value;
}
async function supplyObject(objects,sha256,signal){signal?.throwIfAborted();if(!/^[a-f0-9]{64}$/u.test(sha256||''))fail('供给原件摘要无效');const file=join(objects,sha256+'.blob');await regular(file);const data=await readFile(file);if(hash(data)!==sha256)fail('供给原件摘要不符');return data;}
// 只恢复当前Pod坐标，完成所有文件后再核对内部链接，随后仍由产品校验spec及固定来源。
async function materializePodSupply(pod,objects,destination,{signal}={}) {
 signal?.throwIfAborted();if(!pod)return false;const keys=['checksum','files','name','source','spec','version'];if(JSON.stringify(Object.keys(pod).sort())!==JSON.stringify(keys)||!/^[A-Za-z0-9_.+-]+$/u.test(pod.name||'')||!/^[0-9A-Za-z][0-9A-Za-z._+-]*$/u.test(pod.version||'')||!/^[a-f0-9]{40}$/u.test(pod.checksum||'')||!Array.isArray(pod.files)||!pod.files.length)fail('Pod供给坐标无效');
 const md5=createHash('md5').update(pod.name).digest('hex'),url='https://cdn.cocoapods.org/Specs/'+md5[0]+'/'+md5[1]+'/'+md5[2]+'/'+pod.name+'/'+pod.version+'/'+pod.name+'.podspec.json';if(JSON.stringify(Object.keys(pod.spec||{}).sort())!==JSON.stringify(['sha256','url'])||pod.spec.url!==url)fail('Pod spec来源无效');
 const spec=await supplyObject(objects,pod.spec.sha256,signal),value=JSON.parse(spec);if(value.name!==pod.name||value.version!==pod.version||JSON.stringify(value.source)!==JSON.stringify(pod.source))fail('Pod spec与发布坐标漂移');podSourceCoordinate(value);
 const key=pod.version+'-'+pod.checksum.slice(0,5),release=join(destination,'cache/Pods/Release',pod.name,key),specFile=join(destination,'cache/Pods/Specs/Release',pod.name,key+'.podspec.json'),paths=new Set();
 for(const entry of pod.files){if(!safePath(entry.path)||paths.has(entry.path))fail('Pod发布路径无效或重复');paths.add(entry.path);const target=join(release,entry.path);await directory(dirname(target),true);
  if(entry.type==='file'&&JSON.stringify(Object.keys(entry).sort())===JSON.stringify(['executable','path','sha256','type'])&&typeof entry.executable==='boolean'){const bytes=await supplyObject(objects,entry.sha256,signal);if(await stat(target)){await regular(target);if(hash(await readFile(target))!==entry.sha256)fail('Pod任务缓存漂移');}else await writeFile(target,bytes,{flag:'wx'});await chmod(target,entry.executable?0o755:0o644);}
  else if(!(entry.type==='link'&&JSON.stringify(Object.keys(entry).sort())===JSON.stringify(['path','target','type'])&&typeof entry.target==='string'&&entry.target&&!entry.target.startsWith('/')&&!entry.target.includes('\\')&&safePath(posix.normalize(posix.join(posix.dirname(entry.path),entry.target)))))fail('Pod发布条目或链接无效');
 }
 for(const entry of pod.files.filter(x=>x.type==='link')){signal?.throwIfAborted();const target=join(release,entry.path);if(await stat(target)){if(!(await lstat(target)).isSymbolicLink()||await readlink(target)!==entry.target)fail('Pod链接漂移');}else await symlink(entry.target,target);}
 const tree=await inventory(release),nodes=tree.filter(x=>!x.directory);if(nodes.length!==paths.size||nodes.some(x=>!paths.has(x.path))||tree.some(x=>x.directory&&![...paths].some(path=>path.startsWith(x.path+'/'))))fail('Pod发布树混入状态或未登记项');for(const entry of pod.files.filter(x=>x.type==='link')){const target=await realpath(join(release,entry.path));if(!inside(release,target))fail('Pod内部链接越界');}
 await directory(dirname(specFile),true);if(await stat(specFile)){await regular(specFile);if(hash(await readFile(specFile))!==pod.spec.sha256)fail('Pod任务spec漂移');}else await writeFile(specFile,spec,{flag:'wx'});return true;
}
// Maven只接纳登记中的具体上游文件；URL同时确定后缀、分类器及下载文件名。
function mavenOriginal(entry){
 if(entry.ecosystem!=='maven'||JSON.stringify(Object.keys(entry).sort())!==JSON.stringify(['archives','ecosystem','name','version'])||!/^[0-9A-Za-z][0-9A-Za-z._+-]*$/u.test(entry.version||'')||/^(?:LATEST|RELEASE)$/u.test(entry.version)||entry.version.endsWith('-SNAPSHOT')||entry.archives?.length!==1)fail('Maven登记坐标无效');
 const [group,artifact,...extra]=entry.name.split(':');if(extra.length||!/^[A-Za-z0-9_-]+(?:\.[A-Za-z0-9_-]+)*$/u.test(group||'')||!/^[A-Za-z0-9_.-]+$/u.test(artifact||''))fail('Maven登记名称无效');
 const archive=entry.archives[0];if(JSON.stringify(Object.keys(archive).sort())!==JSON.stringify(['integrity','sha256','url']))fail('Maven原件字段无效');const url=new URL(checkedURL(archive.url)),bases={'repo.maven.apache.org':'/maven2/','dl.google.com':'/dl/android/maven2/','plugins.gradle.org':'/m2/','storage.googleapis.com':'/download.flutter.io/','jitpack.io':'/'},base=bases[url.hostname],prefix=base+group.replaceAll('.','/')+'/'+artifact+'/'+entry.version+'/',leaf=url.pathname.slice(prefix.length),filename=artifact+'-'+entry.version;
 if(!base||url.href!==archive.url||url.search||url.hash||url.port||!url.pathname.startsWith(prefix)||!leaf.startsWith(filename+'.')&&!leaf.startsWith(filename+'-')||!/^[A-Za-z0-9_.+-]+\.(?:jar|aar|pom|module)$/u.test(leaf)||url.hostname==='jitpack.io'&&(group!=='com.github.davidliu'||artifact!=='audioswitch'||!/^[a-f0-9]{40}$/u.test(entry.version)))fail('Maven登记不是准确上游来源');
 return {archive,source:url.origin+base,path:url.pathname.slice(base.length)};
}
// 只复制不可变原件到本轮独占Maven仓库，按上游分区避免同坐标不同来源相互覆盖。
async function materializeMavenCache(objects,work,{signal}={}) {
 signal?.throwIfAborted();await directory(work);const index=await readDependencySupply(objects);if(!index)return [];const records=index.packages.filter(x=>x.ecosystem==='maven').map(mavenOriginal);if(!records.length)return [];
 const destination=join(work,'dependencies/maven');await directory(dirname(destination),true);const repos=[...new Set(records.map(x=>x.source))].sort().map(source=>({source,directory:join(destination,hash(source))}));
 const verify=async root=>{const paths=new Map();for(const record of records){signal?.throwIfAborted();const path=hash(record.source)+'/'+record.path,prior=paths.get(path);if(prior&&prior!==record.archive.sha256)fail('Maven同源文件内容冲突');paths.set(path,record.archive.sha256);const file=join(root,path);await regular(file);const bytes=await readFile(file);if(hash(bytes)!==record.archive.sha256||verifyBytes(bytes,{integrity:record.archive.integrity})!==record.archive.sha256)fail('Maven任务原件摘要不符');}const tree=await inventory(root),files=tree.filter(x=>!x.directory);if(files.length!==paths.size||files.some(x=>!x.sha256||!paths.has(x.path))||tree.some(x=>x.directory&&![...paths.keys()].some(path=>path.startsWith(x.path+'/'))))fail('Maven任务仓库混入状态或未登记项');};
 if(await stat(destination)){await directory(destination);await verify(destination);return repos;}
 const candidate=await fixedScratch(join(dirname(destination),'.maven-'));try{for(const record of records){signal?.throwIfAborted();const bytes=await supplyObject(objects,record.archive.sha256,signal);verifyBytes(bytes,{integrity:record.archive.integrity});const file=join(candidate,hash(record.source),record.path);await directory(dirname(file),true);if(await stat(file)){await regular(file);if(hash(await readFile(file))!==record.archive.sha256)fail('Maven同源文件冲突');}else await writeFile(file,bytes,{flag:'wx',mode:0o644});}await verify(candidate);signal?.throwIfAborted();await rename(candidate,destination);await verify(destination);return repos;}finally{await rm(candidate,{recursive:true,force:true});}
}
// 供给镜像仅插在产品已声明的同源仓库前，缺件仍按原仓库解析；顺序与版本由产品控制。
function mavenSupplyInit(repositories){
 const quote=value=>"'"+value.replaceAll('\\','\\\\').replaceAll("'","\\'")+"'",data='['+repositories.map(x=>'[source:'+quote(x.source)+', directory:'+quote(x.directory)+']').join(',')+']';
 return `// 本轮产品资源视图，不读取共享Gradle状态。\nimport org.gradle.api.artifacts.repositories.MavenArtifactRepository\ndef supplied = ${data}\ndef attach = { repositories ->\n def seen = [] as Set\n repositories.all { original ->\n  if (original instanceof MavenArtifactRepository && !original.name.startsWith('productOriginal_')) {\n   def source = original.url.toString().replaceAll('/+$', '') + '/'\n   def record = supplied.find { it.source == source }\n   if (record != null && seen.add(original.name)) {\n    def local = repositories.maven { name = 'productOriginal_' + original.name; url = new File(record.directory).toURI(); artifactUrls(original.url); metadataSources { gradleMetadata(); mavenPom(); artifact() } }\n    repositories.remove(local)\n    repositories.add(repositories.indexOf(original), local)\n   }\n  }\n }\n}\ngradle.beforeSettings { settings -> attach(settings.pluginManagement.repositories); attach(settings.dependencyResolutionManagement.repositories) }\ngradle.beforeProject { project -> attach(project.buildscript.repositories); attach(project.repositories) }\n`;
}
// 远程Pod必须同时交付锁定spec与完整源码；本地路径Pod由本轮产品工程产生。
function checkCocoaPodsResources(lockfile,directory) {
 const text=readFileSync(lockfile,'utf8'),local=[...text.matchAll(/^  ([A-Za-z0-9_.+-]+):\n    :path: /gmu)].map(x=>x[1]);const checksums=new Map([...text.matchAll(/^  ([A-Za-z0-9_.+-]+): ([a-f0-9]{40})$/gmu)].map(x=>[x[1],x[2]]));
 for(const match of text.matchAll(/^  - "?([A-Za-z0-9_.+-]+)(?:\/[A-Za-z0-9_.+-]+)* \(([^()\s]+)\)"?/gmu)){const [,name,version]=match;if(local.includes(name))continue;const checksum=checksums.get(name);if(!checksum)fail('远程Pod缺少锁定spec摘要');const key=version+'-'+checksum.slice(0,5),spec=join(directory,'cache/Pods/Specs/Release',name,key+'.podspec.json'),release=join(directory,'cache/Pods/Release',name,key);if(!existsSync(spec)||!existsSync(release))fail('锁定CocoaPods原件尚未完整存在：'+name);}
 return {paths:local.map(name=>({name}))};
}
async function prepareGradleResources(work,options,environment){const gradle=options.library.installed.get('gradle');if(!gradle)return;const home=join(work,'dependencies/gradle');await directory(home,true);environment.GRADLE_USER_HOME=home;
 // Maven视图与初始化脚本仅属于本轮产品，缺少可选供给时按既有产品仓库独立解析。
 const mirrors=await materializeMavenCache(options.optionalDependencies,work,{signal:options.signal});if(mirrors.length){const initDirectory=join(home,'init.d'),initFile=join(initDirectory,'product-originals.gradle'),text=mavenSupplyInit(mirrors);await directory(initDirectory,true);if(await stat(initFile)){await regular(initFile);if(await readFile(initFile,'utf8')!==text)fail('Maven资源初始化漂移');}else await writeFile(initFile,text,{flag:'wx'});}

 const projects=[];async function find(path,depth=0){if(depth>12)return;for(const name of await readdir(path)){if(['dependencies','git-sources','.git','tmp','cache','config','apple-tools','resource-tools'].includes(name))continue;const file=join(path,name),s=await lstat(file);if(s.isDirectory())await find(file,depth+1);else if(name==='settings.gradle'||name==='settings.gradle.kts')projects.push(dirname(file));}}await find(work);
 for(const project of projects.filter(x=>x.endsWith('/android'))){const flutter=options.library.installed.get('flutter');if(flutter&&!await stat(join(dirname(project),'flutter-gradle')))await flutterRecipe.prepareFlutterTaskTools(dirname(dirname(flutter.path)),dirname(project),'android',{signal:options.signal,environment:{...environment,JAVA_HOME:dirname(dirname(options.library.installed.get('java').path)),GRADLE_HOME:dirname(dirname(gradle.path)),PRODUCT_BASH_BIN:options.library.installed.get('bash').path}});
  const init=join(work,'gradle-resource-init.gradle');if(!await stat(init))await writeFile(init,'// 仅解析产品现有配置，不编译、不扩展版本。\nallprojects { p -> p.tasks.register("productResolveResources") { doLast { p.configurations.findAll { it.canBeResolved }.each { it.resolve() } } } }\n');
  await exec(gradle.path,['--no-daemon','--console=plain','--init-script',init,...(options.offline?['--offline']:[]),'productResolveResources'],{cwd:project,signal:options.signal,timeout:1800000,maxBuffer:8*1024**2,env:{...cleanEnvironment(options.environment),...environment,JAVA_HOME:dirname(dirname(options.library.installed.get('java').path)),PATH:(await productFoundation(options.library,async(_,t)=>options.library.installed.get(t.id))).path}});
 }
}
const buildSourceTool=sourceRecipe.buildSourceTool;
const posixNames=posixRecipe.posixNames;
function resourceDeclarations(){return {tools:toolDefinitions,parsers:parserDefinitions,android:androidDefinitions,pods:podSourceDefinitions};}
// 完整入口可由调用方的启动Node进入；产品自行取得锁定Node并重新进入自己的入口。
// 启动Node只执行内置下载/摘要/解包，不成为产品编译工具版本的第二真源。
async function bootstrapNode(work,options={}) {
 const owner=await import('./build.mjs');owner.checkWork(work);const environment=options.environment||process.env;
 if(process.platform!=='darwin'||process.arch!=='arm64')fail('本机入口仅支持声明的macOS ARM宿主');
 const store=options.storeRoot||join(homedir(),'.local/share/product-resources');
 if(inside(root,store)||inside(store,root)||inside(work,store)||inside(store,work)||store===work)fail('启动原件库边界交叉');
 const library={root:join(store,'tools'),work,tools:toolDefinitions,installed:new Map()};await directory(library.root,true);
 const context={environment,offline:false,fetcher:fetch,...options,library,optionalTools:undefined,
  optionalDependencies:undefined};
 return installTool(library,toolDefinitions.find(x=>x.id==='node'),context);
}

// 直接依赖共用唯一安装树；上游内部不同版本按原锁保留标准嵌套，不改写原锁。
function editorDependencyPlan(documents){
 const result=new Map(),validName=name=>typeof name==='string'&&/^(?:@[a-z0-9_.-]+\/)?[a-z0-9_.-]+$/iu.test(name);
 const remote=entry=>entry&&!entry.link&&!entry.resolved?.startsWith('file:')&&entry.integrity&&entry.resolved;
 const original=(doc,key,name)=>{
  if(!validName(name))fail('编辑器包名无效');
  for(let at=key;;at=posix.dirname(at)){
   const path=posix.join(at==='.'?'':at,'node_modules',name),entry=doc.lock.packages[path];
   if(entry)return {doc,key:path,entry};if(at==='.'||!at)break;
  }
  return null;
 };
 const edges=item=>Object.keys({...item.entry.dependencies,...item.entry.optionalDependencies,...item.entry.peerDependencies}).sort();
 const identity=item=>JSON.stringify([item.entry.resolved,item.entry.integrity,edges(item).map(name=>{const dep=original(item.doc,item.key,name);return [name,dep?.entry.resolved,dep?.entry.integrity];})]);
 const put=(path,item)=>{if(!safePath(path)||!remote(item.entry))fail('编辑器依赖没有锁定原件');result.set(path,{...item,path});};
 // 根直接依赖必须相同；不能靠覆盖选取另一项目的版本。
 for(const doc of documents){
  if(![2,3].includes(doc.lock.lockfileVersion)||!doc.lock.packages?.[''])fail('编辑器原锁无效');
  for(const name of Object.keys({...doc.package.dependencies,...doc.package.devDependencies}).sort()){
   const item=original(doc,'',name);if(!item)fail('编辑器直接依赖缺少锁：'+name);
   if(!remote(item.entry)){if(!item.entry.link&&!item.entry.resolved?.startsWith('file:'))fail('编辑器本地依赖无效');continue;}
   const path='node_modules/'+name,previous=result.get(path);
   if(previous&&(previous.entry.integrity!==item.entry.integrity||previous.entry.resolved!==item.entry.resolved))fail('编辑器直接依赖版本冲突：'+name);
   if(!previous)put(path,item);
  }
 }
 const queue=[...result.values()];
 for(let index=0;index<queue.length;index++){
  if(queue.length>10000)fail('编辑器依赖闭包超限');
  const item=queue[index];
  for(const name of edges(item)){
   const expected=original(item.doc,item.key,name);
   if(!expected){if(Object.hasOwn(item.entry.dependencies||{},name)&&!Object.hasOwn(item.entry.optionalDependencies||{},name))fail('编辑器内部依赖缺少原锁：'+name);continue;}
   if(!remote(expected.entry))continue;
   let found;
   for(let at=item.path;;at=posix.dirname(at)){
    const candidate=result.get(posix.join(at==='.'?'':at,'node_modules',name));
    if(candidate){found=candidate;break;}if(at==='.'||!at)break;
   }
   if(found&&identity(found)===identity(expected))continue;
   const path=item.path+'/node_modules/'+name;
   if(result.has(path))fail('编辑器嵌套依赖冲突');put(path,expected);queue.push(result.get(path));
  }
 }
 return [...result.values()];
}
// 编辑器安装树保留于target/node_modules；归档和候选只使用本轮已有build/test现场。
async function editorResources(work,{objects,signal,fetcher=fetch,offline=false}={}){
 const owner=await import('./build.mjs');owner.checkWork(work);
 if(process.versions.node!==toolDefinitions.find(tool=>tool.id==='node').version)fail('编辑器入口Node版本不符');
 await directory(join(root,'target/build'),true);
 const marker=join(root,'target/build/.node_modules.lock'),handle=await open(marker,'wx',0o600);
 const documents=[],project=join(root,'target/node_modules'),candidate=join(work,'node_modules'),previous=join(work,'previous-node_modules');let moved=false,published=false;
 try{
  await handle.writeFile(JSON.stringify({pid:process.pid,work})+'\n');
  for(const relative of ['node/frontend','onchina/frontend','crates/scanner']){
   const input=join(root,relative),lockfile=join(input,'package-lock.json');await regular(lockfile);
   const bytes=await readFile(lockfile);documents.push({relative,lockfile,bytes,lock:JSON.parse(bytes),package:JSON.parse(await readFile(join(input,'package.json'),'utf8'))});
  }
  const plan=editorDependencyPlan(documents),store=join(work,'dependencies/npm');await directory(store,true);
  if(await stat(candidate)||await stat(previous))fail('编辑器候选现场已占用');await directory(candidate,true);
  const archives=new Map();
  for(const item of plan){
   signal?.throwIfAborted();let archive=archives.get(item.entry.integrity);
   if(!archive){archive=await acquireArchive({url:item.entry.resolved,integrity:item.entry.integrity},{store,work,optional:objects,offline,signal,fetcher});archives.set(item.entry.integrity,archive);}
   const output=join(work,item.path);await directory(dirname(output),true);
   // 复用安全解包及链接闭包校验，拒绝越界、特殊项与不同内容的重复成员。
   try{await extractArchive(archive,output,{stripRoot:true,signal});}catch(error){throw Error('编辑器依赖解包失败：'+item.path+'；'+error.message,{cause:error});}
  }
  for(const doc of documents)if(!doc.bytes.equals(await readFile(doc.lockfile)))fail('编辑器准备期间原锁变化');
  signal?.throwIfAborted();
  if(await stat(project)){await directory(project);await rename(project,previous);moved=true;}
  await rename(candidate,project);published=true;
  return project;
 }catch(error){if(moved&&!published)await rename(previous,project);throw error;}
 finally{
  await handle.close();await removeResourcePath(marker,{force:true});
  if(published&&moved){await permissions(previous,true);await removeResourcePath(previous,{recursive:true});}
 }
}

async function resources(platform,work,previous={},options={}){return resourceSupplies.run({work},()=>materializeResources(platform,work,previous,options));}
async function prepareResourceSupply(platform,work,previous,options){return resources(platform,work,previous,options);}
async function materializeResources(platform,work,previous={},options={}){
 const owner=await import('./build.mjs');owner.checkWork(work);const request=()=>owner.requirements(platform,work);const requirement=request(),environment=options.environment||process.env;
 if(previous.schema!==undefined&&(previous.schema!==1||previous.product_id!==requirement.product_id||previous.platform!==platform||previous.work!==work))fail('资源请求身份无效');options={environment,fetcher:fetch,offline:false,...options};options.signal?.throwIfAborted();
 const store=options.storeRoot||join(homedir(),'.local/share/product-resources'),optionalTools=options.toolRoot,optionalDependencies=options.dependencyRoot?join(options.dependencyRoot,'objects'):undefined;
 await directory(store,true);if(inside(root,store)||inside(store,root)||inside(work,store)||inside(store,work)||store===work)fail('原件库与源码或工作区交叉');
 // tools承载工具发行件/编译输入，rely承载产品依赖原件；不把可写任务缓存混入任一原件库。
 const library={root:options.toolRoot||join(store,'tools'),work,tools:toolDefinitions,requested:requirement.tools,installed:new Map(),androidPlatforms:androidPlatformDefinitions};await directory(library.root,true);options={...options,platform,optionalTools,optionalDependencies,library,dependencyRoot:options.dependencyRoot||join(store,'rely'),sources:requirement.sources};
 for(const request of requirement.tools){const definition=toolDefinitions.find(x=>x.id===request.id);if(!definition||definition.version!==request.version)fail('需求与产品自己的工具配方不一致：'+request.id);}
 // Node与其它工具同样按本产品声明准备，只消费实际入口。
 const node=toolDefinitions.find(x=>x.id==='node');if(process.platform!=='darwin'||process.arch!=='arm64')fail('本机资源配方仅支持已声明macOS ARM宿主');await installTool(library,node,options);if(hash(await readFile(process.execPath))!==hash(await readFile(library.installed.get('node').path)))fail('运行Node不是产品声明的官方入口字节');
 const android=requirement.tools.some(x=>['android','android-sdk','android-ndk'].includes(x.id));for(const request of requirement.tools){if(android&&['android','android-sdk','android-ndk','cmake'].includes(request.id))continue;await installTool(library,toolDefinitions.find(x=>x.id===request.id),options);}
 const receipt={schema:1,product_id:requirement.product_id,platform,work,tools:Object.fromEntries(library.installed),dependencies:{},archives:{},environment:{},offline:true};for(const key of ['run_id','program_digest'])if(previous[key]!==undefined)receipt[key]=previous[key];
 for(const id of ['posix','bash','grep','sed'])await installTool(library,toolDefinitions.find(x=>x.id===id),options);receipt.tools=Object.fromEntries(library.installed);const foundation=await productFoundation(library,async(_,t)=>library.installed.get(t.id));receipt.environment=await appleEnvironment(library,options);receipt.environment.PATH=[receipt.environment.PATH,foundation.path,...[...library.installed].filter(([id])=>id!=='posix').map(([,x])=>dirname(x.path))].filter(Boolean).join(':');receipt.environment.PRODUCT_WORK_DIR=work;
 if(android)Object.assign(receipt.environment,await installAndroidResources(options));receipt.tools=Object.fromEntries(library.installed);
 for(const source of requirement.sources)await gitCheckout(source,join(work,'git-sources',source.name),options);
 // 归属扩展由本产品判断：prepare产生的原生源码根也只能在同一work中消费。
 const groups=new Map();for(const lock of requirement.locks){const name=lock.source_package||'own';if(!groups.has(name))groups.set(name,[]);groups.get(name).push(lock);}for(const [name,locks]of groups){const base=name==='own'?root:owner.resourceSourceRoot(name,work);await directory(base);const files=kind=>locks.filter(x=>x.ecosystem===kind).map(x=>{if(!safePath(x.path))fail('锁路径越界');return join(base,x.path);}),target=join(work,'dependencies',name);await directory(target,true);let result={request:JSON.stringify(locks)};
  if(files('npm').length)Object.assign(result,await prepareNpm(files('npm'),target,options));if(files('cargo').length)Object.assign(result,await prepareCargo(files('cargo'),target,options));if(files('pub').length)Object.assign(result,await preparePub(files('pub'),join(target,'pub'),options));for(const file of files('cocoapods'))await preparePods(file,join(work,'dependencies'),options);receipt.dependencies[name]=result;
 }
 for(const item of requirement.archives){if(!safePath(item.group))fail('归档分组无效');const file=await packageOriginal(item,options),directory=join(work,'dependencies/archives',item.group);await directoryCheck(directory).catch(async e=>{if(e.code!=='ENOENT')throw e;await directoryCheck(work);await mkdir(directory,{recursive:true});});const path=join(directory,item.sha256+'.blob');if(!await stat(path))await copyFile(file,path,constants.COPYFILE_EXCL);await regular(path);if(hash(await readFile(path))!==item.sha256)fail('任务归档篡改');(receipt.archives[item.group]??=[]).push({...item,path});}
 const flutter=library.installed.get('flutter');if(flutter){const sdk=dirname(dirname(flutter.path));receipt.environment.DART_EXECUTABLE=join(sdk,'bin/cache/dart-sdk/bin/dart');const os=platform.endsWith('ios')?'ios':platform.endsWith('macos')?'macos':null;if(os&&!await stat(join(work,'flutter-tools'))){const delivery=await flutterRecipe.prepareFlutterTaskTools(sdk,work,os,{signal:options.signal,environment:{PRODUCT_BASH_BIN:library.installed.get('bash').path,PRODUCT_RSYNC_BIN:foundation.tools.rsync}});if(delivery.PATH)receipt.environment.PATH=delivery.PATH+':'+receipt.environment.PATH;}if(os)receipt.environment.PATH=join(work,'flutter-tools')+':'+receipt.environment.PATH;receipt.environment.PRODUCT_RSYNC_BIN=foundation.tools.rsync;receipt.environment.PRODUCT_BASH_BIN=library.installed.get('bash').path;}
 await prepareGradleResources(work,options,receipt.environment);options.signal?.throwIfAborted();if(JSON.stringify(request())!==JSON.stringify(requirement)){if((options.depth||0)>=8)fail('资源递归闭包超限');return materializeResources(platform,work,receipt,{...options,depth:(options.depth||0)+1});}
 owner.resourceEnvironment(platform,work,receipt,cleanEnvironment(environment));return receipt;
}

// 公开声明与候选配方；供给对象的领取、提交及删除只由调度方实现。
function supplyRequirements(){
 const wanted=toolDefinitions.filter(tool=>tool.id!=='xcode').map(tool=>{const archive=toolArchive(tool);return {...tool,archive,cache_key:toolSupplyKey(tool),slots:[...new Set([archive?.executable,...(tool.slots||[])].filter(Boolean))]};});
 const xcode=toolDefinitions.find(tool=>tool.id==='xcode');
 return {tools:wanted,apple:{version:xcode.version,source:xcode.source,names:[...new Set([...Object.keys(appleSystemTools),...appleBundleTools])]}};
}
function assertWorkQuiescent(work){for(const pid of supplyGroups.get(work)||[])try{process.kill(-pid,0);fail('资源工具退出未确认');}catch(error){if(error.code!=='ESRCH')throw error;}if(retainedResourcePath(work))fail('资源工具退出未确认');}


// CLI唯一JSON结果；资源日志由子工具stderr进入本轮调用者，信号取消贯穿全部阶段。
if(inlineTestEntry){
void (async()=>{
// 使用真实文件事务与受控HTTPS数据，禁止测试下载或安装真实工具。
const {test} = await import('node:test');
const {default:assert} = await import('node:assert/strict');
const {createHash} = await import('node:crypto');
const {mkdtemp,realpath,mkdir,readFile,writeFile,readdir,rm,symlink,chmod,lstat,rename} = await import('node:fs/promises');
const {dirname,join,resolve} = await import('node:path');
const tmpdir=testRoot;
const {spawnSync} = await import('node:child_process');
const {gzipSync} = await import('node:zlib');
const hash=b=>createHash('sha256').update(b).digest('hex');
async function sandbox(t){const root=await realpath(await mkdtemp(join(tmpdir(),contract.product_id+'-resources-')));t.after(()=>rm(root,{recursive:true,force:true}));return root;}
test('Xcode包内工具链接保留调用名称，越界和坏目标拒绝',async t=>{
 const fixture=await sandbox(t),developerDirectory=join(fixture,'Xcode.app/Contents/Developer');
 const bin=join(developerDirectory,'Toolchains/XcodeDefault.xctoolchain/usr/bin');await mkdir(bin,{recursive:true});
 const clang=join(bin,'clang'),cxx=join(bin,'clang++');await writeFile(clang,'fixture clang',{mode:0o755});await symlink('clang',cxx);
 const library={tools:[{id:'xcode',version:'27.0'}]};
 const run=async(file,args)=>({stdout:(file==='/usr/bin/xcode-select'?developerDirectory:join(bin,args[1]))+'\n'});
 const selected=await verifyAppleTools(library,{names:['clang','clang++'],run,environment:{}});
 assert.equal(selected.tools.clang,clang);assert.equal(selected.tools['clang++'],cxx);
 const system=await verifyAppleTools(library,{names:['xattr'],run,environment:{}});
 assert.equal(system.tools.xattr,'/usr/bin/xattr');
 await rm(cxx);const outside=join(fixture,'outside');await writeFile(outside,'outside',{mode:0o755});await symlink(outside,cxx);
 await assert.rejects(verifyAppleTools(library,{names:['clang++'],run,environment:{}}),/越出当前Xcode/);
 await rm(cxx);await symlink('missing',cxx);
 await assert.rejects(verifyAppleTools(library,{names:['clang++'],run,environment:{}}));
 await rm(cxx);await mkdir(cxx);
 await assert.rejects(verifyAppleTools(library,{names:['clang++'],run,environment:{}}),/越出当前Xcode/);
});
const archive=(body,url='https://example.invalid/locked.tgz')=>({url,sha256:hash(body)});
function tar(entries){const records=[];for(const {name,body='',type='0',target=''}of entries){const b=Buffer.from(body),h=Buffer.alloc(512);h.write(name,0,100);h.write('0000644\0',100);h.write('0000000\0',108);h.write('0000000\0',116);h.write(b.length.toString(8).padStart(11,'0')+'\0',124);h.write('00000000000\0',136);h.fill(32,148,156);h.write(type,156);h.write(target,157,100);h.write('ustar\0',257);h.write('00',263);h.write([...h].reduce((a,b)=>a+b,0).toString(8).padStart(6,'0')+'\0 ',148);records.push(h,b,Buffer.alloc((512-b.length%512)%512));}return gzipSync(Buffer.concat([...records,Buffer.alloc(1024)]));}
test('首次按锁取得；再次复用不联网，损坏原件不覆盖',async t=>{
 const root=await sandbox(t),body=Buffer.from('locked-source'),entry=archive(body);let requests=0;
 const options={store:root,fetcher:async()=>{requests++;return new Response(body);}};
 const file=await acquireArchive(entry,options);assert.equal(await readFile(file,'utf8'),'locked-source');
 assert.equal(await acquireArchive(entry,{...options,offline:true,fetcher:()=>assert.fail('离线联网')}),file);assert.equal(requests,1);
 await chmod(file,0o600);await writeFile(file,'corrupt');await assert.rejects(acquireArchive(entry,options),/摘要/);assert.equal(requests,1);assert.equal(await readFile(file,'utf8'),'corrupt');
});
test('错摘要、错来源、离线缺失、来源越权均失败关闭且无正式原件',async t=>{
 const root=await sandbox(t),body=Buffer.from('source'),entry=archive(body);let requests=0;
 await assert.rejects(acquireArchive(entry,{store:root,offline:true,fetcher:()=>assert.fail('离线联网')}),/离线/);
 await assert.rejects(acquireArchive(entry,{store:root,fetcher:async()=>{requests++;return new Response('wrong');}}),/摘要/);
 await assert.rejects(acquireArchive({...entry,url:'http://example.invalid/source'},{store:root}),/HTTPS/);
 const file=await acquireArchive(entry,{store:root,fetcher:async()=>new Response(body)});
 await assert.rejects(acquireArchive({...entry,url:'https://example.invalid/other'},{store:root,offline:true}),/离线/);
 assert.equal(await readFile(file,'utf8'),'source');assert.equal(requests,1);assert.equal((await readdir(root)).filter(x=>x.endsWith('.pending')||x.endsWith('.lock')).length,0);
});
test('可选供给按准确内容摘要验真，独立运行不需要供给目录',async t=>{
 const root=await sandbox(t),store=join(root,'store'),optional=join(root,'objects'),body=Buffer.from('shared-source'),entry=archive(body),source=join(optional,entry.sha256+'.blob');await mkdir(dirname(source),{recursive:true});await writeFile(source,body);
 const file=await acquireArchive(entry,{store,optional,offline:true,fetcher:()=>assert.fail('供给命中联网')});assert.equal(await readFile(file,'utf8'),'shared-source');await writeFile(source,'altered');assert.equal(await readFile(file,'utf8'),'shared-source');
 await assert.rejects(acquireArchive(entry,{store:join(root,'other'),optional,offline:true}),/摘要/);
});
test('取消下载清理本次候选；短锁只在提交阶段取得',async t=>{
 const root=await sandbox(t),entry=archive(Buffer.from('ab')),abort=new AbortController();
 const fetcher=async()=>new Response(new ReadableStream({start(controller){controller.enqueue(Buffer.from('a'));abort.abort();controller.close();}}));
 await assert.rejects(acquireArchive(entry,{store:root,fetcher,signal:abort.signal}));assert.deepEqual(await readdir(root),[]);
 let state;const file=await acquireArchive(entry,{store:root,fetcher:async()=>{state=await readdir(root);return new Response('ab');}});assert.deepEqual(state,[]);assert.equal(await readFile(file,'utf8'),'ab');
});
test('同对象并发提交只保留一份验真原件，不留全局下载锁',async t=>{
 const root=await sandbox(t),body=Buffer.from('concurrent'),entry=archive(body);let calls=0;const options={store:root,fetcher:async()=>{calls++;await new Promise(r=>setTimeout(r,10));return new Response(body);}};
 const paths=await Promise.all(Array.from({length:8},()=>acquireArchive(entry,options)));assert.equal(new Set(paths).size,1);assert.equal(await readFile(paths[0],'utf8'),'concurrent');assert.equal(calls,8);assert.deepEqual(await readdir(root),[paths[0].slice(root.length+1)]);
});
test('npm唯一顶层与同字节点路径别名，冲突及越界均拒绝',async t=>{
 const root=await sandbox(t),input=join(root,'npm.tgz');
 await writeFile(input,tar([{name:'react/./index.d.ts',body:'types'},{name:'react/index.d.ts',body:'types'}]));
 await extractArchive(input,join(root,'valid'),{stripRoot:true});assert.equal(await readFile(join(root,'valid/index.d.ts'),'utf8'),'types');
 for(const [name,entries]of [
  ['different',[{name:'package/./a',body:'x'},{name:'package/a',body:'y'}]],
  ['duplicate',[{name:'package/a',body:'x'},{name:'package/a',body:'x'}]],
  ['roots',[{name:'package/a',body:'x'},{name:'other/a',body:'x'}]],
  ['escape',[{name:'package/../a',body:'x'}]],
  ['absolute',[{name:'/package/./a',body:'x'}]]
 ]){await writeFile(input,tar(entries));await assert.rejects(extractArchive(input,join(root,name),{stripRoot:true}));assert.equal(await stat(join(root,name)),null);}
 await writeFile(input,tar([{name:'package/./a',body:'x'}]));await assert.rejects(extractArchive(input,join(root,'strict'),{prefix:'package'}),/越界/);
});
test('归档安全解包并隔离不同任务，拒绝路径和链接越界',async t=>{
 const root=await sandbox(t),source=join(root,'source.tgz'),data=tar([{name:'package/a',body:'source'},{name:'package/b',type:'2',target:'a'}]);await writeFile(source,data);
 const first=join(root,'first'),second=join(root,'second');await extractArchive(source,first,{prefix:'package'});await extractArchive(source,second,{prefix:'package'});assert.equal(await realpath(join(first,'b')),join(first,'a'));await writeFile(join(first,'a'),'task1');assert.equal(await readFile(join(second,'a'),'utf8'),'source');
 for(const [name,entries]of [['path',[{name:'../outside',body:'x'}]],['link',[{name:'package/a',body:'x'},{name:'package/b',type:'2',target:'../../outside'}]],['parent',[{name:'package/a',type:'2',target:'b'},{name:'package/a/child',body:'x'},{name:'package/b',body:'x'}]]]){const file=join(root,name+'.tgz');await writeFile(file,tar(entries));await assert.rejects(extractArchive(file,join(root,name),{prefix:name==='path'?'':'package'}),/越界|父目录/);assert.equal((await readdir(root)).includes(name),false);}
});
test('链接原件目录、重复成员与解包取消拒绝且不写第三方目录',async t=>{
 const root=await sandbox(t),external=join(root,'external'),link=join(root,'link');await mkdir(external);await symlink(external,link);await assert.rejects(acquireArchive(archive(Buffer.from('source')),{store:link,offline:true}),/链接/);assert.deepEqual(await readdir(external),[]);
 const input=join(root,'input.tgz');await writeFile(input,tar([{name:'a',body:'x'},{name:'a',body:'y'}]));await assert.rejects(extractArchive(input,join(root,'duplicate')),/重复/);
 const signal=AbortSignal.abort();await assert.rejects(extractArchive(input,join(root,'cancelled'),{signal}));assert.equal((await readdir(root)).includes('cancelled'),false);
});
test('产品配方覆盖自身需求和递归工具，模块只使用内置依赖与本产品固定根接口，独立CLI拒绝错误输入',async t=>{
 const root=await sandbox(t),declarations=resourceDeclarations(),tools=new Map(declarations.tools.map(x=>[x.id,x]));for(const platform of Object.values(contract.platforms))for(const tool of platform.tools){assert.equal(tools.get(tool.id)?.version,tool.version);}
 for(const tool of tools.values())for(const id of tool.requires||[])assert.ok(tools.has(id),'缺少递归工具 '+id);
 for(const name of ['node','protoc','posix','bash','grep','sed'])assert.ok(tools.has(name));const source=await readFile(new URL('./build.mjs',import.meta.url),'utf8');assert.deepEqual([...source.matchAll(/import\(['"](\.\.\/[^'"]+)['"]\)/gu)].map(m=>m[1]),[]);assert.ok([...source.matchAll(/^import .*? from ['"]([^'"]+)['"]/gmu)].every(m=>m[1].startsWith('node:')));
 const result=spawnSync(process.execPath,[join(import.meta.dirname,'build.mjs'),'unknown','--work',root,'--offline'],{env:{HOME:root,LANG:'C',PATH:''},encoding:'utf8'});assert.notEqual(result.status,0);assert.match(result.stderr,/固定入口参数|固定目录/);assert.deepEqual(await readdir(root),[]);
});

test('Git Cargo工作区继承按当前产品锁展开，不留下跨包路径',()=>{
 const input={package:{name:'one',version:{workspace:true}},dependencies:{two:{workspace:true},third:{path:'../third'}}};const workspace={workspace:{package:{version:'1.0.0'},dependencies:{two:{path:'two',version:'2.0.0',features:['a']}}}};const lock=[{name:'third',version:'3.0.0'}];
 const result=normalizeCargoManifest(input,workspace,lock);assert.equal(result.package.version,'1.0.0');assert.equal(result.dependencies.two.path,undefined);assert.equal(result.dependencies.third.version,'=3.0.0');assert.deepEqual(input.package.version,{workspace:true});assert.throws(()=>normalizeCargoManifest(input,workspace,[]),/唯一锁定版本/);
});
test('资源子进程可取消，不能继续输出成功回执',async()=>{
 const signal=AbortSignal.timeout(150);await assert.rejects(runResourceProcess(process.execPath,['-e','setInterval(()=>{},1000)'],{signal,env:{PATH:''}}),/abort|timeout|取消/iu);
});

// 使用产品真实源码工具生产器；编译/Apple能力边界受控，文件事务和输出验真实际执行。
const registry=resourceDeclarations();
async function sourceFixture(t, behavior = {}) {
  const root = await realpath(await mkdtemp(join(tmpdir(), 'source-tool-')));
  const owner = await lstat(root);
  t.after(async () => {
    const current = await lstat(root);
    assert.equal(current.dev, owner.dev); assert.equal(current.ino, owner.ino);
    await rm(root, { recursive: true, force: true });
  });
  const library = { root: join(root, 'tools'), work:join(root,'work'), tools: registry.tools };await mkdir(library.work);
  const tool = structuredClone(registry.tools.find(tool => tool.id === 'perl'));
  const bytes = Buffer.from('official-fixture-archive');
  tool.archive.sha256 = createHash('sha256').update(bytes).digest('hex');
  const pending = join(library.root, 'shared', tool.archive.sha256 + '.pending');
  const payload = join(pending, 'payload'), source = join(pending, 'unpack', tool.archive.root);
  const finalPayload = join(library.root, 'shared', tool.archive.sha256, 'payload');library.pending=pending;library.finalPayload=finalPayload;
  const developerDirectory = join(root, 'Xcode.app/Contents/Developer');
  const sdk = join(developerDirectory, 'Platforms/MacOSX.platform/Developer/SDKs/MacOSX.sdk');
  await mkdir(source, { recursive: true }); await mkdir(sdk, { recursive: true });
  await writeFile(join(pending, 'archive'), bytes);
  await writeFile(join(source, 'Artistic'), 'fixture upstream legal text');
  // 本夹具只声明实际Configure安装路径和受控入口，不复制任何真实工具原件。
  await writeFile(join(source,'config.sh'),
    "installprivlib='"+finalPayload+"/lib/5.42.3'\ninstallarchlib='"+finalPayload+"/lib/5.42.3/aarch64-darwin'\n");
  const posix=join(root,'verified/posix/bin');await mkdir(posix,{recursive:true});
  for(const name of posixNames)await writeFile(join(posix,name),'fixture executable '+name,{mode:0o755});
  // 完整基础工具交付属于夹具输入；不会复制或安装真实工具原件。
  for(const id of ['bash','grep','sed']) {
    const bin=join(root,'verified',id,'bin');await mkdir(bin,{recursive:true});
    await writeFile(join(bin,id),'fixture executable '+id,{mode:0o755});
  }
  const calls = [];
  const exec = async (command, args, options) => {
    calls.push({ command, args, options });
    if (command.endsWith('/xcrun')) return { stdout: sdk + '\n' };
    if (args.includes('-MJSON::PP')) return { stdout: behavior.coreFails ? '' : 'controlled-perl-ok\n' };
    if (behavior.compilerFails && args.includes('-j8')) throw new Error('compiler failed');
    if (args.includes('install')) {
      const destination = args.find(value => value.startsWith('DESTDIR=')).slice(8);
      const staged = join(destination, finalPayload.slice(1));
      await mkdir(join(staged, 'bin'), { recursive: true });
      if (behavior.linkOutput) await symlink(join(pending, 'archive'), join(staged, 'bin/perl'));
      else if (!behavior.missingOutput) {
        const macho = Buffer.alloc(32); macho.writeUInt32LE(0xfeedfacf, 0); macho.writeUInt32LE(0x0100000c, 4);
        await writeFile(join(staged, 'bin/perl'), macho, { mode: 0o755 });
        await mkdir(join(staged, 'lib/5.42.3/aarch64-darwin'), { recursive: true });
        await writeFile(join(staged, 'lib/5.42.3/aarch64-darwin/Config.pm'), 'fixture core module');
      }
    }
    return { stdout: '' };
  };
  const input = { library, tool, pending, payload, source, archive: join(pending, 'archive'), finalPayload,
    environment: { PATH: '/untrusted/bin', RUBYOPT: '-rmalicious', PYTHONPATH: '/untrusted',
      DYLD_INSERT_LIBRARIES: '/untrusted', LD_PRELOAD: '/untrusted', ARCHFLAGS: '-arch x86_64', CFLAGS: 'malicious', PERL5OPT: '-Mmalicious' },
    exec, verify: async (_, tool) => behavior.missingTool === tool.id ? null
      : { path: join(root, 'verified', tool.id, 'bin', tool.command), version: tool.version },
    apple: async () => ({ developerDirectory, version: '27.0',
      tools: Object.fromEntries(['clang', 'clang++', 'ar', 'make', 'ld', 'as', 'nm', 'ranlib', 'strip', 'xcrun', 'otool', 'install_name_tool', 'codesign'].map(name => [name, join(developerDirectory, 'usr/bin', name)])) }),
    // 产品依赖准备只返回归档映射，不创建旧工具库的originals目录。
    prepare: async () => new Map() };
  return { input, calls, bytes };
}
test('源码工具使用准确Apple编译入口并只在候选中收集输出、原件和编译输入', async t => {
  const { input, calls, bytes } = await sourceFixture(t);
  await buildSourceTool(input);
  const configure = calls.find(call => call.args.includes('-des'));
  assert.ok(configure.args.includes('-Dinstallusrbinperl=n'));
  for (const key of ['RUBYOPT', 'PYTHONPATH', 'DYLD_INSERT_LIBRARIES', 'PERL5OPT', 'LD_PRELOAD']) assert.equal(configure.options.env[key], undefined);
  assert.equal(configure.options.env.CFLAGS,'-O2');
  assert.equal(configure.options.env.MACOSX_DEPLOYMENT_TARGET,registry.tools.find(t=>t.id==='posix').version);
  assert.ok(configure.options.env.SDKROOT.startsWith(configure.options.env.DEVELOPER_DIR+'/'));
  assert.equal(configure.options.env.CPP,configure.options.env.CC+' -E');
  assert.ok(!configure.options.env.PATH.split(':').some(p=>['/usr/bin','/bin','/opt/homebrew/bin'].includes(p)));
  assert.equal(configure.options.env.ARCHFLAGS, '-arch arm64');
  assert.ok(configure.options.env.CC.startsWith(input.pending.split('/tools/')[0] + '/Xcode.app/'));
  assert.ok(!configure.options.env.PATH.includes('/untrusted/'));
  assert.ok(calls.some(call => call.args.includes('-MJSON::PP') && call.options.env.PERL5LIB.startsWith(input.payload + '/lib/')));
  assert.equal(await readFile(join(input.payload, 'licenses/Artistic'), 'utf8'), 'fixture upstream legal text');
});
for (const behavior of [{ compilerFails: true }, { missingOutput: true }, { linkOutput: true }, { missingTool: 'node' }, { coreFails: true }]) {
  test('源码工具失败边界保留失败且不写入最终工具对象：' + JSON.stringify(behavior), async t => {
    const { input } = await sourceFixture(t, behavior);
    await assert.rejects(buildSourceTool(input));
    await assert.rejects(readFile(join(input.finalPayload, 'bin/perl')), { code: 'ENOENT' });
  });
}
test('官方完整归档被替换时在任何编译前失败', async t => {
  const { input, calls } = await sourceFixture(t);
  await writeFile(input.archive, 'changed');
  await assert.rejects(buildSourceTool(input), /归档摘要/);
  assert.equal(calls.length, 0);
});
test('候选路径不属于当前工具摘要时在任何编译前失败', async t => {
  const { input, calls } = await sourceFixture(t);
  input.finalPayload += '-other';
  await assert.rejects(buildSourceTool(input), /候选对象身份/);
  assert.equal(calls.length, 0);
});

test('空可选供给不阻断产品取得，npm SRI原件按准确来源复用',async t=>{
 const root=await sandbox(t),body=Buffer.from('sri-original'),entry={url:'https://example.invalid/sri.tgz',integrity:'sha512-'+createHash('sha512').update(body).digest('base64')};
 const file=await acquireArchive(entry,{store:join(root,'first'),optional:join(root,'absent'),fetcher:async()=>new Response(body)});assert.equal(await readFile(file,'utf8'),'sri-original');
 const optional=join(root,'shared/objects'),digest=hash(body),original=join(optional,digest+'.blob');await mkdir(dirname(original),{recursive:true});await writeFile(original,body);await writeFile(join(root,'shared/index.json'),JSON.stringify({schema_version:2,packages:[{archives:[{...entry,sha256:digest}]}],git_sources:[],pods:[]}));
 const cached=await acquireArchive(entry,{store:join(root,'second'),optional,offline:true,fetcher:()=>assert.fail('SRI供给命中联网')});assert.equal(await readFile(cached,'utf8'),'sri-original');
});

test('Pod浮动tag必须由产品固定提交闭合，来源漂移或无摘要HTTP发行件失败',()=>{
 const url='https://github.com/example/project.git',ref='a'.repeat(40),spec={name:'Example',version:'1.0.0',source:{git:url,tag:'v1.0.0'}},definitions=[{name:'Example',version:'1.0.0',url,tag:'v1.0.0',ref}];
 assert.deepEqual(podSourceCoordinate(spec,definitions),{url,ref});assert.throws(()=>podSourceCoordinate(spec,[]),/锁定/);
 assert.throws(()=>podSourceCoordinate({...spec,source:{git:'https://github.com/example/other.git',tag:'v1.0.0'}},definitions),/来源/);
 assert.throws(()=>podSourceCoordinate({...spec,source:{git:url,tag:'v2.0.0'}},definitions),/来源/);
 assert.throws(()=>podSourceCoordinate({name:'HTTP',version:'1',source:{http:'https://example.invalid/archive.zip'}},[]),/锁定/);
 for(const entry of resourceDeclarations().pods){const source=entry.ref?{git:entry.url,tag:entry.tag}:{http:entry.url};const coordinate=podSourceCoordinate({name:entry.name,version:entry.version,source});assert.equal(coordinate.ref||coordinate.sha256,entry.ref||entry.sha256);}
});

// PostgreSQL来源与启动配方归本产品；断言固定公开坐标，不执行资源取得。
test('PostgreSQL使用产品固定官方原件',async()=>{const {resourceDeclarations}=await import('./build.mjs');const definition=resourceDeclarations().tools.find(tool=>tool.id==='postgres');assert.equal(definition.version,'17.11');assert.equal(definition.archive.url,'https://ftp.postgresql.org/pub/source/v17.11/postgresql-17.11.tar.bz2');assert.equal(definition.archive.sha256,'dd27f2b3c59e73ed14aa3324901242bf69a032a6347805f274e6260322d42979');assert.ok(definition.requires.includes('openssl'));});

// 真实进程退出顺序：取消回执必须晚于子工具完成清理，不能用发送信号代替退出确认。
test('资源取消等待真实工具清理并确认退出后才返回失败',{timeout:20000},async t=>{
 const directory=await sandbox(t),ready=join(directory,'ready'),closed=join(directory,'closed');
 const code=`import {writeFileSync} from 'node:fs';process.once('SIGTERM',()=>setTimeout(()=>{writeFileSync(${JSON.stringify(closed)},'closed');process.exit(0);},600));writeFileSync(${JSON.stringify(ready)},'ready');setInterval(()=>{},1000);`;
 const controller=new AbortController();const running=runResourceProcess(process.execPath,['--input-type=module','-e',code],{cwd:directory,env:{PRODUCT_WORK_DIR:directory},signal:controller.signal});
 for(let n=0;n<200;n++){try{await readFile(ready);break;}catch{await new Promise(ok=>setTimeout(ok,10));}}
 assert.equal(await readFile(ready,'utf8'),'ready');const start=Date.now();controller.abort(Error('资源进程取消'));
 await assert.rejects(running,/取消/u);assert.equal(await readFile(closed,'utf8'),'closed');assert.ok(Date.now()-start>=500);
});
test('资源超时等待退出，错误入口和输出超限不产生成功回执',{timeout:20000},async t=>{
 const directory=await sandbox(t),closed=join(directory,'timeout-closed');
 const code=`import {writeFileSync} from 'node:fs';process.once('SIGTERM',()=>setTimeout(()=>{writeFileSync(${JSON.stringify(closed)},'closed');process.exit(0);},400));setInterval(()=>{},1000);`;
 await assert.rejects(runResourceProcess(process.execPath,['--input-type=module','-e',code],{cwd:directory,timeout:500}),/超时/u);
 assert.equal(await readFile(closed,'utf8'),'closed');
 await assert.rejects(runResourceProcess(join(directory,'missing'),[],{cwd:directory}),/无法启动/u);
 await assert.rejects(runResourceProcess(process.execPath,['-e','process.stdout.write("x".repeat(4096))'],{cwd:directory,maxBuffer:64}),/输出超限/u);
});

// 依赖供给夹具只写真实独占文件，覆盖唯一协议及任务视图隔离，不下载和安装工具。
async function dependencySupplyFixture(t,packages=[],pods=[]){const root=await sandbox(t),objects=join(root,'supply/objects'),work=join(root,'work');await mkdir(objects,{recursive:true});await mkdir(work);const index={schema_version:2,packages,git_sources:[],pods};await writeFile(join(dirname(objects),'index.json'),JSON.stringify(index));return {root,objects,work,index};}
const suppliedMaven=(bytes,source='https://repo.maven.apache.org/maven2/',suffix='pom')=>({ecosystem:'maven',name:'example:library',version:'1.0.0',archives:[{url:source+'example/library/1.0.0/library-1.0.0.'+suffix,integrity:'sha256-'+createHash('sha256').update(bytes).digest('base64'),sha256:hash(bytes)}]});
test('无可选依赖供给保持独立，旧schema与Pod整锁快照被拒绝',async t=>{
 const f=await dependencySupplyFixture(t);assert.equal(await readDependencySupply(join(f.root,'absent')),null);assert.deepEqual(await materializeMavenCache(undefined,f.work),[]);
 for(const value of [null,[],{schema_version:1,packages:[],git_sources:[],snapshots:[]},{...f.index,snapshots:[]}]){await writeFile(join(dirname(f.objects),'index.json'),JSON.stringify(value));await assert.rejects(readDependencySupply(f.objects),/协议/);}
});
test('Maven按上游分区重建，JAR分类器及module文件名保留，共享原件不承接写入',async t=>{
 const a=Buffer.from('central-pom'),b=Buffer.from('portal-pom'),c=Buffer.from('classifier-original'),d=Buffer.from('{"formatVersion":"1.1"}');const entries=[suppliedMaven(a),suppliedMaven(b,'https://plugins.gradle.org/m2/'),suppliedMaven(c,undefined,'jar'),suppliedMaven(d,undefined,'module')];entries[2].archives[0].url=entries[2].archives[0].url.replace('.jar','-sources.jar');const f=await dependencySupplyFixture(t,entries);
 for(const bytes of[a,b,c,d])await writeFile(join(f.objects,hash(bytes)+'.blob'),bytes);
 const repos=await materializeMavenCache(f.objects,f.work);assert.equal(repos.length,2);const central=repos.find(x=>x.source.includes('repo.maven.apache.org')),portal=repos.find(x=>x.source.includes('plugins.gradle.org'));assert.equal(await readFile(join(central.directory,'example/library/1.0.0/library-1.0.0.pom'),'utf8'),'central-pom');assert.equal(await readFile(join(portal.directory,'example/library/1.0.0/library-1.0.0.pom'),'utf8'),'portal-pom');assert.equal(await readFile(join(central.directory,'example/library/1.0.0/library-1.0.0-sources.jar'),'utf8'),'classifier-original');
 assert.deepEqual(await materializeMavenCache(f.objects,f.work),repos);const script=mavenSupplyInit(repos);assert.match(script,/beforeSettings/);assert.match(script,/beforeProject/);assert.match(script,/artifactUrls\(original.url\)/);assert.doesNotMatch(script,/modules-2|rely\/maven/);
 await writeFile(join(central.directory,'example/library/1.0.0/library-1.0.0.pom'),'task-changed');assert.equal(await readFile(join(f.objects,hash(a)+'.blob'),'utf8'),'central-pom');await assert.rejects(materializeMavenCache(f.objects,f.work),/摘要/);
});
for(const change of ['sha','sri','source','version','duplicate','state','link','cancel'])test('Maven拒绝错误原件或状态并保留失败：'+change,async t=>{
 const bytes=Buffer.from('maven-original'),entry=suppliedMaven(bytes),f=await dependencySupplyFixture(t,[entry]);await writeFile(join(f.objects,hash(bytes)+'.blob'),bytes);
 if(change==='sha')await writeFile(join(f.objects,hash(bytes)+'.blob'),'changed');if(change==='sri')entry.archives[0].integrity='sha256-'+Buffer.alloc(32).toString('base64');if(change==='source')entry.archives[0].url='https://other.invalid/maven2/example/library/1.0.0/library-1.0.0.pom';if(change==='version')entry.version='LATEST';if(change==='duplicate')f.index.packages.push({...entry,archives:[{...entry.archives[0],sha256:'a'.repeat(64)}]});
 await writeFile(join(dirname(f.objects),'index.json'),JSON.stringify(f.index));if(change==='state'){const [repo]=await materializeMavenCache(f.objects,f.work);await writeFile(join(repo.directory,'gc.properties'),'generated');}if(change==='link'){await mkdir(join(f.root,'outside'));await symlink(join(f.root,'outside'),join(f.work,'dependencies'));}
 const signal=change==='cancel'?AbortSignal.abort(Error('取消')):undefined;await assert.rejects(materializeMavenCache(f.objects,f.work,{signal}));assert.equal(await readFile(join(f.objects,hash(bytes)+'.blob'),'utf8'),change==='sha'?'changed':'maven-original');assert.equal((await readdir(join(f.work,'dependencies')).catch(e=>{if(e.code==='ENOENT')return [];throw e;})).some(x=>x.startsWith('.maven-')),false);
});
async function podSupplyFixture(t){
 // 合成Pod自带固定提交，不读取真实产品的Pod清单作为测试输入。
 const name='PodFixture',version='1.0.0',source={git:'https://github.com/example/PodFixture.git',commit:'1'.repeat(40)},bytes=Buffer.from(JSON.stringify({name,version,source})),file=Buffer.from('pod-source'),md5=createHash('md5').update(name).digest('hex');
 const pod={name,version,checksum:'a'.repeat(40),spec:{url:'https://cdn.cocoapods.org/Specs/'+md5[0]+'/'+md5[1]+'/'+md5[2]+'/'+name+'/'+version+'/'+name+'.podspec.json',sha256:hash(bytes)},source,files:[{type:'file',path:'Example.framework/Versions/A/Headers/source.h',sha256:hash(file),executable:false},{type:'link',path:'Example.framework/Versions/Current',target:'A'},{type:'link',path:'Example.framework/Headers',target:'Versions/Current/Headers'}]};const f=await dependencySupplyFixture(t,[],[pod]);for(const value of[bytes,file])await writeFile(join(f.objects,hash(value)+'.blob'),value);return {...f,pod,bytes,file};
}
test('Pod单坐标供给不依赖整锁与宿主，Framework多级链接仅在本轮物化',async t=>{
 const f=await podSupplyFixture(t);assert.equal(await materializePodSupply(undefined,f.objects,f.work),false);assert.equal(await materializePodSupply(f.pod,f.objects,f.work),true);assert.equal(await materializePodSupply(f.pod,f.objects,f.work),true);const release=join(f.work,'cache/Pods/Release',f.pod.name,f.pod.version+'-aaaaa');assert.equal(await readFile(join(release,'Example.framework/Headers/source.h'),'utf8'),'pod-source');await writeFile(join(release,'Example.framework/Headers/source.h'),'task-write');assert.equal(await readFile(join(f.objects,hash(f.file)+'.blob'),'utf8'),'pod-source');await assert.rejects(materializePodSupply(f.pod,f.objects,f.work),/漂移/);
});
for(const change of ['sha','spec','source','escape','duplicate','cycle','state','cancel'])test('Pod错来源、摘要和不安全链接失败关闭：'+change,async t=>{
 const f=await podSupplyFixture(t);if(change==='state'){await materializePodSupply(f.pod,f.objects,f.work);await writeFile(join(f.work,'cache/Pods/Release',f.pod.name,f.pod.version+'-aaaaa/generated.bin'),'state');}if(change==='sha')await writeFile(join(f.objects,hash(f.file)+'.blob'),'changed');if(change==='spec')f.pod.spec.url+='?other=1';if(change==='source')f.pod.source={git:'https://github.com/example/other.git',tag:'v1'};if(change==='escape')f.pod.files[1].target='../../../../outside';if(change==='duplicate')f.pod.files.push({...f.pod.files[0]});if(change==='cycle')f.pod.files[1].target='Current';const signal=change==='cancel'?AbortSignal.abort(Error('取消')):undefined;await assert.rejects(materializePodSupply(f.pod,f.objects,f.work,{signal}));assert.equal(await readFile(join(f.objects,hash(f.bytes)+'.blob'),'utf8'),f.bytes.toString());
});

// 真实文件事务验证下载候选的归属，不执行真实工具安装或编译。
test('资源下载候选只属于当前产品target现场，永久库不接收半包',async t=>{
 const root=await sandbox(t),work=join(root,'work'),store=join(root,'originals');await mkdir(work);await mkdir(store);
 const body=Buffer.from('owned-pending'),entry=archive(body);let inspected=false;
 const fetcher=async()=>({ok:true,headers:new Headers(),body:{async *[Symbol.asyncIterator](){
  const candidates=await readdir(join(work,'resource-pending'));
  assert.equal(candidates.filter(name=>name.endsWith('.pending')).length,1);
  assert.deepEqual(await readdir(store),[]);inspected=true;yield body;
 },cancel:async()=>{}}});
 const file=await acquireArchive(entry,{store,work,fetcher});assert.equal(inspected,true);
 assert.equal(await readFile(file,'utf8'),body.toString());assert.deepEqual(await readdir(join(work,'resource-pending')),[]);
 await assert.rejects(acquireArchive(archive(Buffer.from('other')),{store,work:dirname(resolve(import.meta.dirname,'..')),fetcher}),/target/);
});


// 夹具复制本仓完整资源实现，只替换文件IO边界并暴露已有私有验真函数，生产接口不新增出口。



// 全文复制本仓模块，合成回执逐次重算文件清单；只在测试副本暴露已有私有入口，不执行工具。


})();
}

// 本地来源接线覆盖原包正常物化、来源/版本漂移与越界；不访问网络。
if(inlineTestEntry){void(async()=>{const {test}=await import('node:test'),{default:assert}=await import('node:assert/strict'),fs=await import('node:fs');
 test('原始Node与OnChina锁中的scanner本地记录只消费所属源码',async()=>{for(const path of ['node/frontend/package-lock.json','onchina/frontend/package-lock.json']){const lock=join(root,path),doc=JSON.parse(fs.readFileSync(lock,'utf8'));assert.equal(await validateNpmLocal(lock,'node_modules/@gmb/scanner-react',doc.packages['node_modules/@gmb/scanner-react']),true);}});
 test('本地包拒绝版本漂移、未声明来源、越界及链接；远端仍走原摘要验真',async()=>{const work=fs.mkdtempSync(join(testRoot(),'npm-local-')),project=join(work,'project'),frontend=join(project,'frontend'),scanner=join(project,'scanner');try{fs.mkdirSync(frontend,{recursive:true});fs.mkdirSync(scanner);fs.writeFileSync(join(frontend,'package.json'),JSON.stringify({dependencies:{'@gmb/scanner-react':'file:../scanner'}}));fs.writeFileSync(join(scanner,'package.json'),JSON.stringify({name:'@gmb/scanner-react',version:'1.0.0'}));const lock=join(frontend,'package-lock.json'),entry={resolved:'file:../scanner',version:'1.0.0'};assert.equal(await validateNpmLocal(lock,'node_modules/@gmb/scanner-react',entry,project),true);assert.equal(await validateNpmLocal(lock,'node_modules/remote',{resolved:'https://registry.npmjs.org/remote/-/remote-1.0.0.tgz'},project),false);await assert.rejects(validateNpmLocal(lock,'node_modules/@gmb/scanner-react',{...entry,version:'2.0.0'},project),/版本/);await assert.rejects(validateNpmLocal(lock,'node_modules/@gmb/scanner-react',{...entry,resolved:'file:../../outside'},project),/越界/);fs.writeFileSync(join(frontend,'package.json'),'{}');await assert.rejects(validateNpmLocal(lock,'node_modules/@gmb/scanner-react',entry,project),/准确声明/);fs.writeFileSync(join(frontend,'package.json'),JSON.stringify({dependencies:{'@gmb/scanner-react':'file:../scanner'}}));fs.renameSync(scanner,scanner+'-actual');fs.symlinkSync(scanner+'-actual',scanner,'dir');await assert.rejects(validateNpmLocal(lock,'node_modules/@gmb/scanner-react',entry,project));}finally{fs.rmSync(work,{recursive:true,force:true});}});
})();}

// 使用真实Git数据与非heads引用验证已保存的bundle导入，不联网，不写正式仓库Git对象。
if(inlineTestEntry){void(async()=>{const {test}=await import('node:test'),{default:assert}=await import('node:assert/strict'),fs=await import('node:fs'),{spawnSync}=await import('node:child_process');test('固定提交bundle只有非heads引用时仍导入真实树并保持来源',async()=>{const work=fs.mkdtempSync(join(testRoot(),'git-bundle-')),git=process.env.PRODUCT_GIT_BIN;try{assert.ok(git&&isAbsolute(git));const env={...process.env,GIT_CONFIG_NOSYSTEM:'1',GIT_CONFIG_GLOBAL:'/dev/null',GIT_AUTHOR_NAME:'测试原件',GIT_AUTHOR_EMAIL:'fixture@example.invalid',GIT_COMMITTER_NAME:'测试原件',GIT_COMMITTER_EMAIL:'fixture@example.invalid'},call=(args,input)=>{const r=spawnSync(git,['-c','core.hooksPath=/dev/null',...args],{cwd:work,env,input,encoding:'utf8'});assert.equal(r.status,0,r.stderr);return r.stdout.trim();},bare=join(work,'source.git'),bundle=join(work,'source.bundle'),checkout=join(work,'checkout');call(['init','--bare','--quiet','--template=',bare]);const blob=call(['-C',bare,'hash-object','-w','--stdin'],'真实原件'),tree=call(['-C',bare,'mktree'], '100644 blob '+blob+'\tcontent.txt\n'),ref=call(['-C',bare,'commit-tree',tree],'固定测试原件\n');call(['-C',bare,'update-ref','refs/tata/fixture',ref]);call(['-C',bare,'bundle','create','--version=2',bundle,'refs/tata/fixture']);const source={url:'https://example.invalid/source.git',ref};await importLockedGitBundle(async args=>call(args),bundle,source,checkout);call(['-C',checkout,'checkout','--quiet','--detach',ref]);assert.equal(fs.readFileSync(join(checkout,'content.txt'),'utf8'),'真实原件');assert.equal(call(['-C',checkout,'remote','get-url','origin']),source.url);assert.equal(call(['-C',checkout,'rev-parse','HEAD']),ref);}finally{fs.rmSync(work,{recursive:true,force:true});}});})();}

// 上游闭包可以保留同名同版本的不同来源，SDK的相对依赖必须仍归其固定Git来源。
if(inlineTestEntry){void(async()=>{const {test}=await import('node:test'),{default:assert}=await import('node:assert/strict');test('同名同版本registry与Git原件不混合；相对Git依赖保留准确提交',()=>{const ref='a'.repeat(40),source='git+https://github.com/example/sdk.git?rev='+ref+'#'+ref,locked=[{name:'shared',version:'0.1.0',source:'registry+https://github.com/rust-lang/crates.io-index'},{name:'shared',version:'0.1.0',source}],doc={package:{name:'owner',version:'1.0.0'},dependencies:{shared:{path:'../shared'}}},result=normalizeCargoManifest(doc,{},locked,source);assert.deepEqual(result.dependencies.shared,{version:'=0.1.0',git:'https://github.com/example/sdk.git',rev:ref});assert.deepEqual(doc.dependencies.shared,{path:'../shared'});const development=normalizeCargoManifest({...doc,'dev-dependencies':{unlocked:{path:'../upstream-test-only'}}},{},locked,source);assert.deepEqual(development['dev-dependencies'].unlocked,{git:'https://github.com/example/sdk.git',rev:ref});const optional=normalizeCargoManifest({dependencies:{unused:{path:'../unused',optional:true}}},{},locked,source);assert.deepEqual(optional.dependencies.unused,{optional:true,git:'https://github.com/example/sdk.git',rev:ref});assert.throws(()=>normalizeCargoManifest(doc,{},locked,'git+https://github.com/example/missing.git?rev='+ref+'#'+ref),/唯一锁定版本/);});})();}

// 链接目标只来自真实原件闭包；工具临时发布视图不改上游源文件。
if(inlineTestEntry){void(async()=>{const {test}=await import('node:test'),{default:assert}=await import('node:assert/strict'),fs=await import('node:fs');test('Git发布视图复制闭包内文件和目录链接，拒绝外部目标与循环',async()=>{const work=fs.mkdtempSync(join(testRoot(),'git-tree-')),root=join(work,'input'),pkg=join(root,'package'),shared=join(root,'shared');try{fs.mkdirSync(pkg,{recursive:true});fs.mkdirSync(shared);fs.writeFileSync(join(shared,'value'),'闭包内原件');fs.symlinkSync('../shared/value',join(pkg,'value'));fs.symlinkSync('../shared',join(pkg,'docs'),'dir');await copyTree(pkg,join(work,'output'),root);assert.equal(fs.readFileSync(join(work,'output/value'),'utf8'),'闭包内原件');assert.equal(fs.readFileSync(join(work,'output/docs/value'),'utf8'),'闭包内原件');fs.writeFileSync(join(work,'outside'),'闭包外');fs.symlinkSync('../../outside',join(pkg,'outside'));await assert.rejects(copyTree(pkg,join(work,'outside-output'),root),/越界/);fs.unlinkSync(join(pkg,'outside'));fs.symlinkSync('.',join(pkg,'cycle'),'dir');await assert.rejects(copyTree(pkg,join(work,'cycle-output'),root),/循环/);}finally{fs.rmSync(work,{recursive:true,force:true});}});})();}

// macOS移动只读目录需要目录自身可写；仅提交期间调整候选根，恢复原权限且不覆盖已有对象。
if(inlineTestEntry){void(async()=>{const {test}=await import('node:test'),{default:assert}=await import('node:assert/strict'),fs=await import('node:fs');test('只读原件目录可原子提交，恢复权限并且不覆盖既有对象',async()=>{const work=fs.mkdtempSync(join(testRoot(),'readonly-commit-')),pending=join(work,'pending'),target=join(work,'object');try{fs.mkdirSync(pending);fs.writeFileSync(join(pending,'payload'),'真实原件');fs.chmodSync(pending,0o555);let checked=0;const verify=async p=>{assert.equal(fs.readFileSync(join(p,'payload'),'utf8'),'真实原件');checked++;};await commitCandidate(pending,target,{verify});assert.equal(fs.statSync(target).mode&0o777,0o555);assert.equal(checked,1);fs.mkdirSync(pending);fs.writeFileSync(join(pending,'payload'),'不得替换');await commitCandidate(pending,target,{verify});assert.equal(fs.readFileSync(join(target,'payload'),'utf8'),'真实原件');assert.equal(fs.readFileSync(join(pending,'payload'),'utf8'),'不得替换');}finally{if(fs.existsSync(target))fs.chmodSync(target,0o700);fs.rmSync(work,{recursive:true,force:true});}});})();}

// 同文件回归只准备夹具归档，验证候选职责和失败清理，不下载或运行产品编译。
if(inlineTestEntry){
 const {test}=await import('node:test'),{default:assert}=await import('node:assert/strict'),{execFileSync}=await import('node:child_process'),fs=await import('node:fs');

}

// 使用真实本轮目录验证解析器取得、复用及失败清场，不触碰共享工具库。
if(process.env.NODE_TEST_CONTEXT&&process.argv.length===2&&process.argv[1]===import.meta.filename){
 const {test}=await import('node:test'),{default:assert}=await import('node:assert/strict');
 const {mkdtemp,mkdir,writeFile,readdir,rm,lstat}=await import('node:fs/promises');
 const {gzipSync}=await import('node:zlib');
 const tar=entries=>{const blocks=[];for(const [name,body]of entries){const bytes=Buffer.from(body),header=Buffer.alloc(512);header.write(name,0,100);header.write('0000644\0',100);header.write('0000000\0',108);header.write('0000000\0',116);header.write(bytes.length.toString(8).padStart(11,'0')+'\0',124);header.write('00000000000\0',136);header.fill(32,148,156);header.write('0',156);header.write('ustar\0',257);header.write('00',263);header.write([...header].reduce((sum,value)=>sum+value,0).toString(8).padStart(6,'0')+'\0 ',148);blocks.push(header,bytes,Buffer.alloc((512-bytes.length%512)%512));}return gzipSync(Buffer.concat([...blocks,Buffer.alloc(1024)]));};


 test('锁定Git远端获取延长超时且其它Git命令保持原上限',()=>{
  assert.equal(gitCommandTimeout(),600000);
  assert.equal(gitCommandTimeout(false),600000);
  assert.equal(gitCommandTimeout(true),1800000);
  assert.throws(()=>gitCommandTimeout('remote'),/Git获取超时类型无效/);
 });
 test('锁定Git bundle仅在任务内检出，离线缺原件失败',async t=>{
  const {execFileSync}=await import('node:child_process'),{mkdtemp,mkdir,writeFile,readFile,readdir,rm}=await import('node:fs/promises');
  await mkdir(fixedWork('test'),{recursive:true});const work=await mkdtemp(join(fixedWork('test'),'git-task-'));t.after(()=>rm(work,{recursive:true,force:true}));
  const repository=join(work,'repository'),bundle=join(work,'source.bundle'),git='/usr/bin/git',run=(...args)=>execFileSync(git,args,{encoding:'utf8'}).trim();
  run('init','--quiet',repository);await writeFile(join(repository,'source.txt'),'locked');run('-C',repository,'add','source.txt');
  run('-C',repository,'-c','user.name=Fixture','-c','user.email=fixture@example.invalid','commit','--quiet','-m','fixture');
  const ref=run('-C',repository,'rev-parse','HEAD');run('-C',repository,'bundle','create',bundle,'HEAD');
  const bytes=await readFile(bundle),digest=hash(bytes),supply=join(work,'supply'),objects=join(supply,'objects');await mkdir(objects,{recursive:true});
  await writeFile(join(objects,digest+'.blob'),bytes);
  const source={url:'https://github.com/example/locked.git',ref},coordinate='git+'+source.url+'?rev='+ref+'#'+ref;
  await writeFile(join(supply,'index.json'),JSON.stringify({schema_version:2,packages:[],git_sources:[{source:coordinate,sha256:digest}],pods:[]}));
  const bin=join(work,'bin');await mkdir(bin);for(const name of posixRecipe.posixNames)await writeFile(join(bin,name),'#!/bin/sh\nexit 0\n',{mode:0o755});
  const tools=toolDefinitions.filter(tool=>['posix','bash','grep','sed'].includes(tool.id));
  const installed=new Map([['git',{path:git}],...tools.map(tool=>[tool.id,{path:join(bin,tool.id==='posix'?'bash':tool.command)}])]);
  const library={root:join(work,'library'),work,tools,installed};await mkdir(library.root);
  const target=join(work,'checkout'),options={library,optionalDependencies:objects,offline:true,environment:{HOME:work}};
  await gitCheckout(source,target,options);assert.equal(await readFile(join(target,'source.txt'),'utf8'),'locked');
  assert.equal(run('-C',target,'rev-parse','HEAD'),ref);assert.equal(await stat(join(work,'rely/git')),null);
  assert.equal((await readdir(join(work,'resource-pending/git-bundles'))).length,1);
  await rm(target,{recursive:true});await rm(join(work,'resource-pending/git-bundles'),{recursive:true});
  await assert.rejects(gitCheckout(source,target,{...options,optionalDependencies:undefined}),/离线缺少Git提交/);
  assert.equal(await stat(target),null);
 });


 test('独立Git首次持久入库、并发仅获取一次、任务清场后离线复用及提交错误不落位',async t=>{
  const {mkdtemp,mkdir,writeFile,readFile,rm,readdir}=await import('node:fs/promises');await mkdir(fixedWork('test'),{recursive:true});
  const base=await mkdtemp(join(fixedWork('test'),'git-independent-'));t.after(()=>rm(base,{recursive:true,force:true}));
  const work=join(base,'work'),store=join(base,'originals');await mkdir(work);await mkdir(store);
  const source={url:'https://github.com/example/fixed.git',ref:'a'.repeat(40)},options={library:{work,root:store},dependencyRoot:store};
  let gets=0;const run=async args=>{
   if(args.includes('fetch'))gets++;
   if(args.includes('rev-parse'))return {stdout:source.ref+'\n'};
   if(args.includes('bundle'))await writeFile(args[4],'fixture locked bundle');
   return {stdout:''};
  };
  const [a,b]=await Promise.all([gitOriginal(source,options,run),gitOriginal(source,options,run)]);
  assert.equal(a,b);assert.equal(gets,1);assert.equal(await readFile(a,'utf8'),'fixture locked bundle');
  await rm(join(work,'resource-pending'),{recursive:true});
  assert.equal(await gitOriginal(source,{...options,offline:true},()=>assert.fail('离线热命中不得联网')),a);
  const other={...source,ref:'b'.repeat(40)};
  await assert.rejects(gitOriginal(other,{...options,offline:true},run),/离线缺少/u);
  await assert.rejects(gitOriginal(other,options,run),/Git取得提交不符/u);
  assert.deepEqual((await readdir(join(store,'objects'))).filter(name=>name.endsWith('.blob')),[basename(a)]);
  assert.ok(!(await readdir(join(store,'objects'))).some(name=>name.endsWith('.lock')));
 });


}

// 旧资源文件的公开能力改由本入口同名命令提供，不保留旧文件。
async function runResourceCLI(command,args){
 if(command==='rsync')return flutterRecipe.copyFlutterArtifact(args);
 throw Error('资源命令无效');
}

if(inlineTestEntry){
 const {test}=await import('node:test'),{default:assert}=await import('node:assert/strict');
 const packageEntry=(version,dependencies={})=>({version,resolved:'https://registry.npmjs.org/example/-/example-'+version+'.tgz',integrity:'sha512-'+Buffer.from(version).toString('base64'),dependencies});
 const document=(dependencies,packages)=>({package:{dependencies},lock:{lockfileVersion:3,packages:{'':{dependencies},...packages}}});
 test('共享编辑器依赖按原锁嵌套差异版本，直接版本冲突和缺件失败',()=>{
  const a=document({a:'1'}, {'node_modules/a':packageEntry('1',{x:'1'}),'node_modules/x':packageEntry('1')});
  const b=document({b:'1'}, {'node_modules/b':packageEntry('1',{x:'2'}),'node_modules/x':packageEntry('2')});
  const plan=editorDependencyPlan([a,b]),paths=new Map(plan.map(x=>[x.path,x.entry.version]));
  assert.equal(paths.get('node_modules/a/node_modules/x'),'1');assert.equal(paths.get('node_modules/b/node_modules/x'),'2');
  assert.throws(()=>editorDependencyPlan([a,document({a:'2'},{'node_modules/a':packageEntry('2')})]),/直接依赖版本冲突/);
  assert.throws(()=>editorDependencyPlan([document({a:'1'},{'node_modules/a':packageEntry('1',{missing:'1'})})]),/内部依赖缺少原锁/);
  assert.throws(()=>editorDependencyPlan([document({'../bad':'1'},{})]),/包名无效/);
 });
 test('真实三锁编辑器闭包保持全部直接包和固定共享路径',async()=>{
  const documents=[];
  for(const relative of ['node/frontend','onchina/frontend','crates/scanner'])documents.push({package:JSON.parse(await readFile(join(root,relative,'package.json'),'utf8')),lock:JSON.parse(await readFile(join(root,relative,'package-lock.json'),'utf8'))});
  const plan=editorDependencyPlan(documents),paths=new Set(plan.map(x=>x.path));assert.equal(paths.size,plan.length);
  for(const name of ['@types/node','@types/react','@types/react-dom','typescript','vite','@vitejs/plugin-react','antd','jsqr','vitest'])assert.ok(paths.has('node_modules/'+name),name);
  assert.ok(plan.some(x=>x.path.includes('/node_modules/vite')&&x.entry.version==='7.3.6'));
  for(const relative of ['node/frontend','onchina/frontend','crates/scanner']){
   const config=JSON.parse(await readFile(join(root,relative,'tsconfig.json'),'utf8'));
   assert.ok(config.compilerOptions.typeRoots.includes('../../target/node_modules/@types'));
   assert.doesNotMatch(JSON.stringify(config),/target\/test\/editor/);
  }
 });
}
return {runResourceProcess,inventory,acquireArchive,extractArchive,validateNpmLocal,normalizeCargoManifest,podSourceCoordinate,readDependencySupply,materializePodSupply,materializeMavenCache,mavenSupplyInit,checkCocoaPodsResources,buildSourceTool,posixNames,resourceDeclarations,bootstrapNode,editorResources,resources,prepareResourceSupply,supplyRequirements,assertWorkQuiescent,runResourceCLI};
})();
export const {runResourceProcess,inventory,acquireArchive,extractArchive,validateNpmLocal,normalizeCargoManifest,podSourceCoordinate,readDependencySupply,materializePodSupply,materializeMavenCache,mavenSupplyInit,checkCocoaPodsResources,buildSourceTool,posixNames,resourceDeclarations,bootstrapNode,editorResources,resources,prepareResourceSupply,supplyRequirements,assertWorkQuiescent}=resourceInternals;

import {mkdtempSync as createEmbeddedDirectory} from 'node:fs';
export const BUILD_SOURCES=Object.freeze({"run":"#!/usr/bin/env bash\n# CitizenChain节点本机Build入口：只构建、签名验真并写入产品产物目录。\n# 本入口不启动或停止节点，不修改链数据；编译成功后只安装已验签的App。\n# 测试、手工调试和清库不走本入口。\nset -euo pipefail\n\nMACOS_APP_BUNDLE=''\nMACOS_APP_PENDING=0\n\ncleanup() {\n    local result=\"${1:-1}\"\n    trap - EXIT INT TERM HUP\n    # 唯一成功路径会撤销 trap；其余提前退出必须失败，避免 Bash 参数展开错误被清理返回值吞掉。\n    [[ \"$result\" != 0 ]] || result=1\n    # 只清理本轮尚未通过完整签名验收的固定 App；历史成功归档和用户数据均不触碰。\n    if [[ \"${MACOS_APP_PENDING:-0}\" == 1 \\\n        && -n \"${TARGET_DIR:-}\" \\\n        && \"${MACOS_APP_BUNDLE:-}\" == \"$TARGET_DIR/release/bundle/macos/citizenchain.app\" ]]; then\n        rm -rf -- \"$MACOS_APP_BUNDLE\"\n    fi\n    # Build 不启动节点或 PG；校验失败不得停止其它任务或用户已运行的实例。\n    exit \"$result\"\n}\ntrap 'cleanup \"$?\"' EXIT\ntrap 'cleanup 130' INT\ntrap 'cleanup 143' TERM\ntrap 'cleanup 129' HUP\n\n# macOS 产品 App 只接受今后唯一的新团队 Developer ID；禁止按枚举顺序选证书，\n# 否则Apple Development或其它团队身份可能被静默当成产品正式签名。\nMACOS_SIGNING_IDENTITY='Developer ID Application: WEI CHENG (MHYMVRN6FC)'\nMACOS_TEAM_ID='MHYMVRN6FC'\nMACOS_BUNDLE_ID='macOS.citizenappchain'\nMACOS_APPLICATION_ID=\"${MACOS_TEAM_ID}.${MACOS_BUNDLE_ID}\"\nMACOS_PROFILE_NAME='CitizenChain Developer ID'\nMACOS_PROFILE_DIR=\"$HOME/Library/Developer/Xcode/UserData/Provisioning Profiles\"\n\nrequire_macos_signing_identity() {\n    /usr/bin/security find-identity -v -p codesigning 2>/dev/null \\\n        | grep -F \"\\\"$MACOS_SIGNING_IDENTITY\\\"\" >/dev/null\n}\n\n# Apple 安全时间戳服务偶发不可用时只重试签名相关步骤。错误不属于该服务、\n# 或三次重试仍失败时立即关闭，不允许改用无时间戳、临时签名等降级路径。\nMACOS_TIMESTAMP_RETRY_DELAYS=(2 5 10)\n\nis_macos_timestamp_service_failure() {\n    local output=\"$1\"\n    [[ \"$output\" == *\"A timestamp was expected but was not found\"* \\\n        || \"$output\" == *\"The timestamp service is not available\"* \\\n        || \"$output\" == *\"timestamp service is not available\"* ]]\n}\n\nrun_with_macos_timestamp_retry() {\n    local action=\"$1\"\n    shift\n    local output='' status=0 retry=0 delay\n    while true; do\n        set +e\n        output=\"$(\"$@\" 2>&1)\"\n        status=$?\n        set -e\n        [[ -z \"$output\" ]] || printf '%s\\n' \"$output\"\n        [[ \"$status\" == 0 ]] && return 0\n        is_macos_timestamp_service_failure \"$output\" || return \"$status\"\n        if (( retry >= ${#MACOS_TIMESTAMP_RETRY_DELAYS[@]} )); then\n            echo \"    [error] $action 因 Apple 安全时间戳服务异常连续重试三次后仍失败\" >&2\n            return \"$status\"\n        fi\n        delay=\"${MACOS_TIMESTAMP_RETRY_DELAYS[$retry]}\"\n        retry=$((retry + 1))\n        echo \"    [warn] $action 未取得 Apple 安全时间戳，${delay} 秒后执行第 $retry 次重试\" >&2\n        sleep \"$delay\"\n    done\n}\n\n# Apple 重新签发描述文件会改变 UUID，因此禁止绑定文件名。只允许唯一一个同时匹配\n# 产品名、Team ID 和公民链 App ID 的 Developer ID 描述文件，重复或旧标识均失败关闭。\nresolve_macos_profile() {\n    local candidate profile_name application_id team_id matched='' matched_count=0\n    shopt -s nullglob\n    for candidate in \"$MACOS_PROFILE_DIR\"/*.provisionprofile \"$MACOS_PROFILE_DIR\"/*.mobileprovision; do\n        profile_name=\"$(security cms -D -i \"$candidate\" 2>/dev/null \\\n            | plutil -extract Name raw -o - - 2>/dev/null || true)\"\n        application_id=\"$(security cms -D -i \"$candidate\" 2>/dev/null \\\n            | plutil -extract 'Entitlements.com\\.apple\\.application-identifier' raw -o - - 2>/dev/null || true)\"\n        team_id=\"$(security cms -D -i \"$candidate\" 2>/dev/null \\\n            | plutil -extract 'Entitlements.com\\.apple\\.developer\\.team-identifier' raw -o - - 2>/dev/null || true)\"\n        if [[ \"$profile_name\" == \"$MACOS_PROFILE_NAME\" \\\n            && \"$application_id\" == \"$MACOS_APPLICATION_ID\" \\\n            && \"$team_id\" == \"$MACOS_TEAM_ID\" ]]; then\n            matched=\"$candidate\"\n            matched_count=$((matched_count + 1))\n        fi\n    done\n    shopt -u nullglob\n    [[ \"$matched_count\" == 1 ]] || {\n        echo \"    [error] 必须且只能安装一个匹配 $MACOS_BUNDLE_ID 的 $MACOS_PROFILE_NAME\" >&2\n        return 1\n    }\n    printf '%s\\n' \"$matched\"\n}\n\nSCRIPT_DIR=\"$CITIZENCHAIN_SOURCE_ROOT/scripts\"\nREPO_ROOT=\"$CITIZENCHAIN_SOURCE_ROOT\"\nCITIZENCHAIN_ROOT=\"$CITIZENCHAIN_SOURCE_ROOT\"\n# 所有独立入口的工具临时状态归本产品target；宿主已交付的产品工作根继续归当前任务。\nPRODUCT_TEMP_SOURCE=\"$CITIZENCHAIN_SOURCE_ROOT\"\nPRODUCT_TARGET_TEMP_ROOT=\"${CITIZENCHAIN_WORK_DIR:-$CITIZENCHAIN_SOURCE_ROOT/target/build/macos}\"\nCITIZENCHAIN_WORK_DIR=\"${CITIZENCHAIN_WORK_DIR:-$PRODUCT_TARGET_TEMP_ROOT}\"\nBUILD_WORK_DIR=\"${CITIZENCHAIN_BUILD_WORK_DIR:-$CITIZENCHAIN_WORK_DIR/work}\"\nCITIZENCHAIN_DEPENDENCY_DIR=\"${CITIZENCHAIN_DEPENDENCY_DIR:-$CITIZENCHAIN_WORK_DIR/dependencies}\"\nTARGET_DIR=\"$BUILD_WORK_DIR/cargo-target\"\nexport CARGO_TARGET_DIR=\"$TARGET_DIR\"\nNODE_FRONTEND_DIST=\"$CITIZENCHAIN_WORK_DIR/node-frontend\"\nONCHINA_BUILD_DIST=\"$CITIZENCHAIN_WORK_DIR/onchina-frontend/dist\"\nPACKAGE_RESOURCES=\"$CITIZENCHAIN_WORK_DIR/resources\"\n# 本段只形成当前调用的候选App；控制台本机任务验收后安装，正式Release与Publish另行验真。\nARTIFACT_DIR=\"${CITIZENCHAIN_ARTIFACT_DIR:-$CITIZENCHAIN_WORK_DIR/artifacts}\"\n\n# 产品自行使用当前环境中的工具，并按自己的锁文件在源码外工作目录准备依赖。\nsource \"$PRODUCT_PREPARE_SHELL\"\n\n# 节点App只编译节点与配套平台，不生成正式Runtime WASM；既有链从链上读取代码。\nunset WASM_FILE\n# Cargo/Tauri 的 release profile 只是本机优化配置；gmb.dev 是本机开发数据隔离环境。\n# 本任务不修改正式gmb数据，也不与正式安装版争用RocksDB。\nexport CITIZENCHAIN_DATA_PROFILE=dev\nmkdir -p \"$TARGET_DIR\" \"$npm_config_cache\" \"$PACKAGE_RESOURCES/onchina-bin\" \"$PACKAGE_RESOURCES/onchina-frontend\"\n\n# ── OnChina 控制台本机配置 ──\n# 启动节点不需要任何机构鉴权/身份。这里只让本机能跑起链上中国平台服务:\n#   ① 构建 onchina 二进制(节点同目录,设置页手动启动时由 onchina_proc 拉起)+ 前端产物;\n#   ② DB 用内嵌私有 PG(方案 A):借本机 PostgreSQL 二进制起一个 onchina 专属实例(127.0.0.1)。\n# 本机构的\"系统签名钥 / 机构身份\"是可选配置(签登录 QR / 签发凭证才需要),非启动前提。\necho \"==> 构建 OnChina 本机优化二进制 + 前端...\"\n( cd \"$REPO_ROOT\" && CARGO_INCREMENTAL=1 cargo build --locked --release -p onchina --config \"$REPO_ROOT/config.toml\" )\necho \"==> 构建链上中国平台前端产物...\"\n\"${NODE:?}\" \"$REPO_ROOT/scripts/build.mjs\" frontend-build macos onchina\necho \"==> 构建节点前端产物...\"\n\"${NODE:?}\" \"$REPO_ROOT/scripts/build.mjs\" frontend-build macos node\ncp \"$TARGET_DIR/release/onchina\" \"$PACKAGE_RESOURCES/onchina-bin/onchina\"\ncp -R \"$ONCHINA_BUILD_DIST\" \"$PACKAGE_RESOURCES/onchina-frontend/dist\"\nPG_PREFIX=\"${CITIZENCHAIN_PG_DIST:-}\"\nif [ -z \"$PG_PREFIX\" ]; then\n    for v in postgresql@17 postgresql@16 postgresql@15 postgresql; do\n        if p=\"$(brew --prefix \"$v\" 2>/dev/null)\" && [ -x \"$p/bin/initdb\" ]; then PG_PREFIX=\"$p\"; break; fi\n    done\nfi\n[[ -z \"$PG_PREFIX\" || -x \"$PG_PREFIX/bin/initdb\" ]] || { echo 'PostgreSQL原件路径无效' >&2; exit 1; }\nif [ -n \"$PG_PREFIX\" ]; then\n    mkdir -p \"$PACKAGE_RESOURCES/postgres/macos\"\n    cp -R \"$PG_PREFIX/.\" \"$PACKAGE_RESOURCES/postgres/macos/\"\n    [[ -x \"$PACKAGE_RESOURCES/postgres/macos/bin/initdb\" ]] || { echo 'macOS节点包缺少PostgreSQL initdb' >&2; exit 1; }\n    export ONCHINA_EMBEDDED_PG=1\n    export ONCHINA_PG_BIN_DIR=\"$PG_PREFIX/bin\"\n    export ONCHINA_PG_PORT=\"${ONCHINA_PG_PORT:-5433}\"\n    export ONCHINA_PG_DATA_DIR=\"$HOME/Library/Application Support/gmb.dev/onchina-pgdata\"\n    echo \"    内嵌私有 PG:$ONCHINA_PG_BIN_DIR(端口 $ONCHINA_PG_PORT)\"\nelse\n    [[ \"${GITHUB_ACTIONS:-false}\" != true ]] || { echo '正式macOS节点包缺少PostgreSQL' >&2; exit 1; }\n    echo \"    [warn] 未找到本机 PostgreSQL(brew install postgresql@16);链上中国平台仍可起但缺 DB,功能受限。\"\nfi\nexport ONCHINA_CHINA_DB=\"$REPO_ROOT/onchina/src/codes/china.sqlite\"\nexport ONCHINA_FRONTEND_DIST=\"$ONCHINA_BUILD_DIST\"\nexport ONCHINA_ENABLE_TLS=1\nexport ONCHINA_TLS_DIR=\"$HOME/Library/Application Support/gmb.dev/onchina-tls\"\n# 公权机构目录只允许从链上投影到本地缓存;开发启动不再打开旧本地生成开关。\n# 链不可达或投影不可读时,链上中国按 fail-closed 不放行平台服务。\n# OnChina 后端不再持有任何链上签名钥:机构操作全部由管理员冷钱包直接冷签,\n# 原平台签名钥与注销凭证签发配置已随注销凭证链路整体删除。\n\necho \"==> 使用既有链上Runtime单独构建本机节点...\"\necho \"    节点Build产物目录: $TARGET_DIR\"\necho \"    本机运行数据目录: $HOME/Library/Application Support/gmb.dev\"\necho \"==> 链上中国平台:节点设置页点击「启动」后访问 https://onchina.local:8964\"\n\n# ── 构建与签名封装 ──\ncd \"$REPO_ROOT/node\"\necho \"==> 构建公民链...\"\nif [[ \"$(uname -s)\" == \"Darwin\" ]]; then\n    require_macos_signing_identity || {\n        echo \"    [error] 未找到唯一允许的新团队 Developer ID：$MACOS_SIGNING_IDENTITY\" >&2\n        exit 1\n    }\n    export APPLE_SIGNING_IDENTITY=\"$MACOS_SIGNING_IDENTITY\"\n    macos_profile=\"$(resolve_macos_profile)\"\n    echo \"    使用新团队 Developer ID 构建带摄像头权限的本机优化 App\"\n    # 使用当前任务按原始 lockfile 安装的 CLI，不能读取源码 node_modules 或全局 CLI。\n    app_bundle=\"$TARGET_DIR/release/bundle/macos/citizenchain.app\"\n    MACOS_APP_BUNDLE=\"$app_bundle\"\n    # Tauri 2 的 build 默认使用优化 profile；--debug 才会切换为调试产物。\n    # 编译与封装分离，时间戳瞬时失败时只重试封装签名，不重复整轮 Rust 编译。\n    # 前端已在私有工程完成构建；清除 Tauri 的源码 npm 钩子，避免重复构建或回写主仓。\n    tauri_override=\"$(\"${PYTHON:?}\" -c 'import json,sys,os; print(json.dumps({\"build\":{\"beforeBuildCommand\":None,\"frontendDist\":sys.argv[1]},\"bundle\":{\"createUpdaterArtifacts\":bool(os.environ.get(\"TAURI_SIGNING_PRIVATE_KEY\")),\"resources\":{sys.argv[2]+\"/\":\"\",sys.argv[3]+\"/\":\"icons/\",sys.argv[4]:\"china.sqlite\"}}}))' \"$NODE_FRONTEND_DIST\" \"$PACKAGE_RESOURCES\" \"$REPO_ROOT/icons\" \"$REPO_ROOT/onchina/src/codes/china.sqlite\")\"\n    CITIZENCHAIN_FRONTEND_DIST=\"$NODE_FRONTEND_DIST\" CARGO_INCREMENTAL=1 \\\n        node \"$NODE_FRONTEND_PROJECT/node_modules/@tauri-apps/cli/tauri.js\" build --config \"$tauri_override\" \\\n        --no-bundle --ci -- --locked --config \"$REPO_ROOT/config.toml\"\n    MACOS_APP_PENDING=1\n    bundle_macos_app() {\n        rm -rf -- \"$app_bundle\"\n        CITIZENCHAIN_FRONTEND_DIST=\"$NODE_FRONTEND_DIST\" CARGO_INCREMENTAL=1 \\\n            node \"$NODE_FRONTEND_PROJECT/node_modules/@tauri-apps/cli/tauri.js\" bundle --config \"$tauri_override\" \\\n            --bundles app --ci\n    }\n    run_with_macos_timestamp_retry \"Tauri App 封装签名\" bundle_macos_app\n\n    app_plist=\"$app_bundle/Contents/Info.plist\"\n    app_executable=\"$app_bundle/Contents/MacOS/citizenchain\"\n    [[ -x \"$app_executable\" ]] || {\n        echo \"    [error] Tauri 构建完成但缺少 App 主程序：$app_executable\" >&2\n        exit 1\n    }\n    # Tauri 负责生成产品 App；描述文件属于本机签名材料，绝不进入仓库或其它产物路径。\n    # 嵌入后必须重新封签根 App，使描述文件、唯一 App ID 与权限成为同一个签名整体。\n    /usr/bin/ditto \"$macos_profile\" \"$app_bundle/Contents/embedded.provisionprofile\"\n    run_with_macos_timestamp_retry \"描述文件嵌入后的 App 封签\" \\\n        /usr/bin/codesign --force --deep --options runtime --timestamp \\\n        --entitlements \"$REPO_ROOT/node/Entitlements.plist\" \\\n        --sign \"$MACOS_SIGNING_IDENTITY\" \"$app_bundle\"\n    codesign --verify --deep --strict \"$app_bundle\"\n    signature_details=\"$(codesign -dv --verbose=4 \"$app_bundle\" 2>&1)\"\n    grep -Fqx \"Authority=$MACOS_SIGNING_IDENTITY\" <<<\"$signature_details\" || {\n        echo \"    [error] macOS App 未使用唯一允许的新团队 Developer ID\" >&2\n        exit 1\n    }\n    grep -Fqx \"TeamIdentifier=$MACOS_TEAM_ID\" <<<\"$signature_details\" || {\n        echo \"    [error] macOS App Team ID 不属于唯一允许的新团队\" >&2\n        exit 1\n    }\n    grep -Eq '^CodeDirectory .*flags=.*\\(runtime\\)' <<<\"$signature_details\" || {\n        echo \"    [error] macOS App 未启用 Hardened Runtime\" >&2\n        exit 1\n    }\n    grep -Eq '^Timestamp=' <<<\"$signature_details\" || {\n        echo \"    [error] macOS App 缺少安全时间戳\" >&2\n        exit 1\n    }\n    [[ \"$(/usr/libexec/PlistBuddy -c 'Print :CFBundleIdentifier' \"$app_plist\")\" == \"$MACOS_BUNDLE_ID\" ]] || {\n        echo \"    [error] macOS App Bundle ID 与 Tauri 配置不一致\" >&2\n        exit 1\n    }\n    embedded_profile_name=\"$(security cms -D -i \"$app_bundle/Contents/embedded.provisionprofile\" 2>/dev/null \\\n        | plutil -extract Name raw -o - - 2>/dev/null || true)\"\n    [[ \"$embedded_profile_name\" == \"$MACOS_PROFILE_NAME\" ]] || {\n        echo \"    [error] macOS App 未嵌入 $MACOS_PROFILE_NAME\" >&2\n        exit 1\n    }\n    /usr/libexec/PlistBuddy -c 'Print :NSCameraUsageDescription' \"$app_plist\" >/dev/null\n    signed_entitlements=\"$(codesign -d --entitlements :- \"$app_bundle\" 2>/dev/null)\"\n    for entitlement in \\\n        com.apple.security.device.camera \\\n        com.apple.security.cs.allow-jit \\\n        com.apple.security.cs.allow-unsigned-executable-memory; do\n        # plutil 把点号解释为字典层级；entitlement 名本身含点号，必须先转义为单个键。\n        entitlement_key_path=\"${entitlement//./\\\\.}\"\n        [[ \"$(printf '%s' \"$signed_entitlements\" | plutil -extract \"$entitlement_key_path\" raw -)\" == \"true\" ]] || {\n            echo \"    [error] macOS App 签名缺少 $entitlement\" >&2\n            exit 1\n        }\n    done\n    get_task_allow=\"$(printf '%s' \"$signed_entitlements\" \\\n        | plutil -extract 'com\\.apple\\.security\\.get-task-allow' raw - 2>/dev/null \\\n        || printf 'false')\"\n    [[ \"$get_task_allow\" == \"false\" ]] || {\n        echo \"    [error] macOS 本机优化 App 禁止 get-task-allow\" >&2\n        exit 1\n    }\n    MACOS_APP_PENDING=0\n    echo \"    本机优化路径、新团队签名、Bundle ID、Hardened Runtime、安全时间戳与 entitlement 校验通过\"\n    # 成功后只保存本次缓存根的候选 App；不得触碰正式产物库。\n    mkdir -p \"$ARTIFACT_DIR\"\n    rm -rf \"$ARTIFACT_DIR/CitizenChain.app\"\n    mv \"$app_bundle\" \"$ARTIFACT_DIR/CitizenChain.app\"\n    app_bundle=\"$ARTIFACT_DIR/CitizenChain.app\"\n    app_executable=\"$app_bundle/Contents/MacOS/citizenchain\"\n    export ONCHINA_FRONTEND_DIST=\"$app_bundle/Contents/Resources/onchina-frontend/dist\"\n    # 本段只生成并验真产品候选；后续安装事务不终止旧实例，也不启动新实例。\n    trap - EXIT INT TERM HUP\n    echo \"    CitizenChain Node macOS候选产物构建完成；Build不会启动节点\"\nelse\n    echo \"    [error] 本入口只负责macOS Build；其它平台使用各自产品构建入口\" >&2\n    exit 1\nfi\n","local":"#!/usr/bin/env bash\n# 保留原非macOS检查与编译矩阵；输出只进入本轮build或test。\nset -euo pipefail\n[[ $# -eq 2 && \"$2\" == /* ]] || { echo \"构建参数无效\" >&2; exit 2; }\ncase \"$1\" in windows|linux-arm|linux-amd) ;; *) exit 2;; esac\nroot=\"${CITIZENCHAIN_SOURCE_ROOT:?}\"; work=\"$2\"\n[[ \"$work\" == \"$root/target/build/\"* || \"$work\" == \"$root/target/test/\"* ]] || { echo \"工作根越界\" >&2; exit 1; }\n[[ -x \"${CARGO:?}\" && -x \"${RUSTC:?}\" ]] || exit 1\n# 非macOS本机检查同样要求本产品锁定的宿主protoc。\nif [[ ! -x \"${PROTOC:?}\" || \"$(\"$PROTOC\" --version)\" != 'libprotoc 35.0' ]]; then\n  echo 'CitizenChain protoc 35.0验真失败' >&2; exit 1\nfi\nexport CARGO_TARGET_DIR=\"$work/cargo-target\"\nunset WASM_FILE\n\"$CARGO\" check --manifest-path \"$root/Cargo.toml\" --locked --release -p node --no-default-features --features std --all-targets\n\"$CARGO\" test --manifest-path \"$root/Cargo.toml\" --locked --release -p node --no-default-features --features std --no-run\n","prepack":"#!/usr/bin/env bash\n# Card 05 打包前置(macOS / Linux):把 onchina 二进制 + 前端产物 + china.sqlite +\n# PostgreSQL官方二进制组装到产品源码外资源目录。冻结plain chainspec已由\n# node 二进制 include_bytes! 内嵌；安装包不复制 258 MB 创世 RocksDB，首启按同一\n# chainspec 本地物化并由节点守卫校验块 0。之后在 node/ 跑 `npm run tauri build`。\n#\n# 用法:\n#   export CITIZENCHAIN_PG_DIST=<postgresql.org 官方二进制解压目录(含 bin/lib/share)>\n#   node citizenchain/scripts/build.mjs prepack\nset -euo pipefail\n\nSCRIPT_DIR=\"$CITIZENCHAIN_SOURCE_ROOT/scripts\"\nROOT=\"$CITIZENCHAIN_SOURCE_ROOT\"\nHERE=\"$ROOT/node\"                             # citizenchain/node\n# 所有独立入口的工具临时状态归本产品target；宿主已交付的产品工作根继续归当前任务。\nPRODUCT_TEMP_SOURCE=\"$CITIZENCHAIN_SOURCE_ROOT\"\nPRODUCT_TARGET_TEMP_ROOT=\"${CITIZENCHAIN_WORK_DIR:-$CITIZENCHAIN_SOURCE_ROOT/target/build/windows}\"\nCITIZENCHAIN_WORK_DIR=\"${CITIZENCHAIN_PREPACK_WORK_DIR:-$PRODUCT_TARGET_TEMP_ROOT}\"\nCITIZENCHAIN_DEPENDENCY_DIR=\"${CITIZENCHAIN_DEPENDENCY_DIR:-$CITIZENCHAIN_WORK_DIR/dependencies}\"\nexport CITIZENCHAIN_WORK_DIR CITIZENCHAIN_DEPENDENCY_DIR\nexport CARGO_TARGET_DIR=\"${CARGO_TARGET_DIR:-$CITIZENCHAIN_WORK_DIR/cargo-target}\"\nPACKAGE_RESOURCES=\"${CITIZENCHAIN_PACKAGE_RESOURCES_DIR:-$CITIZENCHAIN_WORK_DIR/resources}\"\nONCHINA_BUILD_DIST=\"$CITIZENCHAIN_WORK_DIR/onchina-frontend/dist\"\nsource \"$PRODUCT_PREPARE_SHELL\"\ncase \"$(uname -s)\" in\n  Darwin) OS=macos ;;\n  Linux) OS=linux ;;\n  *) OS=linux ;;\nesac\n\necho \"[prepack] build onchina (release)\"\n( cd \"$ROOT\" && cargo build -p onchina --release --config \"$ROOT/config.toml\" )\n\necho \"[prepack] build onchina frontend\"\n( cd \"$ONCHINA_FRONTEND_PROJECT\" && ONCHINA_FRONTEND_DIST=\"$ONCHINA_BUILD_DIST\" npm run build )\n\necho \"[prepack] assemble node/resources\"\nmkdir -p \"$PACKAGE_RESOURCES/onchina-bin\" \"$PACKAGE_RESOURCES/onchina-frontend\" \"$PACKAGE_RESOURCES/postgres\"\n# onchina 二进制随包(Tauri resources/onchina-bin),onchina_proc 从资源目录解析(见 node/src/onchina_proc)。\ncp \"$CARGO_TARGET_DIR/release/onchina\" \"$PACKAGE_RESOURCES/onchina-bin/onchina\"\nchmod +x \"$PACKAGE_RESOURCES/onchina-bin/onchina\"\nrm -rf \"$PACKAGE_RESOURCES/onchina-frontend/dist\"\ncp -R \"$ONCHINA_BUILD_DIST\" \"$PACKAGE_RESOURCES/onchina-frontend/dist\"\n\n# PostgreSQL 官方二进制(postgresql.org):把已解压的 PG 安装目录(含 bin/lib/share)\n# 指向 CITIZENCHAIN_PG_DIST,脚本拷进 resources/postgres/$OS;未提供则告警(安装包将缺内嵌 PG)。\nif [ -n \"${CITIZENCHAIN_PG_DIST:-}\" ] && [ -d \"$CITIZENCHAIN_PG_DIST/bin\" ]; then\n  rm -rf \"$PACKAGE_RESOURCES/postgres/$OS\"\n  mkdir -p \"$PACKAGE_RESOURCES/postgres/$OS\"\n  cp -R \"$CITIZENCHAIN_PG_DIST/.\" \"$PACKAGE_RESOURCES/postgres/$OS/\"\n  echo \"[prepack] PostgreSQL 已组装($OS)\"\nelse\n  echo \"[prepack][warn] 未提供 CITIZENCHAIN_PG_DIST。\"\n  echo \"                请从 https://www.postgresql.org/download/ 取本平台官方二进制(含 bin/lib/share),\"\n  echo \"                解压后 export CITIZENCHAIN_PG_DIST=<解压目录> 再重跑;否则安装包不含内嵌 PG。\"\nfi\n\n# 中文注释：release 状态包只作为正式创世审计制品保留在 target/build/chainspec，不进入任一\n# 平台安装包；清掉旧预打包残留，保证本机 prepack 与 GitHub CI 使用同一轻量合同。\nrm -rf \"$PACKAGE_RESOURCES/genesis-state\"\necho \"[prepack] 已确认安装包不携带 genesis-state；首启按冻结 plain chainspec 本地物化\"\n\necho \"[prepack] 完成：资源目录 $PACKAGE_RESOURCES\"\n","prepare":"#!/usr/bin/env bash\n# 公民链产品工具准备：使用调用环境中的标准工具和产品锁文件。\n# 必须由 build.mjs run source，使产品路径和包管理器设置留在当前进程。\nset -euo pipefail\n\nPREPARE_SCRIPT_DIR=\"$CITIZENCHAIN_SOURCE_ROOT/scripts\"\nCITIZENCHAIN_ROOT=\"$CITIZENCHAIN_SOURCE_ROOT\"\nNODE_FRONTEND_SOURCE=\"$CITIZENCHAIN_ROOT/node/frontend\"\nONCHINA_FRONTEND_SOURCE=\"$CITIZENCHAIN_ROOT/onchina/frontend\"\n\ncase \"$(uname -s)/$(uname -m)\" in\n  Darwin/arm64) CITIZENCHAIN_WORK_PLATFORM=macos ;;\n  Linux/aarch64) CITIZENCHAIN_WORK_PLATFORM=linux-arm ;;\n  Linux/x86_64) CITIZENCHAIN_WORK_PLATFORM=linux-amd ;;\n  *) echo 'CitizenChain当前宿主未声明' >&2; exit 1 ;;\nesac\nCITIZENCHAIN_WORK_DIR=\"${CITIZENCHAIN_WORK_DIR:-$CITIZENCHAIN_ROOT/target/build}\"\nCITIZENCHAIN_DEPENDENCY_DIR=\"${CITIZENCHAIN_DEPENDENCY_DIR:-$CITIZENCHAIN_WORK_DIR/dependencies}\"\n\"${PYTHON:?}\" - \"$CITIZENCHAIN_ROOT\" \"$CITIZENCHAIN_WORK_DIR\" \"$CITIZENCHAIN_DEPENDENCY_DIR\" \"${CARGO_TARGET_DIR:-$CITIZENCHAIN_WORK_DIR/cargo-target}\" <<'CHECK_WORK'\nfrom pathlib import Path\nimport sys\nsource = Path(sys.argv[1]).resolve()\nfor value in sys.argv[2:]:\n    raw = Path(value)\n    target = raw.resolve()\n    if not raw.is_absolute() or source / 'target' not in target.parents:\n        raise SystemExit(f'CitizenChain可写目录必须是本产品target内绝对路径：{value}')\nCHECK_WORK\nexport npm_config_cache=\"${npm_config_cache:-$CITIZENCHAIN_DEPENDENCY_DIR/npm}\"\nmkdir -p \"$npm_config_cache\" \"$CITIZENCHAIN_WORK_DIR/source\"\nexport npm_config_audit=false npm_config_fund=false\n\n# 只消费本产品资源回执交付的官方protoc 35.0。\nif [[ ! -x \"${PROTOC:?}\" || \"$(\"$PROTOC\" --version)\" != 'libprotoc 35.0' ]]; then\n  echo 'CitizenChain protoc 35.0验真失败' >&2; exit 1\nfi\nexport PROTOC\n\nprepare_node_project() {\n  local source=\"$1\" relative=\"$2\" destination\n  destination=\"$CITIZENCHAIN_WORK_DIR/source/$relative\"\n  [[ -f \"$source/package.json\" && -f \"$source/package-lock.json\" ]] \\\n    || { echo \"公民链Node工程缺少锁文件：$source\" >&2; return 1; }\n  # 每次只重建当前产品工作根中的准确工程副本，保留包管理器缓存和其它任务目录。\n  rm -rf -- \"$destination\"\n  node - \"$source\" \"$destination\" <<'COPY_PROJECT'\nconst fs = require('node:fs');\nconst path = require('node:path');\nconst [source, destination] = process.argv.slice(2);\nfs.cpSync(source, destination, {\n  recursive: true,\n  errorOnExist: true,\n  force: false,\n  filter: (entry) => !['node_modules', 'dist'].includes(path.basename(entry)),\n});\nCOPY_PROJECT\n  local -a npm_args=(ci)\n  case \"${CITIZENCHAIN_OFFLINE:-false}\" in\n    true) npm_args+=(--offline) ;;\n    false) ;;\n    *) echo 'CITIZENCHAIN_OFFLINE只接受true或false' >&2; return 1 ;;\n  esac\n  (cd \"$destination\" && npm \"${npm_args[@]}\")\n  BUILD_NODE_PROJECT=\"$destination\"\n}\n\necho '==> 准备产品依赖：citizenchain/crates/scanner'\nprepare_node_project \"$CITIZENCHAIN_ROOT/crates/scanner\" citizenchain/crates/scanner\nSCANNER_REACT_PROJECT=\"$BUILD_NODE_PROJECT\"\necho '==> 准备产品依赖：citizenchain/node/frontend'\nprepare_node_project \"$NODE_FRONTEND_SOURCE\" citizenchain/node/frontend\nNODE_FRONTEND_PROJECT=\"$BUILD_NODE_PROJECT\"\necho '==> 准备产品依赖：citizenchain/onchina/frontend'\nprepare_node_project \"$ONCHINA_FRONTEND_SOURCE\" citizenchain/onchina/frontend\nONCHINA_FRONTEND_PROJECT=\"$BUILD_NODE_PROJECT\"\n\n# 图片原件只复制到本轮统一资源目录，前端不另建图片副本。\nmkdir -p \"$CITIZENCHAIN_WORK_DIR/source/citizenchain/icons\"\ncp -R \"$CITIZENCHAIN_ROOT/icons/.\" \"$CITIZENCHAIN_WORK_DIR/source/citizenchain/icons/\"\nexport SCANNER_REACT_PROJECT NODE_FRONTEND_PROJECT ONCHINA_FRONTEND_PROJECT\n\nexport CITIZENCHAIN_PROJECT_ROOT=\"$CITIZENCHAIN_WORK_DIR/source/citizenchain\"\n"});
export async function runEmbeddedBuild(kind,args=[],environment=process.env){
 const platform=kind==='windows'?'windows':environment.PLATFORM||(kind==='local'?args[0]:'macos');platformContract(platform);
 const work=environment.PRODUCT_WORK_DIR||environment.CITIZENCHAIN_WORK_DIR||temporaryRoot(platform,'build');checkWork(work);
 return withFixedWork(taskScope(work),()=>embeddedBuildTask(kind,args,{...environment,CITIZENCHAIN_WORK_DIR:work}),{environment});
}
async function embeddedBuildTask(kind,args=[],environment=process.env){
 if((kind!=='windows'&&!Object.hasOwn(BUILD_SOURCES,kind))||kind==='prepare')fail('构建子命令无效');
 if(kind==='local'&&(!isAbsolute(args[1])||!inside(environment.CITIZENCHAIN_WORK_DIR,args[1])))fail('本机构建输出越出固定工作根');
 if(kind==='windows')return prepackWindows(environment);
 const platform=environment.PLATFORM||args[0]||'macos';platformContract(platform);const base=environment.PRODUCT_WORK_DIR||environment.CITIZENCHAIN_WORK_DIR||temporaryRoot(platform,'build');checkWork(base);
 const directory=join(base,'embedded'),prepareFile=join(directory,'prepare.sh'),file=join(directory,kind==='constitution'?'step.py':'step.sh');
 mkdirSync(directory,{recursive:true});writeFileSync(prepareFile,BUILD_SOURCES.prepare,{mode:0o700});writeFileSync(file,BUILD_SOURCES[kind],{mode:0o700});
 const env={...environment,CITIZENCHAIN_SOURCE_ROOT:root,CITIZENCHAIN_ROOT:root,PRODUCT_PREPARE_SHELL:prepareFile,PRODUCT_NODE_BIN:environment.NODE||process.execPath};
 try{return await runBuildProcess(kind==='constitution'?environment.PYTHON:environment.PRODUCT_BASH_BIN||environment.PRODUCT_TEST_SHELL,[file,...args],env,root);}finally{rmSync(directory,{recursive:true,force:true});}
}
if(!inlineTestEntry&&directEntry&&(process.argv[2]==='windows'||Object.hasOwn(BUILD_SOURCES,process.argv[2]))){void runEmbeddedBuild(process.argv[2],process.argv.slice(3)).catch(e=>{console.error(e.message);process.exitCode=1;});}

if(!inlineTestEntry&&directEntry&&process.argv[2]!=='shell-source'&&!(process.argv[2]==='windows'||Object.hasOwn(BUILD_SOURCES,process.argv[2]))){
 void runCLI().catch(error=>{console.error(error);process.exitCode=1;});
}

if(!inlineTestEntry&&directEntry&&process.argv[2]==='shell-source'){if(process.argv.length!==4||process.argv[3]!=='prepare')fail('准备源码选择无效');process.stdout.write(BUILD_SOURCES.prepare);}

import {cpSync as copyPackageTree} from 'node:fs';
// Windows预打包保留原二进制、前端、行政区数据库与PG组装，只写本轮现场。
export async function prepackWindows(env){const work=env.PRODUCT_WORK_DIR||env.CITIZENCHAIN_WORK_DIR;checkWork(work);const cargo=env.CARGO,node=env.NODE;if(!cargo||!node)fail('Windows预打包缺少验真工具');const project=env.CITIZENCHAIN_PROJECT_ROOT||env.PRODUCT_SOURCE_DIR;if(!project||!inside(work,project)||realpathSync(project)!==project)fail('预打包工程不属于当前任务');const target=env.CARGO_TARGET_DIR;if(!target||!inside(work,target))fail('预打包Cargo输出越界');const resources=env.CITIZENCHAIN_PACKAGE_RESOURCES_DIR||join(work,'resources');if(!inside(work,resources))fail('预打包资源输出越界');await runBuildProcess(cargo,['build','-p','onchina','--release','--locked','--config',join(root,'config.toml')],env,root);const frontend=join(project,'onchina/frontend');await runBuildProcess(node,[join(dirname(dirname(node)),'lib/node_modules/npm/bin/npm-cli.js'),'run','build'],env,frontend);for(const name of ['onchina-bin','onchina-frontend','postgres'])mkdirSync(join(resources,name),{recursive:true});copyFileSync(join(target,'release/onchina.exe'),join(resources,'onchina-bin/onchina.exe'));const dist=env.ONCHINA_FRONTEND_DIST||join(work,'onchina-frontend/dist');if(!inside(work,dist)||realpathSync(dist)!==dist)fail('预打包前端输出越界');rmSync(join(resources,'onchina-frontend/dist'),{recursive:true,force:true});copyPackageTree(dist,join(resources,'onchina-frontend/dist'),{recursive:true});const pg=env.CITIZENCHAIN_PG_DIST;if(pg&&existsSync(join(pg,'bin'))){const out=join(resources,'postgres/windows');rmSync(out,{recursive:true,force:true});copyPackageTree(pg,out,{recursive:true});}else process.stderr.write('[prepack] 未交付PostgreSQL原件，保持原有可选组装行为\n');rmSync(join(resources,'genesis-state'),{recursive:true,force:true});return {resources};}

// 本文件回归使用固定根；夹具支持随测试正文集中在正式实现之后。
function fixtureWork(){const work=fixedWork('build/'+Object.keys(contract.platforms)[0]);if(existsSync(work))finishFixedWork(work);return checkFixedWork(work,{create:true});}
function removeFixture(path,options={}){if([fixedWork('test'),...Object.keys(contract.platforms).map(platform=>fixedWork('build/'+platform))].includes(path)){if(existsSync(path))clearFixedWork(path);return;}rmSync(path,options);}
function writeFixture(path,data,options){writeFileSync(path,data,options);}
function copyFixture(source,destination,...options){copyFileSync(source,destination,...options);}

// 内嵌回归只由node --test直接运行本文件时注册，导入和正常执行不运行测试。
if(inlineTestEntry){
void (async()=>{
// 产品独立入口：真实只读需求、资源身份、路径隔离与锁定归档失败关闭。
const {test} = await import('node:test');
const {spawnSync} = await import('node:child_process');
const {default:assert} = await import('node:assert/strict');
const {existsSync,lstatSync,mkdtempSync,readFileSync,readdirSync,realpathSync,rmSync,mkdirSync,symlinkSync,writeFileSync} = await import('node:fs');
const tmpdir=testRoot;
const {dirname,join,resolve} = await import('node:path');


const sandbox=fixtureWork;
const root=resolve(import.meta.dirname,'..'),base=existsSync(join(root,'app/pubspec.yaml'))?join(root,'app'):root;
function historyFixture(platform='windows'){
 const file={macos:'macos.yml',windows:'windows.yml','linux-arm':'linuxarm.yml','linux-amd':'linuxamd.yml',wasm:'wasm.yml'}[platform];
 const sha='a'.repeat(40),workflow='.github/workflows/'+file,runs=new Map(),releases=new Map(),refs=new Map(),calls=[];
 for(const [id,conclusion]of [[1,'success'],[2,'failure'],[3,'failure'],[4,'success']])runs.set(id,{id,run_attempt:1,status:'completed',conclusion,
  path:workflow,event:'workflow_dispatch',head_branch:'main',head_sha:sha,repository:{full_name:'crcfrcn/citizenchain'}});
 runs.get(4).status='in_progress';runs.get(4).conclusion=null;
 const jobs=[{id:41,run_id:4,head_sha:sha,name:platform==='wasm'?'release':nodeDeliveryName(platform),status:'completed',conclusion:'success'},
  {id:42,run_id:4,head_sha:sha,name:'finalize',status:'in_progress',conclusion:null}];
 const release=(id,run,version)=>{
  const tag=`citizenchain-${platform}-v${version}-r${run}-a1`;
  const assets=githubReleaseFiles(platform,version).map((name,index)=>({id:id*10+index,name,state:'uploaded',size:3,digest:'sha256:'+'b'.repeat(64)}));
  const proof={schema:1,repository:'crcfrcn/citizenchain',product_id:'citizenchain',platform,workflow,run_id:run,run_attempt:1,
   source_sha:sha,tag,version,assets:assets.map(asset=>({name:asset.name,size:asset.size,sha256:'b'.repeat(64)}))};
  const value={id,tag_name:tag,target_commitish:sha,body:`<!-- automation:${JSON.stringify(proof)} -->`,draft:false,prerelease:false,
   published_at:'2026-10-16T00:00:00Z',assets};
  releases.set(id,value);refs.set(tag,{ref:'refs/tags/'+tag,object:{type:'commit',sha}});return value;
 };
 release(101,1,platform==='wasm'?'1':'0.9.0');release(104,4,platform==='wasm'?'2':'1.0.0');
 releases.set(999,{id:999,tag_name:`citizenchain-${platform==='macos'?'windows':'macos'}-v1.0.0-r9-a1`,draft:false,prerelease:false});
 const state={runs,releases,refs,jobs,calls,release,denyDelete:false};
 const fetchImpl=async(url,options)=>{
  const target=new URL(url),path=target.pathname.slice('/repos/crcfrcn/citizenchain'.length),method=options.method;
  assert.equal(target.origin,'https://api.github.com');assert.equal(options.redirect,'error');calls.push([method,path]);
  if(method==='DELETE'&&state.denyDelete)return new Response(null,{status:403});
  let match;
  if(path==='/actions/runs/4/attempts/1/jobs')return Response.json({jobs});
  if(path==='/actions/workflows/'+file+'/runs')return Response.json({workflow_runs:[...runs.values()]});
  if(path==='/releases')return Response.json([...releases.values()]);
  if((match=/^\/actions\/runs\/(\d+)$/u.exec(path))){const id=Number(match[1]);if(method==='DELETE'){runs.delete(id);return new Response(null,{status:204});}return runs.has(id)?Response.json(runs.get(id)):new Response(null,{status:404});}
  if((match=/^\/releases\/(\d+)$/u.exec(path))){const id=Number(match[1]);if(method==='DELETE'){releases.delete(id);return new Response(null,{status:204});}return releases.has(id)?Response.json(releases.get(id)):new Response(null,{status:404});}
  if(path.startsWith('/git/ref/tags/')){const tag=decodeURIComponent(path.slice('/git/ref/tags/'.length));return refs.has(tag)?Response.json(refs.get(tag)):new Response(null,{status:404});}
  if(method==='DELETE'&&path.startsWith('/git/refs/tags/')){refs.delete(decodeURIComponent(path.slice('/git/refs/tags/'.length)));return new Response(null,{status:204});}
  throw Error('夹具拒绝未声明请求：'+method+' '+path);
 };
 return {...state,state,fetchImpl,request:{run_id:4,run_attempt:1,source_sha:sha,business_result:'success'},environment:{GH_TOKEN:'fixture',
  GITHUB_ACTIONS:'true',GITHUB_REPOSITORY:'crcfrcn/citizenchain',GITHUB_WORKFLOW:'citizenchain.'+platform,
  GITHUB_EVENT_NAME:'workflow_dispatch',GITHUB_REF:'refs/heads/main',GITHUB_SHA:sha,GITHUB_RUN_ID:'4',GITHUB_RUN_ATTEMPT:'1'}};
}
test('五平台业务收尾可并行且各自只删除所属历史',async()=>{
 await Promise.all(['macos','windows','linux-arm','linux-amd','wasm'].map(async platform=>{
  const fixture=historyFixture(platform);
  const result=await finalizeGithubHistory(platform,fixture.request,fixture.environment,{fetchImpl:fixture.fetchImpl});
  assert.equal(result.platform,platform);assert.equal(result.kept_release,104);
  assert.deepEqual([...fixture.runs.keys()].sort((a,b)=>a-b),[3,4]);
  assert.deepEqual([...fixture.releases.keys()].sort((a,b)=>a-b),[104,999]);
  assert.ok(fixture.calls.every(([,path])=>!path.includes('caches')));
 }));
});
test('五份既有Workflow的收尾只依赖本平台业务并保留独立缓存',()=>{
 for(const [platform,file]of Object.entries({macos:'macos.yml',windows:'windows.yml','linux-arm':'linuxarm.yml','linux-amd':'linuxamd.yml',wasm:'wasm.yml'})){
  const source=readFileSync(join(root,'.github/workflows',file),'utf8'),parts=source.split('\n  finalize:\n');
  assert.equal(parts.length,2);const finalizer=parts[1];
  assert.match(finalizer,/^    needs: \[release\]$/mu);
  assert.match(finalizer,/^    if: \$\{\{ always\(\) && github\.ref == 'refs\/heads\/main' \}\}$/mu);
  assert.match(finalizer,/^      actions: write$/mu);assert.match(finalizer,/^      contents: write$/mu);
  assert.ok(finalizer.includes('node scripts/build.mjs ci-finalize '+platform+'\n'));
  assert.ok(source.includes('group: "${{ github.repository }}-'+platform+'"'));
  assert.ok(source.includes('path: target/cache/'+platform));
  assert.doesNotMatch(source,/workflow_call:|workflow_run:|repository_dispatch:|schedule:/u);
  assert.doesNotMatch(finalizer,/cargo |rust-toolchain|cache\/save|cache\/restore|uses: \.\//u);
 }
});
test('业务失败的产品收尾保留旧有效正式版及本次失败身份',async()=>{
 const fixture=historyFixture();fixture.request.business_result='failure';fixture.jobs[0].conclusion='failure';
 fixture.releases.get(104).assets=[];
 const result=await finalizeGithubHistory('windows',fixture.request,fixture.environment,{fetchImpl:fixture.fetchImpl});
 assert.equal(result.business_result,'failure');assert.equal(result.final_run_verified,false);assert.equal(result.kept_release,101);
 assert.deepEqual(result.kept_runs.sort((a,b)=>a-b),[1,4]);
 assert.deepEqual([...fixture.runs.keys()].sort((a,b)=>a-b),[1,4]);
 assert.equal(fixture.releases.has(104),false);assert.equal(fixture.releases.has(101),true);
});
test('产品历史收尾回读业务Job后保留本平台记录且不伪称Run已完成',async()=>{
 const fixture=historyFixture();fixture.runs.set(5,{...fixture.runs.get(4),id:5,status:'in_progress',conclusion:null});
 fixture.release(103,3,'0.9.1').assets=[];
 const result=await finalizeGithubHistory('windows',fixture.request,fixture.environment,{fetchImpl:fixture.fetchImpl});
 assert.equal(result.run_status,'in_progress');assert.equal(result.final_run_verified,false);assert.equal(result.business_result,'success');
 assert.deepEqual(result.kept_runs.sort((a,b)=>a-b),[3,4]);assert.equal(result.kept_release,104);
 assert.deepEqual([...fixture.runs.keys()].sort((a,b)=>a-b),[3,4,5]);
 assert.deepEqual([...fixture.releases.keys()].sort((a,b)=>a-b),[104,999]);
 assert.equal(fixture.refs.has('citizenchain-windows-v0.9.0-r1-a1'),false);
 assert.equal(fixture.refs.has('citizenchain-windows-v1.0.0-r4-a1'),true);
 assert.ok(fixture.calls.every(([,path])=>!path.includes('caches')&&!path.includes('macos.yml')));
 const deletes=fixture.calls.filter(([method])=>method==='DELETE');
 const repeated=await finalizeGithubHistory('windows',fixture.request,fixture.environment,{fetchImpl:fixture.fetchImpl});
 assert.deepEqual(repeated.deleted_runs,[]);assert.deepEqual(repeated.deleted_releases,[]);
 assert.equal(fixture.calls.filter(([method])=>method==='DELETE').length,deletes.length);
});
test('历史收尾拒绝业务未结束、身份错配和来源漂移且不先删除',async()=>{
 for(const change of [
  fixture=>{fixture.jobs[0].status='in_progress';fixture.jobs[0].conclusion=null;},
  fixture=>{fixture.jobs[1].status='queued';},
  fixture=>{fixture.request.business_result='failure';},
  fixture=>{fixture.environment.GITHUB_WORKFLOW='citizenchain.linux-arm';},
  fixture=>{fixture.runs.get(4).status='completed';fixture.runs.get(4).conclusion='success';},
  fixture=>{fixture.request.source_sha='c'.repeat(40);},
  fixture=>{fixture.runs.get(4).event='repository_dispatch';},
  fixture=>{fixture.releases.get(101).target_commitish='c'.repeat(40);},
  fixture=>{fixture.releases.get(104).assets.pop();},
  fixture=>{fixture.request.run_id=1;},
 ]){
  const fixture=historyFixture();change(fixture);
  await assert.rejects(finalizeGithubHistory('windows',fixture.request,fixture.environment,{fetchImpl:fixture.fetchImpl}));
  assert.equal(fixture.calls.filter(([method])=>method==='DELETE').length,0);assert.equal(fixture.runs.size,4);
 }
});
test('历史删除失败即停，保留最新有效版本及原始失败回执',async()=>{
 const fixture=historyFixture();fixture.state.denyDelete=true;
 await assert.rejects(finalizeGithubHistory('windows',fixture.request,fixture.environment,{fetchImpl:fixture.fetchImpl}),/HTTP 403/u);
 assert.equal(fixture.calls.filter(([method])=>method==='DELETE').length,1);
 assert.equal(fixture.releases.has(104),true);assert.equal(fixture.releases.has(101),true);assert.equal(fixture.runs.size,4);
});
test('本平台正式版号必须高于自己的历史正式Release',async()=>{
 const release=version=>({draft:false,prerelease:false,tag_name:`citizenchain-windows-v${version}-r1-a1`});
 const read=rows=>({fetchImpl:async()=>Response.json(rows)});
 assert.equal(await assertFreshGithubVersion('windows',{GH_TOKEN:'fixture'},read([release('0.9.9')])),
  JSON.parse(readFileSync(join(root,'node/tauri.conf.json'),'utf8')).version);
 await assert.rejects(assertFreshGithubVersion('windows',{GH_TOKEN:'fixture'},read([release('1.0.0')])),/没有高于/u);
});
test('四节点交付主名和清单URL使用正式版本及准确编码',()=>{
 for(const [platform,display]of Object.entries({macos:'macOS',windows:'Windows','linux-arm':'LinuxARM','linux-amd':'LinuxAMD'})){
  const name=nodeDeliveryName(platform,'1.0.0');
  assert.equal(name,`公民链·${display}-v1.0.0`);
  const source=readFileSync(join(root,'.github/workflows',({macos:'macos.yml',windows:'windows.yml','linux-arm':'linuxarm.yml','linux-amd':'linuxamd.yml'})[platform]),'utf8');
  assert.ok(source.split('\n').includes('run-name: "'+nodeDeliveryName(platform)+'"'));
  assert.ok(source.split('\n').includes('    name: "'+nodeDeliveryName(platform)+'"'));
  const url=new URL(githubReleaseAssetUrl('citizenchain-'+platform+'-v1.0.0-r1-a1',name+'.json'));
  assert.equal(decodeURIComponent(url.pathname.split('/').at(-1)),name+'.json');
  assert.equal(url.search,'');assert.equal(url.hash,'');
  const declared=JSON.parse(JSON.stringify(contract)).platforms[platform];
  const extensions={macos:['.app'],windows:['.exe','.exe.sig'],'linux-arm':['.deb','.AppImage','.AppImage.sig'],'linux-amd':['.deb','.AppImage','.AppImage.sig']}[platform];
  assert.deepEqual(declared.files,extensions.map(extension=>nodeDeliveryName(platform)+extension));
 }
 for(const version of ['','1.0','01.0.0','1.0.0/other','1.0.0?other','1.0.0-rc1'])assert.throws(()=>nodeDeliveryName('macos',version),/名称无效/u);
 assert.throws(()=>nodeDeliveryName('wasm','1.0.0'),/名称无效/u);
});
test('两种节点入口的编译前门槛顺序一致且首错阻断后续',async()=>{
 const work=fixedWork('build/macos'),seen=[];
 const qr=async(platform,path,environment,options)=>{seen.push('qr');assert.equal(path,work);assert.equal(options.controlled,true);};
 const rpc=async()=>{seen.push('rpc');};
 await runNodePrebuildTests('macos',work,{}, {controlled:true,qr,rpc});assert.deepEqual(seen,['qr','rpc']);
 seen.length=0;const original=Error('QR原始失败');
 await assert.rejects(runNodePrebuildTests('macos',work,{}, {qr:async()=>{seen.push('qr');throw original;},rpc}),error=>error===original);
 assert.deepEqual(seen,['qr']);
});
test('QR协议门槛保留离线约束、固定目标及WASM隔离',async()=>{
 const work=fixedWork('build/windows'),calls=[];
 const runner=async(...args)=>{calls.push(args);};
 const environment={CARGO:realpathSync(process.execPath),CARGO_NET_OFFLINE:'true'};
 await runQrProtocolTests('windows',work,environment,{controlled:true,runner});
 assert.equal(calls.length,1);assert.ok(calls[0][1].includes('--locked'));assert.ok(calls[0][1].includes('--offline'));
 assert.equal(calls[0][2].CARGO_TARGET_DIR,join(work,'work/qr-test-target'));
 calls.length=0;await runQrProtocolTests('wasm',fixedWork('build/wasm'),{}, {runner});assert.equal(calls.length,0);
 await assert.rejects(runQrProtocolTests('windows',work,{CARGO:'cargo'},{controlled:true,runner}),/受控 Cargo/u);
 await assert.rejects(runQrProtocolTests('windows',fixedWork('build/linux-arm'),environment,{runner}),/平台或现场/u);
});
test('Windows正式资产清单只回读本平台准确Release',async()=>{
 const work=checkFixedWork(fixedWork('build/windows'),{create:true});
 try{
  writeFileSync(join(work,nodeDeliveryName('windows')+'.exe'),'abc');writeFileSync(join(work,nodeDeliveryName('windows')+'.exe.sig'),'signature');
  const environment={GITHUB_RUN_ID:'12',GITHUB_RUN_ATTEMPT:'1',GITHUB_SHA:'a'.repeat(40),GITHUB_ACTIONS:'true',
   GITHUB_REPOSITORY:'crcfrcn/citizenchain',GITHUB_WORKFLOW:'citizenchain.windows',GH_TOKEN:'fixture'};
  const plan=await prepareGithubAssets('windows',work,null,environment);
  assert.equal(plan.assets.length,3);assert.equal(plan.tag,'citizenchain-windows-v1.0.0-r12-a1');
  assert.equal(plan.title,'公民链·Windows-v1.0.0');
  assert.deepEqual(plan.assets.map(value=>value.name).sort(),['公民链·Windows-v1.0.0.exe','公民链·Windows-v1.0.0.exe.sig','公民链·Windows-v1.0.0.json'].sort());
  const installer=plan.assets.find(value=>value.name.endsWith('.exe'));
  assert.equal(installer.sha256,'ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad');
  assert.notEqual(installer.sha256,outputDigest(join(plan.directory,installer.name)));
  const link=join(work,'asset-link');symlinkSync(join(plan.directory,installer.name),link);
  try{assert.throws(()=>releaseAssetSha256(link),/规范普通文件/u);}finally{rmSync(link);}
  const base='https://api.github.com/repos/crcfrcn/citizenchain/releases';
  const release={id:70,url:`${base}/70`,name:plan.title,tag_name:plan.tag,target_commitish:environment.GITHUB_SHA,prerelease:false,draft:true,
   body:readFileSync(plan.notes,'utf8'),assets:plan.assets.map((value,index)=>({id:index+1,name:value.name,state:'uploaded',size:value.size,digest:'sha256:'+value.sha256,browser_download_url:githubReleaseAssetUrl(plan.tag,value.name)}))};
  let listed=[release],byId=release,pages=null;
  const calls=[],fetchImpl=async url=>{
   calls.push(url);
   if(url.startsWith(base+'?'))return Response.json(pages?pages.get(Number(new URL(url).searchParams.get('page')))??[]:listed);
   if(url===`${base}/70`)return byId?Response.json(byId):new Response(null,{status:404});
   if(url===`${base}/tags/${plan.tag}`)return Response.json(release);
   return new Response(null,{status:404});
  };
  assert.deepEqual(await verifyGithubRelease('windows','draft',environment,{fetchImpl}),{platform:'windows',tag:plan.tag,phase:'draft'});
  assert.deepEqual(calls,[`${base}?per_page=100&page=1`,`${base}/70`]);
  release.name='错误标题';await assert.rejects(verifyGithubRelease('windows','draft',environment,{fetchImpl}),/身份或状态/u);release.name=plan.title;
  const assetURL=release.assets[0].browser_download_url;
  release.assets[0].browser_download_url=decodeURIComponent(assetURL);
  assert.deepEqual(await verifyGithubRelease('windows','draft',environment,{fetchImpl}),{platform:'windows',tag:plan.tag,phase:'draft'});
  release.assets[0].browser_download_url=assetURL+'?other';await assert.rejects(verifyGithubRelease('windows','draft',environment,{fetchImpl}),/资产回读不符/u);
  release.assets[0].browser_download_url=assetURL;
  writeFileSync(join(plan.directory,installer.name),'abd');
  await assert.rejects(verifyGithubRelease('windows','draft',environment,{fetchImpl}),/本轮资产字节已改变/u);
  writeFileSync(join(plan.directory,installer.name),'abc');
  release.assets[0].digest='sha256:'+'0'.repeat(64);
  await assert.rejects(verifyGithubRelease('windows','draft',environment,{fetchImpl}),/资产回读不符/u);
  release.assets[0].digest='sha256:'+plan.assets[0].sha256;
  listed=[];await assert.rejects(verifyGithubRelease('windows','draft',environment,{fetchImpl}),/草稿Release缺失/u);
  listed=[{...release,id:0}];await assert.rejects(verifyGithubRelease('windows','draft',environment,{fetchImpl}),/草稿Release身份无效/u);
  listed=[release];byId=null;await assert.rejects(verifyGithubRelease('windows','draft',environment,{fetchImpl}),/Release回读失败/u);
  byId={...release,id:71};await assert.rejects(verifyGithubRelease('windows','draft',environment,{fetchImpl}),/身份漂移/u);
  byId=release;
  const full=Array.from({length:100},(_,index)=>({tag_name:`other-${index}`}));
  listed=full;await assert.rejects(verifyGithubRelease('windows','draft',environment,{fetchImpl}),/分页超限/u);
  pages=new Map([[1,full],[2,[release]]]);
  assert.deepEqual(await verifyGithubRelease('windows','draft',environment,{fetchImpl}),{platform:'windows',tag:plan.tag,phase:'draft'});
  pages=new Map([[1,[release,...full.slice(1)]],[2,[release]]]);
  await assert.rejects(verifyGithubRelease('windows','draft',environment,{fetchImpl}),/草稿Tag不唯一/u);
  pages=null;
  listed=[release];release.draft=false;release.published_at='2026-10-14T00:00:00Z';calls.length=0;
  await assert.rejects(verifyGithubRelease('windows','draft',environment,{fetchImpl}),/身份或状态不符/u);
  calls.length=0;
  assert.deepEqual(await verifyGithubRelease('windows','published',environment,{fetchImpl}),{platform:'windows',tag:plan.tag,phase:'published'});
  assert.deepEqual(calls,[`${base}/tags/${plan.tag}`]);
  release.draft=true;await assert.rejects(verifyGithubRelease('windows','published',environment,{fetchImpl}),/身份或状态不符/u);
 }finally{clearFixedWork(work);}
});
test('六项节点守卫定向筛选对应真实Rust模块与注册用例',()=>{
 // 从实际执行清单取名并核对源码注册链，防止模块迁移后精确筛选静默空跑。
 const filters=[...runDesktopRpcTests.toString().matchAll(/\['node','([^']*::runtime_policy::tests::[^']+)'\]/gu)].map(match=>match[1]);
 assert.equal(filters.length,6);assert.equal(new Set(filters).size,6);
 assert.match(readFileSync(join(root,'node/src/main.rs'),'utf8'),/^mod core;/mu);
 assert.match(readFileSync(join(root,'node/src/core/mod.rs'),'utf8'),/#\[path\s*=\s*"\.\.\/guard\/mod\.rs"\]\s*pub\(crate\) mod node_guard;/u);
 assert.match(readFileSync(join(root,'node/src/guard/mod.rs'),'utf8'),/^mod runtime_policy;/mu);
 const source=readFileSync(join(root,'node/src/guard/runtime_policy.rs'),'utf8');
 assert.match(source,/#\[cfg\(test\)\]\s*mod tests\s*\{/u);
 for(const filter of filters){
  const parts=filter.split('::'),name=parts.pop();
  assert.deepEqual(parts,['core','node_guard','runtime_policy','tests']);
  assert.match(name,/^[a-z][a-z0-9_]*$/u);
  assert.match(source,new RegExp('#\\[test\\]\\s+fn '+name+'\\s*\\(','u'));
 }
});

test('macOS编译产物安装只接受本仓现场，更新失败恢复原App',async()=>{
 const testWork=testRoot(),work=mkdtempSync(join(testWork,'macos-install-work-'));
 const parent=mkdtempSync(join(testWork,'macos-install-target-')),destination=join(parent,'CitizenChain.app');
 const candidate=join(work,nodeDeliveryName('macos')+'.app'),file=join(candidate,'Contents/MacOS/citizenchain');
 const verify=async(path,expected)=>{const digest=outputDigest(path);if(expected!==null&&digest!==expected)throw Error('候选摘要漂移');return digest;};
 try{
  mkdirSync(dirname(file),{recursive:true});writeFixture(join(candidate,'Contents/Info.plist'),'fixture');writeFixture(file,'first');
  await assert.rejects(installMacosApp(work,{}, {destination:'/tmp/CitizenChain.app',verify}),/越界/);
  await installMacosApp(work,{}, {destination,verify});
  assert.equal(readFileSync(join(destination,'Contents/MacOS/citizenchain'),'utf8'),'first');
  writeFixture(file,'second');await installMacosApp(work,{}, {destination,verify});
  assert.equal(readFileSync(join(destination,'Contents/MacOS/citizenchain'),'utf8'),'second');
  writeFixture(file,'third');const third=outputDigest(candidate);
  await assert.rejects(installMacosApp(work,{}, {destination,verify:async(path,expected)=>{
   const digest=await verify(path,expected);if(path===destination&&expected===third)throw Error('安装后回读失败');return digest;
  }}),/安装后回读失败/);
  assert.equal(readFileSync(join(destination,'Contents/MacOS/citizenchain'),'utf8'),'second');
  assert.equal(existsSync(destination+'.old')||existsSync(destination+'.next'),false);
  mkdirSync(destination+'.next');await assert.rejects(installMacosApp(work,{}, {destination,verify}),/未收口/);
 }finally{rmSync(parent,{recursive:true,force:true});rmSync(work,{recursive:true,force:true});}
});
test('节点内嵌脚本只调用产品资源回执的Python入口',()=>{
 assert.match(BUILD_SOURCES.prepare,/"\$\{PYTHON:\?\}" - "\$CITIZENCHAIN_ROOT"/u);
 assert.match(BUILD_SOURCES.run,/\$\("\$\{PYTHON:\?\}" -c /u);
 for(const name of ['prepare','run'])assert.doesNotMatch(BUILD_SOURCES[name],/\bpython3\b/u);
});
test('工程复制目标在Bash严格未绑定变量模式下从已赋值参数计算',()=>{
 const lines=BUILD_SOURCES.prepare.split('\n'),at=lines.indexOf('prepare_node_project() {');
 assert.ok(at>=0);
 const assignment=lines.slice(at+1,at+3).join('\n');
 const script=['set -u','prepare_node_project() {',assignment,'  printf "%s" "$destination"','}','prepare_node_project "$1" "$2"'].join('\n');
 const work=join(testRoot(),'prepared-fixture'),env={CITIZENCHAIN_WORK_DIR:work};
 const valid=spawnSync('/bin/bash',['-c',script,'fixture','/source','node/frontend'],{env,encoding:'utf8'});
 assert.equal(valid.status,0,valid.stderr);assert.equal(valid.stdout,join(work,'source/node/frontend'));
 const missing=spawnSync('/bin/bash',['-c',script,'fixture','/source'],{env,encoding:'utf8'});
 assert.notEqual(missing.status,0);assert.match(missing.stderr,/unbound variable/);
});
test('macOS三个内嵌npm工程均由本产品原始锁准备',()=>{
 const locks=contract.platforms.macos.locks.filter(item=>item.ecosystem==='npm').map(item=>item.path);
 assert.deepEqual(locks,['crates/scanner/package-lock.json','node/frontend/package-lock.json','onchina/frontend/package-lock.json']);
 const calls=BUILD_SOURCES.prepare.split('\n').filter(line=>line.startsWith('prepare_node_project '));
 assert.equal(calls.length,locks.length);
 for(const path of locks)assert.ok(existsSync(join(root,path)),path+'原始锁不存在');
});
const fixture=work=>{
 const platform=Object.keys(contract.platforms).find(value=>value.endsWith('android'))||Object.keys(contract.platforms)[0];
 const own={};for(const value of contract.platforms[platform].locks){const key={npm:'npmCache',pub:'pubCache',cargo:'cargoHome'}[value.ecosystem];if(key&&!own[key]){own[key]=join(work,key);mkdirSync(own[key]);}}
 return {schema:1,product_id:contract.product_id,platform,work,offline:true,
 tools:Object.fromEntries(contract.platforms[platform].tools.map(tool=>[tool.id,{version:tool.version,path:process.execPath}])),
 dependencies:{own},archives:{},environment:{}};
};
test('节点Clang动态库只从当前Xcode真实工具链交付',()=>{
 const work=mkdtempSync(join(testRoot(),'libclang-')),developer=join(work,'Xcode.app/Contents/Developer');
 const usr=join(developer,'Toolchains/Test.xctoolchain/usr'),bin=join(usr,'bin'),lib=join(usr,'lib');
 const clang=join(bin,'clang'),dylib=join(lib,'libclang.dylib');
 try{
  mkdirSync(bin,{recursive:true});mkdirSync(lib);writeFileSync(clang,'clang',{mode:0o755});writeFileSync(dylib,'libclang');
  const supplied={DEVELOPER_DIR:developer,CC:clang},nodeEnv=nodeCargoEnvironment(supplied);
  assert.equal(checkedLibclangDirectory(supplied),lib);
  assert.equal(nodeEnv.LIBCLANG_PATH,lib);assert.equal(nodeEnv.DYLD_LIBRARY_PATH,lib);
  assert.equal(nodeEnv.CARGO_PROFILE_RELEASE_BUILD_OVERRIDE_STRIP,'none');
  assert.equal(supplied.CARGO_PROFILE_RELEASE_BUILD_OVERRIDE_STRIP,undefined);
  const outside=join(work,'outside.dylib');writeFileSync(outside,'outside');rmSync(dylib);symlinkSync(outside,dylib);
  assert.throws(()=>checkedLibclangDirectory({DEVELOPER_DIR:developer,CC:clang}),/准确普通文件/);
  rmSync(dylib);assert.throws(()=>checkedLibclangDirectory({DEVELOPER_DIR:developer,CC:clang}),/ENOENT/);
  assert.throws(()=>checkedLibclangDirectory({DEVELOPER_DIR:developer,CC:outside}),/准确Xcode/);
 }finally{rmSync(work,{recursive:true,force:true});}
});
test('每个平台从自身原始锁只读提出需求；缺失原始Pod锁按源码事实拒绝',async()=>{
 const work=sandbox();try{for(const platform of Object.keys(contract.platforms)){
  const before=readdirSync(work),apple=platform.endsWith('ios')?'ios':platform.endsWith('macos')?'macos':null;
  if(apple&&existsSync(join(base,apple,'Podfile'))&&!existsSync(join(base,apple,'Podfile.lock'))){
   await assert.rejects(async()=>requirements(platform,work),/CocoaPods原始锁缺失/);
  }else{
   const result=await requirements(platform,work);assert.equal(result.product_id,contract.product_id);
   assert.equal(result.platform,platform);assert.equal(result.schema,1);
   assert.ok(result.tools.every(value=>value.id&&value.version));
   assert.ok(result.locks.every(value=>['cargo','pub','npm','cocoapods'].includes(value.ecosystem)));
   assert.equal(result.locks.filter(value=>value.ecosystem==='cargo'&&value.path==='crates/protocol/Cargo.lock').length,platform==='wasm'?0:1);
  }
  assert.deepEqual(readdirSync(work),before);
 }}finally{removeFixture(work,{recursive:true});}
});
test('平台、源码内工作根和链接工作根在任何写入前拒绝',async()=>{
 const work=sandbox();try{
  await assert.rejects(async()=>requirements('unknown',work),/平台/);
  assert.throws(()=>checkWork(root),/本产品target/);
  mkdirSync(join(work,'actual'));symlinkSync(join(work,'actual'),join(work,'linked'));
  assert.throws(()=>checkWork(join(work,'linked')),/固定目录/);
 }finally{removeFixture(work,{recursive:true});}
});
test('资源回执隔离产品、平台、工作根，准确工具版本且禁止注入',()=>{
 const work=sandbox();try{
  const receipt=fixture(work),platform=receipt.platform;
  assert.throws(()=>resourceEnvironment(platform,work,{...receipt,product_id:'another'}),/身份/);
  assert.throws(()=>resourceEnvironment(platform,work,{...receipt,offline:false}),/身份/);
  assert.throws(()=>resourceEnvironment(platform,work,{...receipt,tools:{}}),/工具/);
  assert.throws(()=>resourceEnvironment(platform,work,{...receipt,environment:{NODE_OPTIONS:'--inspect'}}),/注入/);
  const id=Object.keys(receipt.tools)[0];assert.throws(()=>resourceEnvironment(platform,work,{...receipt,tools:{...receipt.tools,[id]:{...receipt.tools[id],version:'wrong'}}}),/版本/);
  const env=resourceEnvironment(platform,work,receipt,{HOME:'/home',TOKEN:'private',INJECTED_CONTEXT:'/private'});
  assert.equal(env.TOKEN,undefined);assert.equal(env.INJECTED_CONTEXT,undefined);assert.equal(env.CARGO_NET_OFFLINE,'true');
  assert.equal(env[contract.product_id.toUpperCase()+'_WORK_DIR'],work);
 }finally{removeFixture(work,{recursive:true});}
});
test('原始锁需要的依赖必须显式交付，不能使用用户默认缓存',()=>{
 const work=sandbox();try{
  const receipt=fixture(work),own=receipt.dependencies.own;
  for(const key of Object.keys(own)){const missing={...own};delete missing[key];
   assert.throws(()=>resourceEnvironment(receipt.platform,work,{...receipt,dependencies:{own:missing}}),/依赖回执/);}
  const key=Object.keys(own)[0];if(key){
   const linked=join(work,'linked');symlinkSync(own[key],linked);
   assert.throws(()=>resourceEnvironment(receipt.platform,work,{...receipt,dependencies:{own:{...own,[key]:linked}}}),/依赖回执/);
  }
 }finally{removeFixture(work,{recursive:true});}
});
test('工程复制在同轮解析包并隔离写入，内部链接重新指向副本',()=>{
 const work=sandbox();try{
  const source=join(work,'input'),output=join(work,'view');mkdirSync(source);
  writeFixture(join(source,'package.json'),'{"name":"input"}');
  writeFixture(join(source,'code.js'),'source');symlinkSync('code.js',join(source,'linked.js'));
  mkdirSync(join(source,'node_modules'));writeFixture(join(source,'node_modules/old'),'generated');
  createView(source,output);writeFixture(join(output,'package.json'),'{"name":"generated"}');
  assert.equal(readFileSync(join(source,'package.json'),'utf8'),'{"name":"input"}');
  assert.equal(realpathSync(join(output,'linked.js')),join(output,'code.js'));
  assert.equal(existsSync(join(output,'node_modules')),false);
  assert.throws(()=>createView(source,output),/已存在/);
 }finally{removeFixture(work,{recursive:true});}
});
test('工程输出的父链接和输入外部链接均拒绝，不能写入第三方目录',()=>{
 const work=sandbox();try{
  const source=join(work,'source'),external=join(work,'external');mkdirSync(source);mkdirSync(external);
  writeFixture(join(source,'code'),'source');symlinkSync(external,join(work,'linked'));
  assert.throws(()=>createView(source,join(work,'linked/view')),/链接/);assert.deepEqual(readdirSync(external),[]);
  symlinkSync('/etc/passwd',join(source,'outside'));
  assert.throws(()=>createView(source,join(work,'bad-view')),/越界/);
 }finally{removeFixture(work,{recursive:true});}
});
test('未经本产品锁声明的归档回执不能用于编译',async()=>{
 const work=sandbox();try{
  const receipt=fixture(work);
  // 同一工具回执不能为归档注入增加来源；验证在任何暂存写入前结束。
  await assert.rejects(checkArchives(receipt.platform,work,{...receipt,archives:{injected:[{name:'unknown',version:'1.0.0',url:'https://example.invalid/archive',sha256:'a'.repeat(64),path:join(work,'missing')}]}}),/产品锁/);
 }finally{removeFixture(work,{recursive:true});}
});

// 本产品独立拥有此实现，通过自身公开接口执行。
test('独立命令行从自身声明输出JSON，未知平台失败且不写工作根',async()=>{
 const work=checkFixedWork(fixedWork('test'),{create:true});
  for(const platform of Object.keys(contract.platforms)){
   const before=readdirSync(work),result=spawnSync(process.execPath,[join(root,'scripts/build.mjs'),'requirements',platform,'--work',work],{env:{HOME:work,LANG:'C',LC_ALL:'C',NODE_TEST_CONTEXT:'1',CITIZENCHAIN_TEST_SESSION:testSession},encoding:'utf8'});
   const apple=platform.endsWith('ios')?'ios':platform.endsWith('macos')?'macos':null;
   if(apple&&existsSync(join(base,apple,'Podfile'))&&!existsSync(join(base,apple,'Podfile.lock'))){assert.notEqual(result.status,0);assert.match(result.stderr,/CocoaPods原始锁缺失/);}
   else{assert.equal(result.status,0,result.stderr);const value=JSON.parse(result.stdout);assert.equal(value.product_id,contract.product_id);assert.equal(value.platform,platform);}
   assert.deepEqual(readdirSync(work),before);
  }
  const invalid=spawnSync(process.execPath,[join(root,'scripts/build.mjs'),'requirements','unknown','--work',work],{env:{HOME:work,NODE_TEST_CONTEXT:'1',CITIZENCHAIN_TEST_SESSION:testSession},encoding:'utf8'});
  assert.notEqual(invalid.status,0);assert.match(invalid.stderr,/平台/);
});

// 完整入口控制边界：替身只替换耗时阶段，不调用真实编译或用户安全存储。
test('产品独立execute完成全部自有阶段后才返回唯一结果',async()=>{
 const {execute,outputDigest}=await import('./build.mjs');const work=sandbox(),platform=Object.keys(contract.platforms)[0],declared=contract.platforms[platform],calls=[];
 try{
  const result={schema:1,product_id:contract.product_id,platform,work,completion:declared.completion,run_id:'123456789',files:[]};
  const stages={requirements:async()=>{calls.push('requirements');},resources:async()=>{calls.push('resources');return {};},prepare:async()=>{calls.push('prepare');},build:async()=>{
   calls.push('build');for(const name of declared.files){const path=join(work,name);mkdirSync(dirname(path),{recursive:true});writeFixture(path,'isolated-candidate-fixture');result.files.push({path,sha256:outputDigest(path)});}return result;
  }};
  assert.deepEqual(await execute(platform,work,{run_id:'123456789'},{stages}),result);
  assert.deepEqual(calls,['requirements','resources','prepare','requirements','resources','build']);
  assert.equal(existsSync(work),false, '独立执行结束必须移除空的平台目录');
  result.files=[]; calls.length=0;
  assert.deepEqual(await execute(platform,work,{run_id:'123456789'},{stages}),result);
  assert.equal(existsSync(work),false, '下一轮结束仍须移除空的平台目录');
 }finally{removeFixture(work,{recursive:true});}
});
test('失败、取消、并发和伪造终态不能复用工作根或留下成功回执',async()=>{
 const {execute}=await import('./build.mjs'),platform=Object.keys(contract.platforms)[0];
 for(const failure of ['resources','prepare','build','identity','cancel']){
  const work=sandbox(),abort=new AbortController(),calls=[];
  try{
   const stages={requirements:()=>{},resources:async()=>{calls.push('resources');if(failure==='resources')throw Error('fixture failure');return {};},prepare:async()=>{calls.push('prepare');if(failure==='prepare')throw Error('fixture failure');if(failure==='cancel')abort.abort();},build:async()=>{calls.push('build');if(failure==='build')throw Error('fixture failure');return {schema:1,product_id:'forged'};}};
   await assert.rejects(execute(platform,work,{}, {stages,signal:abort.signal}));
   assert.equal(existsSync(join(work,'build-result.json')),false);assert.equal(existsSync(join(work,'.product-build.lock')),false);
   if(['resources','prepare','cancel'].includes(failure))assert.equal(calls.includes('build'),false);
  }finally{removeFixture(work,{recursive:true});}
 }
 const work=sandbox();try{writeFixture(join(work,'.product-build.lock'),'owned');await assert.rejects(execute(platform,work,{}));assert.equal(readFileSync(join(work,'.product-build.lock'),'utf8'),'owned');}finally{rmSync(join(work,'.product-build.lock'),{force:true});removeFixture(work,{recursive:true});}
});

test('产品取消等待工具进程组退出，不提前交付结果',async()=>{
 const {runBuildProcess}=await import('./build.mjs'),work=sandbox(),abort=new AbortController();let polling,deadline;
 try{
  const pidFile=join(work,'descendant.pid');
  const script="const fs=require('node:fs'),{spawn}=require('node:child_process');const child=spawn(process.execPath,['-e','setInterval(()=>{},1000)'],{stdio:'ignore'});fs.writeFileSync(process.argv[1],String(child.pid));setInterval(()=>{},1000);";
  const execution=runBuildProcess(process.execPath,['-e',script,pidFile],process.env,work,{capture:true,signal:abort.signal,timeout:5000});
  polling=setInterval(()=>{if(existsSync(pidFile))abort.abort();},20);deadline=setTimeout(()=>abort.abort(),2000);
  await assert.rejects(execution,/取消/);assert.ok(existsSync(pidFile));const pid=Number(readFileSync(pidFile,'utf8'));
  assert.throws(()=>process.kill(pid,0),error=>error.code==='ESRCH');
 }finally{clearInterval(polling);clearTimeout(deadline);removeFixture(work,{recursive:true});}
});

// 覆盖独立入口、单/多平台物理边界和源码输入排除，统一测试阶段才执行。
test('本仓target由当前平台声明决定，外部或链接工作根不能越界',()=>{
 for(const platform of Object.keys(contract.platforms)){
  const expected=join(root,'target');
  assert.equal(productTarget(platform),expected);
 }
 assert.throws(()=>productTarget('undeclared-platform'));
 assert.throws(()=>checkWork(join(root,'..','foreign-work')),/target/);
 assert.throws(()=>checkWork(join(root,'target')),/target/);
 const work=sandbox();try{assert.equal(checkWork(work),work);assert.throws(()=>checkWork(join(work,'nested')),/固定目录/);}finally{removeFixture(work,{recursive:true,force:true});}
});


// 完整宿主通道由调用方核验结果并收尾；独立执行仍必须立即清空。


})();
}


// 原非macOS完整调用、边界与首条失败收口回归归同一正式入口。
if(inlineTestEntry){void(async()=>{
const {test}=await import('node:test');const {default:assert}=await import('node:assert/strict');const {mkdtempSync,writeFileSync,readFileSync,chmodSync,rmSync}=await import('node:fs');
test('非macOS构建要求准确protoc并在首条失败后停止',async()=>{
 const work=fixtureWork(),inputs=join(testRoot(),'local-inputs');mkdirSync(inputs,{recursive:true});
 const tool=join(inputs,'cargo'),protoc=join(inputs,'protoc'),calls=join(inputs,'calls');
 try{
  writeFixture(tool,[
   '#!'+process.execPath,
   'const fs=require("node:fs");',
   'fs.appendFileSync(process.env.CALLS,JSON.stringify({args:process.argv.slice(2),target:process.env.CARGO_TARGET_DIR,wasm:process.env.WASM_FILE})+"\\n");',
   'process.exit(Number(process.env.TOOL_STATUS));',
  ].join('\n')+'\n');chmodSync(tool,0o700);
  writeFixture(protoc,'#!'+process.execPath+'\nconsole.log("libprotoc 35.0");\n');chmodSync(protoc,0o700);
  for(const status of ['0','23']){
   checkFixedWork(work,{create:true});
   writeFixture(calls,'');
   const env={...process.env,PLATFORM:'windows',PRODUCT_WORK_DIR:work,PRODUCT_BASH_BIN:process.env.PRODUCT_TEST_SHELL||'/bin/bash',CARGO:tool,RUSTC:tool,PROTOC:protoc,CALLS:calls,TOOL_STATUS:status,WASM_FILE:'forbidden'};
   if(status==='0')await runEmbeddedBuild('local',['windows',join(work,'work')],env);
   else await assert.rejects(runEmbeddedBuild('local',['windows',join(work,'work')],env),/产品工具执行失败/);
   const data=readFileSync(calls,'utf8').trim().split('\n').map(s=>{try{return JSON.parse(s);}catch{throw Error('Cargo夹具调用记录不是完整JSON，状态 '+status+'：'+JSON.stringify(s));}});
   assert.equal(data.length,status==='0'?2:1);
   assert.equal(data[0].target,join(work,'work/cargo-target'));assert.equal(data[0].wasm,undefined);
   assert.ok(data[0].args.includes('check'));if(status==='0')assert.ok(data[1].args.includes('--no-run'));
  }
  checkFixedWork(work,{create:true});
  writeFixture(calls,'');writeFixture(protoc,'#!'+process.execPath+'\nconsole.log("libprotoc 34.0");\n');
  await assert.rejects(runEmbeddedBuild('local',['windows',join(work,'work')],{PRODUCT_WORK_DIR:work,PRODUCT_BASH_BIN:process.env.PRODUCT_TEST_SHELL||'/bin/bash',CARGO:tool,RUSTC:tool,PROTOC:protoc,CALLS:calls}),/产品工具执行失败/);
  assert.equal(readFileSync(calls,'utf8'),'');
  await assert.rejects(runEmbeddedBuild('local',['windows',root],{PRODUCT_WORK_DIR:work}));
 }finally{removeFixture(work,{recursive:true,force:true});rmSync(inputs,{recursive:true,force:true});}
});
})();}

// 实际根源码可复制到自有target，target不会递归进入工程；外部源码的嵌套输出仍拒绝。
if(inlineTestEntry){void(async()=>{const {test}=await import('node:test'),{default:assert}=await import('node:assert/strict'),fs=await import('node:fs');test('实际根源码复制到本轮target且不复制工具现场',()=>{const work=fs.mkdtempSync(join(testRoot(),'own-view-'));try{const view=join(work,'project');createView(root,view);assert.equal(fs.existsSync(join(view,'target')),false);assert.equal(fs.existsSync(join(view,'.git')),false);assert.deepEqual(fs.readFileSync(join(view,'icons/logo.png')),fs.readFileSync(join(root,'icons/logo.png')));assert.throws(()=>createView(root,join(root,'icons/forbidden-view')),/边界/);}finally{removeFixture(work,{recursive:true,force:true});}});})();}

// 供给模式只接收所属任务资源，通道失败不切换为独立准备。

// 本产品公开GitHub接口只使用普通参数与返回值；平台流程及终态处理归本仓。
const automationWorkflows=Object.freeze({'linux-arm':'linuxarm.yml','linux-amd':'linuxamd.yml','macos':'macos.yml','windows':'windows.yml','wasm':'wasm.yml'});
const automationRepository='crcfrcn/citizenchain';
const automationTransport=new (await import('node:async_hooks')).AsyncLocalStorage();
function automationRoute(route,action){
 const workflow=automationWorkflows[route?.platform];
 if(!workflow||route.repository!=='citizenchain'||route.product!=='citizenchain'
  ||route.workflow!==workflow||route.canonicalId!==`citizenchain.${route.platform}.release`
  )
  throw Error('公民链自动化目标或授权动作不匹配');
 return workflow;
}

async function automationRequest(path,token,{method='GET',body}={}){
 const lifecycle=/^actions\/(?:workflows\/(?:linuxarm\.yml|linuxamd\.yml|macos\.yml|windows\.yml|wasm\.yml)\/dispatches|runs\/[1-9]\d*(?:\/(?:jobs(?:\?per_page=100&page=[1-9]\d*)?|cancel|force-cancel))?)$/u.test(path);
 const historyRead=method==='GET'&&/^(?:actions\/workflows\/(?:linuxarm\.yml|linuxamd\.yml|macos\.yml|windows\.yml|wasm\.yml)\/runs\?branch=main&event=workflow_dispatch&per_page=100&page=[1-9]\d*|releases\?per_page=100&page=[1-9]\d*|git\/ref\/tags\/citizenchain-[A-Za-z0-9.-]+)$/u.test(path);
 if(!lifecycle&&!historyRead)
  throw Error('公民链 GitHub 请求目标无效');
 const response=await (automationTransport.getStore()?.fetchImpl??fetch)(`https://api.github.com/repos/${automationRepository}/${path}`,{
  method,redirect:'error',credentials:'omit',headers:{Authorization:`Bearer ${token}`,
   Accept:'application/vnd.github+json','Content-Type':'application/json','X-GitHub-Api-Version':'2026-03-10'},
  ...(body===undefined?{}:{body:JSON.stringify(body)}),signal:AbortSignal.timeout(30000)});
 if(!response.ok)throw Error(`公民链 GitHub 请求失败：HTTP ${response.status}`);
 if(response.status===202||response.status===204)return {};
 if(!response.body)throw Error('公民链 GitHub 回执缺失');
 let size=0;const chunks=[];
 for await(const chunk of response.body){size+=chunk.length;if(size>8*1024*1024)throw Error('公民链 GitHub 回执超限');chunks.push(chunk);}
 return JSON.parse(new TextDecoder('utf-8',{fatal:true}).decode(Buffer.concat(chunks)));
}
function automationCheckedRun(run,route,expected){
 const workflow=automationRoute(route);
 if(!Number.isSafeInteger(run?.id)||run.id!==expected||run.path!==`.github/workflows/${workflow}`
  ||run.event!=='workflow_dispatch'||run.head_branch!=='main'
  ||run.repository?.full_name!==automationRepository||!/^[0-9a-f]{40}$/u.test(run.head_sha||'')
  ||run.html_url!==`https://github.com/${automationRepository}/actions/runs/${expected}`)
  throw Error('公民链 GitHub Run 身份不符');
 return run;
}
// 整个Run结束后只读验收产品历史；删除仍仅由本平台既有YAML内的产品收尾执行。
export async function verifyGithubHistory(platform,source,token){
 const route={repository:'citizenchain',product:'citizenchain',platform,workflow:automationWorkflows[platform],canonicalId:`citizenchain.${platform}.release`};
 const workflow=automationRoute(route);
 automationCheckedRun(source,route,source?.id);
 if(source.status!=='completed'||source.conclusion!=='success'||!Number.isSafeInteger(source.run_attempt)||source.run_attempt<1)
  throw Error('产品历史验收必须等待原Run成功终态');
 const pages=async(path,field)=>{
  const rows=[];
  for(let page=1;page<=100;page++){
   const value=await automationRequest(path+(path.includes('?')?'&':'?')+'per_page=100&page='+page,token);
   const items=field?value?.[field]:value;
   if(!Array.isArray(items)||items.length>100)throw Error('产品历史验收分页无效');
   rows.push(...items);if(items.length<100)return rows;
  }
  throw Error('产品历史验收分页超限');
 };
 const runs=await pages(`actions/workflows/${workflow}/runs?branch=main&event=workflow_dispatch`,'workflow_runs');
 const settled=[],ids=new Set();
 for(const run of runs){
  automationCheckedRun(run,route,run?.id);
  if(ids.has(run.id)||!Number.isSafeInteger(run.run_attempt)||run.run_attempt<1)throw Error('产品历史Run重复或attempt无效');
  ids.add(run.id);if(run.status==='completed')settled.push(run);
 }
 const successes=settled.filter(run=>run.conclusion==='success'),failures=settled.filter(run=>run.conclusion!=='success');
 if(successes.length!==1||successes[0].id!==source.id||successes[0].head_sha!==source.head_sha
  ||successes[0].run_attempt!==source.run_attempt||failures.length>1)throw Error('产品历史未收敛为本平台最新一成功一失败');
 const versionPattern=platform==='wasm'?'[1-9][0-9]*':'(?:0|[1-9][0-9]*)\\.(?:0|[1-9][0-9]*)\\.(?:0|[1-9][0-9]*)';
 const pattern=new RegExp('^citizenchain-'+platform+'-v('+versionPattern+')-r([1-9][0-9]*)-a([1-9][0-9]*)$','u');
 const releases=(await pages('releases')).filter(release=>pattern.test(release?.tag_name||'')&&release.draft===false&&release.prerelease===false);
 if(releases.length!==1)throw Error('产品历史没有本平台唯一正式Release');
 const release=releases[0],match=pattern.exec(release.tag_name);
 const lines=String(release.body||'').split('\n').filter(line=>line.startsWith('<!-- automation:')&&line.endsWith(' -->'));
 if(lines.length!==1)throw Error('产品终态Release来源证明不唯一');
 const proof=JSON.parse(lines[0].slice('<!-- automation:'.length,-' -->'.length));
 if(!Number.isSafeInteger(release.id)||release.id<1||!release.published_at||release.target_commitish!==source.head_sha
  ||Number(match[2])!==source.id||Number(match[3])!==source.run_attempt
  ||proof.schema!==1||proof.repository!==automationRepository||proof.product_id!=='citizenchain'||proof.platform!==platform
  ||proof.workflow!==`.github/workflows/${workflow}`||proof.run_id!==source.id||proof.run_attempt!==source.run_attempt
  ||proof.source_sha!==source.head_sha||proof.tag!==release.tag_name||String(proof.version)!==match[1]
  ||release.name!==(platform==='wasm'?`公民链·WASM-v${match[1]}`:nodeDeliveryName(platform,match[1])))
  throw Error('产品终态Release身份或版本不符');
 const expected=githubReleaseFiles(platform,match[1]);
 if(!Array.isArray(proof.assets)||!Array.isArray(release.assets)||proof.assets.length!==expected.length
  ||release.assets.length!==expected.length||new Set(proof.assets.map(asset=>asset.name)).size!==expected.length
  ||new Set(release.assets.map(asset=>asset.name)).size!==expected.length)throw Error('产品终态资产集合不完整');
 for(const name of expected){
  const asset=release.assets.find(value=>value.name===name),declared=proof.assets.find(value=>value.name===name);
  if(!asset||!declared||!Number.isSafeInteger(declared.size)||declared.size<1||!/^[0-9a-f]{64}$/u.test(declared.sha256||'')
   ||asset.state!=='uploaded'||asset.size!==declared.size||asset.digest!=='sha256:'+declared.sha256
   ||!matchesGithubReleaseAssetUrl(release.tag_name,name,asset.browser_download_url))throw Error('产品终态资产回读不符：'+name);
 }
 const reference=await automationRequest('git/ref/tags/'+encodeURIComponent(release.tag_name),token);
 if(reference.ref!=='refs/tags/'+release.tag_name||reference.object?.type!=='commit'||reference.object.sha!==source.head_sha)
  throw Error('产品终态Tag提交不符');
 const current=automationCheckedRun(await automationRequest('actions/runs/'+source.id,token),route,source.id);
 if(current.status!=='completed'||current.conclusion!=='success'||current.run_attempt!==source.run_attempt||current.head_sha!==source.head_sha)
  throw Error('产品终态验收期间原Run身份变化');
 return {platform,run_id:source.id,run_attempt:source.run_attempt,source_sha:source.head_sha,final_run_verified:true,
  kept_runs:settled.map(run=>run.id),kept_release:release.id,tag:release.tag_name};
}
// 本产品独立拥有此实现，通过自身公开接口执行。
export async function dispatchAutomation(platform,token,{inputs={}}={}){
 const route={repository:'citizenchain',product:'citizenchain',platform,workflow:automationWorkflows[platform],canonicalId:`citizenchain.${platform}.release`};
 const workflow=automationRoute(route);
 if(typeof token!=='string'||!token||!inputs||typeof inputs!=='object'||Array.isArray(inputs))throw Error('公民链派发参数无效');
 const value=await automationRequest(`actions/workflows/${workflow}/dispatches`,token,{method:'POST',body:{ref:'main',inputs}});
 if(!Number.isSafeInteger(value.workflow_run_id)||value.workflow_run_id<1
  ||value.html_url!==`https://github.com/${automationRepository}/actions/runs/${value.workflow_run_id}`)
  throw Error('公民链派发未返回准确 Run，禁止再次派发');
 const run=automationCheckedRun(await automationRequest(`actions/runs/${value.workflow_run_id}`,token),route,value.workflow_run_id);
 return run;
}
// 只读取当前 Run 的 Job 与终态日志，日志和下载均有限额，令牌不进入任务输出。
function automationSafeText(value,token){
 return String(value??'').split(token).join('[已脱敏]').replace(/gh[sopur]_[A-Za-z0-9_]+/gu,'[已脱敏]').slice(0,4000);
}
async function automationJobs(runId,token){
 const jobs=[];
 for(let page=1;page<=50;page++){
  const value=await automationRequest(`actions/runs/${runId}/jobs?per_page=100&page=${page}`,token);
  if(!Array.isArray(value?.jobs))throw Error('GitHub Job 分页响应无效');
  jobs.push(...value.jobs);if(value.jobs.length<100)return jobs;
 }
 throw Error('GitHub Job 页数超限');
}
async function automationLogArchive(bytes){
 if(!Buffer.isBuffer(bytes)||bytes.length>32*1024*1024)throw Error('GitHub 日志归档超限');
 const {inflateRawSync}=await import('node:zlib');let end=-1;
 for(let at=bytes.length-22;at>=Math.max(0,bytes.length-65558);at--){
  if(bytes.readUInt32LE(at)===0x06054b50){end=at;break;}
 }
 if(end<0)throw Error('GitHub 日志 ZIP 结构无效');
 const count=bytes.readUInt16LE(end+10);let offset=bytes.readUInt32LE(end+16),total=0;
 if(count>2000)throw Error('GitHub 日志文件数超限');
 const files=[];
 for(let index=0;index<count;index++){
  if(offset+46>bytes.length||bytes.readUInt32LE(offset)!==0x02014b50)throw Error('GitHub 日志目录损坏');
  const flags=bytes.readUInt16LE(offset+8),method=bytes.readUInt16LE(offset+10),packed=bytes.readUInt32LE(offset+20),size=bytes.readUInt32LE(offset+24);
  const nameLength=bytes.readUInt16LE(offset+28),extra=bytes.readUInt16LE(offset+30),comment=bytes.readUInt16LE(offset+32),local=bytes.readUInt32LE(offset+42);
  const name=bytes.subarray(offset+46,offset+46+nameLength).toString('utf8');offset+=46+nameLength+extra+comment;
  if(flags&1||![0,8].includes(method)||local+30>bytes.length||bytes.readUInt32LE(local)!==0x04034b50)throw Error('GitHub 日志压缩格式无效');
  total+=size;if(size>32*1024*1024||total>128*1024*1024)throw Error('GitHub 解压日志超限');
  const start=local+30+bytes.readUInt16LE(local+26)+bytes.readUInt16LE(local+28);
  if(start+packed>bytes.length)throw Error('GitHub 日志内容越界');
  const data=method===0?bytes.subarray(start,start+packed):inflateRawSync(bytes.subarray(start,start+packed),{maxOutputLength:size});
  if(data.length!==size)throw Error('GitHub 日志大小不一致');
  if(name.endsWith('.txt'))files.push({name,text:data.toString('utf8')});
 }
 return files;
}
async function automationReadLogs(runId,token){
 const response=await (automationTransport.getStore()?.fetchImpl??fetch)(`https://api.github.com/repos/${automationRepository}/actions/runs/${runId}/logs`,{
  redirect:'manual',credentials:'omit',headers:{Authorization:`Bearer ${token}`,Accept:'application/vnd.github+json',
   'X-GitHub-Api-Version':'2026-03-10'},signal:AbortSignal.timeout(30000)});
 if(response.status===404)return [];
 let download=response;
 if(response.status===302){
  const location=new URL(response.headers.get('location'));
  if(location.protocol!=='https:'||location.username||location.password)throw Error('GitHub 日志地址无效');
  download=await (automationTransport.getStore()?.fetchImpl??fetch)(location.href,{redirect:'error',credentials:'omit',signal:AbortSignal.timeout(60000)});
 }
 if(!download.ok||!download.body)throw Error('GitHub 日志读取失败');
 let size=0;const parts=[];
 for await(const part of download.body){size+=part.length;if(size>32*1024*1024)throw Error('GitHub 日志下载超限');parts.push(part);}
 return automationLogArchive(Buffer.concat(parts));
}








// 本产品自动化合成回归：所有网络与授权均由夹具替代，不触发真实流程。
if(inlineTestEntry){
 const {test}=await import('node:test'),{default:assert}=await import('node:assert/strict');
 const platform=Object.keys(automationWorkflows)[0],workflow=automationWorkflows[platform];
 const route={repository:'citizenchain',product:'citizenchain',platform,workflow,canonicalId:`citizenchain.${platform}.release`,title:'测试自动化'};
 const action={id:route.canonicalId,canonicalId:route.canonicalId,platform,productId:'citizenchain',flow:'release',authorizationLevel:2};
 const token='ghs_fixture',sha='a'.repeat(40),runId=123;
 const run=(conclusion='success',status='completed')=>({id:runId,run_number:7,run_attempt:1,
  path:`.github/workflows/${workflow}`,event:'workflow_dispatch',head_branch:'main',head_sha:sha,
  repository:{full_name:automationRepository},status,conclusion,
  html_url:`https://github.com/${automationRepository}/actions/runs/${runId}`});
 const json=value=>Response.json(value),empty=status=>new Response(null,{status});
 const terminalFixture=(target=platform)=>{
  const workflow=automationWorkflows[target],version=target==='wasm'?'2':'1.0.0';
  const source={...run(),path:`.github/workflows/${workflow}`};
  const tag=`citizenchain-${target}-v${version}-r${runId}-a1`;
  const assets=githubReleaseFiles(target,version).map(name=>({name,state:'uploaded',size:3,digest:'sha256:'+'b'.repeat(64),
   browser_download_url:`https://github.com/${automationRepository}/releases/download/${tag}/${encodeURIComponent(name)}`}));
  const proof={schema:1,repository:automationRepository,product_id:'citizenchain',platform:target,workflow:source.path,
   run_id:runId,run_attempt:1,source_sha:sha,tag,version,assets:assets.map(asset=>({name:asset.name,size:3,sha256:'b'.repeat(64)}))};
  const release={id:17,tag_name:tag,name:target==='wasm'?`公民链·WASM-v${version}`:nodeDeliveryName(target,version),
   target_commitish:sha,published_at:'2026-10-16T00:00:00Z',draft:false,prerelease:false,body:`<!-- automation:${JSON.stringify(proof)} -->`,assets};
  const state={source,runs:[source],releases:[release],reference:{ref:'refs/tags/'+tag,object:{type:'commit',sha}},calls:[]};
  state.fetchImpl=async(url,options)=>{
   assert.equal(options.method,'GET');state.calls.push(new URL(url).pathname);
   const response=state.response(new URL(url).pathname);if(response)return response;
   throw Error('终态夹具拒绝未知请求');
  };
  state.response=path=>{
   if(path.endsWith(`/workflows/${workflow}/runs`))return json({workflow_runs:state.runs});
   if(path.endsWith('/releases'))return json(state.releases);
   if(path.endsWith('/git/ref/tags/'+tag))return json(state.reference);
   if(path.endsWith('/runs/'+runId))return json(state.source);
   return null;
  };return state;
 };



 test('citizenchain 自动化拒绝错误仓库、平台和提交身份',()=>{
  assert.throws(()=>automationRoute({...route,repository:'other'}),/目标/u);
  assert.throws(()=>automationRoute({...route,platform:'other'}),/目标/u);
  assert.throws(()=>automationCheckedRun({...run(),head_sha:'bad'},route,runId),/身份/u);
  assert.throws(()=>automationCheckedRun({...run(),repository:{full_name:'other/repo'}},route,runId),/身份/u);
 });


 test('五平台成功终态独立只读核验记录和正式资产，首次成功可无失败记录',async()=>{
  await Promise.all(Object.keys(automationWorkflows).map(async platform=>{
   const state=terminalFixture(platform);
   const result=await automationTransport.run({fetchImpl:state.fetchImpl},()=>verifyGithubHistory(platform,state.source,token));
   assert.equal(result.platform,platform);assert.equal(result.final_run_verified,true);assert.equal(result.kept_release,17);
   assert.ok(state.calls.every(path=>!path.includes('/caches')));
  }));
 });
 test('终态验收拒绝未结束、重复成功、多个失败、资产和Tag漂移及分页错误',async()=>{
  for(const change of ['active','success','failure','asset','tag','http','page']){
   const state=terminalFixture();
   if(change==='active')state.source.status='in_progress';
   if(change==='success')state.runs.push({...state.source,id:124,html_url:`https://github.com/${automationRepository}/actions/runs/124`});
   if(change==='failure')for(const id of [121,122])state.runs.push({...state.source,id,conclusion:'failure',html_url:`https://github.com/${automationRepository}/actions/runs/${id}`});
   if(change==='asset')state.releases[0].assets[0].digest='sha256:'+'c'.repeat(64);
   if(change==='tag')state.reference.object.sha='c'.repeat(40);
   const fetchImpl=change==='http'?async()=>empty(403):change==='page'?async()=>json({workflow_runs:null}):state.fetchImpl;
   await assert.rejects(()=>automationTransport.run({fetchImpl},()=>verifyGithubHistory(platform,state.source,token)),/终态|历史|请求失败/u);
  }
 });


}

// 外部调用方使用普通参数和返回值；此公开接口不接收任务宿主。
export const automationClient=Object.freeze({automationWorkflows,automationRepository,automationTransport,automationRoute,automationRequest,automationCheckedRun,verifyGithubHistory,dispatchAutomation,automationSafeText,automationJobs,automationLogArchive,automationReadLogs});

// 合成回归只在本文件作为测试入口时注册；普通导入不要求调度器、其它仓库或外部服务。
if(process.env.NODE_TEST_CONTEXT&&process.argv.length===2&&process.argv[1]===import.meta.filename){
 const {test}=await import('node:test'),{default:assert}=await import('node:assert/strict');
 test('独立自动化接口不接收任务宿主，准确派发与错误回执只请求一次',async()=>{
  const api=automationClient,workflows=api.automationWorkflows||api.automationWorkflow;
  const platform=Object.keys(workflows)[0],workflow=workflows[platform],repository=api.automationRepository;
  const dispatch=api.dispatchAutomation||api.dispatchProductAutomation||api.dispatch;
  let dispatches=0,mode='success';
  const run={id:731,run_number:1,run_attempt:1,path:'.github/workflows/'+workflow,event:'workflow_dispatch',head_branch:'main',
   head_sha:'a'.repeat(40),repository:{full_name:repository},status:'completed',conclusion:'success',
   html_url:'https://github.com/'+repository+'/actions/runs/731'};
  const fetchImpl=async(url,options)=>{
   if(new URL(url).pathname.endsWith('/dispatches')){
    dispatches++;assert.equal(options.method,'POST');
    return mode==='ambiguous'?new Response(null,{status:204}):Response.json({workflow_run_id:731,html_url:run.html_url});
   }
   assert.ok(new URL(url).pathname.endsWith('/runs/731'));
   return Response.json(mode==='wrong'?{...run,head_sha:'invalid'}:run);
  };
  const invoke=()=>api.automationTransport?api.automationTransport.run({fetchImpl},()=>dispatch(platform,'fixture',{fetchImpl})):dispatch(platform,'fixture',{fetchImpl});
  assert.equal((await invoke()).id,731);assert.equal(dispatches,1);
  for(const failure of ['ambiguous','wrong']){mode=failure;dispatches=0;await assert.rejects(invoke);assert.equal(dispatches,1);}
 });
}

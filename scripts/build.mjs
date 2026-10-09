#!/usr/bin/env node
const directEntry = process.argv[1] === import.meta.filename && !process.execArgv.some(value => /^(?:-e|-p|--eval|--print)(?:=|$)/u.test(value));
const inlineTestEntry = directEntry && Boolean(process.env.NODE_TEST_CONTEXT) && process.argv.length === 2;
// 本产品独立拥有资源需求、工程准备与编译；公开回执仅提供验真资源，不提供执行命令。
import {spawn} from 'node:child_process';
import {checkFixedWork,clearFixedWork,finishFixedWork,fixedWork,withFixedWork,taskScope,trackWorkProcess,workEnvironment} from './target.mjs';
import {AsyncLocalStorage} from 'node:async_hooks';
import {rmSync,chmodSync,closeSync,openSync,readlinkSync,unlinkSync,copyFileSync,existsSync,lstatSync,mkdirSync,readFileSync,readdirSync,realpathSync,symlinkSync,writeFileSync} from 'node:fs';
import {dirname,isAbsolute,join,parse,relative,resolve,sep} from 'node:path';
import {fileURLToPath,pathToFileURL} from 'node:url';
import {createHash} from 'node:crypto';

const root=resolve(dirname(fileURLToPath(import.meta.url)),'..');
export const contract=JSON.parse(readFileSync(join(root,'scripts/flows.json'),'utf8'));
const product=contract.product_id, prefix=product.toUpperCase();
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
 platformContract(platform);const expected=fixedWork(scope==='test'?'test':'build');
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
 const [id,platform,flow,...extra]=String(environment.GITHUB_WORKFLOW||'').split('.');
 if(id!==product||extra.length||!Object.hasOwn(contract.platforms,platform)||!['ci','release'].includes(flow))fail('远端临时目录缺少准确产品平台流程身份');
 const temporary=temporaryRoot(platform,flow,null);
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

// 只读声明与原始锁；每个第一方Git来源必须同时匹配固定URL、40位提交和resolved-ref。
export function lockedSources() {
 const source=sourceRoot(),path=join(source,'pubspec.yaml');if(!existsSync(path))return [];
 const manifest=readFileSync(path,'utf8'),lock=readFileSync(join(source,'pubspec.lock'),'utf8'),result=[];
 for(const name of ['citizen_sdk','tatachat_sdk']) {
  const block=text=>[...text.matchAll(new RegExp('^  '+name+':\\r?\\n(?: {4,}[^\\n]*\\n|[ \\t]*\\n)+','gm'))];
  const a=block(manifest),b=block(lock);if(!a.length)continue;
  if(a.length!==1||b.length!==1)fail('Git来源记录不唯一');
  const value=(text,key)=>{const m=[...text.matchAll(new RegExp('^ +'+key+':\\s*([^\\n]+)$','gm'))];if(m.length!==1)fail('Git来源字段不唯一');return m[0][1].trim().replace(/^["']|["']$/gu,'');};
  const url=value(a[0][0],'url'),ref=value(a[0][0],'ref');
  if(!/^https:\/\/github\.com\/[a-z0-9-]+\/[a-z0-9-]+\.git$/u.test(url)||!/^[a-f0-9]{40}$/u.test(ref)
   ||value(a[0][0],'path')!=='.'||value(b[0][0],'url')!==url||value(b[0][0],'resolved-ref')!==ref||value(b[0][0],'ref')!==ref)fail('Git声明和锁不一致');
  result.push({name,url,ref});
 }return result;
}
export function requirements(platform,work) {
 checkWork(work);const declared=platformContract(platform);
 const locks=declared.locks.map(value=>({...value})),sources=lockedSources(),archives=[];
 for(const source of sources) {
  const packageRoot=join(work,'git-sources',source.name);
  if(existsSync(packageRoot)) {
   const path=source.name==='citizen_sdk'?'Cargo.lock':'native/Cargo.lock';
   locks.push({ecosystem:'cargo',path,source_package:source.name});
   if(source.name==='citizen_sdk') {
    const lock=JSON.parse(readFileSync(join(packageRoot,'scripts/dependencies.lock.json'),'utf8'));
    const p=nativePlatform(platform);
    const entries=[['zxing-cpp',lock.environment['zxing-cpp']],...((p==='LinuxARM'||p==='LinuxAMD')?Object.entries(lock.native.sources):p==='Windows'?[['sqlite',lock.native.sources.sqlite]]:[])];
    for(const [name,value]of entries)archives.push({ecosystem:'native',name,...value,group:'sdk-native'});
   }
  }
 }
 // 原生源归档坐标归本产品已有声明；准备后才提出展开源码的Cargo锁。
 for(const lock of declared.locks){const file=join(root,lock.path);if(!existsSync(file)||!lstatSync(file).isFile()||lstatSync(file).isSymbolicLink())fail('原始锁缺失或带链接：'+lock.path);}
 return {schema:1,product_id:product,platform,tools:declared.tools,locks,sources,archives};
}

export function resourceEnvironment(platform,work,receipt,base={}) {
 checkWork(work);const declared=platformContract(platform);
 if(!receipt||receipt.schema!==1||receipt.product_id!==product||receipt.platform!==platform||receipt.work!==work||receipt.offline!==true
  ||!receipt.tools||!receipt.dependencies||!receipt.archives)fail('资源回执身份无效');
 const env={HOME:base.HOME,USER:base.USER,LOGNAME:base.LOGNAME,LANG:'zh_CN.UTF-8',LC_ALL:'C',
  ...receipt.environment,TMPDIR:join(work,'tmp')+sep,TMP:join(work,'tmp'),TEMP:join(work,'tmp'),XDG_CACHE_HOME:join(work,'cache'),XDG_CONFIG_HOME:join(work,'config'),
  CARGO_TARGET_DIR:join(work,'work/cargo-target'),CARGO_NET_OFFLINE:'true',CARGO_INCREMENTAL:'1',
  npm_config_offline:'true',npm_config_audit:'false',npm_config_fund:'false'};
 const allowedEnvironment=new Set(['PRODUCT_WORK_DIR','PRODUCT_BASH_BIN','PRODUCT_RSYNC_BIN','PATH','DEVELOPER_DIR','SDKROOT','DART_EXECUTABLE','XCODEBUILD','CODESIGN','SECURITY','XCRUN','XCODE_SELECT','CC','CXX','SWIFT','OTOOL','INSTALL_NAME_TOOL','LIPO','MAKE','AR','RANLIB','NM','STRIP','LLVM_NM','LD','LDCXX','CARGO_TARGET_AARCH64_APPLE_DARWIN_LINKER','ANDROID_HOME','ANDROID_SDK_ROOT','ANDROID_NDK_HOME','ANDROID_USER_HOME','ANDROID_EMULATOR_HOME','GRADLE_INIT_SCRIPT','GRADLE_USER_HOME']);
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
 return env;
}
function checkDependency(work,path){if(!isAbsolute(path)||resolve(path)!==path||!inside(work,path)||path===work||!lstatSync(path).isDirectory()||realpathSync(path)!==path)fail('依赖回执越界或无效');}
// 工程输入复制到本轮真实目录，保证包解析与写入均不进入正式源码；内部链接映射到同轮副本。
export function createView(source,destination) {
 const ownTarget=source===root&&inside(join(root,'target'),destination)&&['build','test'].includes(relative(join(root,'target'),destination).split(sep)[0]);
 if(realpathSync(source)!==source||!lstatSync(source).isDirectory()||!isAbsolute(destination)||resolve(destination)!==destination||inside(source,destination)&&!ownTarget||inside(destination,source))fail('工程输入与输出边界无效');
 let parent=dirname(destination);while(!existsSync(parent))parent=dirname(parent);
 if(ownTarget)checkWork(join(root,'target',relative(join(root,'target'),destination).split(sep)[0]));
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
export async function build(platform,work,receipt,base) {
 const env=resourceEnvironment(platform,work,receipt,base),declared=platformContract(platform);
 await checkArchives(platform,work,receipt,true);await stageArchives(work,receipt);
 const shell=receipt.tools.bash?.path;
 if(!shell)fail('缺少显式Shell资源');
 const project=env[prefix+'_PROJECT_ROOT'];

  env.CITIZENCHAIN_BUILD_WORK_DIR=join(work,'work');env.CITIZENCHAIN_DEPENDENCY_DIR=join(work,'dependencies');env.CITIZENCHAIN_ARTIFACT_DIR=work;
  if(platform==='wasm'){
   env.WASM_BUILD_FROM_SOURCE='1';env.CITIZENCHAIN_PRODUCTION_WASM_BUILD='1';
   await run(env.CARGO,['--config',join(root,'config.toml'),'build','--locked','--offline','--release','-p','citizenchain','--no-default-features','--features','std'],env);
   const file=join(env.CARGO_TARGET_DIR,'release/wbuild/citizenchain/citizenchain.compact.compressed.wasm');
   if(!lstatSync(file).isFile()||!lstatSync(file).size||lstatSync(file).isSymbolicLink())fail('真实WASM候选缺失');
  }else await runEmbeddedBuild(platform==='macos'?'run':'local',platform==='macos'?[]:[platform,join(work,'work')],{...env,PRODUCT_BASH_BIN:shell});

 return completeBuild(platform,work,receipt,env);
}

// 每次调用拥有自己的取消和进程集合，导入API并发也不能共享执行状态。
const executions=new AsyncLocalStorage();
export async function runBuildProcess(file,args,env,cwd=root,{capture=false,input,accepted=[0],timeout=7200000,signal=executions.getStore()?.signal,passHost=false,streamError=false}={}) {
 signal?.throwIfAborted();
 return new Promise((ok,reject)=>{
  const child=spawn(file,args,{cwd,env:workEnvironment(env),detached:true,stdio:['pipe','pipe','pipe',...(passHost?[3]:[])]});
  trackWorkProcess(child.pid);
  let stdout=[],stderr=[],bytes=0,reason,settled=false;
  const stop=()=>{try{process.kill(-child.pid,'SIGTERM');}catch(error){if(error.code!=='ESRCH')reason='无法取消产品工具进程组';}};
  let killer;
  const terminate=()=>{stop();clearTimeout(killer);killer=setTimeout(()=>{try{process.kill(-child.pid,'SIGKILL');}catch{}},1500);};
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
   const alive=()=>{if(!child.pid)return false;try{process.kill(-child.pid,0);return true;}catch(error){return error.code!=='ESRCH';}};
   if(alive()){reason??='产品工具退出后仍有后代';stop();for(let n=0;n<15&&alive();n++)await new Promise(r=>setTimeout(r,100));if(alive())try{process.kill(-child.pid,'SIGKILL');}catch{};for(let n=0;n<15&&alive();n++)await new Promise(r=>setTimeout(r,100));}
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
// 摘要只读执行源码；所属根技术文档及target等运行数据不改变编译身份。
function sourceDigest() {
 const hash=createHash('sha256'),rootData=new Set(['cache','target','rely','tools','tasks','TATA.md','MAP.md','CODEX.md','CLAUDE.md','README.md','CitizenChainNode.md','CitizenChainRuntime.md','CitizenChainOnChina.md']);
 const generated=new Set(['.git','node_modules','.dart_tool','.gradle','.symlinks','Pods','build','target','ephemeral','.cache','.DS_Store']);
 function visit(path){for(const name of readdirSync(path).sort()){
  if(generated.has(name)||path===root&&rootData.has(name))continue;
  const file=join(path,name),info=lstatSync(file);hash.update(relative(root,file)+'\n');
  if(info.isDirectory())visit(file);else if(info.isFile()){hash.update(String(Boolean(info.mode&0o111)));hash.update(readFileSync(file));}
  else if(info.isSymbolicLink()){const real=realpathSync(file);if(!inside(root,real))fail('产品源码链接越界');hash.update(readlinkSync(file));}
  else fail('产品源码特殊输入未声明');
 }}visit(root);return hash.digest('hex');
}

// 宿主完整Build先由调用方消费回执、安装并收尾；独立执行由本产品清空现场。
export async function execute(platform,work,request={},options={}) {
 checkWork(work);
 return withFixedWork(taskScope(work),()=>executeTask(platform,work,request,options),{environment:options.environment||process.env,retain:request.resource_mode==='provided'||(options.environment||process.env).PRODUCT_HOST_FD==='3'});
}
async function executeTask(platform,work,request={},options={}) {
 checkWork(work);platformContract(platform);
 if(!inside(productTarget(platform),work)||work===productTarget(platform))fail('执行工作根与当前产品平台不一致');
 options.signal?.throwIfAborted();
 if(!request||typeof request!=='object'||Array.isArray(request)||Object.keys(request).some(k=>!['schema','product_id','platform','work','run_id','program_digest'].includes(k))
  ||request.schema!==undefined&&request.schema!==1||request.run_id!==undefined&&!/^[1-9][0-9]{8}$/u.test(request.run_id)||request.program_digest!==undefined&&!/^[a-f0-9]{64}$/u.test(request.program_digest)
  ||request.product_id!==undefined&&request.product_id!==product||request.platform!==undefined&&request.platform!==platform||request.work!==undefined&&request.work!==work)fail('公开Build请求身份或字段无效');
 chmodSync(work,0o700);
 const lock=join(work,'.product-build.lock'),resultFile=join(work,'build-result.json');
 if(existsSync(resultFile))fail('本轮完整Build已有结果，禁止复用旧终态');
 const handle=openSync(lock,'wx',0o600);closeSync(handle);
 const cancellation=new AbortController(),abort=()=>cancellation.abort();options.signal?.addEventListener('abort',abort,{once:true});if(options.signal?.aborted)abort();
 const state={signal:cancellation.signal,cancellation,host:options.host,unconfirmed:false,finished:false};
 try{return await executions.run(state,async()=>{
  const initial=sourceDigest(),stages=options.stages||{requirements,resources:(...args)=>import('./resources.mjs').then(m=>m.resources(...args)),prepare,build};
  const unchanged=()=>{state.signal.throwIfAborted();if(sourceDigest()!==initial)fail('产品源码或锁在执行期间改变');};
  const resourcesOptions={signal:state.signal,offline:Boolean(options.offline),environment:options.environment||process.env};
  await stages.requirements(platform,work);unchanged();
  let receipt=await stages.resources(platform,work,request,resourcesOptions);unchanged();
  await stages.prepare(platform,work,receipt,resourcesOptions.environment);unchanged();
  await stages.requirements(platform,work);
  receipt=await stages.resources(platform,work,receipt,resourcesOptions);unchanged();
  const result=await stages.build(platform,work,receipt,resourcesOptions.environment);unchanged();
  checkBuildResult(result,platform,work,request.run_id);
  writeFileSync(resultFile,JSON.stringify(result)+'\n',{flag:'wx',mode:0o600});return result;
 });}catch(error){if(String(error?.message).includes('退出未确认'))state.unconfirmed=true;throw error;}finally{state.finished=true;state.socket?.destroy();options.signal?.removeEventListener('abort',abort);if(!state.unconfirmed){unlinkSync(lock);if(request.resource_mode!=='provided'&&(options.environment||process.env).PRODUCT_HOST_FD!=='3')clearWork(work);}}
}
export function checkBuildResult(value,platform,work,runId) {
 const declared=platformContract(platform);
 if(!value||Object.keys(value).sort().join(',')!==(runId?'completion,files,platform,product_id,run_id,schema,work':'completion,files,platform,product_id,schema,work')
  ||value.schema!==1||value.product_id!==product||value.platform!==platform||value.work!==work||value.completion!==declared.completion
  ||runId&&value.run_id!==runId||!Array.isArray(value.files)||value.files.length!==declared.files.length)fail('完整Build结果身份或完成方式无效');
 for(let n=0;n<value.files.length;n++){const entry=value.files[n],file=join(work,declared.files[n]);
  if(Object.keys(entry).sort().join(',')!=='path,sha256'||entry.path!==file||!inside(work,file)||realpathSync(file)!==file||!/^[a-f0-9]{64}$/u.test(entry.sha256)||outputDigest(file)!==entry.sha256)fail('完整Build产物摘要或边界无效');}
 return value;
}
async function completeBuild(platform,work,receipt,env) {
 const declared=platformContract(platform);
 if(declared.completion==='device-install')fail('本产品未声明设备安装实现');
 if(declared.completion==='macos-artifact')for(const name of declared.files)await run(env.CODESIGN,['--verify','--deep','--strict',join(work,name)],env);
 const result={schema:1,product_id:product,platform,work,completion:declared.completion,
  files:declared.files.map(name=>{const path=join(work,name);if(!inside(work,path)||realpathSync(path)!==path)fail('Build候选越界');return {path,sha256:outputDigest(path)};})};
 if(receipt.run_id)result.run_id=receipt.run_id;return checkBuildResult(result,platform,work,receipt.run_id);
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
  const {editorResources}=await import('./resources.mjs');await editorResources(work,{objects,signal:cancellation.signal});
  const {localDocTypeLines}=await import('./docs.mjs');
  writeFileSync(join(work,'editor/node/frontend/local-docs.generated.d.ts'),[...localDocTypeLines,'export declare const LOCAL_DOCS: readonly LocalDoc[];',''].join('\n'));
  console.log('公民链编辑器类型已就绪；会话关闭后清空 target/test');
  while(alive()&&!cancellation.signal.aborted)await new Promise(resolve=>{const timer=setTimeout(done,1000);function done(){clearTimeout(timer);cancellation.signal.removeEventListener('abort',done);resolve();}cancellation.signal.addEventListener('abort',done,{once:true});});
 });}finally{for(const name of ['SIGTERM','SIGINT'])process.removeListener(name,cancel);}
}
async function runCLI(){
 if(process.argv[2]==='editor')return editorCommand(process.argv.slice(3));
 const [operation,,flag,work]=process.argv.slice(2);
 if(['execute','resources','prepare','build'].includes(operation)&&flag==='--work'){
  checkWork(work);
  return withFixedWork(taskScope(work),()=>runCommand(),{environment:process.env,retain:process.env.PRODUCT_HOST_FD==='3'||process.env.PRODUCT_RESOURCE_FD==='4'});
 }
 return runCommand();
}
async function runCommand(){
 const [command,platform,option,work,...extra]=process.argv.slice(2);
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
 checkWork(work);
 if(command==='requirements')process.stdout.write(JSON.stringify(requirements(platform,work))+'\n');
 else{
  const cancellation=new AbortController();for(const name of ['SIGTERM','SIGINT'])process.once(name,()=>cancellation.abort());
  let input='';for await(const chunk of process.stdin){input+=chunk;if(Buffer.byteLength(input)>2*1024*1024)fail('公开输入超限');}
  const request=input?JSON.parse(input):{},options={environment:process.env,signal:cancellation.signal,offline:extra.includes('--offline')};
  let result;
  if(command==='execute'){
   const {bootstrapNode}=await import('./resources.mjs');const node=await bootstrapNode(work,options);
   if(createHash('sha256').update(readFileSync(process.execPath)).digest('hex')!==createHash('sha256').update(readFileSync(node.path)).digest('hex')){
    const environment=Object.fromEntries(['HOME','USER','LOGNAME','LANG','LC_ALL','PRODUCT_TOOL_ROOT','PRODUCT_DEPENDENCY_ROOT','PRODUCT_HOST_FD','PRODUCT_WORK_LEASE'].filter(k=>typeof process.env[k]==='string').map(k=>[k,process.env[k]]));
    result=JSON.parse((await runBuildProcess(node.path,[fileURLToPath(import.meta.url),command,platform,option,work,...extra],workEnvironment(environment),root,{capture:true,streamError:true,input:JSON.stringify(request),signal:cancellation.signal,passHost:environment.PRODUCT_HOST_FD==='3'})).stdout);
   }else result=await execute(platform,work,request,options);
  }else if(command==='resources')result=await (await import('./resources.mjs')).resources(platform,work,request,options);
  else result=await executions.run({signal:cancellation.signal},()=>command==='prepare'?prepare(platform,work,request,process.env):build(platform,work,request,process.env));
  process.stdout.write(JSON.stringify(result)+'\n');
 }
}
}

// CLI拒绝必须真实失败，不能留成未完成顶层await或输出成功回执。

import {mkdtempSync as createEmbeddedDirectory} from 'node:fs';
export const BUILD_SOURCES=Object.freeze({"run":"#!/usr/bin/env bash\n# CitizenChain节点本机Build入口：只构建、签名验真并写入产品产物目录。\n# 本入口不启动或停止节点，不修改链数据；启动由产品独立Start入口负责。\n# 测试、手工调试和清库不走本入口。\nset -euo pipefail\n\nMACOS_APP_BUNDLE=''\nMACOS_APP_PENDING=0\n\ncleanup() {\n    local result=\"${1:-1}\"\n    trap - EXIT INT TERM HUP\n    # 唯一成功路径会撤销 trap；其余提前退出必须失败，避免 Bash 参数展开错误被清理返回值吞掉。\n    [[ \"$result\" != 0 ]] || result=1\n    # 只清理本轮尚未通过完整签名验收的固定 App；历史成功归档和用户数据均不触碰。\n    if [[ \"${MACOS_APP_PENDING:-0}\" == 1 \\\n        && -n \"${TARGET_DIR:-}\" \\\n        && \"${MACOS_APP_BUNDLE:-}\" == \"$TARGET_DIR/release/bundle/macos/citizenchain.app\" ]]; then\n        rm -rf -- \"$MACOS_APP_BUNDLE\"\n    fi\n    # Build 不启动节点或 PG；校验失败不得停止其它任务或用户已运行的实例。\n    exit \"$result\"\n}\ntrap 'cleanup \"$?\"' EXIT\ntrap 'cleanup 130' INT\ntrap 'cleanup 143' TERM\ntrap 'cleanup 129' HUP\n\n# macOS 产品 App 只接受今后唯一的新团队 Developer ID；禁止按枚举顺序选证书，\n# 否则Apple Development或其它团队身份可能被静默当成产品正式签名。\nMACOS_SIGNING_IDENTITY='Developer ID Application: WEI CHENG (MHYMVRN6FC)'\nMACOS_TEAM_ID='MHYMVRN6FC'\nMACOS_BUNDLE_ID='macOS.citizenappchain'\nMACOS_APPLICATION_ID=\"${MACOS_TEAM_ID}.${MACOS_BUNDLE_ID}\"\nMACOS_PROFILE_NAME='CitizenChain Developer ID'\nMACOS_PROFILE_DIR=\"$HOME/Library/Developer/Xcode/UserData/Provisioning Profiles\"\n\nrequire_macos_signing_identity() {\n    /usr/bin/security find-identity -v -p codesigning 2>/dev/null \\\n        | grep -F \"\\\"$MACOS_SIGNING_IDENTITY\\\"\" >/dev/null\n}\n\n# Apple 安全时间戳服务偶发不可用时只重试签名相关步骤。错误不属于该服务、\n# 或三次重试仍失败时立即关闭，不允许改用无时间戳、临时签名等降级路径。\nMACOS_TIMESTAMP_RETRY_DELAYS=(2 5 10)\n\nis_macos_timestamp_service_failure() {\n    local output=\"$1\"\n    [[ \"$output\" == *\"A timestamp was expected but was not found\"* \\\n        || \"$output\" == *\"The timestamp service is not available\"* \\\n        || \"$output\" == *\"timestamp service is not available\"* ]]\n}\n\nrun_with_macos_timestamp_retry() {\n    local action=\"$1\"\n    shift\n    local output='' status=0 retry=0 delay\n    while true; do\n        set +e\n        output=\"$(\"$@\" 2>&1)\"\n        status=$?\n        set -e\n        [[ -z \"$output\" ]] || printf '%s\\n' \"$output\"\n        [[ \"$status\" == 0 ]] && return 0\n        is_macos_timestamp_service_failure \"$output\" || return \"$status\"\n        if (( retry >= ${#MACOS_TIMESTAMP_RETRY_DELAYS[@]} )); then\n            echo \"    [error] $action 因 Apple 安全时间戳服务异常连续重试三次后仍失败\" >&2\n            return \"$status\"\n        fi\n        delay=\"${MACOS_TIMESTAMP_RETRY_DELAYS[$retry]}\"\n        retry=$((retry + 1))\n        echo \"    [warn] $action 未取得 Apple 安全时间戳，${delay} 秒后执行第 $retry 次重试\" >&2\n        sleep \"$delay\"\n    done\n}\n\n# Apple 重新签发描述文件会改变 UUID，因此禁止绑定文件名。只允许唯一一个同时匹配\n# 产品名、Team ID 和公民链 App ID 的 Developer ID 描述文件，重复或旧标识均失败关闭。\nresolve_macos_profile() {\n    local candidate profile_name application_id team_id matched='' matched_count=0\n    shopt -s nullglob\n    for candidate in \"$MACOS_PROFILE_DIR\"/*.provisionprofile \"$MACOS_PROFILE_DIR\"/*.mobileprovision; do\n        profile_name=\"$(security cms -D -i \"$candidate\" 2>/dev/null \\\n            | plutil -extract Name raw -o - - 2>/dev/null || true)\"\n        application_id=\"$(security cms -D -i \"$candidate\" 2>/dev/null \\\n            | plutil -extract 'Entitlements.com\\.apple\\.application-identifier' raw -o - - 2>/dev/null || true)\"\n        team_id=\"$(security cms -D -i \"$candidate\" 2>/dev/null \\\n            | plutil -extract 'Entitlements.com\\.apple\\.developer\\.team-identifier' raw -o - - 2>/dev/null || true)\"\n        if [[ \"$profile_name\" == \"$MACOS_PROFILE_NAME\" \\\n            && \"$application_id\" == \"$MACOS_APPLICATION_ID\" \\\n            && \"$team_id\" == \"$MACOS_TEAM_ID\" ]]; then\n            matched=\"$candidate\"\n            matched_count=$((matched_count + 1))\n        fi\n    done\n    shopt -u nullglob\n    [[ \"$matched_count\" == 1 ]] || {\n        echo \"    [error] 必须且只能安装一个匹配 $MACOS_BUNDLE_ID 的 $MACOS_PROFILE_NAME\" >&2\n        return 1\n    }\n    printf '%s\\n' \"$matched\"\n}\n\nSCRIPT_DIR=\"$CITIZENCHAIN_SOURCE_ROOT/scripts\"\nREPO_ROOT=\"$CITIZENCHAIN_SOURCE_ROOT\"\nCITIZENCHAIN_ROOT=\"$CITIZENCHAIN_SOURCE_ROOT\"\n# 所有独立入口的工具临时状态归本产品target；宿主已交付的产品工作根继续归当前任务。\nPRODUCT_TEMP_SOURCE=\"$CITIZENCHAIN_SOURCE_ROOT\"\nPRODUCT_TARGET_TEMP_ROOT=\"${CITIZENCHAIN_WORK_DIR:-$CITIZENCHAIN_SOURCE_ROOT/target/build}\"\nCITIZENCHAIN_WORK_DIR=\"${CITIZENCHAIN_WORK_DIR:-${TMPDIR:-$PRODUCT_TARGET_TEMP_ROOT}/citizenchain/macos}\"\nBUILD_WORK_DIR=\"${CITIZENCHAIN_BUILD_WORK_DIR:-$CITIZENCHAIN_WORK_DIR/work}\"\nCITIZENCHAIN_DEPENDENCY_DIR=\"${CITIZENCHAIN_DEPENDENCY_DIR:-$CITIZENCHAIN_WORK_DIR/dependencies}\"\nTARGET_DIR=\"$BUILD_WORK_DIR/cargo-target\"\nexport CARGO_TARGET_DIR=\"$TARGET_DIR\"\nNODE_FRONTEND_DIST=\"$CITIZENCHAIN_WORK_DIR/node-frontend\"\nONCHINA_BUILD_DIST=\"$CITIZENCHAIN_WORK_DIR/onchina-frontend/dist\"\nPACKAGE_RESOURCES=\"$CITIZENCHAIN_WORK_DIR/resources\"\n# Build只形成当前调用的候选App；正式Release与Publish由产品发布流程另行验真。\nARTIFACT_DIR=\"${CITIZENCHAIN_ARTIFACT_DIR:-$CITIZENCHAIN_WORK_DIR/artifacts}\"\n\n# 产品自行使用当前环境中的工具，并按自己的锁文件在源码外工作目录准备依赖。\nsource \"$PRODUCT_PREPARE_SHELL\"\n\n# 本机Build脚本只使用当前工作区源码构建 runtime WASM，禁止接受外部 WASM 覆盖。\nunset WASM_FILE\n# Cargo/Tauri 的 release profile 只是本机优化配置；gmb.dev 是本机开发数据隔离环境。\n# 本任务不修改正式gmb数据，也不与正式安装版争用RocksDB。\nexport CITIZENCHAIN_DATA_PROFILE=dev\nmkdir -p \"$TARGET_DIR\" \"$npm_config_cache\" \"$PACKAGE_RESOURCES/onchina-bin\" \"$PACKAGE_RESOURCES/onchina-frontend\"\n\n# ── OnChina 控制台本机配置 ──\n# 启动节点不需要任何机构鉴权/身份。这里只让本机能跑起链上中国平台服务:\n#   ① 构建 onchina 二进制(节点同目录,设置页手动启动时由 onchina_proc 拉起)+ 前端产物;\n#   ② DB 用内嵌私有 PG(方案 A):借本机 PostgreSQL 二进制起一个 onchina 专属实例(127.0.0.1)。\n# 本机构的\"系统签名钥 / 机构身份\"是可选配置(签登录 QR / 签发凭证才需要),非启动前提。\necho \"==> 构建 OnChina 本机优化二进制 + 前端...\"\n( cd \"$REPO_ROOT\" && CARGO_INCREMENTAL=1 cargo build --locked --release -p onchina --config \"$REPO_ROOT/config.toml\" )\necho \"==> 构建链上中国平台前端产物...\"\n( cd \"$ONCHINA_FRONTEND_PROJECT\" && ONCHINA_FRONTEND_DIST=\"$ONCHINA_BUILD_DIST\" npm run build )\necho \"==> 构建节点前端产物...\"\n( cd \"$NODE_FRONTEND_PROJECT\" && CITIZENCHAIN_FRONTEND_DIST=\"$NODE_FRONTEND_DIST\" npm run build )\ncp \"$TARGET_DIR/release/onchina\" \"$PACKAGE_RESOURCES/onchina-bin/onchina\"\ncp -R \"$ONCHINA_BUILD_DIST\" \"$PACKAGE_RESOURCES/onchina-frontend/dist\"\nPG_PREFIX=\"\"\nfor v in postgresql@17 postgresql@16 postgresql@15 postgresql; do\n    if p=\"$(brew --prefix \"$v\" 2>/dev/null)\" && [ -x \"$p/bin/initdb\" ]; then PG_PREFIX=\"$p\"; break; fi\ndone\nif [ -n \"$PG_PREFIX\" ]; then\n    export ONCHINA_EMBEDDED_PG=1\n    export ONCHINA_PG_BIN_DIR=\"$PG_PREFIX/bin\"\n    export ONCHINA_PG_PORT=\"${ONCHINA_PG_PORT:-5433}\"\n    export ONCHINA_PG_DATA_DIR=\"$HOME/Library/Application Support/gmb.dev/onchina-pgdata\"\n    echo \"    内嵌私有 PG:$ONCHINA_PG_BIN_DIR(端口 $ONCHINA_PG_PORT)\"\nelse\n    echo \"    [warn] 未找到本机 PostgreSQL(brew install postgresql@16);链上中国平台仍可起但缺 DB,功能受限。\"\nfi\nexport ONCHINA_CHINA_DB=\"$REPO_ROOT/onchina/src/codes/china.sqlite\"\nexport ONCHINA_FRONTEND_DIST=\"$ONCHINA_BUILD_DIST\"\nexport ONCHINA_ENABLE_TLS=1\nexport ONCHINA_TLS_DIR=\"$HOME/Library/Application Support/gmb.dev/onchina-tls\"\n# 公权机构目录只允许从链上投影到本地缓存;开发启动不再打开旧本地生成开关。\n# 链不可达或投影不可读时,链上中国按 fail-closed 不放行平台服务。\n# OnChina 后端不再持有任何链上签名钥:机构操作全部由管理员冷钱包直接冷签,\n# 原平台签名钥与注销凭证签发配置已随注销凭证链路整体删除。\n\necho \"==> 使用当前工作区源码构建本机 runtime WASM...\"\necho \"    节点Build产物目录: $TARGET_DIR\"\necho \"    本机运行数据目录: $HOME/Library/Application Support/gmb.dev\"\necho \"==> 链上中国平台:节点设置页点击「启动」后访问 https://onchina.local:8964\"\n\n# ── 构建与签名封装 ──\ncd \"$REPO_ROOT/node\"\necho \"==> 构建公民链...\"\nif [[ \"$(uname -s)\" == \"Darwin\" ]]; then\n    require_macos_signing_identity || {\n        echo \"    [error] 未找到唯一允许的新团队 Developer ID：$MACOS_SIGNING_IDENTITY\" >&2\n        exit 1\n    }\n    export APPLE_SIGNING_IDENTITY=\"$MACOS_SIGNING_IDENTITY\"\n    macos_profile=\"$(resolve_macos_profile)\"\n    echo \"    使用新团队 Developer ID 构建带摄像头权限的本机优化 App\"\n    # 使用当前任务按原始 lockfile 安装的 CLI，不能读取源码 node_modules 或全局 CLI。\n    app_bundle=\"$TARGET_DIR/release/bundle/macos/citizenchain.app\"\n    MACOS_APP_BUNDLE=\"$app_bundle\"\n    # Tauri 2 的 build 默认使用优化 profile；--debug 才会切换为调试产物。\n    # 编译与封装分离，时间戳瞬时失败时只重试封装签名，不重复整轮 Rust 编译。\n    # 前端已在私有工程完成构建；清除 Tauri 的源码 npm 钩子，避免重复构建或回写主仓。\n    tauri_override=\"$(python3 -c 'import json,sys; print(json.dumps({\"build\":{\"beforeBuildCommand\":None,\"frontendDist\":sys.argv[1]},\"bundle\":{\"resources\":{sys.argv[2]+\"/\":\"\",sys.argv[3]+\"/\":\"icons/\",sys.argv[4]:\"china.sqlite\"}}}))' \"$NODE_FRONTEND_DIST\" \"$PACKAGE_RESOURCES\" \"$REPO_ROOT/icons\" \"$REPO_ROOT/onchina/src/codes/china.sqlite\")\"\n    CITIZENCHAIN_FRONTEND_DIST=\"$NODE_FRONTEND_DIST\" CARGO_INCREMENTAL=1 \\\n        node \"$NODE_FRONTEND_PROJECT/node_modules/@tauri-apps/cli/tauri.js\" build --config \"$tauri_override\" \\\n        --no-bundle --ci -- --locked --config \"$REPO_ROOT/config.toml\"\n    MACOS_APP_PENDING=1\n    bundle_macos_app() {\n        rm -rf -- \"$app_bundle\"\n        CITIZENCHAIN_FRONTEND_DIST=\"$NODE_FRONTEND_DIST\" CARGO_INCREMENTAL=1 \\\n            node \"$NODE_FRONTEND_PROJECT/node_modules/@tauri-apps/cli/tauri.js\" bundle --config \"$tauri_override\" \\\n            --bundles app --ci\n    }\n    run_with_macos_timestamp_retry \"Tauri App 封装签名\" bundle_macos_app\n\n    app_plist=\"$app_bundle/Contents/Info.plist\"\n    app_executable=\"$app_bundle/Contents/MacOS/citizenchain\"\n    [[ -x \"$app_executable\" ]] || {\n        echo \"    [error] Tauri 构建完成但缺少 App 主程序：$app_executable\" >&2\n        exit 1\n    }\n    # Tauri 负责生成产品 App；描述文件属于本机签名材料，绝不进入仓库或其它产物路径。\n    # 嵌入后必须重新封签根 App，使描述文件、唯一 App ID 与权限成为同一个签名整体。\n    /usr/bin/ditto \"$macos_profile\" \"$app_bundle/Contents/embedded.provisionprofile\"\n    run_with_macos_timestamp_retry \"描述文件嵌入后的 App 封签\" \\\n        /usr/bin/codesign --force --deep --options runtime --timestamp \\\n        --entitlements \"$REPO_ROOT/node/Entitlements.plist\" \\\n        --sign \"$MACOS_SIGNING_IDENTITY\" \"$app_bundle\"\n    codesign --verify --deep --strict \"$app_bundle\"\n    signature_details=\"$(codesign -dv --verbose=4 \"$app_bundle\" 2>&1)\"\n    grep -Fqx \"Authority=$MACOS_SIGNING_IDENTITY\" <<<\"$signature_details\" || {\n        echo \"    [error] macOS App 未使用唯一允许的新团队 Developer ID\" >&2\n        exit 1\n    }\n    grep -Fqx \"TeamIdentifier=$MACOS_TEAM_ID\" <<<\"$signature_details\" || {\n        echo \"    [error] macOS App Team ID 不属于唯一允许的新团队\" >&2\n        exit 1\n    }\n    grep -Eq '^CodeDirectory .*flags=.*\\(runtime\\)' <<<\"$signature_details\" || {\n        echo \"    [error] macOS App 未启用 Hardened Runtime\" >&2\n        exit 1\n    }\n    grep -Eq '^Timestamp=' <<<\"$signature_details\" || {\n        echo \"    [error] macOS App 缺少安全时间戳\" >&2\n        exit 1\n    }\n    [[ \"$(/usr/libexec/PlistBuddy -c 'Print :CFBundleIdentifier' \"$app_plist\")\" == \"$MACOS_BUNDLE_ID\" ]] || {\n        echo \"    [error] macOS App Bundle ID 与 Tauri 配置不一致\" >&2\n        exit 1\n    }\n    embedded_profile_name=\"$(security cms -D -i \"$app_bundle/Contents/embedded.provisionprofile\" 2>/dev/null \\\n        | plutil -extract Name raw -o - - 2>/dev/null || true)\"\n    [[ \"$embedded_profile_name\" == \"$MACOS_PROFILE_NAME\" ]] || {\n        echo \"    [error] macOS App 未嵌入 $MACOS_PROFILE_NAME\" >&2\n        exit 1\n    }\n    /usr/libexec/PlistBuddy -c 'Print :NSCameraUsageDescription' \"$app_plist\" >/dev/null\n    signed_entitlements=\"$(codesign -d --entitlements :- \"$app_bundle\" 2>/dev/null)\"\n    for entitlement in \\\n        com.apple.security.device.camera \\\n        com.apple.security.cs.allow-jit \\\n        com.apple.security.cs.allow-unsigned-executable-memory; do\n        # plutil 把点号解释为字典层级；entitlement 名本身含点号，必须先转义为单个键。\n        entitlement_key_path=\"${entitlement//./\\\\.}\"\n        [[ \"$(printf '%s' \"$signed_entitlements\" | plutil -extract \"$entitlement_key_path\" raw -)\" == \"true\" ]] || {\n            echo \"    [error] macOS App 签名缺少 $entitlement\" >&2\n            exit 1\n        }\n    done\n    get_task_allow=\"$(printf '%s' \"$signed_entitlements\" \\\n        | plutil -extract 'com\\.apple\\.security\\.get-task-allow' raw - 2>/dev/null \\\n        || printf 'false')\"\n    [[ \"$get_task_allow\" == \"false\" ]] || {\n        echo \"    [error] macOS 本机优化 App 禁止 get-task-allow\" >&2\n        exit 1\n    }\n    MACOS_APP_PENDING=0\n    echo \"    本机优化路径、新团队签名、Bundle ID、Hardened Runtime、安全时间戳与 entitlement 校验通过\"\n    # 成功后只保存本次缓存根的候选 App；不得触碰正式产物库。\n    mkdir -p \"$ARTIFACT_DIR\"\n    rm -rf \"$ARTIFACT_DIR/CitizenChain.app\"\n    mv \"$app_bundle\" \"$ARTIFACT_DIR/CitizenChain.app\"\n    app_bundle=\"$ARTIFACT_DIR/CitizenChain.app\"\n    app_executable=\"$app_bundle/Contents/MacOS/citizenchain\"\n    export ONCHINA_FRONTEND_DIST=\"$app_bundle/Contents/Resources/onchina-frontend/dist\"\n    # Build只生成并验真产品产物，不终止旧实例，也不启动新实例。\n    trap - EXIT INT TERM HUP\n    echo \"    CitizenChain Node macOS候选产物构建完成；Build不会启动节点\"\nelse\n    echo \"    [error] 本入口只负责macOS Build；其它平台使用各自产品构建入口\" >&2\n    exit 1\nfi\n","local":"#!/usr/bin/env bash\n# 保留原非macOS检查与编译矩阵；输出只进入本轮build或test。\nset -euo pipefail\n[[ $# -eq 2 && \"$2\" == /* ]] || { echo \"构建参数无效\" >&2; exit 2; }\ncase \"$1\" in windows|linux-arm|linux-amd) ;; *) exit 2;; esac\nroot=\"${CITIZENCHAIN_SOURCE_ROOT:?}\"; work=\"$2\"\n[[ \"$work\" == \"$root/target/build/\"* || \"$work\" == \"$root/target/test/\"* ]] || { echo \"工作根越界\" >&2; exit 1; }\n[[ -x \"${CARGO:?}\" && -x \"${RUSTC:?}\" ]] || exit 1\nexport CARGO_TARGET_DIR=\"$work/cargo-target\"\nunset WASM_FILE\n\"$CARGO\" check --manifest-path \"$root/Cargo.toml\" --locked --release -p node --no-default-features --features std --all-targets\n\"$CARGO\" test --manifest-path \"$root/Cargo.toml\" --locked --release -p node --no-default-features --features std --no-run\n","prepack":"#!/usr/bin/env bash\n# Card 05 打包前置(macOS / Linux):把 onchina 二进制 + 前端产物 + china.sqlite +\n# PostgreSQL官方二进制组装到产品源码外资源目录。冻结plain chainspec已由\n# node 二进制 include_bytes! 内嵌；安装包不复制 258 MB 创世 RocksDB，首启按同一\n# chainspec 本地物化并由节点守卫校验块 0。之后在 node/ 跑 `npm run tauri build`。\n#\n# 用法:\n#   export CITIZENCHAIN_PG_DIST=<postgresql.org 官方二进制解压目录(含 bin/lib/share)>\n#   node citizenchain/scripts/build.mjs prepack\nset -euo pipefail\n\nSCRIPT_DIR=\"$CITIZENCHAIN_SOURCE_ROOT/scripts\"\nROOT=\"$CITIZENCHAIN_SOURCE_ROOT\"\nHERE=\"$ROOT/node\"                             # citizenchain/node\n# 所有独立入口的工具临时状态归本产品target；宿主已交付的产品工作根继续归当前任务。\nPRODUCT_TEMP_SOURCE=\"$CITIZENCHAIN_SOURCE_ROOT\"\nPRODUCT_TARGET_TEMP_ROOT=\"${CITIZENCHAIN_WORK_DIR:-$CITIZENCHAIN_SOURCE_ROOT/target/build}\"\nCITIZENCHAIN_WORK_DIR=\"${CITIZENCHAIN_PREPACK_WORK_DIR:-${TMPDIR:-$PRODUCT_TARGET_TEMP_ROOT}/citizenchain/prepack}\"\nCITIZENCHAIN_DEPENDENCY_DIR=\"${CITIZENCHAIN_DEPENDENCY_DIR:-$CITIZENCHAIN_WORK_DIR/dependencies}\"\nexport CITIZENCHAIN_WORK_DIR CITIZENCHAIN_DEPENDENCY_DIR\nexport CARGO_TARGET_DIR=\"${CARGO_TARGET_DIR:-$CITIZENCHAIN_WORK_DIR/cargo-target}\"\nPACKAGE_RESOURCES=\"${CITIZENCHAIN_PACKAGE_RESOURCES_DIR:-$CITIZENCHAIN_WORK_DIR/resources}\"\nONCHINA_BUILD_DIST=\"$CITIZENCHAIN_WORK_DIR/onchina-frontend/dist\"\nsource \"$PRODUCT_PREPARE_SHELL\"\ncase \"$(uname -s)\" in\n  Darwin) OS=macos ;;\n  Linux) OS=linux ;;\n  *) OS=linux ;;\nesac\n\necho \"[prepack] build onchina (release)\"\n( cd \"$ROOT\" && cargo build -p onchina --release --config \"$ROOT/config.toml\" )\n\necho \"[prepack] build onchina frontend\"\n( cd \"$ONCHINA_FRONTEND_PROJECT\" && ONCHINA_FRONTEND_DIST=\"$ONCHINA_BUILD_DIST\" npm run build )\n\necho \"[prepack] assemble node/resources\"\nmkdir -p \"$PACKAGE_RESOURCES/onchina-bin\" \"$PACKAGE_RESOURCES/onchina-frontend\" \"$PACKAGE_RESOURCES/postgres\"\n# onchina 二进制随包(Tauri resources/onchina-bin),onchina_proc 从资源目录解析(见 node/src/onchina_proc)。\ncp \"$CARGO_TARGET_DIR/release/onchina\" \"$PACKAGE_RESOURCES/onchina-bin/onchina\"\nchmod +x \"$PACKAGE_RESOURCES/onchina-bin/onchina\"\nrm -rf \"$PACKAGE_RESOURCES/onchina-frontend/dist\"\ncp -R \"$ONCHINA_BUILD_DIST\" \"$PACKAGE_RESOURCES/onchina-frontend/dist\"\n\n# PostgreSQL 官方二进制(postgresql.org):把已解压的 PG 安装目录(含 bin/lib/share)\n# 指向 CITIZENCHAIN_PG_DIST,脚本拷进 resources/postgres/$OS;未提供则告警(安装包将缺内嵌 PG)。\nif [ -n \"${CITIZENCHAIN_PG_DIST:-}\" ] && [ -d \"$CITIZENCHAIN_PG_DIST/bin\" ]; then\n  rm -rf \"$PACKAGE_RESOURCES/postgres/$OS\"\n  mkdir -p \"$PACKAGE_RESOURCES/postgres/$OS\"\n  cp -R \"$CITIZENCHAIN_PG_DIST/.\" \"$PACKAGE_RESOURCES/postgres/$OS/\"\n  echo \"[prepack] PostgreSQL 已组装($OS)\"\nelse\n  echo \"[prepack][warn] 未提供 CITIZENCHAIN_PG_DIST。\"\n  echo \"                请从 https://www.postgresql.org/download/ 取本平台官方二进制(含 bin/lib/share),\"\n  echo \"                解压后 export CITIZENCHAIN_PG_DIST=<解压目录> 再重跑;否则安装包不含内嵌 PG。\"\nfi\n\n# 中文注释：release 状态包只作为正式创世审计制品保留在 target/build/chainspec，不进入任一\n# 平台安装包；清掉旧预打包残留，保证本机 prepack 与 GitHub CI 使用同一轻量合同。\nrm -rf \"$PACKAGE_RESOURCES/genesis-state\"\necho \"[prepack] 已确认安装包不携带 genesis-state；首启按冻结 plain chainspec 本地物化\"\n\necho \"[prepack] 完成：资源目录 $PACKAGE_RESOURCES\"\n","chainspec":"#!/usr/bin/env bash\n# 烘焙 CitizenChain 冻结 chainspec(plain 形态,ADR-031 D5)。\n#\n# 当前创世只直铸国家/省/市公权机构;镇级和新增机构运行期注册上链。\n# 冻结 SSOT 为 plain JSON(runtime WASM + genesis patch + bootnodes)。脚本启动临时节点物化块 0,\n# 同时导出用于正式创世审计的 genesis-state 链数据库包;CitizenApp/smoldot 用 stateRootHash 轻形态。\n#\n# 默认模式只生成预览文件到 target/build/chainspec,不覆盖冻结 SSOT。\n# 正式创世必须在 GitHub WASM CI 成功后执行:\n#   citizenchain/scripts/build.mjs chainspec --finalize \\\n#     --wasm /path/to/citizenchain.compact.compressed.wasm \\\n#     --wasm-ci-run-id <RUN_ID> --wasm-ci-head-sha <HEAD_SHA>\n#\n# 正式模式会同步:\n#   1. citizenchain/node/citizenchain.json   (节点冻结 SSOT)\n#   2. citizenapp/assets/chainspec.json                        (smoldot 轻形态:stateRootHash)\n#   3. citizenapp/assets/light_sync_state.json                 (smoldot checkpoint)\n#   4. citizenapp/assets/public_institutions/*.json            (块 0 公权机构缓存)\n#   5. citizenserve/scripts/wrangler.toml             (公开链身份派生配置)\n#\n# 流程:导出 plain spec → 临时节点物化创世(记录耗时)→ RPC 宪法创世检查\n#       → 读块 0 头生成轻形态与 lightSyncState → 从同一块生成公权机构缓存\n#       → 导出 genesis-state → 全部校验后 finalize 同步。\nset -euo pipefail\n\nSCRIPT_DIR=\"$CITIZENCHAIN_SOURCE_ROOT/scripts\"\nCHAIN_ROOT=\"$CITIZENCHAIN_SOURCE_ROOT\"\nREPO_ROOT=\"$CITIZENCHAIN_SOURCE_ROOT\"\nHOST_TEMP=\"$(node \"$CHAIN_ROOT/scripts/build.mjs\" temporary-root '' macos)\" || exit 1\nexport CARGO_TARGET_DIR=\"$HOST_TEMP/bake-chainspec/cargo-target\"\nexport TMPDIR=\"$HOST_TEMP/bake-chainspec/tmp\"\nmkdir -p \"$TMPDIR\"\nOUT=\"$CHAIN_ROOT/target/build/chainspec/citizenchain.json\"\nAPP_OUT=\"$CHAIN_ROOT/target/build/chainspec/chainspec.app.json\"\nAPP_LIGHT_SYNC_STATE_OUT=\"$CHAIN_ROOT/target/build/chainspec/light_sync_state.json\"\nAPP_PUBLIC_INSTITUTION_OUT=\"$CHAIN_ROOT/target/build/chainspec/public_institutions\"\nCLOUDFLARE_WRANGLER_OUT=\"$CHAIN_ROOT/target/build/chainspec/wrangler.toml\"\nGENESIS_STATE_OUT=\"$CHAIN_ROOT/target/build/chainspec/genesis-state\"\nFINALIZE=0\nSKIP_CHECK=0\nWASM_FILE_ARG=\"\"\nWASM_CI_RUN_ID=\"\"\nWASM_CI_HEAD_SHA=\"\"\nRPC_PORT=19944\n# 中文注释：临时节点必须经明确配置的可信 HTTPS 入口读取；不拼造不存在的本机 TLS 服务。\nRPC_URL=\"${CITIZENCHAIN_RPC_URL:-}\"\npython3 - \"$RPC_URL\" <<'TLS_RPC'\nimport sys\nfrom urllib.parse import urlsplit\nurl = urlsplit(sys.argv[1])\nif url.scheme != \"https\" or not url.hostname or url.username or url.password or url.fragment:\n    raise SystemExit(\"CITIZENCHAIN_RPC_URL 必须配置可信 HTTPS 入口\")\nTLS_RPC\n\nvalidate_genesis_state_package() {\n    local package_root=\"$1\" require_release=\"$2\" path relative\n    [[ -f \"$package_root/manifest.json\" ]] || { echo \"错误:创世状态包缺少 manifest.json:$package_root\" >&2; return 1; }\n    [[ -d \"$package_root/chains/citizenchain/db\" ]] || { echo \"错误:创世状态包缺少链数据库:$package_root\" >&2; return 1; }\n\n    # 正式包不得携带临时节点生成的 TLS、network、keystore 或日志目录；只允许清单和链数据库。\n    while IFS= read -r -d '' path; do\n        relative=\"${path#\"$package_root\"/}\"\n        if [[ -L \"$path\" ]]; then\n            echo \"错误:创世状态包禁止符号链接:$relative\" >&2\n            return 1\n        fi\n        case \"$relative\" in\n            manifest.json|chains|chains/citizenchain|chains/citizenchain/db|chains/citizenchain/db/*) ;;\n            *)\n                echo \"错误:创世状态包包含白名单外残留:$relative\" >&2\n                return 1\n                ;;\n        esac\n    done < <(find \"$package_root\" -mindepth 1 -print0)\n\n    python3 - \"$package_root/manifest.json\" <<'PYEOF'\nimport json\nimport sys\n\nmanifest_path = sys.argv[1]\nwith open(manifest_path, encoding=\"utf-8\") as f:\n    manifest = json.load(f)\nrequired = (\n    \"package_format\", \"chain_id\", \"genesis_hash\", \"state_root\", \"chainspec_hash\",\n    \"runtime_wasm_hash\", \"light_sync_state_hash\", \"public_institution_root\",\n    \"artifact_stage\",\n)\nmissing = [key for key in required if not manifest.get(key)]\nif missing:\n    raise SystemExit(f\"创世状态包 manifest 缺少字段:{','.join(missing)}\")\nif manifest[\"package_format\"] != \"citizenchain-genesis-state\":\n    raise SystemExit(\"创世状态包 manifest.package_format 无效\")\nif manifest[\"chain_id\"] != \"citizenchain\":\n    raise SystemExit(\"创世状态包 manifest.chain_id 无效\")\nif manifest.get(\"included_paths\") != [\"chains/citizenchain/db\"]:\n    raise SystemExit(\"创世状态包 manifest.included_paths 必须精确等于 chains/citizenchain/db\")\nif manifest[\"artifact_stage\"] not in (\"preview\", \"release\"):\n    raise SystemExit(\"创世状态包 manifest.artifact_stage 无效\")\nPYEOF\n\n    if [[ \"$require_release\" == \"1\" ]]; then\n        python3 - \"$package_root/manifest.json\" <<'PYEOF'\nimport json\nimport sys\n\nwith open(sys.argv[1], encoding=\"utf-8\") as f:\n    manifest = json.load(f)\nif manifest.get(\"artifact_stage\") != \"release\":\n    raise SystemExit(\"正式创世状态包 artifact_stage 必须为 release\")\nif not str(manifest.get(\"runtime_wasm_ci_run_id\", \"\")).isdigit():\n    raise SystemExit(\"正式创世状态包 runtime_wasm_ci_run_id 无效\")\nhead_sha = manifest.get(\"runtime_wasm_ci_head_sha\")\nif not isinstance(head_sha, str) or len(head_sha) != 40 or any(c not in \"0123456789abcdef\" for c in head_sha):\n    raise SystemExit(\"正式创世状态包 runtime_wasm_ci_head_sha 无效\")\nPYEOF\n    fi\n}\n\nusage() {\n    cat <<'EOF'\nUsage:\n  citizenchain/scripts/build.mjs chainspec [--out FILE] [--skip-check]\n  citizenchain/scripts/build.mjs chainspec --finalize --wasm FILE --wasm-ci-run-id ID --wasm-ci-head-sha SHA [--out FILE]\n\nOptions:\n  --out FILE       生成 plain chainspec 的输出路径。默认 citizenchain/target/build/chainspec/citizenchain.json\n  --genesis-state-out DIR\n                   生成已物化创世链状态包的输出目录。默认 citizenchain/target/build/chainspec/genesis-state\n  --wasm FILE      GitHub WASM CI 产出的 runtime wasm。正式创世必须提供\n  --wasm-ci-run-id ID\n                   该 WASM artifact 所属 GitHub Actions run id\n  --wasm-ci-head-sha SHA\n                   该 WASM artifact 所属提交 SHA\n  --finalize       同步节点/App/公权机构缓存/Cloudflare 的全部冻结派生产物\n  --skip-check     跳过宪法创世检查。只用于排障,正式创世不得使用\n  -h, --help       显示帮助\nEOF\n}\n\nwhile (($#)); do\n    case \"$1\" in\n        --out)\n            OUT=\"${2:?--out 需要文件路径}\"\n            shift 2\n            ;;\n        --genesis-state-out)\n            GENESIS_STATE_OUT=\"${2:?--genesis-state-out 需要目录路径}\"\n            shift 2\n            ;;\n        --wasm)\n            WASM_FILE_ARG=\"${2:?--wasm 需要 wasm 文件路径}\"\n            shift 2\n            ;;\n        --wasm-ci-run-id)\n            WASM_CI_RUN_ID=\"${2:?--wasm-ci-run-id 需要 run id}\"\n            shift 2\n            ;;\n        --wasm-ci-head-sha)\n            WASM_CI_HEAD_SHA=\"${2:?--wasm-ci-head-sha 需要提交 SHA}\"\n            shift 2\n            ;;\n        --finalize)\n            FINALIZE=1\n            shift\n            ;;\n        --skip-check)\n            SKIP_CHECK=1\n            shift\n            ;;\n        -h|--help)\n            usage\n            exit 0\n            ;;\n        *)\n            echo \"未知参数: $1\" >&2\n            usage >&2\n            exit 2\n            ;;\n    esac\ndone\n\nif [[ \"$FINALIZE\" == \"1\" && -z \"$WASM_FILE_ARG\" ]]; then\n    echo \"错误: --finalize 必须同时提供 --wasm FILE,确保 :code 来自已通过 CI 的 WASM。\" >&2\n    exit 2\nfi\nif [[ \"$FINALIZE\" == \"1\" && ( -z \"$WASM_CI_RUN_ID\" || -z \"$WASM_CI_HEAD_SHA\" ) ]]; then\n    echo \"错误: --finalize 必须提供 --wasm-ci-run-id 与 --wasm-ci-head-sha,记录 CI artifact 来源。\" >&2\n    exit 2\nfi\nif [[ \"$FINALIZE\" == \"1\" && \"$SKIP_CHECK\" == \"1\" ]]; then\n    echo \"错误: 正式 --finalize 禁止 --skip-check。\" >&2\n    exit 2\nfi\nif [[ -n \"$WASM_CI_RUN_ID\" && ! \"$WASM_CI_RUN_ID\" =~ ^[0-9]+$ ]]; then\n    echo \"错误: --wasm-ci-run-id 必须为纯数字。\" >&2\n    exit 2\nfi\nif [[ -n \"$WASM_CI_HEAD_SHA\" && ! \"$WASM_CI_HEAD_SHA\" =~ ^[0-9a-f]{40}$ ]]; then\n    echo \"错误: --wasm-ci-head-sha 必须为 40 位小写十六进制提交 SHA。\" >&2\n    exit 2\nfi\n\nif [[ -n \"$WASM_FILE_ARG\" ]]; then\n    if [[ ! -s \"$WASM_FILE_ARG\" ]]; then\n        echo \"错误: WASM 文件不存在或为空: $WASM_FILE_ARG\" >&2\n        exit 2\n    fi\n    WASM_FILE=\"$(cd \"$(dirname \"$WASM_FILE_ARG\")\" && pwd)/$(basename \"$WASM_FILE_ARG\")\"\n    export WASM_FILE\n    unset WASM_BUILD_FROM_SOURCE\n    echo \"==> 使用指定 WASM_FILE: $WASM_FILE\"\nelse\n    export WASM_BUILD_FROM_SOURCE=1\n    unset WASM_FILE\n    echo \"==> 未指定 --wasm,仅做本地预览:从源码构建 runtime WASM\"\nfi\n\nmkdir -p \"$(dirname \"$OUT\")\" \"$CHAIN_ROOT/target/build/chainspec\"\nTMP=\"$(mktemp \"$CHAIN_ROOT/target/build/chainspec/.citizenchain.plain.XXXXXX.json\")\"\nNODE_TMP_DIR=\"$(mktemp -d \"$CHAIN_ROOT/target/build/chainspec/.bakenode.XXXXXX\")\"\nNODE_PID=\"\"\ncleanup() {\n    [[ -n \"$NODE_PID\" ]] && kill \"$NODE_PID\" 2>/dev/null || true\n    rm -f \"$TMP\"\n    rm -rf \"$NODE_TMP_DIR\"\n}\ntrap cleanup EXIT\n\necho \"==> 导出 fresh plain chainspec...\"\n(\n    cd \"$CHAIN_ROOT\"\n    cargo run --config \"$CHAIN_ROOT/config.toml\" -p node -- export-chain-spec --chain citizenchain-fresh > \"$TMP\"\n)\n\nrpc() {\n    # RPC 轮询必须有限时；解析错误交给调用点决定是否继续等待或立即失败。\n    curl -fsS --max-time 10 --proto '=https' --proto-redir '=https' -H 'content-type: application/json' \\\n        -d \"{\\\"jsonrpc\\\":\\\"2.0\\\",\\\"id\\\":1,\\\"method\\\":\\\"$1\\\",\\\"params\\\":$2}\" \\\n        \"$RPC_URL\" | python3 -c '\nimport json\nimport sys\n\ndata = json.load(sys.stdin)\nerror = data.get(\"error\")\nif error:\n    raise SystemExit(f\"RPC error: {error}\")\nif data.get(\"result\") is None:\n    raise SystemExit(\"RPC result is null\")\nprint(json.dumps(data[\"result\"], ensure_ascii=False))\n'\n}\n\necho \"==> 启动临时节点物化创世(国家/省/市公权机构,记录耗时)...\"\nGENESIS_T0=$(date +%s)\n(\n    cd \"$CHAIN_ROOT\"\n    CITIZENCHAIN_HEADLESS=1 \"$CARGO_TARGET_DIR/debug/citizenchain\" --chain \"$TMP\" \\\n        --base-path \"$NODE_TMP_DIR\" --rpc-port \"$RPC_PORT\" \\\n        --no-mdns --no-prometheus --no-telemetry \\\n        >\"$NODE_TMP_DIR/node.log\" 2>&1\n) &\nNODE_PID=$!\n\nGENESIS_HASH=\"null\"\nfor _ in $(seq 1 120); do\n    sleep 5\n    if ! kill -0 \"$NODE_PID\" 2>/dev/null; then\n        echo \"错误: 临时节点提前退出,日志尾部:\" >&2\n        tail -20 \"$NODE_TMP_DIR/node.log\" >&2\n        exit 1\n    fi\n    GENESIS_HASH=$(rpc chain_getBlockHash '[0]' 2>/dev/null || echo null)\n    [[ \"$GENESIS_HASH\" != \"null\" && -n \"$GENESIS_HASH\" ]] && break\ndone\nif [[ \"$GENESIS_HASH\" == \"null\" || -z \"$GENESIS_HASH\" ]]; then\n    echo \"错误: 10 分钟内未完成创世物化,日志尾部:\" >&2\n    tail -20 \"$NODE_TMP_DIR/node.log\" >&2\n    exit 1\nfi\nGENESIS_SECS=$(( $(date +%s) - GENESIS_T0 ))\nGENESIS_HASH_STR=$(echo \"$GENESIS_HASH\" | tr -d '\"')\nSTATE_ROOT=$(rpc chain_getHeader \"[$GENESIS_HASH]\" | python3 -c 'import sys,json;print(json.loads(sys.stdin.read())[\"stateRoot\"])')\necho \"==> 创世物化完成: 耗时 ${GENESIS_SECS}s, genesis=$GENESIS_HASH_STR, stateRoot=$STATE_ROOT\"\n\nif [[ \"$SKIP_CHECK\" != \"1\" ]]; then\n    echo \"==> 检查宪法创世与冻结条件(RPC 模式)...\"\n    CHECK_ARGS=(\"$SCRIPT_DIR/check-constitution-genesis.py\" --rpc \"$RPC_URL\" --at \"$GENESIS_HASH_STR\")\n    if [[ -n \"$WASM_FILE_ARG\" ]]; then\n        CHECK_ARGS+=(--expect-code-file \"$WASM_FILE\")\n    fi\n    python3 \"${CHECK_ARGS[@]}\"\nelse\n    echo \"==> 已跳过宪法创世检查(--skip-check)\"\nfi\n\necho \"==> 生成 CitizenApp 轻形态 chainspec(stateRootHash)...\"\npython3 - \"$TMP\" \"$APP_OUT\" \"$STATE_ROOT\" <<'PYEOF'\nimport json, sys\nplain_path, app_path, state_root = sys.argv[1], sys.argv[2], sys.argv[3]\nplain = json.load(open(plain_path))\n# 轻形态:去掉 runtimeGenesis(完整 state 不进 App),只留 stateRootHash;\n# smoldot 据此自建创世头,校验后续区块。\napp = {k: plain[k] for k in\n       (\"name\", \"id\", \"chainType\", \"bootNodes\", \"telemetryEndpoints\",\n        \"protocolId\", \"properties\", \"codeSubstitutes\") if k in plain}\napp[\"genesis\"] = {\"stateRootHash\": state_root}\njson.dump(app, open(app_path, \"w\"), ensure_ascii=False, indent=2)\nprint(f\"    {app_path}\")\nPYEOF\n\necho \"==> 生成 CitizenApp lightSyncState checkpoint...\"\nLIGHT_SYNC_STATE_JSON=\"$(rpc sync_state_genLightSyncState '[]')\"\npython3 - \"$APP_LIGHT_SYNC_STATE_OUT\" \"$LIGHT_SYNC_STATE_JSON\" <<'PYEOF'\nimport json\nimport re\nimport sys\n\nout_path, raw = sys.argv[1], sys.argv[2]\nlss = json.loads(raw)\nrequired = (\"finalizedBlockHeader\", \"grandpaAuthoritySet\")\nmissing = [key for key in required if key not in lss]\nif missing:\n    raise SystemExit(f\"lightSyncState 缺少字段:{','.join(missing)}\")\nfor key in required:\n    value = lss[key]\n    if not isinstance(value, str) or not re.fullmatch(r\"0x[0-9a-fA-F]+\", value):\n        raise SystemExit(f\"lightSyncState.{key} 必须为 0x 十六进制字符串\")\nwith open(out_path, \"w\", encoding=\"utf-8\") as f:\n    json.dump(lss, f, ensure_ascii=False, indent=2)\n    f.write(\"\\n\")\nprint(f\"    {out_path}\")\nPYEOF\n\necho \"==> 从同一块 0 生成 CitizenApp 公权机构缓存...\"\nrm -rf \"$APP_PUBLIC_INSTITUTION_OUT\"\nnode \"$REPO_ROOT/citizenapp/scripts/generate_public_institution_bundle.mjs\" \\\n    --rpc-url \"$RPC_URL\" \\\n    --at \"$GENESIS_HASH_STR\" \\\n    --chainspec \"$APP_OUT\" \\\n    --out-dir \"$APP_PUBLIC_INSTITUTION_OUT\" \\\n    --chain-id citizenchain\n\nPUBLIC_INSTITUTION_ROOT=\"$(python3 - \"$APP_PUBLIC_INSTITUTION_OUT\" \"$APP_OUT\" \"$GENESIS_HASH_STR\" \"$STATE_ROOT\" <<'PYEOF'\nimport hashlib\nimport json\nimport os\nimport sys\n\nbundle_dir, app_spec_path, genesis_hash, state_root = sys.argv[1:]\nmanifest_path = os.path.join(bundle_dir, \"manifest.json\")\nwith open(manifest_path, encoding=\"utf-8\") as f:\n    manifest = json.load(f)\n\ndef sha256_file(path):\n    h = hashlib.sha256()\n    with open(path, \"rb\") as f:\n        for chunk in iter(lambda: f.read(1024 * 1024), b\"\"):\n            h.update(chunk)\n    return h.hexdigest()\n\nexpected_manifest_fields = {\n    \"chain_id\", \"snapshot_block_number\", \"snapshot_block_hash\",\n    \"genesis_hash\", \"state_root\", \"chainspec_hash\", \"public_institution_root\",\n    \"version\", \"shard_hashes\", \"provinces\",\n}\nif manifest.get(\"chain_id\") != \"citizenchain\" or set(manifest) != expected_manifest_fields:\n    raise SystemExit(\"公权机构 manifest 身份无效\")\nif manifest.get(\"snapshot_block_number\") != 0:\n    raise SystemExit(\"创世公权机构缓存必须钉死块 0\")\nif str(manifest.get(\"snapshot_block_hash\", \"\")).lower() != genesis_hash.lower():\n    raise SystemExit(\"公权机构 snapshot_block_hash 与创世哈希不一致\")\nif str(manifest.get(\"genesis_hash\", \"\")).lower() != genesis_hash.lower():\n    raise SystemExit(\"公权机构 genesis_hash 与创世哈希不一致\")\nif str(manifest.get(\"state_root\", \"\")).lower() != state_root.lower():\n    raise SystemExit(\"块 0 公权机构 state_root 与创世状态根不一致\")\nif manifest.get(\"chainspec_hash\") != sha256_file(app_spec_path):\n    raise SystemExit(\"公权机构 chainspec_hash 与本次轻形态 chainspec 不一致\")\n\nprovinces = manifest.get(\"provinces\")\nif not isinstance(provinces, list) or len(provinces) != 43:\n    raise SystemExit(\"创世公权机构缓存必须精确包含 43 个省级分片\")\nfor item in provinces:\n    province_name = item.get(\"province_name\")\n    shard_path = os.path.join(bundle_dir, f\"{province_name}.json\")\n    if not os.path.isfile(shard_path):\n        raise SystemExit(f\"公权机构分片缺失:{province_name}\")\n    if item.get(\"shard_hash\") != sha256_file(shard_path):\n        raise SystemExit(f\"公权机构分片哈希不一致:{province_name}\")\nroot_json = json.dumps(provinces, ensure_ascii=False, separators=(\",\", \":\"))\ncomputed_root = hashlib.sha256(root_json.encode()).hexdigest()\nif manifest.get(\"public_institution_root\") != computed_root:\n    raise SystemExit(\"公权机构 public_institution_root 校验失败\")\nprint(computed_root)\nPYEOF\n)\"\necho \"==> 公权机构缓存根: $PUBLIC_INSTITUTION_ROOT\"\n\necho \"==> 暂存 Cloudflare 公开链身份配置...\"\npython3 - \"$REPO_ROOT/citizenserve/scripts/wrangler.toml\" \"$CLOUDFLARE_WRANGLER_OUT\" \"$GENESIS_HASH_STR\" \"$STATE_ROOT\" <<'PYEOF'\nimport os\nimport re\nimport sys\n\nsource_path, out_path, genesis_hash, state_root = sys.argv[1:]\nwith open(source_path, encoding=\"utf-8\") as f:\n    text = f.read()\ntext, genesis_count = re.subn(\n    r'^(\\s*CHAIN_GENESIS_HASH\\s*=\\s*)\"[^\"]*\"\\s*$',\n    lambda match: f'{match.group(1)}\"{genesis_hash}\"',\n    text,\n    flags=re.MULTILINE,\n)\ntext, state_count = re.subn(\n    r'^(\\s*CHAIN_STATE_ROOT\\s*=\\s*)\"[^\"]*\"\\s*$',\n    lambda match: f'{match.group(1)}\"{state_root}\"',\n    text,\n    flags=re.MULTILINE,\n)\nif genesis_count == 0 or state_count == 0 or genesis_count != state_count:\n    raise SystemExit(\"Cloudflare wrangler.toml 链身份配置数量异常\")\nos.makedirs(os.path.dirname(out_path), exist_ok=True)\nwith open(out_path, \"w\", encoding=\"utf-8\") as f:\n    f.write(text)\nprint(f\"    {out_path} ({genesis_count} 个环境)\")\nPYEOF\n\nkill \"$NODE_PID\" 2>/dev/null || true\nwait \"$NODE_PID\" 2>/dev/null || true\nNODE_PID=\"\"\n\necho \"==> 生成创世链状态包(供节点安装包首启直接复制链数据库)...\"\nrm -rf \"$GENESIS_STATE_OUT\"\nmkdir -p \"$GENESIS_STATE_OUT/chains/citizenchain\"\nif [[ ! -d \"$NODE_TMP_DIR/chains/citizenchain/db\" ]]; then\n    echo \"错误: 临时节点未生成 chains/citizenchain/db,无法制作创世链状态包。\" >&2\n    find \"$NODE_TMP_DIR\" -maxdepth 4 -type d | sort >&2\n    exit 1\nfi\ncp -a \"$NODE_TMP_DIR/chains/citizenchain/db\" \"$GENESIS_STATE_OUT/chains/citizenchain/db\"\nARTIFACT_STAGE=\"preview\"\n[[ \"$FINALIZE\" == \"1\" ]] && ARTIFACT_STAGE=\"release\"\npython3 - \"$GENESIS_STATE_OUT/manifest.json\" \"$GENESIS_HASH_STR\" \"$STATE_ROOT\" \"$TMP\" \"${WASM_FILE:-}\" \"$APP_LIGHT_SYNC_STATE_OUT\" \"$PUBLIC_INSTITUTION_ROOT\" \"$GENESIS_SECS\" \"$WASM_CI_RUN_ID\" \"$WASM_CI_HEAD_SHA\" \"$ARTIFACT_STAGE\" <<'PYEOF'\nimport datetime\nimport hashlib\nimport json\nimport os\nimport sys\n\nmanifest_path, genesis_hash, state_root, chainspec_path, wasm_path, light_sync_state_path, public_institution_root, secs, wasm_ci_run_id, wasm_ci_head_sha, artifact_stage = sys.argv[1:]\n\ndef sha256_file(path):\n    if not path or not os.path.isfile(path):\n        return \"\"\n    h = hashlib.sha256()\n    with open(path, \"rb\") as f:\n        for chunk in iter(lambda: f.read(1024 * 1024), b\"\"):\n            h.update(chunk)\n    return h.hexdigest()\n\ndef sha256_runtime_code(path):\n    with open(path, encoding=\"utf-8\") as f:\n        spec = json.load(f)\n    code = spec.get(\"genesis\", {}).get(\"runtimeGenesis\", {}).get(\"code\", \"\")\n    if not isinstance(code, str) or not code.startswith(\"0x\"):\n        return \"\"\n    return hashlib.sha256(bytes.fromhex(code[2:])).hexdigest()\n\nmanifest = {\n    \"package_format\": \"citizenchain-genesis-state\",\n    \"chain_id\": \"citizenchain\",\n    \"artifact_stage\": artifact_stage,\n    \"snapshot_block_number\": 0,\n    \"snapshot_block_hash\": genesis_hash,\n    \"genesis_hash\": genesis_hash,\n    \"state_root\": state_root,\n    \"chainspec_hash\": sha256_file(chainspec_path),\n    \"runtime_wasm_hash\": sha256_file(wasm_path) or sha256_runtime_code(chainspec_path),\n    \"runtime_wasm_ci_run_id\": wasm_ci_run_id,\n    \"runtime_wasm_ci_head_sha\": wasm_ci_head_sha,\n    \"light_sync_state_hash\": sha256_file(light_sync_state_path),\n    \"public_institution_root\": public_institution_root,\n    \"genesis_materialization_secs\": int(secs),\n    \"included_paths\": [\"chains/citizenchain/db\"],\n    \"generated_at\": datetime.datetime.now(datetime.timezone.utc).isoformat(),\n}\nwith open(manifest_path, \"w\", encoding=\"utf-8\") as f:\n    json.dump(manifest, f, ensure_ascii=False, indent=2)\n    f.write(\"\\n\")\nprint(f\"    {manifest_path}\")\nPYEOF\n\nvalidate_genesis_state_package \"$GENESIS_STATE_OUT\" \"$FINALIZE\"\necho \"==> 创世链状态包白名单校验通过:仅包含 manifest.json 与 chains/citizenchain/db\"\n\nmv \"$TMP\" \"$OUT\"\ntrap - EXIT\nrm -rf \"$NODE_TMP_DIR\"\necho \"==> 已生成: $OUT\"\necho \"==> 首启物化耗时 ${GENESIS_SECS}s(验收记录);创世哈希 $GENESIS_HASH_STR\"\n\nif [[ \"$FINALIZE\" == \"1\" ]]; then\n    echo \"==> 正式覆盖前校验全部暂存发布物...\"\n    CITIZENAPP_CHAINSPEC=\"$APP_OUT\" \\\n    CITIZENAPP_LIGHT_SYNC_STATE=\"$APP_LIGHT_SYNC_STATE_OUT\" \\\n    CITIZENCHAIN_PLAIN_SPEC=\"$OUT\" \\\n    CITIZENCHAIN_GENESIS_STATE_MANIFEST=\"$GENESIS_STATE_OUT/manifest.json\" \\\n    CITIZENAPP_PUBLIC_INSTITUTION_MANIFEST=\"$APP_PUBLIC_INSTITUTION_OUT/manifest.json\" \\\n    CITIZENSERVE_CLOUDFLARE_WRANGLER=\"$CLOUDFLARE_WRANGLER_OUT\" \\\n    CITIZENAPP_REQUIRE_STATE_ROOT=1 \\\n        \"$REPO_ROOT/citizenapp/scripts/check-chainspec-frozen.sh\"\n\n    NODE_SPEC=\"$CHAIN_ROOT/node/citizenchain.json\"\n    APP_SPEC=\"$REPO_ROOT/citizenapp/assets/chainspec.json\"\n    APP_LIGHT_SYNC_STATE=\"$REPO_ROOT/citizenapp/assets/light_sync_state.json\"\n    APP_PUBLIC_INSTITUTION=\"$REPO_ROOT/citizenapp/assets/public_institutions\"\n    CLOUDFLARE_WRANGLER=\"$REPO_ROOT/citizenserve/scripts/wrangler.toml\"\n    install -m 0644 \"$OUT\" \"$NODE_SPEC\"\n    install -m 0644 \"$APP_OUT\" \"$APP_SPEC\"\n    install -m 0644 \"$APP_LIGHT_SYNC_STATE_OUT\" \"$APP_LIGHT_SYNC_STATE\"\n    rm -rf \"$APP_PUBLIC_INSTITUTION\"\n    mkdir -p \"$APP_PUBLIC_INSTITUTION\"\n    cp -a \"$APP_PUBLIC_INSTITUTION_OUT/.\" \"$APP_PUBLIC_INSTITUTION/\"\n    install -m 0644 \"$CLOUDFLARE_WRANGLER_OUT\" \"$CLOUDFLARE_WRANGLER\"\n    echo \"==> 已同步冻结 SSOT:\"\n    echo \"    $NODE_SPEC\"\n    echo \"    $APP_SPEC (轻形态 stateRootHash)\"\n    echo \"    $APP_LIGHT_SYNC_STATE (lightSyncState checkpoint)\"\n    echo \"    $APP_PUBLIC_INSTITUTION (块 0 公权机构缓存)\"\n    echo \"    $CLOUDFLARE_WRANGLER (公开链身份派生配置)\"\n    echo \"==> 正式创世审计状态包已生成（安装包仍按冻结 plain chainspec 本地物化）:\"\n    echo \"    $GENESIS_STATE_OUT\"\nelse\n    echo \"==> 预览模式完成,未覆盖冻结 SSOT。正式创世请加 --finalize --wasm <CI_WASM>。\"\nfi\n","constitution":"#!/usr/bin/env python3\n\"\"\"检查公民宪法创世冻结条件。\n\n本脚本支持读取已有 chainspec 的 raw storage,也支持通过 RPC 读取已物化块 0 storage。\n检查项:\n1. `:code` 存在,可选校验其字节等于指定 CI WASM。\n2. `LegislationYuan::Laws[0]` 是宪法、全国 scope、v1 生效、无待生效版。\n3. `LegislationYuan::LawVersions[0][1]` 存在，章号全局唯一、同章节号唯一、条号全局唯一，\n   且包含全部不可修改条款。\n4. `ConstitutionImmutableManifest` 清单与 v1 条文摘要逐字匹配。\n5. `LawsByScope[Constitution][0] == [0]`, `NextLawId == 1`。\n\"\"\"\n\nfrom __future__ import annotations\n\nimport argparse\nimport hashlib\nimport json\nimport sys\nfrom dataclasses import dataclass\nfrom pathlib import Path\n\nPALLET = b\"LegislationYuan\"\nCONSTITUTION_LAW_ID = 0\nGENESIS_VERSION = 1\nTIER_CONSTITUTION = 0\nLAW_STATUS_EFFECTIVE = 1\nIMMUTABLE_ARTICLES = [1, 2, 3, 17, 19, 24, 34, 42]\nCODE_KEY = \"0x3a636f6465\"\n\n\ndef twox_128(data: bytes) -> bytes:\n    # 完整创世校验才需要计算 Substrate storage key；纯 SCALE 自检不得依赖第三方包。\n    try:\n        import xxhash\n    except ModuleNotFoundError as exc:\n        raise RuntimeError(\"完整创世校验需要安装 Python xxhash 包\") from exc\n\n    return (\n        xxhash.xxh64(data, seed=0).intdigest().to_bytes(8, \"little\")\n        + xxhash.xxh64(data, seed=1).intdigest().to_bytes(8, \"little\")\n    )\n\n\ndef blake2_128(data: bytes) -> bytes:\n    return hashlib.blake2b(data, digest_size=16).digest()\n\n\ndef blake2_256(data: bytes) -> bytes:\n    return hashlib.blake2b(data, digest_size=32).digest()\n\n\ndef u32(v: int) -> bytes:\n    return v.to_bytes(4, \"little\")\n\n\ndef u64(v: int) -> bytes:\n    return v.to_bytes(8, \"little\")\n\n\ndef map_prefix(storage: bytes) -> bytes:\n    return twox_128(PALLET) + twox_128(storage)\n\n\ndef blake2_128_concat(encoded: bytes) -> bytes:\n    return blake2_128(encoded) + encoded\n\n\ndef storage_value(storage: bytes) -> str:\n    return \"0x\" + map_prefix(storage).hex()\n\n\ndef storage_map(storage: bytes, key: bytes) -> str:\n    return \"0x\" + (map_prefix(storage) + blake2_128_concat(key)).hex()\n\n\ndef storage_double_map(storage: bytes, key1: bytes, key2: bytes) -> str:\n    return \"0x\" + (map_prefix(storage) + blake2_128_concat(key1) + blake2_128_concat(key2)).hex()\n\n\nclass Scale:\n    def __init__(self, data: bytes) -> None:\n        self.data = data\n        self.i = 0\n\n    def _need(self, n: int) -> None:\n        if self.i + n > len(self.data):\n            raise ValueError(\"SCALE 数据长度不足\")\n\n    def u8(self) -> int:\n        self._need(1)\n        v = self.data[self.i]\n        self.i += 1\n        return v\n\n    def u32(self) -> int:\n        self._need(4)\n        v = int.from_bytes(self.data[self.i : self.i + 4], \"little\")\n        self.i += 4\n        return v\n\n    def u64(self) -> int:\n        self._need(8)\n        v = int.from_bytes(self.data[self.i : self.i + 8], \"little\")\n        self.i += 8\n        return v\n\n    def raw(self, n: int) -> bytes:\n        self._need(n)\n        v = self.data[self.i : self.i + n]\n        self.i += n\n        return v\n\n    def compact(self) -> int:\n        first = self.u8()\n        mode = first & 0x03\n        if mode == 0:\n            return first >> 2\n        if mode == 1:\n            second = self.u8()\n            return ((second << 8) | first) >> 2\n        if mode == 2:\n            rest = self.raw(3)\n            return int.from_bytes(bytes([first]) + rest, \"little\") >> 2\n        length = (first >> 2) + 4\n        return int.from_bytes(self.raw(length), \"little\")\n\n    def vec_bytes(self) -> bytes:\n        return self.raw(self.compact())\n\n    def opt_bytes(self) -> bytes | None:\n        tag = self.u8()\n        if tag == 0:\n            return None\n        if tag != 1:\n            raise ValueError(f\"非法 Option tag: {tag}\")\n        return self.vec_bytes()\n\n    def opt_u32(self) -> int | None:\n        tag = self.u8()\n        if tag == 0:\n            return None\n        if tag != 1:\n            raise ValueError(f\"非法 Option tag: {tag}\")\n        return self.u32()\n\n\n@dataclass\nclass Law:\n    law_id: int\n    tier: int\n    scope_code: int\n    effective_version: int | None\n    latest_version: int\n    pending_version: int | None\n    status: int\n\n\n@dataclass\nclass Version:\n    law_id: int\n    version: int\n    articles: dict[int, bytes]\n    published_at: int\n    effective_at: int\n\n\ndef parse_law(raw: bytes) -> Law:\n    s = Scale(raw)\n    law_id = s.u64()\n    tier = s.u8()\n    scope_code = s.u32()\n    houses_len = s.compact()\n    # houses = Vec<CidNumber>，每个 CID 自带 SCALE compact 长度；CID 长度不是协议常量，\n    # 禁止按历史固定字节数跳过，否则机构 CID 格式调整后会错位解码后续版本字段。\n    for _ in range(houses_len):\n        s.vec_bytes()\n    effective_version = s.opt_u32()\n    latest_version = s.u32()\n    pending_version = s.opt_u32()\n    status = s.u8()\n    if s.i != len(raw):\n        raise ValueError(\"Law SCALE 存在未识别尾部字段\")\n    return Law(law_id, tier, scope_code, effective_version, latest_version, pending_version, status)\n\n\ndef self_test() -> None:\n    \"\"\"锁定 SCALE 解码与宪法章号、同章节号、条号三层唯一性。\"\"\"\n\n    def compact_small(value: int) -> bytes:\n        if not 0 <= value < 64:\n            raise ValueError(\"self-test 只编码单字节 compact\")\n        return bytes([value << 2])\n\n    houses = (b\"CID\", b\"ZS000-NRC0A-000000001-2026\")\n    raw = b\"\".join(\n        (\n            u64(0),\n            bytes([TIER_CONSTITUTION]),\n            u32(0),\n            compact_small(len(houses)),\n            *(compact_small(len(house)) + house for house in houses),\n            bytes([1]),\n            u32(GENESIS_VERSION),\n            u32(GENESIS_VERSION),\n            bytes([0]),\n            bytes([LAW_STATUS_EFFECTIVE]),\n        )\n    )\n    law = parse_law(raw)\n    if law != Law(0, TIER_CONSTITUTION, 0, 1, 1, None, LAW_STATUS_EFFECTIVE):\n        raise AssertionError(f\"Law SCALE self-test 失败:{law}\")\n\n    def vec_bytes(value: bytes) -> bytes:\n        return compact_small(len(value)) + value\n\n    def encode_article(number: int) -> bytes:\n        return b\"\".join(\n            (\n                u32(number),\n                vec_bytes(f\"article-{number}\".encode()),\n                bytes([0]),\n                vec_bytes(b\"body\"),\n                bytes([0]),\n                compact_small(0),\n            )\n        )\n\n    def encode_section(number: int, article_numbers: tuple[int, ...]) -> bytes:\n        return b\"\".join(\n            (\n                u32(number),\n                vec_bytes(f\"section-{number}\".encode()),\n                bytes([0]),\n                compact_small(len(article_numbers)),\n                *(encode_article(article_number) for article_number in article_numbers),\n            )\n        )\n\n    def encode_chapter(number: int, sections: tuple[tuple[int, tuple[int, ...]], ...]) -> bytes:\n        return b\"\".join(\n            (\n                u32(number),\n                vec_bytes(f\"chapter-{number}\".encode()),\n                bytes([0]),\n                compact_small(len(sections)),\n                *(encode_section(section_number, article_numbers)\n                  for section_number, article_numbers in sections),\n            )\n        )\n\n    def encode_version(chapters: tuple[tuple[int, tuple[tuple[int, tuple[int, ...]], ...]], ...]) -> bytes:\n        return b\"\".join(\n            (\n                u64(CONSTITUTION_LAW_ID),\n                u32(GENESIS_VERSION),\n                vec_bytes(b\"constitution\"),\n                bytes([0]),\n                compact_small(len(chapters)),\n                *(encode_chapter(chapter_number, sections)\n                  for chapter_number, sections in chapters),\n                bytes(32),\n                bytes([0]),\n                u64(0),\n                u64(0),\n                u64(0),\n            )\n        )\n\n    # 不同章允许复用相同节号。\n    valid_version = encode_version(((1, ((7, (1,)),)), (2, ((7, (2,)),))))\n    parsed = parse_version(valid_version)\n    if sorted(parsed.articles) != [1, 2]:\n        raise AssertionError(f\"合法宪法结构解析异常: {sorted(parsed.articles)}\")\n\n    invalid_versions = (\n        (encode_version(((1, ((1, (1,)),)), (1, ((1, (2,)),)))), \"重复章号\"),\n        (encode_version(((1, ((3, (1,)), (3, (2,)))),)), \"重复节号\"),\n        (encode_version(((1, ((1, (9,)),)), (2, ((1, (9,)),)))), \"重复条号\"),\n    )\n    for invalid_version, expected_error in invalid_versions:\n        try:\n            parse_version(invalid_version)\n        except ValueError as exc:\n            if expected_error not in str(exc):\n                raise AssertionError(f\"结构错误类型异常: {exc}\") from exc\n        else:\n            raise AssertionError(f\"未拒绝宪法{expected_error}\")\n    print(\"constitution SCALE self-test ok\")\n\n\ndef skip_clause(s: Scale) -> None:\n    s.u32()\n    s.vec_bytes()\n    s.opt_bytes()\n\n\ndef parse_article(s: Scale) -> tuple[int, bytes]:\n    start = s.i\n    number = s.u32()\n    s.vec_bytes()\n    s.opt_bytes()\n    s.vec_bytes()\n    s.opt_bytes()\n    for _ in range(s.compact()):\n        skip_clause(s)\n    return number, s.data[start : s.i]\n\n\ndef parse_section(s: Scale, articles: dict[int, bytes], section_numbers: set[int]) -> None:\n    section_number = s.u32()\n    if section_number in section_numbers:\n        raise ValueError(f\"同一章出现重复节号: {section_number}\")\n    section_numbers.add(section_number)\n    s.vec_bytes()\n    s.opt_bytes()\n    for _ in range(s.compact()):\n        number, raw_article = parse_article(s)\n        if number in articles:\n            raise ValueError(f\"宪法全文出现重复条号: {number}\")\n        articles[number] = raw_article\n\n\ndef parse_chapter(s: Scale, articles: dict[int, bytes], chapter_numbers: set[int]) -> None:\n    chapter_number = s.u32()\n    if chapter_number in chapter_numbers:\n        raise ValueError(f\"宪法全文出现重复章号: {chapter_number}\")\n    chapter_numbers.add(chapter_number)\n    s.vec_bytes()\n    s.opt_bytes()\n    section_numbers: set[int] = set()\n    for _ in range(s.compact()):\n        parse_section(s, articles, section_numbers)\n\n\ndef parse_version(raw: bytes) -> Version:\n    s = Scale(raw)\n    law_id = s.u64()\n    version = s.u32()\n    s.vec_bytes()\n    s.opt_bytes()\n    articles: dict[int, bytes] = {}\n    chapter_numbers: set[int] = set()\n    for _ in range(s.compact()):\n        parse_chapter(s, articles, chapter_numbers)\n    s.raw(32)\n    s.u8()\n    s.u64()\n    published_at = s.u64()\n    effective_at = s.u64()\n    return Version(law_id, version, articles, published_at, effective_at)\n\n\ndef parse_vec_u64(raw: bytes) -> list[int]:\n    s = Scale(raw)\n    return [s.u64() for _ in range(s.compact())]\n\n\ndef parse_manifest(raw: bytes) -> tuple[list[int], list[bytes]]:\n    s = Scale(raw)\n    numbers = [s.u32() for _ in range(s.compact())]\n    hashes = [s.raw(32) for _ in range(s.compact())]\n    return numbers, hashes\n\n\nclass RpcTop:\n    \"\"\"--rpc 模式:以 state_getStorage(key, at) 透明替代 raw.top 字典。\n\n    plain chainspec(ADR-031 D5)不再物化 GB 级 raw state,检查改为\n    对临时节点的创世块按键查询,键与断言逻辑与文件模式完全一致。\n    \"\"\"\n\n    def __init__(self, url: str, at: str | None) -> None:\n        from urllib.parse import urlsplit\n        parsed = urlsplit(url)\n        if parsed.scheme != \"https\" or not parsed.hostname or parsed.username or parsed.password or parsed.fragment:\n            raise ValueError(\"RPC 必须配置完整 HTTPS 地址\")\n        self.url = url\n        self.at = at\n\n    def get(self, key: str) -> str | None:\n        import urllib.request\n\n        if not key.startswith(\"0x\"):\n            key = \"0x\" + key\n        params = [key] + ([self.at] if self.at else [])\n        body = json.dumps(\n            {\"jsonrpc\": \"2.0\", \"id\": 1, \"method\": \"state_getStorage\", \"params\": params}\n        ).encode()\n        req = urllib.request.Request(\n            self.url, data=body, headers={\"content-type\": \"application/json\"}\n        )\n        # 中文注释：保留系统 CA 验证，禁止重定向将 HTTPS 查询降级。\n        class NoRedirect(urllib.request.HTTPRedirectHandler):\n            def redirect_request(self, request, fp, code, message, headers, newurl):\n                return None\n        opener = urllib.request.build_opener(NoRedirect())\n        with opener.open(req, timeout=30) as resp:\n            return json.loads(resp.read()).get(\"result\")\n\n\ndef top_value(top, key: str, label: str) -> bytes:\n    value = top.get(key.lower()) or top.get(key)\n    if value is None:\n        raise AssertionError(f\"缺少 {label}: {key}\")\n    raw = value[2:] if value.startswith(\"0x\") else value\n    return bytes.fromhex(raw)\n\n\ndef check(path: Path | None, expect_code_file: Path | None, rpc_top=None) -> None:\n    if rpc_top is not None:\n        top = rpc_top\n    else:\n        spec = json.loads(path.read_text())\n        top = spec.get(\"genesis\", {}).get(\"raw\", {}).get(\"top\", {})\n        if not isinstance(top, dict):\n            raise AssertionError(\"chainspec 缺 genesis.raw.top\")\n\n    code = top_value(top, CODE_KEY, \":code\")\n    if not code:\n        raise AssertionError(\":code 为空\")\n    if expect_code_file is not None:\n        expected = expect_code_file.read_bytes()\n        if code != expected:\n            raise AssertionError(\n                f\":code 与 WASM 文件不一致: chainspec={len(code)} bytes, wasm={len(expected)} bytes\"\n            )\n\n    law = parse_law(top_value(top, storage_map(b\"Laws\", u64(0)), \"Laws[0]\"))\n    assert law.law_id == CONSTITUTION_LAW_ID, f\"Laws[0].law_id 异常: {law.law_id}\"\n    assert law.tier == TIER_CONSTITUTION, f\"Laws[0].tier 不是 Constitution: {law.tier}\"\n    assert law.scope_code == 0, f\"Laws[0].scope_code 不是全国 0: {law.scope_code}\"\n    assert law.effective_version == GENESIS_VERSION, f\"宪法创世生效版本应为 v1: {law}\"\n    assert law.latest_version == GENESIS_VERSION, f\"宪法创世最新版本应为 v1: {law}\"\n    assert law.pending_version is None, f\"宪法创世不得有待生效版本: {law}\"\n    assert law.status == LAW_STATUS_EFFECTIVE, f\"宪法创世状态应为 Effective: {law.status}\"\n\n    version = parse_version(\n        top_value(top, storage_double_map(b\"LawVersions\", u64(0), u32(1)), \"LawVersions[0][1]\")\n    )\n    assert version.law_id == CONSTITUTION_LAW_ID, f\"LawVersion law_id 异常: {version.law_id}\"\n    assert version.version == GENESIS_VERSION, f\"LawVersion version 异常: {version.version}\"\n\n    missing = [n for n in IMMUTABLE_ARTICLES if n not in version.articles]\n    if missing:\n        raise AssertionError(f\"宪法 v1 缺不可修改条款: {missing}\")\n\n    numbers, hashes = parse_manifest(\n        top_value(top, storage_value(b\"ConstitutionImmutableManifest\"), \"ConstitutionImmutableManifest\")\n    )\n    assert numbers == IMMUTABLE_ARTICLES, f\"manifest 清单异常: {numbers}\"\n    assert len(hashes) == len(numbers), \"manifest 条号与摘要数量不一致\"\n    for number, digest in zip(numbers, hashes):\n        actual = blake2_256(version.articles[number])\n        if actual != digest:\n            raise AssertionError(f\"manifest 第 {number} 条摘要与宪法 v1 条文不一致\")\n\n    scope = parse_vec_u64(\n        top_value(\n            top,\n            storage_double_map(b\"LawsByScope\", bytes([TIER_CONSTITUTION]), u32(0)),\n            \"LawsByScope[Constitution][0]\",\n        )\n    )\n    assert scope == [CONSTITUTION_LAW_ID], f\"宪法层级唯一性异常: {scope}\"\n\n    next_law_id = Scale(top_value(top, storage_value(b\"NextLawId\"), \"NextLawId\")).u64()\n    assert next_law_id == 1, f\"NextLawId 应为 1: {next_law_id}\"\n\n    print(\"constitution genesis check ok\")\n    print(f\"  spec: {path}\")\n    print(f\"  :code bytes: {len(code)}\")\n    print(\"  law_id=0 tier=Constitution effective_version=1 latest_version=1 pending=None\")\n    print(\"  immutable articles:\", \",\".join(str(n) for n in numbers))\n\n\ndef main() -> int:\n    parser = argparse.ArgumentParser(description=\"检查公民宪法创世冻结条件(chainspec 文件或 --rpc 临时节点)\")\n    parser.add_argument(\"chainspec\", type=Path, nargs=\"?\")\n    parser.add_argument(\"--expect-code-file\", type=Path)\n    parser.add_argument(\"--rpc\", help=\"临时节点 RPC 地址,必须为可信 HTTPS\")\n    parser.add_argument(\"--at\", help=\"创世块哈希(--rpc 模式钉块查询)\")\n    parser.add_argument(\"--self-test\", action=\"store_true\", help=\"只运行 SCALE 解码自检\")\n    args = parser.parse_args()\n\n    if args.self_test:\n        self_test()\n        return 0\n\n    if args.rpc is None and args.chainspec is None:\n        parser.error(\"必须提供 chainspec 文件或 --rpc\")\n\n    try:\n        check(\n            args.chainspec,\n            args.expect_code_file,\n            rpc_top=RpcTop(args.rpc, args.at) if args.rpc else None,\n        )\n    except Exception as exc:  # noqa: BLE001\n        print(f\"constitution genesis check failed: {exc}\", file=sys.stderr)\n        return 1\n    return 0\n\n\nif __name__ == \"__main__\":\n    raise SystemExit(main())\n","prepare":"#!/usr/bin/env bash\n# 公民链产品工具准备：使用调用环境中的标准工具和产品锁文件。\n# 必须由 build.mjs run source，使产品路径和包管理器设置留在当前进程。\nset -euo pipefail\n\nPREPARE_SCRIPT_DIR=\"$CITIZENCHAIN_SOURCE_ROOT/scripts\"\nCITIZENCHAIN_ROOT=\"$CITIZENCHAIN_SOURCE_ROOT\"\nNODE_FRONTEND_SOURCE=\"$CITIZENCHAIN_ROOT/node/frontend\"\nONCHINA_FRONTEND_SOURCE=\"$CITIZENCHAIN_ROOT/onchina/frontend\"\n\ncase \"$(uname -s)/$(uname -m)\" in\n  Darwin/arm64) CITIZENCHAIN_WORK_PLATFORM=macos ;;\n  Linux/aarch64) CITIZENCHAIN_WORK_PLATFORM=linux-arm ;;\n  Linux/x86_64) CITIZENCHAIN_WORK_PLATFORM=linux-amd ;;\n  *) echo 'CitizenChain当前宿主未声明' >&2; exit 1 ;;\nesac\nCITIZENCHAIN_WORK_DIR=\"${CITIZENCHAIN_WORK_DIR:-$CITIZENCHAIN_ROOT/target/build}\"\nCITIZENCHAIN_DEPENDENCY_DIR=\"${CITIZENCHAIN_DEPENDENCY_DIR:-$CITIZENCHAIN_WORK_DIR/dependencies}\"\npython3 - \"$CITIZENCHAIN_ROOT\" \"$CITIZENCHAIN_WORK_DIR\" \"$CITIZENCHAIN_DEPENDENCY_DIR\" \"${CARGO_TARGET_DIR:-$CITIZENCHAIN_WORK_DIR/cargo-target}\" <<'CHECK_WORK'\nfrom pathlib import Path\nimport sys\nsource = Path(sys.argv[1]).resolve()\nfor value in sys.argv[2:]:\n    raw = Path(value)\n    target = raw.resolve()\n    if not raw.is_absolute() or source / 'target' not in target.parents:\n        raise SystemExit(f'CitizenChain可写目录必须是本产品target内绝对路径：{value}')\nCHECK_WORK\nexport npm_config_cache=\"${npm_config_cache:-$CITIZENCHAIN_DEPENDENCY_DIR/npm}\"\nmkdir -p \"$npm_config_cache\" \"$CITIZENCHAIN_WORK_DIR/source\"\nexport npm_config_audit=false npm_config_fund=false\n\ncase \"$(uname -s)/$(uname -m)\" in\n  Darwin/arm64) CITIZENCHAIN_PROTOC_PLATFORM=macos ;;\n  Linux/aarch64) CITIZENCHAIN_PROTOC_PLATFORM=linux-arm ;;\n  Linux/x86_64) CITIZENCHAIN_PROTOC_PLATFORM=linux-amd ;;\n  *) echo \"CitizenChain protoc不支持当前宿主：$(uname -s)/$(uname -m)\" >&2; exit 1 ;;\nesac\n# protoc由CitizenChain自己的锁定声明取得；开发者本机与CI使用同一官方版本和摘要。\nif [ -z \"${PROTOC:-}\" ]; then\nPROTOC=\"$(node \"$CITIZENCHAIN_ROOT/scripts/resources.mjs\" protoc \\\n  \"$CITIZENCHAIN_PROTOC_PLATFORM\" \"$CITIZENCHAIN_DEPENDENCY_DIR/protoc/$CITIZENCHAIN_PROTOC_PLATFORM\")\"\n[[ \"$(\"$PROTOC\" --version)\" == 'libprotoc 35.0' ]] \\\n  || { echo 'CitizenChain protoc 35.0验真失败' >&2; exit 1; }\nexport PROTOC\n\nprepare_node_project() {\n  local source=\"$1\" relative=\"$2\" destination=\"$CITIZENCHAIN_WORK_DIR/source/$relative\"\n  [[ -f \"$source/package.json\" && -f \"$source/package-lock.json\" ]] \\\n    || { echo \"公民链Node工程缺少锁文件：$source\" >&2; return 1; }\n  # 每次只重建当前产品工作根中的准确工程副本，保留包管理器缓存和其它任务目录。\n  rm -rf -- \"$destination\"\n  node - \"$source\" \"$destination\" <<'COPY_PROJECT'\nconst fs = require('node:fs');\nconst path = require('node:path');\nconst [source, destination] = process.argv.slice(2);\nfs.cpSync(source, destination, {\n  recursive: true,\n  errorOnExist: true,\n  force: false,\n  filter: (entry) => !['node_modules', 'dist'].includes(path.basename(entry)),\n});\nCOPY_PROJECT\n  local -a npm_args=(ci)\n  case \"${CITIZENCHAIN_OFFLINE:-false}\" in\n    true) npm_args+=(--offline) ;;\n    false) ;;\n    *) echo 'CITIZENCHAIN_OFFLINE只接受true或false' >&2; return 1 ;;\n  esac\n  (cd \"$destination\" && npm \"${npm_args[@]}\")\n  BUILD_NODE_PROJECT=\"$destination\"\n}\n\necho '==> 准备产品依赖：citizenchain/crates/scanner'\nprepare_node_project \"$CITIZENCHAIN_ROOT/crates/scanner\" citizenchain/crates/scanner\nSCANNER_REACT_PROJECT=\"$BUILD_NODE_PROJECT\"\necho '==> 准备产品依赖：citizenchain/node/frontend'\nprepare_node_project \"$NODE_FRONTEND_SOURCE\" citizenchain/node/frontend\nNODE_FRONTEND_PROJECT=\"$BUILD_NODE_PROJECT\"\necho '==> 准备产品依赖：citizenchain/onchina/frontend'\nprepare_node_project \"$ONCHINA_FRONTEND_SOURCE\" citizenchain/onchina/frontend\nONCHINA_FRONTEND_PROJECT=\"$BUILD_NODE_PROJECT\"\n\n# 图片原件只复制到本轮统一资源目录，前端不另建图片副本。\nmkdir -p \"$CITIZENCHAIN_WORK_DIR/source/citizenchain/icons\"\ncp -R \"$CITIZENCHAIN_ROOT/icons/.\" \"$CITIZENCHAIN_WORK_DIR/source/citizenchain/icons/\"\nexport SCANNER_REACT_PROJECT NODE_FRONTEND_PROJECT ONCHINA_FRONTEND_PROJECT\n\n# 本轮工程的生成回调调用唯一正式文档实现，不复制第二套程序。\n\"${PRODUCT_NODE_BIN:?}\" - \"$CITIZENCHAIN_WORK_DIR\" \"$CITIZENCHAIN_ROOT\" \"$NODE_FRONTEND_PROJECT/package.json\" <<'JS'\nconst fs=require(\"node:fs\"),path=require(\"node:path\");const [work,source,file]=process.argv.slice(2);const p=JSON.parse(fs.readFileSync(file,\"utf8\"));p.scripts[\"generate:docs\"]=JSON.stringify(process.execPath)+\" \"+JSON.stringify(path.join(source,\"scripts/docs.mjs\"));fs.writeFileSync(file,JSON.stringify(p,null,2)+\"\\n\");\nJS\nexport CITIZENCHAIN_PROJECT_ROOT=\"$CITIZENCHAIN_WORK_DIR/source/citizenchain\"\n"});
export async function runEmbeddedBuild(kind,args=[],environment=process.env){
 const work=environment.PRODUCT_WORK_DIR||environment.CITIZENCHAIN_WORK_DIR||temporaryRoot(undefined,'build');checkWork(work);
 return withFixedWork(taskScope(work),()=>embeddedBuildTask(kind,args,{...environment,CITIZENCHAIN_WORK_DIR:work}),{environment});
}
async function embeddedBuildTask(kind,args=[],environment=process.env){
 if((kind!=='windows'&&!Object.hasOwn(BUILD_SOURCES,kind))||kind==='prepare')fail('构建子命令无效');
 if(kind==='local'&&(!isAbsolute(args[1])||!inside(environment.CITIZENCHAIN_WORK_DIR,args[1])))fail('本机构建输出越出固定工作根');
 if(kind==='windows')return prepackWindows(environment);
 const platform=environment.PLATFORM||args[0]||'macos',base=environment.PRODUCT_WORK_DIR||environment.CITIZENCHAIN_WORK_DIR||temporaryRoot(Object.hasOwn(contract.platforms,platform)?platform:'macos','build');checkWork(base);
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
function fixtureWork(){const work=checkFixedWork(fixedWork('build'),{create:true});finishFixedWork(work);return work;}
function removeFixture(path,options={}){if(path===fixedWork('build')||path===fixedWork('test')){if(existsSync(path))clearFixedWork(path);return;}rmSync(path,options);}
function writeFixture(path,data,options){writeFileSync(path,data,options);if(String(path).endsWith('/scripts/build.mjs')&&String(data).includes("from './target.mjs'"))copyFileSync(join(root,'scripts/target.mjs'),join(dirname(path),'target.mjs'));}
function copyFixture(source,destination,...options){copyFileSync(source,destination,...options);if(String(destination).endsWith('/scripts/build.mjs'))copyFileSync(join(root,'scripts/target.mjs'),join(dirname(destination),'target.mjs'));}

// 内嵌回归只由node --test直接运行本文件时注册，导入和正常执行不运行测试。
if(inlineTestEntry){
void (async()=>{
// 产品独立入口：真实只读需求、资源身份、路径隔离与锁定归档失败关闭。
const {test} = await import('node:test');
const {spawnSync} = await import('node:child_process');
const {default:assert} = await import('node:assert/strict');
const {existsSync,lstatSync,mkdtempSync,readFileSync,readdirSync,realpathSync,rmSync,mkdirSync,symlinkSync,writeFileSync} = await import('node:fs');
const { testRoot : tmpdir } = await import('./build.mjs');
const {dirname,join,resolve} = await import('node:path');
const {contract,requirements,resourceEnvironment,checkWork,productTarget,createView,checkArchives} = await import('./build.mjs');

const sandbox=fixtureWork;
const root=resolve(import.meta.dirname,'..'),base=existsSync(join(root,'app/pubspec.yaml'))?join(root,'app'):root;
const fixture=work=>{
 const platform=Object.keys(contract.platforms).find(value=>value.endsWith('android'))||Object.keys(contract.platforms)[0];
 const own={};for(const value of contract.platforms[platform].locks){const key={npm:'npmCache',pub:'pubCache',cargo:'cargoHome'}[value.ecosystem];if(key){own[key]=join(work,key);mkdirSync(own[key]);}}
 return {schema:1,product_id:contract.product_id,platform,work,offline:true,
 tools:Object.fromEntries(contract.platforms[platform].tools.map(tool=>[tool.id,{version:tool.version,path:process.execPath}])),
 dependencies:{own},archives:{},environment:{}};
};
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

// 真实命令行只读自身入口；清除私有环境与工具搜索路径，不能从控制台补齐执行条件。
test('独立命令行从自身声明输出JSON，未知平台失败且不写工作根',async()=>{
 const work=sandbox();try{
  for(const platform of Object.keys(contract.platforms)){
   const before=readdirSync(work),result=spawnSync(process.execPath,[join(root,'scripts/build.mjs'),'requirements',platform,'--work',work],{env:{HOME:work,LANG:'C',LC_ALL:'C'},encoding:'utf8'});
   const apple=platform.endsWith('ios')?'ios':platform.endsWith('macos')?'macos':null;
   if(apple&&existsSync(join(base,apple,'Podfile'))&&!existsSync(join(base,apple,'Podfile.lock'))){assert.notEqual(result.status,0);assert.match(result.stderr,/CocoaPods原始锁缺失/);}
   else{assert.equal(result.status,0,result.stderr);const value=JSON.parse(result.stdout);assert.equal(value.product_id,contract.product_id);assert.equal(value.platform,platform);}
   assert.deepEqual(readdirSync(work),before);
  }
  const invalid=spawnSync(process.execPath,[join(root,'scripts/build.mjs'),'requirements','unknown','--work',work],{env:{HOME:work},encoding:'utf8'});
  assert.notEqual(invalid.status,0);assert.match(invalid.stderr,/平台/);
 }finally{removeFixture(work,{recursive:true});}
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
  assert.deepEqual(readdirSync(work),[], '独立执行结束必须彻底清空现场');
  result.files=[]; calls.length=0;
  assert.deepEqual(await execute(platform,work,{run_id:'123456789'},{stages}),result);
  assert.deepEqual(readdirSync(work),[], '下一轮结束仍须清空现场');
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


// 复制本产品真实入口到自有测试现场；只替换资源供给边界，反向导入和CLI子进程真实执行。
test('CLI异步资源可反向导入唯一校验，正常参数和离线失败均准确收口',()=>{
 const area=sandbox();
 try{
  const source=join(area,'source'),scripts=join(source,'scripts'),file=join(scripts,'build.mjs');
  const platform=Object.keys(contract.platforms)[0];
  const work=join(source,'target','build');
  mkdirSync(scripts,{recursive:true});mkdirSync(work,{recursive:true});
  writeFixture(file,readFileSync(join(root,'scripts/build.mjs')));
  for(const name of ['target.mjs'])writeFixture(join(scripts,name),readFileSync(join(root,'scripts',name)));
  writeFixture(join(scripts,'flows.json'),JSON.stringify(contract));
  const provider=[
   "import {writeFileSync} from 'node:fs';",
   "import {join} from 'node:path';",
   "const refuse = false;",
   "export async function bootstrapNode(work,options){",
   " const owner=await import('./build.mjs');owner.checkWork(work);",
   " writeFileSync(join(work,'bootstrap.json'),JSON.stringify({offline:options.offline,work}));",
   " if(refuse&&options.offline)throw Error('合成离线缺少锁定资源');",
   " return {path:process.execPath};",
   "}",
   "export async function resources(platform,work,request,options){",
   " const owner=await import('./build.mjs');owner.checkWork(work);owner.platformContract(platform);",
   " if(refuse&&options.offline)throw Error('合成离线缺少锁定资源');",
   " return {schema:1,product_id:owner.contract.product_id,platform,work,offline:options.offline,request};",
   "}",
  ].join('\n');
  writeFixture(join(scripts,'resources.mjs'),provider);
  const env={HOME:area,LANG:'C',PATH:''},marker=join(work,'bootstrap.json');
  const options={cwd:source,env,input:'{}',encoding:'utf8',timeout:5000,maxBuffer:1024*1024};
  const check=(result,status)=>{
   assert.equal(result.error,undefined);assert.equal(result.signal,null);assert.equal(result.status,status);
   assert.doesNotMatch(result.stderr,/unsettled top-level await/u);
  };
  // 普通模块导入不启动CLI；结果来自当前入口完整正文，不截取/重写其控制结构。
  const imported=spawnSync(process.execPath,['--input-type=module','--eval',
   "import {pathToFileURL} from 'node:url';await import(pathToFileURL("+JSON.stringify(file)+"));process.stdout.write('module-ready\\n');"],options);
  check(imported,0);assert.equal(imported.stdout,'module-ready\n');assert.deepEqual(readdirSync(work),[]);
  const input=JSON.stringify({schema:1,product_id:contract.product_id,platform,work});
  for(const offline of [false,true]){
   const result=spawnSync(process.execPath,[file,'resources',platform,'--work',work,...(offline?['--offline']:[])],{...options,input});
   check(result,0);
   assert.deepEqual(JSON.parse(result.stdout),{schema:1,product_id:contract.product_id,platform,work,offline,request:JSON.parse(input)});
  }
  // execute先真实完成反向导入和Node选择，再由原请求校验拒绝，不能以假Build成功代替。
  const invalid=spawnSync(process.execPath,[file,'execute',platform,'--work',work,'--offline'],{...options,input:'{"schema":99}'});
  check(invalid,1);assert.equal(invalid.stdout,'');assert.match(invalid.stderr,/公开Build请求身份或字段无效/u);
  assert.equal(existsSync(marker),false,'失败的真实入口必须清除引导材料');
  for(const extra of [['--offline','--offline'],['--unknown']]){
   const result=spawnSync(process.execPath,[file,'execute',platform,'--work',work,...extra],options);
   check(result,1);assert.equal(result.stdout,'');assert.match(result.stderr,/固定入口参数无效/u);assert.equal(existsSync(marker),false);
  }
  const malformed=spawnSync(process.execPath,[file,'resources',platform,'--work',work],{...options,input:'{'});
  check(malformed,1);assert.equal(malformed.stdout,'');assert.match(malformed.stderr,/SyntaxError/u);
  const unknown=spawnSync(process.execPath,[file,'resources','unknown','--work',work],options);
  check(unknown,1);assert.match(unknown.stderr,/平台未声明/u);
  writeFixture(join(scripts,'resources.mjs'),provider.replace('const refuse = false;','const refuse = true;'));
  for(const command of ['execute','resources']){
   const result=spawnSync(process.execPath,[file,command,platform,'--work',work,'--offline'],options);
   check(result,1);assert.equal(result.stdout,'');assert.match(result.stderr,/合成离线缺少锁定资源/u);
  }
  assert.equal(existsSync(join(work,'.product-build.lock')),false);
  assert.equal(existsSync(join(work,'build-result.json')),false);
 }finally{rmSync(area,{recursive:true,force:true});}
});

// 完整宿主通道由调用方核验结果并收尾；独立执行仍必须立即清空。
test('宿主完整Build在调用方消费前保留成功或失败现场，独立入口仍清空',async()=>{
 const {execute,outputDigest,clearWork}=await import('./build.mjs'),platform=Object.keys(contract.platforms)[0],declared=contract.platforms[platform];
 for(const [host,failure] of [['3',false],['3',true],['4',false],[undefined,false]]){
  const work=sandbox();try{
   let result;
   const stages={requirements:()=>{},resources:async()=>({}),prepare:async()=>{writeFixture(join(work,'partial'),'本轮现场');if(failure)throw Error('宿主失败夹具');},build:async()=>{
    result={schema:1,product_id:contract.product_id,platform,work,completion:declared.completion,run_id:'123456789',files:declared.files.map(name=>{const path=join(work,name);mkdirSync(dirname(path),{recursive:true});writeFixture(path,'当前产物');return {path,sha256:outputDigest(path)};})};return result;
   }};
   const pending=execute(platform,work,{run_id:'123456789'},{stages,environment:host?{PRODUCT_HOST_FD:host}:{}});
   if(failure)await assert.rejects(pending,/宿主失败夹具/);else assert.deepEqual(await pending,result);
   assert.equal(existsSync(join(work,'.product-build.lock')),false);
   if(host==='3'){
    assert.equal(existsSync(join(work,'partial')),true);
    if(!failure){assert.equal(existsSync(join(work,'build-result.json')),true);for(const file of result.files)assert.equal(outputDigest(file.path),file.sha256);}
    clearWork(work);
   }
   assert.deepEqual(readdirSync(work),[]);
  }finally{removeFixture(work,{recursive:true,force:true});}
 }
});

})();
}


// 原非macOS完整调用、边界与首条失败收口回归归同一正式入口。
if(inlineTestEntry){void(async()=>{
const {test}=await import('node:test');const {default:assert}=await import('node:assert/strict');const {mkdtempSync,writeFileSync,readFileSync,chmodSync,rmSync}=await import('node:fs');
test('非macOS构建复用准确工作根并在首条失败后停止',async()=>{const work=fixtureWork(),inputs=join(testRoot(),'local-inputs');mkdirSync(inputs,{recursive:true});const tool=join(inputs,'cargo'),calls=join(inputs,'calls');try{writeFixture(tool,'#!'+process.execPath+'\nconst fs=require("node:fs");fs.appendFileSync(process.env.CALLS,JSON.stringify({args:process.argv.slice(2),target:process.env.CARGO_TARGET_DIR,wasm:process.env.WASM_FILE})+"\\n");process.exit(Number(process.env.TOOL_STATUS));');chmodSync(tool,0o700);for(const status of ['0','23']){writeFixture(calls,'');const env={...process.env,PLATFORM:'windows',PRODUCT_WORK_DIR:work,PRODUCT_BASH_BIN:process.env.PRODUCT_TEST_SHELL||'/bin/bash',CARGO:tool,RUSTC:tool,CALLS:calls,TOOL_STATUS:status,WASM_FILE:'forbidden'};if(status==='0')await runEmbeddedBuild('local',['windows',join(work,'work')],env);else await assert.rejects(runEmbeddedBuild('local',['windows',join(work,'work')],env));const data=readFileSync(calls,'utf8').trim().split('\n').map(s=>JSON.parse(s));assert.equal(data.length,status==='0'?2:1);assert.equal(data[0].target,join(work,'work/cargo-target'));assert.equal(data[0].wasm,undefined);assert.ok(data[0].args.includes('check'));if(status==='0')assert.ok(data[1].args.includes('--no-run'));}await assert.rejects(runEmbeddedBuild('local',['windows',root],{PRODUCT_WORK_DIR:work}));}finally{removeFixture(work,{recursive:true,force:true});}});
})();}

// 实际根源码可复制到自有target，target不会递归进入工程；外部源码的嵌套输出仍拒绝。
if(inlineTestEntry){void(async()=>{const {test}=await import('node:test'),{default:assert}=await import('node:assert/strict'),fs=await import('node:fs');test('实际根源码复制到本轮target且不复制工具现场',()=>{const work=fs.mkdtempSync(join(testRoot(),'own-view-'));try{const view=join(work,'project');createView(root,view);assert.equal(fs.existsSync(join(view,'target')),false);assert.equal(fs.existsSync(join(view,'.git')),false);assert.deepEqual(fs.readFileSync(join(view,'icons/logo.png')),fs.readFileSync(join(root,'icons/logo.png')));assert.throws(()=>createView(root,join(root,'icons/forbidden-view')),/边界/);}finally{removeFixture(work,{recursive:true,force:true});}});})();}

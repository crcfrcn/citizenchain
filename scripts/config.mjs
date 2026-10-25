import {runtimeVersion, assertRuntimeProof, assertRuntimeRunName, runtimeLaunchRequest, runtimeSettleRequest} from './runtime.mjs';
// 本产品配置需求的公开入口；只登记名称、类型和规则，不保存配置值。
export const manifest=Object.freeze({
  "schema": 1,
  "product_id": "citizenchain",
  "configuration": {
    "id": "citizenchain",
    "kind": "chain",
    "title": "公民链配置",
    "items": [
      {
        "name": "NRC_PUBLICKEY",
        "description": "国储会管理员冷签公钥",
        "accessLevel": "protected",
        "kind": "authorization",
        "format": "hex32"
      },
      {
        "name": "CHAIN_GENESISHASH",
        "description": "公民链创世块哈希",
        "accessLevel": "public",
        "kind": "不适用",
        "format": "不适用"
      },
      {
        "name": "APPLE_PASSWORD",
        "description": "苹果公证专用密码",
        "accessLevel": "protected",
        "kind": "不适用",
        "format": "不适用"
      }
    ]
  }
});
export function describeConfiguration(){return JSON.parse(JSON.stringify(manifest));}

// 产品交付声明归本仓；Runtime候选版本只由runtime.mjs从实际链版本派生，不复制固定版本。
export const product=Object.freeze({id:'citizenchain',title:'公民链',repository:'crcfrcn/citizenchain',entry:'scripts/build.mjs',
  platforms:Object.freeze({
    macos:{name:'macOS',workflow:'macos.yml',host:'darwin',arch:'arm64',target:'aarch64-apple-darwin',bundle:'dmg',extension:'.dmg'},
    windows:{name:'Windows',workflow:'windows.yml',host:'win32',arch:'x64',target:'x86_64-pc-windows-msvc',bundle:'nsis',extension:'.exe'},
    'linux-arm':{name:'LinuxARM',workflow:'linuxarm.yml',host:'linux',arch:'arm64',target:'aarch64-unknown-linux-gnu',bundle:'deb',extension:'.deb'},
    'linux-amd':{name:'LinuxAMD',workflow:'linuxamd.yml',host:'linux',arch:'x64',target:'x86_64-unknown-linux-gnu',bundle:'deb',extension:'.deb'},
    wasm:{name:'WASM',entry:'scripts/runtime.mjs',workflow:'wasm.yml',host:'linux',arch:'x64',target:'wasm32v1-none',files:['citizenchain.wasm','citizenchain.compact.wasm','citizenchain.compact.compressed.wasm']},
  }),
  tools:Object.freeze({node:'25.2.1',rust:'1.97.1',protoc:'35.0',postgres:'17.11',xcode:'27.0'}),
  locks:Object.freeze(['Cargo.lock','crates/protocol/Cargo.lock','crates/scanner/package-lock.json','node/frontend/package-lock.json','onchina/platform/frontend/package-lock.json']),
});

// 公开声明和通用规则；WASM分支只转调runtime.mjs，不读取链或编排WASM流程。
const requireValue=(condition,message)=>{if(!condition)throw Error(message);};
export function deliveryVersion(value){
  requireValue(typeof value==='string'&&value.trim()===value&&/^(0|[1-9][0-9]*)\.(0|[1-9][0-9]*)\.(0|[1-9][0-9]*)$/u.test(value)
    &&value.split('.').every(part=>BigInt(part)<2n**64n),'交付版本必须是规范 X.Y.Z');
  return value;
}
// WASM规则只有runtime.mjs一份实现；这里保持原公开调用名。
export {runtimeVersion} from './runtime.mjs';
export function releaseName(platform,value){
  requireValue(Object.hasOwn(product.platforms,platform),'产品平台无效');
  return product.title+'·'+product.platforms[platform].name+'·v'+(platform==='wasm'?runtimeVersion(value):deliveryVersion(value));
}
export function artifactNames(platform,value){
  const declared=product.platforms[platform];releaseName(platform,value);
  return declared.files?[...declared.files]:[`citizenchain-${declared.name}-v${value}${declared.extension}`];
}
export function artifactLabel(platform,value,name){
  requireValue(artifactNames(platform,value).includes(name),'附件不属于该平台');
  return releaseName(platform,value)+(platform==='wasm'?name.slice('citizenchain'.length):product.platforms[platform].extension);
}
export function assertRun(platform,run,id=run?.id){
  const declared=product.platforms[platform];
  requireValue(declared&&Number.isSafeInteger(id)&&id>0&&run?.id===id&&run.repository?.full_name===product.repository
    &&run.path==='.github/workflows/'+declared.workflow&&run.event==='workflow_dispatch'&&run.head_branch==='main'
    &&/^[a-f0-9]{40}$/u.test(run.head_sha||'')&&Number.isSafeInteger(run.run_attempt)&&run.run_attempt>0
    &&Number.isSafeInteger(run.run_number)&&run.run_number>0
    &&run.html_url===`https://github.com/${product.repository}/actions/runs/${id}`,'原编译 Run 身份无效');
  return run;
}
export function retainedBuildRuns(platform,runs){
  const selected=runs.filter(run=>run.event==='workflow_dispatch'&&run.head_branch==='main');
  selected.forEach(run=>assertRun(platform,run));
  const terminal=selected.filter(run=>run.status==='completed').toSorted((a,b)=>b.run_number-a.run_number||b.id-a.id);
  const success=terminal.find(run=>run.conclusion==='success')||null;
  const failure=terminal.find(run=>run.conclusion!=='success')||null;
  return {success,failure,obsolete:terminal.filter(run=>run.id!==success?.id&&run.id!==failure?.id)};
}
// 四端最后Job只计算暂定保留项；当前Run仍是in_progress，不伪造GitHub最终终态。
export function nativeCleanupPlan(platform,current,results,runs,{cleanupFailed=false}={}){
  requireValue(['macos','windows','linux-arm','linux-amd'].includes(platform),'清理仅限公民链四端');
  if(current)assertRun(platform,current);
  requireValue(!current||(current.status==='in_progress'&&current.conclusion===null),'本轮不是正在执行的原编译Run');
  const jobs=['prepare','build','release'],states=['success','failure','cancelled','skipped'];
  requireValue(!current||results&&jobs.every(job=>states.includes(results[job])),'前置Job结果不完整');
  const outcome=current?(!cleanupFailed&&jobs.every(job=>results[job]==='success')?'success':'failure'):null;
  const terminal=[],maintenance=[],protectedIds=new Set(current?[current.id]:[]),seen=new Set();
  for(const run of runs){
    if(run.repository?.full_name!==product.repository||run.path!=='.github/workflows/'+product.platforms[platform].workflow||run.head_branch!=='main'){
      protectedIds.add(run.id);continue;
    }
    requireValue(!seen.has(run.id),'原Run查询出现重复身份');seen.add(run.id);
    requireValue(Number.isSafeInteger(run.id)&&run.id>0&&Number.isSafeInteger(run.run_number)&&run.run_number>0
      &&Number.isSafeInteger(run.run_attempt)&&run.run_attempt>0&&/^[a-f0-9]{40}$/u.test(run.head_sha||'')
      &&run.html_url===`https://github.com/${product.repository}/actions/runs/${run.id}`,'历史Run身份无效');
    requireValue(!current||run.run_number<=current.run_number,'存在更晚的同平台Run，禁止旧任务清理');
    requireValue(['workflow_dispatch','repository_dispatch'].includes(run.event),'历史Run事件归属不明');
    if(current&&run.id===current.id){
      requireValue(run.event===current.event&&run.run_attempt===current.run_attempt&&run.head_sha===current.head_sha
        &&run.run_number===current.run_number&&run.status===current.status&&run.conclusion===null,'本轮身份或状态发生变化');
      continue;
    }
    if(run.status!=='completed'){protectedIds.add(run.id);continue;}
    requireValue(typeof run.conclusion==='string'&&run.conclusion.length>0,'历史Run没有真实终态');
    if(run.event==='workflow_dispatch'){assertRun(platform,run);terminal.push(run);}
    else{
      const prefix=product.title+'·'+product.platforms[platform].name+'·v';
      requireValue(run.display_title?.startsWith(prefix),'旧收尾记录没有本平台身份');
      deliveryVersion(run.display_title.slice(prefix.length));maintenance.push(run);
    }
  }
  const latest=terminal.toSorted((a,b)=>b.run_number-a.run_number||b.id-a.id);
  const success=outcome==='success'?current:latest.find(run=>run.conclusion==='success')||null;
  const failure=outcome==='failure'?current:latest.find(run=>run.conclusion!=='success')||null;
  // 旧成功Run最后删除以缩小异常窗口；删除后本轮再失败仍无法恢复旧Run，必须如实报错。
  const obsolete=[...maintenance,...latest.filter(run=>run.id!==success?.id&&run.id!==failure?.id)]
    .toSorted((a,b)=>Number(a.event==='workflow_dispatch'&&a.conclusion==='success')-Number(b.event==='workflow_dispatch'&&b.conclusion==='success')||a.run_number-b.run_number);
  return {outcome,success,failure,obsolete,protectedIds:[...protectedIds]};
}
// 原Run及显式维护共用同一删除实现；不派发、不取消，也不依赖Release清理成功。
export async function cleanupNativeRuns(platform,{list,get,remove},current=null,results=null,cleanupFailed=false){
  const plan=async()=>{
    if(current){const actual=await get(current.id);assertRun(platform,actual,current.id);
      requireValue(['head_sha','run_attempt','run_number','status','conclusion'].every(key=>actual[key]===current[key]),'当前Run身份或状态变化');}
    return nativeCleanupPlan(platform,current,results,await list(),{cleanupFailed});
  };
  const initial=await plan(),deleted=[];
  for(const old of initial.obsolete){
    const fresh=await plan(),candidate=fresh.obsolete.find(run=>run.id===old.id),actual=await get(old.id);
    if(!actual)continue;
    requireValue(candidate&&actual.status==='completed'&&['run_attempt','head_sha','path','event','head_branch','run_number','conclusion'].every(key=>actual[key]===old[key])
      &&actual.repository?.full_name===product.repository,'删除前旧Run身份或终态变化');
    await remove(old.id);requireValue(!await get(old.id),'旧Run删除未确认');deleted.push(old.id);
  }
  const remaining=await plan();requireValue(!remaining.obsolete.length,'历史尚未收口，清理失败；不自动重派或rerun');
  return {deleted,success:remaining.success?.id??null,failure:remaining.failure?.id??null,protected:remaining.protectedIds};
}
export function readReleaseProof(platform,release){
  const lines=String(release.body||'').split(/\r?\n/u).filter(line=>line.startsWith('<!-- automation:')&&line.endsWith(' -->'));
  requireValue(lines.length===1,'正式版本来源证明必须唯一');
  const proof=JSON.parse(lines[0].slice(16,-4));
  requireValue(proof.schema===1&&proof.repository===product.repository&&proof.product_id===product.id&&proof.platform===platform
    &&proof.workflow==='.github/workflows/'+product.platforms[platform].workflow
    &&proof.tag===releaseName(platform,proof.version)&&release.tag_name===proof.tag&&release.name===proof.tag
    &&/^[a-f0-9]{40}$/u.test(proof.source_sha||'')&&release.target_commitish===proof.source_sha
    &&Number.isSafeInteger(proof.run_id)&&proof.run_id>0&&Number.isSafeInteger(proof.run_attempt)&&proof.run_attempt>0,'正式版本来源不符');
  if(platform==='wasm')assertRuntimeProof(proof);
  return proof;
}
// 复用原正式来源及附件校验；不构造或补造已删除的Run。
export function assertReleaseAssets(platform,release,{partial=false,draft=false}={}){
  const proof=readReleaseProof(platform,release),expected=artifactNames(platform,proof.version);
  requireValue(Number.isSafeInteger(release.id)&&release.id>0&&release.prerelease===false
    &&(draft||release.draft===false&&Boolean(release.published_at)),'正式交付状态无效');
  requireValue(Array.isArray(proof.assets)&&JSON.stringify(proof.assets.map(asset=>asset.name).sort())===JSON.stringify(expected.sort())
    &&Array.isArray(release.assets)&&(partial?release.assets.length<=expected.length:release.assets.length===expected.length),'正式附件集合不完整');
  const seen=new Set();
  for(const asset of release.assets){const original=proof.assets.find(item=>item.name===asset.name);
    requireValue(original&&!seen.has(asset.name)&&Number.isSafeInteger(asset.id)&&asset.id>0&&asset.state==='uploaded'
      &&Number.isSafeInteger(asset.size)&&asset.size>0&&asset.size===original.size
      &&/^sha256:[a-f0-9]{64}$/u.test(asset.digest||'')&&asset.digest==='sha256:'+original.sha256
      &&asset.label===artifactLabel(platform,proof.version,asset.name)&&asset.label===original.label
      &&asset.url===`https://api.github.com/repos/${product.repository}/releases/assets/${asset.id}`,'正式附件身份或原始字节摘要不符');
    seen.add(asset.name);
  }
  return proof;
}
export function assertRelease(platform,release,run,options={}){
  assertRun(platform,run);const proof=assertReleaseAssets(platform,release,options);
  requireValue(proof.run_id===run.id&&proof.run_attempt===run.run_attempt&&proof.source_sha===run.head_sha,'正式版本与原编译 Run 不符');
  if(platform==='wasm')assertRuntimeRunName(proof,run);
  return proof;
}
export function compareDeliveryVersions(left,right){
  const a=deliveryVersion(left).split('.').map(BigInt),b=deliveryVersion(right).split('.').map(BigInt);
  for(let i=0;i<3;i++)if(a[i]!==b[i])return a[i]>b[i]?1:-1;
  return 0;
}
// 只有完整正式交付占用发布版本；草稿/半成品仍由原候选检查拒绝，不能自动绕过。
const publishedNativeReleases=(platform,releases)=>releases.filter(release=>
  release.tag_name.startsWith(product.title+'·'+product.platforms[platform].name+'·v')&&release.draft===false&&Boolean(release.published_at));
export async function assertNativePublishedTags(platform,releases,getTag){
  requireValue(['macos','windows','linux-arm','linux-amd'].includes(platform),'正式节点Tag核对仅限四端');
  for(const release of publishedNativeReleases(platform,releases)){
    const proof=assertReleaseAssets(platform,release),ref=await getTag(proof.tag);
    requireValue(ref?.object.type==='commit'&&ref.object.sha===proof.source_sha,'正式Tag提交不符');
  }
}
export function nextDeliveryVersion(platform,runs,releases,initial){
  requireValue(platform!=='wasm','WASM版本只能读取链上Runtime后加1');
  const latest=retainedBuildRuns(platform,runs).success;
  let version=deliveryVersion(initial);
  if(latest){
    const matching=releases.filter(release=>{try{return readReleaseProof(platform,release).run_id===latest.id;}catch{return false;}});
    requireValue(matching.length===1,'最新成功编译没有唯一正式交付');
    const proof=assertRelease(platform,matching[0],latest),parts=proof.version.split('.').map(BigInt);
    parts[2]++;version=deliveryVersion(parts.join('.'));
  }
  // Run历史缺失不会释放已发布的软件版本号；失败重试没有新正式交付时仍使用同一候选。
  for(const release of publishedNativeReleases(platform,releases)){
    const proof=assertReleaseAssets(platform,release);
    if(compareDeliveryVersions(proof.version,version)>=0){const parts=proof.version.split('.').map(BigInt);parts[2]++;version=deliveryVersion(parts.join('.'));}
  }
  return version;
}
// 调用方只传输公开请求；四端清理归原Run最后Job，不另外派发收尾或rerun。
export function automationRequest(platform,operation,input={}){
  requireValue(Object.hasOwn(product.platforms,platform),'产品平台无效');
  const workflow=product.platforms[platform].workflow;
  if(operation==='runs')return {path:`actions/workflows/${workflow}/runs?branch=main&event=workflow_dispatch`,field:'workflow_runs'};
  if(operation==='releases')return {path:'releases'};
  if(operation==='launch'&&platform==='wasm')return runtimeLaunchRequest(input);
  if(operation==='launch'){
    requireValue(/^[a-f0-9]{40}$/u.test(input.sha||''),'派发源码无效');
    const inputs={version:deliveryVersion(input.version),source_sha:input.sha};
    return {path:`actions/workflows/${workflow}/dispatches`,method:'POST',body:{ref:'main',inputs}};
  }
  if(operation==='run'||operation==='cancel'){
    requireValue(Number.isSafeInteger(input.runId)&&input.runId>0,'原RunID无效');
    return {path:'actions/runs/'+input.runId+(operation==='cancel'?'/cancel':''),...(operation==='cancel'?{method:'POST'}:{})};
  }
  if(operation==='settle'){
    requireValue(!['macos','windows','linux-arm','linux-amd'].includes(platform),'四端清理由原Run最后Job执行，禁止另派收尾');
    return runtimeSettleRequest(input.run);
  }
  throw Error('产品未声明该自动化请求');
}
export function workflowRecords(platform,runs){
  const {success,failure}=retainedBuildRuns(platform,runs);
  return [success,failure,...runs.filter(run=>run.status!=='completed')].filter(Boolean).map(run=>{
    assertRun(platform,run);
    return {record_type:'workflow',repository:product.repository,product_id:product.id,product_title:product.title,
      platform,software_flow:'release',title:run.display_title,run_id:run.id,run_attempt:run.run_attempt,run_number:run.run_number,
      source_sha:run.head_sha,url:run.html_url,updated_at:run.updated_at,
      state:run.status==='completed'?(run.conclusion==='success'?'success':'failed'):'pending'};
  });
}

// 节点及 Runtime 发布的公开合同；连接地址和所有机密只由本轮授权输入交付。
export const runtimeUpgrade=Object.freeze({platform:'wasm',files:product.platforms.wasm.files,compressed:'citizenchain.compact.compressed.wasm',
  secretRefs:[{name:'NRC_PUBLICKEY'},{name:'NODE_WSS'}]});
export const deployment=Object.freeze({platform:'linux-arm',repository:product.repository,product:product.id,workflow:'linuxarm.yml',
  work:'/var/lib/citizenchain-deploy',service:'citizenchain-node.service',package:'citizenchain',executable:'/usr/bin/citizenchain',
  helper:'/usr/local/sbin/citizenchain-deploy',unit:'citizenchain-deploy',data:'/opt/citizenchain/data',
  serviceFile:'/etc/systemd/system/citizenchain-node.service',architecture:'arm64',sshUser:'ubuntu',ready:'deploy-ready:arm64:release'});



// BEGIN INLINE TESTS
// 产品纯规则回归不访问网络、凭据、链或真实历史；编译文件中不放测试执行器。
if(process.env.NODE_TEST_CONTEXT&&process.argv[1]===import.meta.filename&&process.argv.length===2){
  const {test}=await import('node:test'),{default:assert}=await import('node:assert/strict');
  const run=(id,conclusion='success')=>({id,run_number:id,run_attempt:1,event:'workflow_dispatch',head_branch:'main',
    repository:{full_name:product.repository},path:'.github/workflows/linuxarm.yml',head_sha:'a'.repeat(40),
    html_url:`https://github.com/${product.repository}/actions/runs/${id}`,status:'completed',conclusion,display_title:releaseName('linux-arm','1.0.0')});
  function release(source,version='1.0.0',id=100){
    const name=artifactNames('linux-arm',version)[0],label=artifactLabel('linux-arm',version,name),hash='f'.repeat(64);
    const proof={schema:1,repository:product.repository,product_id:product.id,platform:'linux-arm',workflow:source.path,
      version,tag:source.display_title,run_id:source.id,run_attempt:source.run_attempt,source_sha:source.head_sha,assets:[{name,label,size:3,sha256:hash}]};
    return {id,tag_name:proof.tag,name:proof.tag,target_commitish:source.head_sha,draft:false,prerelease:false,published_at:'2026-10-19T00:00:00Z',
      body:'<!-- automation:'+JSON.stringify(proof)+' -->',assets:[{id:200,name,label,size:3,digest:'sha256:'+hash,state:'uploaded',url:`https://api.github.com/repos/${product.repository}/releases/assets/200`}]};
  }
  test('交付版本和五个平台完整产物声明拒绝歧义',()=>{
    for(const value of ['1','01.0.0','1.0.0-rc','1.0.0\n',null])assert.throws(()=>deliveryVersion(value));
    assert.equal(releaseName('wasm','2'),'公民链·WASM·v2');
    assert.equal(artifactNames('wasm','2').length,3);assert.equal(Object.keys(product.platforms).length,5);
    assert.equal(product.platforms.wasm.entry,'scripts/runtime.mjs');
    assert.equal(product.entry,'scripts/build.mjs');
  });
  test('权威节点服务只公开TCP P2P并使用无capabilities的隔离账户',async()=>{
    const {readFileSync}=await import('node:fs');
    const text=readFileSync(new URL('../node/citizenchain.service',import.meta.url),'utf8');
    const fields=new Map(text.split('\n').flatMap(line=>{const match=/^(\w+)=(.*)$/u.exec(line);return match?[[match[1],match[2]]]:[];}));
    for(const [name,value] of Object.entries({User:'citizenchain',NoNewPrivileges:'true',CapabilityBoundingSet:'',
      AmbientCapabilities:'',PrivateTmp:'true',PrivateDevices:'true',ProtectSystem:'strict',ProtectHome:'true',
      ReadWritePaths:'/opt/citizenchain/data',UMask:'0077'}))assert.equal(fields.get(name),value,name);
    assert.ok(text.includes('--listen-addr /ip4/0.0.0.0/tcp/30333/wss'));
    assert.ok(text.includes('--rpc-methods Safe'));
    assert.ok(text.includes('--no-prometheus'));
    for(const flag of ['--rpc-external','--unsafe-rpc-external','--rpc-cors all','MemoryDenyWriteExecute='])
      assert.equal(text.includes(flag),false,'不得扩大RPC暴露或阻断WASM/JIT：'+flag);
  });
  test('macOS仅对release构建依赖关闭strip，真实编译环境完整透传',async()=>{
    const {readFileSync}=await import('node:fs'),{platformCargoEnvironment}=await import('./build.mjs');
    const text=readFileSync(new URL('../.github/workflows/macos.yml',import.meta.url),'utf8');
    const build=text.slice(text.indexOf('\n  build:'),text.indexOf('\n  release:'));
    const key='CARGO_PROFILE_RELEASE_BUILD_OVERRIDE_STRIP';
    const value=new RegExp('^      '+key+": '([^']+)'$",'m').exec(build)?.[1];
    assert.equal(value,'none','仅关闭宿主构建脚本/宏依赖的strip，保留交付目标原profile');
    const input={[key]:value,PATH:'/usr/bin:/bin'};
    assert.equal(platformCargoEnvironment('macos','/product/target/build/macos',input)[key],'none');
    assert.deepEqual(input,{[key]:'none',PATH:'/usr/bin:/bin'},'既有环境交付不修改调用输入');
    assert.equal(text.match(new RegExp(key,'g'))?.length,1);
    assert.equal(text.includes('CARGO_PROFILE_RELEASE_STRIP'),false);
    for(const name of ['windows.yml','linuxarm.yml','linuxamd.yml','wasm.yml'])
      assert.equal(readFileSync(new URL('../.github/workflows/'+name,import.meta.url),'utf8').includes(key),false,'不扩大到其它平台');
  });
  test('最新成功失败与活动Run隔离，收尾Run不消费编译版本',()=>{
    const rows=[run(1),run(2,'failure'),run(3),{...run(4),status:'in_progress'}, {...run(99),event:'repository_dispatch'}];
    const selected=retainedBuildRuns('linux-arm',rows);
    assert.equal(selected.success.id,3);assert.equal(selected.failure.id,2);assert.deepEqual(selected.obsolete.map(row=>row.id),[1]);
    assert.equal(nextDeliveryVersion('linux-arm',rows,[release(rows[2])],'1.0.0'),'1.0.1');
    assert.equal(nextDeliveryVersion('linux-arm',[run(1,'failure')],[],'1.0.0'),'1.0.0');
  });
  test('成功资产缺件、摘要或原attempt变化时禁止推进版本',()=>{
    const source=run(1),value=release(source);
    assert.equal(assertRelease('linux-arm',value,source).version,'1.0.0');
    value.assets[0].digest='sha256:'+'e'.repeat(64);assert.throws(()=>assertRelease('linux-arm',value,source));
    assert.throws(()=>assertRelease('linux-arm',release(source),{...source,run_attempt:2}));
    assert.throws(()=>nextDeliveryVersion('linux-arm',[source],[],'1.0.0'));
    assert.throws(()=>assertRun('windows',source));
  });
  test('缺失成功Run不能复用已发布版本，失败重试保持未发布候选',async()=>{
    const source=run(1),published=release(source),rows=[run(2,'failure')];
    assert.equal(nextDeliveryVersion('linux-arm',rows,[published],'1.0.0'),'1.0.1');
    assert.equal(nextDeliveryVersion('linux-arm',[...rows,run(3,'failure')],[published],'1.0.0'),'1.0.1');
    assert.equal(compareDeliveryVersions('1.0.10','1.0.9'),1);
    assert.equal(compareDeliveryVersions('1.0.0','1.0.0'),0);
    await assertNativePublishedTags('linux-arm',[published],async()=>({object:{type:'commit',sha:source.head_sha}}));
    await assert.rejects(assertNativePublishedTags('linux-arm',[published],async()=>({object:{type:'commit',sha:'b'.repeat(40)}})),/Tag提交不符/);
    await assert.rejects(assertNativePublishedTags('linux-arm',[published],async()=>null),/Tag提交不符/);
    const incomplete=structuredClone(published);incomplete.assets=[];
    assert.throws(()=>nextDeliveryVersion('linux-arm',rows,[incomplete],'1.0.0'),/附件集合不完整/);
    const draft={...published,draft:true,published_at:null};
    assert.equal(nextDeliveryVersion('linux-arm',rows,[draft],'1.0.0'),'1.0.0','草稿仍交由原候选冲突检查拒绝');
  });

  test('四端恢复不再派发收尾，取消描述仍只定位原Run',()=>{
    for(const platform of ['macos','windows','linux-arm','linux-amd'])assert.throws(()=>automationRequest(platform,'settle',{run:run(42)}));
    assert.equal(automationRequest('linux-arm','cancel',{runId:42}).path,'actions/runs/42/cancel');
    assert.throws(()=>automationRequest('linux-arm','launch',{version:'1.0.0',sha:'invalid'}));
  });
  test('四端暂定保留正常收敛、缺失类别、旧收尾清空且保护所有活动和异平台记录',()=>{
    const good={prepare:'success',build:'success',release:'success'};
    for(const platform of ['macos','windows','linux-arm','linux-amd']){
      const own=id=>({...run(id),path:'.github/workflows/'+product.platforms[platform].workflow,display_title:releaseName(platform,'1.0.0')});
      const current={...own(10),status:'in_progress',conclusion:null};
      const rows=[own(1),{...own(2),conclusion:'failure'},own(3),{...own(4),status:'in_progress',conclusion:null},
        {...own(8),event:'repository_dispatch'},{...own(9),event:'repository_dispatch',conclusion:'failure'},current,
        {...own(11),path:'.github/workflows/wasm.yml'},{...own(12),repository:{full_name:'foreign/repo'}}];
      const success=nativeCleanupPlan(platform,current,good,rows);
      assert.equal(success.success.id,10);assert.equal(success.failure.id,2);
      assert.deepEqual(new Set(success.obsolete.map(row=>row.id)),new Set([1,3,8,9]));
      for(const id of [4,10,11,12])assert.ok(success.protectedIds.includes(id));
      const failed=nativeCleanupPlan(platform,current,{...good,build:'failure',release:'skipped'},rows);
      assert.equal(failed.outcome,'failure');assert.equal(failed.success.id,3);assert.equal(failed.failure.id,10);
      assert.ok(failed.obsolete.some(row=>row.id===2));assert.equal(current.conclusion,null);
      assert.equal(nativeCleanupPlan(platform,current,good,[current]).failure,null);
      assert.equal(nativeCleanupPlan(platform,current,{...good,prepare:'failure'},[current]).success,null);
      assert.throws(()=>nativeCleanupPlan(platform,current,good,[...rows,own(13)]));
      assert.throws(()=>nativeCleanupPlan(platform,current,good,[own(1),own(1)]));
      assert.throws(()=>nativeCleanupPlan(platform,current,{},rows));
      assert.throws(()=>nativeCleanupPlan(platform,current,good,[{...current,run_attempt:2}]));
      assert.throws(()=>nativeCleanupPlan(platform,current,good,[{...own(1),event:'push'}]));
    }
    assert.throws(()=>nativeCleanupPlan('wasm',run(1),good,[]));
  });
  // 执行既有Workflow中的真实清理正文；GitHub替身仅在内存模拟，不领取授权或访问远端。
  async function cleanupFixture(options={}){
    const {readFileSync}=await import('node:fs'),{fileURLToPath}=await import('node:url');
    const text=readFileSync(new URL('../.github/workflows/linuxarm.yml',import.meta.url),'utf8');
    const marker='        script: |\n',at=text.lastIndexOf(marker);
    assert.ok(at>text.indexOf('\n  cleanup:\n'));
    const script=text.slice(at+marker.length).split('\n').map(line=>line.startsWith('          ')?line.slice(10):line).join('\n');
    const current={...run(10),status:'in_progress',conclusion:null};
    const oldSuccess={...run(1),display_title:releaseName('linux-arm','0.9.0')};
    const rows=[oldSuccess,run(2,'failure'),run(3,'failure'),{...run(4),status:'in_progress',conclusion:null},
      {...run(8),event:'repository_dispatch'},{...run(9,'failure'),event:'repository_dispatch'},current];
    if(options.orphanRelease)rows.splice(rows.indexOf(oldSuccess),1);
    if(options.oldActive)Object.assign(oldSuccess,{status:'in_progress',conclusion:null});
    const releases=[release(oldSuccess,'0.9.0',101),...(options.failed||options.missingRelease?[]:[release(current)])];
    const tags=new Map(releases.map(value=>[value.tag_name,{object:{type:'commit',sha:current.head_sha}}]));
    if(options.badOldAsset)releases[0].assets[0].digest='sha256:'+'b'.repeat(64);
    if(options.immutable)releases[0].immutable=true;
    if(options.badOldTag)tags.get(releases[0].tag_name).object.sha='b'.repeat(40);
    const calls=[],warnings=[],results={prepare:'success',build:options.failed?'failure':'success',release:options.failed?'skipped':'success'};
    let writes=0,deletedSuccess=false;
    const absent=()=>{throw Object.assign(Error('合成404'),{status:404});};
    const actions={listWorkflowRuns:()=>{},getWorkflowRun:async({run_id})=>{
      if(options.afterSuccessDelete&&deletedSuccess&&run_id===10)throw Error('合成删除旧成功后断网');
      const value=rows.find(row=>row.id===run_id);if(!value)return absent();
      return {data:structuredClone(options.drift&&run_id===2?{...value,run_attempt:2}:value)};
    },deleteWorkflowRun:async({run_id})=>{
      calls.push(['deleteRun',run_id]);assert.notEqual(run_id,10);assert.notEqual(run_id,4);
      if(options.failDelete)throw Object.assign(Error('合成403'),{status:403});
      if(!options.unchangedDelete)rows.splice(rows.findIndex(row=>row.id===run_id),1);
      if(run_id===1&&!options.unchangedDelete)deletedSuccess=true;
    }};
    const repos={listReleases:()=>{},getRelease:async({release_id})=>{
      if(options.replacementMissing&&release_id===100)return absent();
      const value=releases.find(row=>row.id===release_id);return value?{data:structuredClone(value)}:absent();
    },deleteRelease:async({release_id})=>{calls.push(['deleteRelease',release_id]);if(options.failAssetDelete)throw Object.assign(Error('合成资产403'),{status:403});releases.splice(releases.findIndex(row=>row.id===release_id),1);}};
    const git={getRef:async({ref})=>{const value=tags.get(ref.slice(5));return value?{data:structuredClone(value)}:absent();},
      deleteRef:async({ref})=>{calls.push(['deleteTag',ref]);if(!options.unchangedTagDelete)tags.delete(ref.slice(5));}};
    const github={rest:{actions,repos,git},paginate:async method=>structuredClone(method===actions.listWorkflowRuns?rows:releases)};
    const summary={addHeading(){return this;},addRaw(){return this;},async write(){writes++;}};
    const env={PLATFORM:'linux-arm',CITIZENCHAIN_RELEASE_VERSION:'1.0.0',CITIZENCHAIN_SOURCE_SHA:current.head_sha,
      GITHUB_RUN_ATTEMPT:'1',GITHUB_WORKSPACE:fileURLToPath(new URL('../',import.meta.url)),JOB_RESULTS:JSON.stringify(results)};
    let error;
    try{const AsyncFunction=Object.getPrototypeOf(async function(){}).constructor;
      await new AsyncFunction('github','context','core','process',script)(github,{repo:{owner:'crcfrcn',repo:'citizenchain'},runId:10,sha:current.head_sha},{summary,warning:message=>warnings.push(message)},{env});
    }catch(value){error=value;}
    return {error,calls,rows,writes,current,releases,tags,warnings};
  }
  test('真实清理正文正常删除旧Run和全部旧收尾，编译失败仍保持失败候选',async()=>{
    for(const failed of [false,true]){
      const result=await cleanupFixture({failed});assert.ifError(result.error);assert.equal(result.writes,1);
      assert.deepEqual(result.rows.map(row=>row.id).sort((a,b)=>a-b),failed?[1,4,10]:[3,4,10]);
      assert.equal(result.current.status,'in_progress');assert.equal(result.current.conclusion,null);
    }
  });
  test('真实清理正文拒绝API失败、删除未确认、attempt漂移和成功交付缺失',async()=>{
    for(const options of [{failDelete:true},{unchangedDelete:true},{drift:true},{missingRelease:true}]){
      const result=await cleanupFixture(options);assert.ok(result.error);assert.equal(result.writes,0);
      assert.ok(result.rows.some(row=>row.id===4));assert.ok(result.rows.some(row=>row.id===10));
    }
  });
  test('旧成功删除后再失败明确报错，不虚报两条终态或恢复已删除Run',async()=>{
    const result=await cleanupFixture({afterSuccessDelete:true});assert.match(result.error?.message||'',/删除旧成功后断网/);
    assert.equal(result.writes,0);assert.equal(result.rows.some(row=>row.id===1),false);
    assert.equal(result.current.status,'in_progress');assert.equal(result.current.conclusion,null);
  });
  test('旧Run缺失时有完整更高版本替代才删旧正式资产，没有替代仍保留',async()=>{
    for(const failed of [false,true]){
      const result=await cleanupFixture({failed,orphanRelease:true});assert.ifError(result.error);
      assert.deepEqual(result.rows.map(row=>row.id).sort((a,b)=>a-b),failed?[4,10]:[3,4,10]);
      assert.equal(result.writes,1);assert.equal(result.releases.some(row=>row.id===101),failed);
      assert.equal(result.tags.has(releaseName('linux-arm','0.9.0')),failed);
      assert.equal(result.warnings.length,failed?1:0);
      assert.equal(result.calls.some(row=>row[0]==='deleteRelease'&&row[1]===100),false);
    }
  });
  test('孤立交付的替代缺失、旧资产或Tag漂移、不可变及删除失败明确报错',async()=>{
    for(const options of [{replacementMissing:true},{badOldAsset:true},{badOldTag:true},{immutable:true},{unchangedTagDelete:true},{failAssetDelete:true}]){
      const result=await cleanupFixture({orphanRelease:true,...options});assert.ok(result.error,JSON.stringify(options));
      assert.equal(result.writes,0);assert.ok(result.rows.some(row=>row.id===4));assert.ok(result.rows.some(row=>row.id===10));
      assert.ok(result.releases.some(row=>row.id===100));
      if(!options.failAssetDelete)assert.equal(result.calls.some(row=>row[0]==='deleteRelease'),false);
    }
    const active=await cleanupFixture({oldActive:true});assert.ifError(active.error);
    assert.ok(active.rows.some(row=>row.id===1&&row.status==='in_progress'));
    assert.ok(active.releases.some(row=>row.id===101));assert.ok(active.tags.has(releaseName('linux-arm','0.9.0')));
  });
  test('显式维护只处理真实终态，保留全部活动任务及异平台记录',async()=>{
    const rows=[run(1),run(2,'failure'),run(3),run(4,'failure'),{...run(5),status:'in_progress',conclusion:null},
      {...run(6),event:'repository_dispatch'},{...run(7),path:'.github/workflows/macos.yml'}];
    const get=async id=>structuredClone(rows.find(row=>row.id===id)??null),deleted=[];
    const result=await cleanupNativeRuns('linux-arm',{list:async()=>structuredClone(rows),get,
      remove:async id=>{deleted.push(id);rows.splice(rows.findIndex(row=>row.id===id),1);}});
    assert.deepEqual(deleted,[2,6,1]);assert.deepEqual(rows.map(row=>row.id),[3,4,5,7]);
    assert.equal(result.success,3);assert.equal(result.failure,4);
    assert.ok(result.protected.includes(5));assert.ok(result.protected.includes(7));
    assert.equal((await cleanupNativeRuns('linux-arm',{list:async()=>structuredClone(rows),get,remove:()=>assert.fail('重复删除')})).deleted.length,0);
  });
  test('编译模块只公开编译辅助接口，不重新加入产品流程执行器',async()=>{
    const build=await import('./build.mjs');
    assert.deepEqual(Object.keys(build).sort(),['assertBuildWork','buildPlatform','checkWork','npmCLI','platformCargoEnvironment','runBuildProcess','stagePostgresResources','stageWindowsOpenSsl'].sort());
  });

}
// END INLINE TESTS

// 只返回声明数据；派发与观察归通用调用方，版本及交付归本仓YAML。
export function describeAction({flow,platform,inputs={}}){
 const workflow=product.platforms[platform]?.workflow;
 if(flow==='release'&&workflow)return {repository:"crcfrcn/citizenchain",workflow,ref:'main',inputs};
 throw Error('本产品未声明该流程：'+flow+'·'+platform);
}

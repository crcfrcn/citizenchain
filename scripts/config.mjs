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
        "environment": "rtupg",
        "name": "NRC_PUBLIC_KEY",
        "title": "NRC_PUBLIC_KEY",
        "description": "国储会现任管理员公钥；开发升级与节点实例部署共用的冷签信任锚",
        "revealable": true,
        "accessLevel": "protected",
        "kind": "authorization",
        "format": "hex32"
      },
      {
        "environment": "rtupg",
        "name": "CHAIN_GENESIS_HASH",
        "title": "CHAIN_GENESIS_HASH",
        "description": "公民链固定创世块哈希",
        "revealable": true,
        "accessLevel": "public"
      }
    ]
  }
});
export function describeConfiguration(){return JSON.parse(JSON.stringify(manifest));}

// 产品交付声明归本仓；运行时协议版本只读取 runtime/src/lib.rs，不在此复制。
export const product=Object.freeze({id:'citizenchain',title:'公民链',repository:'crcfrcn/citizenchain',entry:'scripts/build.mjs',
  platforms:Object.freeze({
    macos:{name:'macOS',workflow:'macos.yml',host:'darwin',arch:'arm64',target:'aarch64-apple-darwin',bundle:'dmg',extension:'.dmg'},
    windows:{name:'Windows',workflow:'windows.yml',host:'win32',arch:'x64',target:'x86_64-pc-windows-msvc',bundle:'nsis',extension:'.exe'},
    'linux-arm':{name:'LinuxARM',workflow:'linuxarm.yml',host:'linux',arch:'arm64',target:'aarch64-unknown-linux-gnu',bundle:'deb',extension:'.deb'},
    'linux-amd':{name:'LinuxAMD',workflow:'linuxamd.yml',host:'linux',arch:'x64',target:'x86_64-unknown-linux-gnu',bundle:'deb',extension:'.deb'},
    wasm:{name:'WASM',workflow:'wasm.yml',host:'linux',arch:'x64',target:'wasm32v1-none',files:['citizenchain.wasm','citizenchain.compact.wasm','citizenchain.compact.compressed.wasm']},
  }),
  tools:Object.freeze({node:'25.2.1',rust:'1.97.1',protoc:'35.0',postgres:'17.11',xcode:'27.0'}),
  locks:Object.freeze(['Cargo.lock','crates/protocol/Cargo.lock','crates/scanner/package-lock.json','node/frontend/package-lock.json','onchina/platform/frontend/package-lock.json']),
});

// 自动化只共用纯声明与纯规则；网络、执行、发布及清理步骤归既有 Workflow。
const requireValue=(condition,message)=>{if(!condition)throw Error(message);};
export function deliveryVersion(value){
  requireValue(typeof value==='string'&&value.trim()===value&&/^(0|[1-9][0-9]*)\.(0|[1-9][0-9]*)\.(0|[1-9][0-9]*)$/u.test(value)
    &&value.split('.').every(part=>BigInt(part)<2n**64n),'交付版本必须是规范 X.Y.Z');
  return value;
}
export function releaseName(platform,value){
  requireValue(Object.hasOwn(product.platforms,platform),'产品平台无效');
  return product.title+'·'+product.platforms[platform].name+'·v'+deliveryVersion(value);
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
export function readReleaseProof(platform,release){
  const lines=String(release.body||'').split(/\r?\n/u).filter(line=>line.startsWith('<!-- automation:')&&line.endsWith(' -->'));
  requireValue(lines.length===1,'正式版本来源证明必须唯一');
  const proof=JSON.parse(lines[0].slice(16,-4));
  requireValue(proof.schema===1&&proof.repository===product.repository&&proof.product_id===product.id&&proof.platform===platform
    &&proof.workflow==='.github/workflows/'+product.platforms[platform].workflow
    &&proof.tag===releaseName(platform,proof.version)&&release.tag_name===proof.tag&&release.name===proof.tag
    &&/^[a-f0-9]{40}$/u.test(proof.source_sha||'')&&release.target_commitish===proof.source_sha
    &&Number.isSafeInteger(proof.run_id)&&proof.run_id>0&&Number.isSafeInteger(proof.run_attempt)&&proof.run_attempt>0,'正式版本来源不符');
  return proof;
}
export function assertRelease(platform,release,run,{partial=false,draft=false}={}){
  assertRun(platform,run);const proof=readReleaseProof(platform,release),expected=artifactNames(platform,proof.version);
  requireValue(proof.run_id===run.id&&proof.run_attempt===run.run_attempt&&proof.source_sha===run.head_sha
    &&Number.isSafeInteger(release.id)&&release.id>0&&release.prerelease===false
    &&(draft||release.draft===false&&Boolean(release.published_at)),'正式版本与原编译 Run 不符');
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
export function nextDeliveryVersion(platform,runs,releases,initial){
  const latest=retainedBuildRuns(platform,runs).success;
  if(!latest)return deliveryVersion(initial);
  const matching=releases.filter(release=>{
    try{return readReleaseProof(platform,release).run_id===latest.id;}catch{return false;}
  });
  requireValue(matching.length===1,'最新成功编译没有唯一正式交付');
  const proof=assertRelease(platform,matching[0],latest),parts=proof.version.split('.').map(BigInt);
  parts[2]++;return deliveryVersion(parts.join('.'));
}
// 调用方只传输这些公开请求；恢复描述只能请求原Run收尾，不能派发编译或rerun。
export function automationRequest(platform,operation,input={}){
  requireValue(Object.hasOwn(product.platforms,platform),'产品平台无效');
  const workflow=product.platforms[platform].workflow;
  if(operation==='runs')return {path:`actions/workflows/${workflow}/runs?branch=main&event=workflow_dispatch`,field:'workflow_runs'};
  if(operation==='releases')return {path:'releases'};
  if(operation==='launch'){
    requireValue(/^[a-f0-9]{40}$/u.test(input.sha||''),'派发源码无效');
    return {path:`actions/workflows/${workflow}/dispatches`,method:'POST',body:{ref:'main',inputs:{version:deliveryVersion(input.version),source_sha:input.sha}}};
  }
  if(operation==='run'||operation==='cancel'){
    requireValue(Number.isSafeInteger(input.runId)&&input.runId>0,'原RunID无效');
    return {path:'actions/runs/'+input.runId+(operation==='cancel'?'/cancel':''),...(operation==='cancel'?{method:'POST'}:{})};
  }
  if(operation==='settle'){
    const run=assertRun(platform,input.run),prefix=product.title+'·'+product.platforms[platform].name+'·v';
    requireValue(run.display_title?.startsWith(prefix),'原Run没有准确交付版本');
    return {path:'dispatches',method:'POST',body:{event_type:'citizenchain-finalize-'+platform,client_payload:{platform,
      version:deliveryVersion(run.display_title.slice(prefix.length)),run_id:run.id,run_attempt:run.run_attempt,source_sha:run.head_sha}}};
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
  secretRefs:[{environment:'rtupg',name:'NRC_PUBLIC_KEY'},{environment:'topup',name:'NODE_WS'}]});
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
  function release(source){
    const name=artifactNames('linux-arm','1.0.0')[0],label=artifactLabel('linux-arm','1.0.0',name),hash='f'.repeat(64);
    const proof={schema:1,repository:product.repository,product_id:product.id,platform:'linux-arm',workflow:source.path,
      version:'1.0.0',tag:source.display_title,run_id:source.id,run_attempt:source.run_attempt,source_sha:source.head_sha,assets:[{name,label,size:3,sha256:hash}]};
    return {id:100,tag_name:proof.tag,name:proof.tag,target_commitish:source.head_sha,draft:false,prerelease:false,published_at:'2026-10-19T00:00:00Z',
      body:'<!-- automation:'+JSON.stringify(proof)+' -->',assets:[{id:200,name,label,size:3,digest:'sha256:'+hash,state:'uploaded',url:`https://api.github.com/repos/${product.repository}/releases/assets/200`}]};
  }
  test('交付版本和五个平台完整产物声明拒绝歧义',()=>{
    for(const value of ['1','01.0.0','1.0.0-rc','1.0.0\n',null])assert.throws(()=>deliveryVersion(value));
    assert.equal(releaseName('wasm','2.3.4'),'公民链·WASM·v2.3.4');
    assert.equal(artifactNames('wasm','2.3.4').length,3);assert.equal(Object.keys(product.platforms).length,5);
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
  test('恢复描述只绑定原Run收尾，取消也不走编译dispatch或rerun',()=>{
    const source=run(42),request=automationRequest('linux-arm','settle',{run:source});
    assert.equal(request.path,'dispatches');assert.equal(request.body.event_type,'citizenchain-finalize-linux-arm');
    assert.equal(request.body.client_payload.run_id,42);assert.equal(request.body.client_payload.run_attempt,1);
    assert.equal(automationRequest('linux-arm','cancel',{runId:42}).path,'actions/runs/42/cancel');
    assert.throws(()=>automationRequest('linux-arm','launch',{version:'1.0.0',sha:'invalid'}));
  });
  test('编译模块只公开编译辅助接口，不重新加入产品流程执行器',async()=>{
    const build=await import('./build.mjs');
    assert.deepEqual(Object.keys(build).sort(),['assertBuildWork','checkWork','npmCLI','platformCargoEnvironment','runBuildProcess','stageWindowsOpenSsl'].sort());
  });
}
// END INLINE TESTS

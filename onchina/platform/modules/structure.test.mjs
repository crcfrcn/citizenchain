// 结构回归复用产品既有测试流程；源码定位不启动业务服务。
import {test} from 'node:test';
import assert from 'node:assert/strict';
import {readFileSync,readdirSync,existsSync,statSync,realpathSync,mkdirSync,mkdtempSync,rmSync,writeFileSync,symlinkSync,lstatSync,cpSync} from 'node:fs';
import {resolve,dirname,join,relative,isAbsolute,sep} from 'node:path';
import {execFileSync} from 'node:child_process';

const root=resolve(import.meta.dirname,'../../..');
const onchina=join(root,'onchina');
const layout=JSON.parse(readFileSync(join(import.meta.dirname,'layout.fixture.json'),'utf8'));
const expected=JSON.parse(readFileSync(join(import.meta.dirname,'routes.fixture.json'),'utf8'));
const source=path=>readFileSync(join(onchina,path),'utf8');
const ignored=new Set(['.git','target','node_modules','dist','tasks']);
function files(at){return readdirSync(at,{withFileTypes:true}).filter(entry=>!ignored.has(entry.name)&&!entry.isSymbolicLink()).flatMap(entry=>entry.isDirectory()?files(join(at,entry.name)):[join(at,entry.name)]);}
function retiredReferences(text,onchinaContext=false){return [...layout.retiredPatterns,...(onchinaContext?layout.onchinaOnlyPatterns:[])].filter(pattern=>new RegExp(pattern,'u').test(text));}
// 读取真实ModuleRoute声明；不执行Rust或生成路由。
function routeRecords(text,owner){
 const result=[];
 for(const match of text.matchAll(/ModuleRoute::new\(\s*"([^"\n]+)"\s*,\s*"([^"\n]+)"\s*,\s*&\[([^\]]+)\]\s*,/gu)){
  let index=match.index+match[0].length,start=index,depth=1;
  for(;index<text.length;index++){if(text[index]==='(')depth++;if(text[index]===')'&&--depth===0)break;}
  const handlers=[...text.slice(start,index).matchAll(/\b(get|post|patch|put|delete|head|options)\(\s*([\w:]+)/gu)];
  const methods=[...match[3].matchAll(/"([A-Z]+)"/gu)].map(value=>value[1].toLowerCase());
  assert.deepEqual(handlers.map(value=>value[1]),methods,owner+': '+match[2]);
  result.push({group:match[1],path:match[2],methods,handlers:handlers.map(value=>value[2].replace(/^crate::/u,'')),owner});
 }
 return result;
}
const routeSource=owner=>owner==='platform'?'platform/communication/routes.rs':owner+'/main.rs';
const ordered=routes=>routes.toSorted((a,b)=>(a.group+' '+a.path).localeCompare(b.group+' '+b.path));
function definitionOwners(kind,name){
 const pattern=new RegExp('\\b'+kind+'\\s+'+name+'\\b','u');
 // 夹具使用统一斜线；源码读取仍保留原生平台路径。
 return files(onchina).filter(file=>file.endsWith('.rs')&&pattern.test(readFileSync(file,'utf8'))).map(file=>relative(onchina,file).split(sep).join('/'));
}

test('五个部分、功能目录和真实入口与结构夹具一致',()=>{
 assert.deepEqual(readdirSync(onchina,{withFileTypes:true}).filter(entry=>entry.isDirectory()).map(entry=>entry.name).sort(),layout.parts);
 assert.deepEqual(readdirSync(onchina,{withFileTypes:true}).filter(entry=>entry.isFile()).map(entry=>entry.name).sort(),layout.rootFiles);
 for(const [part,expectedFunctions]of Object.entries(layout.functions)){
  assert.deepEqual(readdirSync(join(onchina,part),{withFileTypes:true}).filter(entry=>entry.isDirectory()).map(entry=>entry.name).sort(),expectedFunctions);
 }
 for(const path of layout.requiredFiles)assert.ok(existsSync(join(onchina,path)),path);
 for(const path of layout.absentFiles)assert.equal(existsSync(join(onchina,path)),false,path);
 for(const file of files(onchina)){
  const parts=relative(root,file).split(sep);
  assert.equal(parts.slice(1,-1).includes('src'),false,file);
  assert.equal(parts.slice(1,-1).includes('backend'),false,file);
  assert.ok(parts.length<=4,file);
 }
});

test('通用会话、提交和注册局办理函数只在所属功能中定义',()=>{
 for(const [name,owner]of Object.entries(layout.functionOwners))assert.deepEqual(definitionOwners('fn',name),[owner],name);
 for(const [name,owner]of Object.entries(layout.structOwners))assert.deepEqual(definitionOwners('struct',name),[owner],name);
 for(const [name,owner]of Object.entries(layout.constantOwners)){
  assert.deepEqual(definitionOwners('const',name),[owner],name);
  const declaration=source(owner).match(new RegExp('\\bconst\\s+'+name+'\\s*:[^=]+=[ \\t]*([^;]+);','u'));
  assert.equal(declaration?.[1].trim(),layout.constantValues[name],name);
 }
 assert.match(source('platform/interface.rs'),/#\[path = "transactions\/mod\.rs"\]\s*mod transactions;/u);
 assert.match(source('platform/main.rs'),/include!\("interface\.rs"\)/u);
 for(const owner of Object.keys(layout.moduleTargets).filter(owner=>owner!=='platform')){
  assert.match(source(layout.moduleTargets[owner]),/include!\("\.\.\/platform\/interface\.rs"\)/u,owner);
 }
 for(const owner of ['registry','legislation','private','judicial'])assert.doesNotMatch(source('platform/interface.rs'),new RegExp('mod '+owner+'\\b','u'));
});

test('显式Rust模块、include及前端相对导入都指向实际文件',()=>{
 const candidates=value=>[value,...['.ts','.tsx','.js','.jsx','.json','.css'].map(ext=>value+ext),...['.ts','.tsx','.js'].map(ext=>join(value,'index'+ext))];
 for(const file of files(onchina)){
  const text=/\.(?:rs|ts|tsx)$/u.test(file)?readFileSync(file,'utf8'):'';
  if(file.endsWith('.rs')){
   for(const match of text.matchAll(/#\[path\s*=\s*"([^"]+)"\]/gu))assert.ok(existsSync(resolve(dirname(file),match[1])),file+': '+match[1]);
   for(const match of text.matchAll(/include!\("([^"]+)"\)/gu))assert.ok(existsSync(resolve(dirname(file),match[1])),file+': '+match[1]);
  }
  if(/\.(?:ts|tsx)$/u.test(file))for(const match of text.matchAll(/(?:\bfrom\s*|\bimport\s*(?:\(\s*)?)["'](\.[^"'\n]+)["']/gu)){
   const value=resolve(dirname(file),match[1].split('?')[0]);assert.ok(candidates(value).some(path=>existsSync(path)&&statSync(path).isFile()),file+': '+match[1]);
  }
 }
 assert.match(source('platform/frontend/index.html'),/src="\/main\.tsx"/u);
 // 根解决方案只引用现行配置，避免旧前端路径被迁移遗漏或重新引入。
 const solution=JSON.parse(source('tsconfig.json'));
 assert.deepEqual(solution.files,[]);
 assert.ok(solution.references.length>0);
 for(const reference of solution.references){
  const path=resolve(onchina,reference.path),within=relative(onchina,path);
  assert.ok(within!==''&&within!=='..'&&!within.startsWith('../')&&!within.startsWith('..\\')&&!isAbsolute(within),reference.path);
  assert.ok(existsSync(path)&&statSync(path).isFile(),reference.path);
 }
 assert.equal(existsSync(join(onchina,'frontend/tsconfig.json')),false);
});

test('公共机构信息面板只定义在平台且原有消费者引用同一实现',()=>{
 const candidates=files(onchina).filter(file=>/\.(?:ts|tsx)$/u.test(file)&&/export function OwnInstitutionInfoPanel\b/u.test(readFileSync(file,'utf8')));
 assert.deepEqual(candidates.map(file=>relative(onchina,file).split(sep).join('/')),[layout.commonPanel]);
 for(const file of layout.commonPanelConsumers){
  const text=source(file);assert.match(text,/import[^;]*OwnInstitutionInfoPanel[^;]*institutions_OwnInstitutionInfoPanel['"]/u,file);
  assert.doesNotMatch(text,/import[^;]*OwnInstitutionInfoPanel[^;]*judicial_JudicialDisplay/u,file);
 }
});

test('生产源码、注释和根技术文档不含退役目录或错误协议真源',()=>{
 for(const file of files(onchina).filter(file=>/\.(?:rs|ts|tsx)$/u.test(file))){
  assert.deepEqual(retiredReferences(readFileSync(file,'utf8'),true),[],relative(root,file));
 }
 for(const file of layout.consumerDocuments)assert.deepEqual(retiredReferences(readFileSync(join(root,file),'utf8'),file==='CitizenChainOnChina.md'),[],file);
});

test('旧路径负例确实被断言识别，当前功能路径不会被误报',()=>{
 for(const example of layout.retiredExamples)assert.ok(retiredReferences(example,true).length>0,example);
 for(const example of ['onchina/platform/transactions/handler.rs','onchina/registry/admins/catalog.rs','node/src/main.rs','onchina/private/membership/handler.rs'])assert.deepEqual(retiredReferences(example,true),[],example);
});

test('原HTTP方法路径、路由分区和迁移后的handler归属逐项一致',()=>{
 const text=source('platform/communication/routes.rs');
 assert.deepEqual(ordered(Object.keys(layout.moduleTargets).flatMap(owner=>routeRecords(source(routeSource(owner)),owner))),ordered(expected.routes));
 const admin=[...text.matchAll(/\.route_layer\(\s*middleware::from_fn_with_state\(\s*state\.clone\(\),\s*([\w:]+)/gu)].map(value=>value[1]);
 const global=[...text.matchAll(/\.layer\(\s*middleware::from_fn_with_state\(\s*state\.clone\(\),\s*([\w:]+)/gu)].map(value=>value[1]);
 global.push(...[...text.matchAll(/\.layer\((build_cors_layer)\(\)\)/gu)].map(value=>value[1]));
 assert.deepEqual(admin,expected.adminMiddleware);
 assert.deepEqual(global,expected.globalMiddleware);
});

// 跨目录消费者必须纳入扫描；负例夹具只在其所有者测试中解释。
test('仓库消费者不再引用退役OnChina输入，Runtime生成器读取唯一现行字典',()=>{
 const paths=layout.consumerRoots.flatMap(path=>files(join(root,path))).filter(path=>/\.(?:rs|ts|tsx|py|mjs|json|toml|ya?ml|md)$/u.test(path));
 for(const file of paths){
  const text=readFileSync(file,'utf8');
  for(const pattern of layout.consumerPatterns)assert.doesNotMatch(text,new RegExp(pattern,'u'),relative(root,file));
 }
 const generator=readFileSync(join(root,'runtime/primitives/gen_area_data.py'),'utf8');
 assert.match(generator,/onchina\/platform\/codes\/china\.sqlite/u);
 assert.match(generator,/\?mode=ro/u);
});

test('十二项修复保留统一分配、上传约束与真实路由语法',()=>{
 for(const name of layout.retiredDefinitions)assert.deepEqual(definitionOwners('fn',name),[],name);
 const routes=source('platform/communication/routes.rs');
 assert.doesNotMatch(routes,/\.route\(\s*"[^"\n]*\/:\w+/u);
 const declarations=Object.keys(layout.moduleTargets).map(owner=>source(routeSource(owner))).join('\n');
 assert.equal([...declarations.matchAll(/\.layer\((?:crate::)?documents::upload_body_limit\(\)\)/gu)].length,layout.uploadRoutes.length);
 for(const path of layout.uploadRoutes)assert.ok(expected.routes.some(route=>route.path===path&&route.methods.includes('post')),path);
 assert.match(source('platform/transactions/submit.rs'),/super::chain_url::rpc_http_client\(\)/u);
 assert.match(source('registry/projection/mod.rs'),/projection::commit_projection_batch/u);
});


// 只导入公开纯函数核对目标选择，不执行资源准备、工具链或构建。
test('专属构建从真实Cargo入口选择单模块且拒绝错误声明',async()=>{
 const {buildTargets,selectTargets,validateDescription}=await import('../../../scripts/onchina.mjs');
 const targets=buildTargets(root);
 assert.deepEqual([...targets.keys()],Object.keys(layout.moduleTargets));
 for(const [id,path]of Object.entries(layout.moduleTargets)){
  assert.equal(targets.get(id).entry,join(onchina,path));
  assert.deepEqual(selectTargets(root,id).map(target=>target.id),[id]);
 }
 assert.throws(()=>selectTargets(root,'missing'),/不存在/);
 const description={id:'registry',version:'1.2.3',interface_version:1,frontend_source:'frontend/Workspace.tsx',frontend_exports:['RegistryWorkspace']};
 assert.equal(validateDescription(description,targets.get('registry')),description);
 for(const patch of [{id:'private'},{version:'1'},{interface_version:2},{frontend_source:'../Workspace.tsx'}]){
  assert.throws(()=>validateDescription({...description,...patch},targets.get('registry')),/声明无效/);
 }
});
test('宿主生产装配和公开前端不再静态编译其他业务模块',()=>{
 const shared=source('platform/interface.rs');
 assert.doesNotMatch(shared,/#\[path = "\.\.\/(?:registry|legislation|private|judicial)\//u);
 for(const file of files(join(onchina,'platform/frontend')).filter(file=>/\.(?:ts|tsx)$/u.test(file)&&!file.endsWith('.test.ts'))){
  const text=readFileSync(file,'utf8');
  assert.doesNotMatch(text,/from\s*['"]\.\.\/\.\.\/(?:registry|legislation|private|judicial)\/frontend\//u,file);
 }
 assert.match(source('platform/modules/mod.rs'),/verify_frontend\(&frontend, &description\)/u);
 assert.match(source('platform/modules/mod.rs'),/drop\(child\.stdin\.take\(\)\)/u);
});


// 编译阶段显式登记此项；普通测试没有产物输入时跳过，不隐式构建或启动业务服务。
test('实际模块产物的程序声明与JS/CSS首行逐项一致', {skip: !process.env.ONCHINA_TEST_MODULE_ROOT}, async()=>{
 const directory=process.env.ONCHINA_TEST_MODULE_ROOT;
 assert.ok(isAbsolute(directory));assert.equal(realpathSync(directory),directory);
 const {buildTargets,validateDescription}=await import('../../../scripts/onchina.mjs');
 const targets=buildTargets(root);
 const ids=JSON.parse(process.env.ONCHINA_TEST_MODULE_IDS||'null');
 assert.ok(Array.isArray(ids)&&ids.length>0&&new Set(ids).size===ids.length,'必须交付所选实际模块ID');
 for(const id of ids){
  assert.ok(typeof id==='string'&&targets.has(id)&&id!=='platform','实际Cargo模块目标');
  const target=targets.get(id),base=join(directory,id),executable=join(base,target.name+(process.platform==='win32'?'.exe':''));
  assert.equal(realpathSync(base),base);assert.equal(realpathSync(executable),executable);
  const description=validateDescription(JSON.parse(execFileSync(executable,['--describe'],{encoding:'utf8',timeout:10000,maxBuffer:1024*1024})),target);
  for(const name of ['module.js','module.css']){
   const path=join(base,'frontend',name);assert.equal(realpathSync(path),path);
   const header=readFileSync(path,'utf8').split(/\r?\n/u)[0];
   assert.ok(header.startsWith('/*onchina:')&&header.endsWith('*/'),name);
   assert.deepEqual(JSON.parse(header.slice(10,-2)),{id:description.id,version:description.version,interface_version:description.interface_version});
  }
 }
});


test('原生平台读取产品声明并拒绝未声明架构',async()=>{
 const {nativePlatform}=await import('../../../scripts/onchina.mjs');
 const {product}=await import('../../../scripts/config.mjs');
 for(const [id,declared]of Object.entries(product.platforms).filter(([id])=>id!=='wasm'))assert.equal(nativePlatform(declared.host,declared.arch),id);
 assert.throws(()=>nativePlatform('unsupported','unsupported'),/未声明/);
});


// 只核对公开协调入口和真实文件生命周期，不执行编译、依赖准备或服务。
test('专属现场互斥、普通失败清理、退出未确认保留',async()=>{
 const {withOnChinaWork}=await import('../../../scripts/onchina.mjs');
 const base=join(root,'target/test');mkdirSync(base,{recursive:true});
 const directory=mkdtempSync(join(base,'onchina-work-')),work=join(directory,'work');
 try{
  await withOnChinaWork(work,async()=>{
   const sentinel=join(work,'owned');writeFileSync(sentinel,'fixture');
   await assert.rejects(withOnChinaWork(work,async()=>assert.fail('不能进入其他活动现场')),error=>error.code==='EEXIST');
   assert.equal(readFileSync(sentinel,'utf8'),'fixture');
  });
  assert.equal(existsSync(work),false);
  const failure=Error('fixture failure');
  await assert.rejects(withOnChinaWork(work,async()=>{throw failure;}),error=>error===failure);
  assert.equal(existsSync(work),false);
  const cancelled=Object.assign(Error('fixture interrupted'),{retainWork:true});
  await assert.rejects(withOnChinaWork(work,async()=>{throw cancelled;}),error=>error===cancelled);
  assert.equal(existsSync(work),true);
 }finally{rmSync(directory,{recursive:true,force:true});}
});

test('npm入口、平台隔离与四端前置验收登记保持闭环',async()=>{
 const {argumentsOf,buildPaths}=await import('../../../scripts/onchina.mjs');
 const {product,artifactNames}=await import('../../../scripts/config.mjs');
 const pkg=JSON.parse(source('platform/frontend/package.json'));
 assert.equal(pkg.scripts.build,'node ../../../scripts/onchina.mjs build');
 assert.equal(pkg.scripts.dev,'npm run build && npm run preview');
 assert.equal(pkg.scripts.preview,'node ../../../scripts/onchina.mjs preview');
 assert.equal(argumentsOf(['build','--platform','macos','--module','registry']).selection,'registry');
 for(const action of ['prepared','frontend','missing'])assert.throws(()=>argumentsOf([action]),/动作无效/);
 assert.throws(()=>argumentsOf(['build','--project','/tmp/foreign']),/参数无效/);
 for(const [id,declared]of Object.entries(product.platforms).filter(([id])=>id!=='wasm')){
  const paths=buildPaths(id,'registry');
  assert.equal(paths.work,join(root,'target/onchina',id,'work'));
  assert.equal(paths.output,join(root,'target/onchina',id,'registry'));
  assert.notEqual(paths.work,join(root,'target/build',id));
  const workflow=readFileSync(join(root,'.github/workflows',declared.workflow),'utf8');
  const compile=workflow.indexOf('node scripts/build.mjs '+id);
  assert.ok(compile>0,id+' compile registration');
  for(const command of ['onchina/platform/modules/structure.test.mjs','npm run typecheck --prefix onchina/platform/frontend','npm test --prefix onchina/platform/frontend']){
   const index=workflow.indexOf(command);assert.ok(index>=0&&index<compile,id+': '+command);
  }
 }
 // App仅供macOS核对，正式交付集合仍只有一个DMG。
 assert.deepEqual(artifactNames('macos','1.0.0'),['citizenchain-macOS-v1.0.0.dmg']);
 assert.throws(()=>buildPaths('wasm'),/目标无效/);
 assert.throws(()=>buildPaths('macos','../escape'),/目标无效/);
});


test('宿主产物验收读取真实资源并拒绝缺失、越界和覆盖工程',async()=>{
 const {withOnChinaWork,verifyPreparedOnChina}=await import('../../../scripts/onchina.mjs');
 const base=join(root,'target/test');mkdirSync(base,{recursive:true});
 const directory=mkdtempSync(join(base,'onchina-assets-')),work=join(directory,'work');
 try{
  await withOnChinaWork(work,async()=>{
   const frontend=join(work,'frontend');mkdirSync(join(frontend,'assets'),{recursive:true});
   writeFileSync(join(frontend,'assets/main.js'),'/* fixture */');
   writeFileSync(join(frontend,'assets/main.css'),'/* fixture */');
   const html=join(frontend,'index.html');
   const input={project:root,work,environment:{},result:{targets:['platform'],frontend}};
   writeFileSync(html,'<script src="./assets/main.js"></script><link href="./assets/main.css">');
   await verifyPreparedOnChina(input);
   rmSync(join(frontend,'assets/main.css'));
   await assert.rejects(verifyPreparedOnChina(input),error=>error.code==='ENOENT');
   writeFileSync(html,'<script src="../../outside.js"></script>');
   await assert.rejects(verifyPreparedOnChina(input),/越界或缺失/);
   writeFileSync(html,'<div>fixture</div>');
   await assert.rejects(verifyPreparedOnChina(input),/缺少真实脚本/);
   await assert.rejects(verifyPreparedOnChina({...input,result:{targets:['platform'],frontend:join(work,'source')}}),/不能覆盖工程/);
  });
  assert.equal(existsSync(work),false);
 }finally{rmSync(directory,{recursive:true,force:true});}
});


// 直接执行既有工作流脚本，覆盖失败码、无资源及文件/目录链接诊断，不跟随链接。
test('两Linux失败资源诊断保留原编译退出码且只读取链接', {skip: process.platform === 'win32'}, ()=>{
 const base=join(root,'target/test');mkdirSync(base,{recursive:true});
 const directory=mkdtempSync(join(base,'linux-diagnostics-')),work=join(directory,'work');
 try{
  for(const [platform,workflow]of [['linux-arm','linuxarm.yml'],['linux-amd','linuxamd.yml']]){
   const text=readFileSync(join(root,'.github/workflows',workflow),'utf8');
   const block=name=>text.split('    - name: '+name+'\n')[1].split('    - name: ')[0];
   const script=value=>value.split('      run: |\n')[1].split('\n').filter(line=>line.startsWith('        ')).map(line=>line.slice(8)).join('\n');
   const compile=block('编译及核对完整产物'),diagnostic=block('输出失败资源链接');
   assert.match(compile,/^      id: compile$/mu);
   assert.equal(script(compile),'set -euo pipefail\nnode scripts/build.mjs '+platform);
   assert.throws(()=>execFileSync('bash',['-e','-c','node(){ return 23; };\n'+script(compile)],{stdio:'pipe'}),error=>error.status===23);
   assert.match(diagnostic,/if: \$\{\{ failure\(\) && steps\.compile\.outcome == 'failure' \}\}/u);
   const run=()=>execFileSync('bash',['-e','-c',script(diagnostic)],{encoding:'utf8',env:{...process.env,CITIZENCHAIN_WORK_DIR:work}});
   assert.equal(run(),'');
   const resources=join(work,'resources'),outside=join(directory,'outside');
   mkdirSync(resources,{recursive:true});mkdirSync(outside,{recursive:true});
   writeFileSync(join(resources,'regular'),'owned');writeFileSync(join(outside,'target'),'target');
   symlinkSync(outside,join(resources,'directory-link'));symlinkSync(join(outside,'target'),join(resources,'file-link'));
   symlinkSync(join(outside,'missing'),join(outside,'nested-link'));
   const output=run();
   for(const name of ['directory-link','file-link'])assert.ok(output.includes(join(resources,name)+' -> '));
   assert.ok(output.includes(outside));assert.ok(output.includes(join(outside,'target')));
   assert.ok(!output.includes('nested-link'));assert.ok(!output.includes(join(resources,'regular')));
   assert.equal(readFileSync(join(outside,'target'),'utf8'),'target');assert.ok(lstatSync(join(resources,'file-link')).isSymbolicLink());
   rmSync(work,{recursive:true});rmSync(outside,{recursive:true});
  }
 }finally{rmSync(directory,{recursive:true,force:true});}
});


// 执行实际工作流物理复制命令，再用既有编译API复制，验证别名链不会进入资源包。
test('macOS和两Linux PostgreSQL物理副本保留内容且坏链接仍失败', {skip: process.platform === 'win32'}, ()=>{
 const base=join(root,'target/test');mkdirSync(base,{recursive:true});
 const directory=mkdtempSync(join(base,'pg-physical-'));
 try{
  for(const workflow of ['linuxarm.yml','linuxamd.yml','macos.yml']){
   const text=readFileSync(join(root,'.github/workflows',workflow),'utf8');
   const name=workflow==='macos.yml'?'准备 macOS 原生依赖、官方 protoc 与签名身份':'准备 Linux 节点依赖和官方 protoc';
   const preparation=text.split('    - name: '+name+'\n')[1].split('    - name: ')[0];
   const copy=preparation.split('\n').find(line=>line.trimStart().startsWith('cp -RL ')).trim();
   assert.ok(preparation.includes('echo "CITIZENCHAIN_PG_DIST=$RUNNER_TEMP/pg-runtime" >> "$GITHUB_ENV"'));
   const original=join(directory,'pg-dist'),physical=join(directory,'pg-runtime'),resources=join(directory,'resources');
   for(const name of ['bin','lib','share'])mkdirSync(join(original,name),{recursive:true});
   writeFileSync(join(original,'bin/postgres'),'fixture-binary');writeFileSync(join(original,'share/catalog'),'fixture-catalog');
   writeFileSync(join(original,'lib/libpq.so.5.17'),'fixture-library');
   symlinkSync('libpq.so.5.17',join(original,'lib/libpq.so.5'));symlinkSync('libpq.so.5',join(original,'lib/libpq.so'));
   // macOS安装的四种dylib别名同样必须物理复制，原Linux多级别名断言保持。
   const libraries=[['pq','5'],['pgtypes','3'],['ecpg','6'],['ecpg_compat','3']];
   for(const [name,version]of libraries){
    writeFileSync(join(original,'lib','lib'+name+'.'+version+'.dylib'),'fixture-'+name);
    symlinkSync('lib'+name+'.'+version+'.dylib',join(original,'lib','lib'+name+'.dylib'));
   }
   execFileSync('bash',['-e','-c',copy],{env:{...process.env,RUNNER_TEMP:directory}});
   for(const name of ['bin','lib','share'])cpSync(join(physical,name),join(resources,name),{recursive:true,dereference:true});
   for(const name of ['libpq.so','libpq.so.5','libpq.so.5.17']){
    assert.ok(lstatSync(join(physical,'lib',name)).isFile());assert.ok(lstatSync(join(resources,'lib',name)).isFile());
    assert.equal(readFileSync(join(resources,'lib',name),'utf8'),'fixture-library');
   }
   for(const [name,version]of libraries){
    for(const file of ['lib'+name+'.dylib','lib'+name+'.'+version+'.dylib']){
     assert.ok(lstatSync(join(physical,'lib',file)).isFile());assert.ok(lstatSync(join(resources,'lib',file)).isFile());
     assert.equal(readFileSync(join(resources,'lib',file),'utf8'),'fixture-'+name);
    }
    assert.ok(lstatSync(join(original,'lib','lib'+name+'.dylib')).isSymbolicLink());
   }
   assert.equal(readFileSync(join(resources,'bin/postgres'),'utf8'),'fixture-binary');
   assert.equal(readFileSync(join(resources,'share/catalog'),'utf8'),'fixture-catalog');
   assert.ok(lstatSync(join(original,'lib/libpq.so')).isSymbolicLink());
   rmSync(physical,{recursive:true});symlinkSync('missing',join(original,'lib/broken'));
   assert.throws(()=>execFileSync('bash',['-e','-c',copy],{env:{...process.env,RUNNER_TEMP:directory},stdio:'pipe'}),error=>error.status!==0);
   for(const path of [original,physical,resources])rmSync(path,{recursive:true,force:true});
  }
 }finally{rmSync(directory,{recursive:true,force:true});}
});

// 用官方Node模块mock执行真实编排，不编译Rust或安装依赖；所有夹具退出后清空。
test('合并Cargo仍交付五目标、单模块独立检查且失败原样终止',()=>{
 const script = String.raw`
 import {mock} from 'node:test';
 import assert from 'node:assert/strict';
 import {mkdtempSync,mkdirSync,writeFileSync,rmSync} from 'node:fs';
 import {join,dirname} from 'node:path';
 import {pathToFileURL} from 'node:url';
 const root=process.cwd(),base=join(root,'target/test');mkdirSync(base,{recursive:true});
 const work=mkdtempSync(join(base,'onchina-batch-')),calls=[],jobs=[];
 let failure='';
 const stop=new Error('原始构建失败');
 const put=(path,text='fixture')=>{mkdirSync(dirname(path),{recursive:true});writeFileSync(path,text);};
 await mock.module(pathToFileURL(join(root,'scripts/build.mjs')).href,{namedExports:{
  assertBuildWork:()=>{},checkWork:()=>{},npmCLI:()=>'',stageWindowsOpenSsl:()=>{},
  platformCargoEnvironment:(_platform,_work,env)=>({...env}),
  runBuildProcess:async(command,args,env)=>{
   if(command==='fixture-cargo'){
    calls.push(args);
    if(failure==='cargo')throw stop;
    for(let i=0;i<args.length;i++)if(args[i]==='--bin')put(join(work,'cargo/release',args[i+1]+(process.platform==='win32'?'.exe':'')));
   }else if(args[0]==='--describe'){
    const id=command.split(/[\\/]/u).at(-1).replace(/\.exe$/u,'').slice(8);
    return {stdout:JSON.stringify({id,version:'1.0.0',interface_version:1,frontend_source:'frontend/Workspace.tsx',frontend_exports:['Workspace']})};
   }else if(args[1]==='frontend-worker'){
    const job=JSON.parse(env.ONCHINA_BUILD_JOB);jobs.push(job);
    if(failure==='frontend')throw stop;
    if(job.description)put(join(job.destination,'module.js'),'module fixture');
    else{put(join(job.destination,'index.html'),'<script src="main.js"></script>');put(join(job.destination,'main.js'));}
   }
   return {stdout:''};
  }
 }});
 try{
  const {buildPreparedOnChina,nativePlatform}=await import(pathToFileURL(join(root,'scripts/onchina.mjs')).href);
  const request={project:root,platform:nativePlatform(),work,environment:{CARGO:'fixture-cargo',NODE:process.execPath,CARGO_TARGET_DIR:join(work,'cargo')},prepareDependencies:false};
  const all=await buildPreparedOnChina(request);
  assert.deepEqual(all.targets,['platform','registry','legislation','private','judicial']);
  assert.equal(calls.length,1);
  assert.deepEqual(calls[0].filter((_v,i,a)=>a[i-1]==='--bin'),['onchina','onchina-registry','onchina-legislation','onchina-private','onchina-judicial']);
  assert.deepEqual(jobs.map(job=>job.checkNode),[true,false,false,false,false]);
  calls.length=0;jobs.length=0;
  const single=await buildPreparedOnChina({...request,selection:'registry'});
  assert.deepEqual(single.targets,['registry']);assert.equal(calls.length,1);
  assert.deepEqual(calls[0].slice(-2),['--bin','onchina-registry']);assert.deepEqual(jobs.map(job=>job.checkNode),[true]);
  calls.length=0;jobs.length=0;
  await buildPreparedOnChina({...request,selection:'registry',frontendOnly:true});
  assert.equal(calls.length,0);assert.deepEqual(jobs.map(job=>job.checkNode),[true]);
  for(const phase of ['cargo','frontend']){
   calls.length=0;jobs.length=0;failure=phase;
   await assert.rejects(buildPreparedOnChina(request),error=>error===stop);
   assert.equal(calls.length,1);assert.equal(jobs.length,phase==='cargo'?0:1);
  }
 }finally{rmSync(work,{recursive:true,force:true});mock.restoreAll();}
 `;
 execFileSync(process.execPath,['--experimental-test-module-mocks','--input-type=module','-e',script],{cwd:root,stdio:'pipe'});
});

test('默认节点编译排除前端嵌入和基准CLI并保留显式能力及生产防护',()=>{
 const manifest=readFileSync(join(root,'node/Cargo.toml'),'utf8');
 assert.match(manifest,/^default = \["std"\]$/mu);
 assert.match(manifest,/^custom-protocol = \["tauri\/custom-protocol"\]$/mu);
 assert.match(manifest,/^frame-benchmarking-cli = .*optional = true/mu);
 assert.match(manifest,/"frame-benchmarking-cli\/runtime-benchmarks"/u);
 assert.match(readFileSync(join(root,'node/src/core/mod.rs'),'utf8'),/#\[cfg\(feature = "runtime-benchmarks"\)\]\s*pub\(crate\) mod benchmarking;/u);
 assert.match(readFileSync(join(root,'node/src/core/cli.rs'),'utf8'),/#\[cfg\(feature = "runtime-benchmarks"\)\]\s*#\[command\(subcommand\)\]\s*Benchmark\(/u);
 assert.match(readFileSync(join(root,'node/src/core/command.rs'),'utf8'),/#\[cfg\(feature = "runtime-benchmarks"\)\]\s*Some\(Subcommand::Benchmark/u);
 assert.match(manifest,/^frame-benchmarking = /mu);
 assert.match(readFileSync(join(root,'node/src/guard/runtime_policy.rs'),'utf8'),/dyn frame_benchmarking::Benchmark<citizenchain::Block>/u);
});

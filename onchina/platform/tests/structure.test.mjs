// 结构回归复用产品既有测试流程；源码定位不启动业务服务。
import {test} from 'node:test';
import assert from 'node:assert/strict';
import {readFileSync,readdirSync,existsSync,statSync,realpathSync} from 'node:fs';
import {resolve,dirname,join,relative,isAbsolute,sep} from 'node:path';
import {execFileSync} from 'node:child_process';

const root=resolve(import.meta.dirname,'../../..');
const onchina=join(root,'onchina');
const layout=JSON.parse(readFileSync(join(import.meta.dirname,'layout.json'),'utf8'));
const expected=JSON.parse(readFileSync(join(import.meta.dirname,'routes.json'),'utf8'));
for(const owner of ['registry','legislation','private'])expected.routes.push(...JSON.parse(readFileSync(join(onchina,owner,'tests/routes.json'),'utf8')));
let metadata;
function cargoMetadata(){return metadata??=JSON.parse(execFileSync(process.env.CARGO||'cargo',['metadata','--no-deps','--format-version','1','--locked','--offline'],{cwd:root,encoding:'utf8',maxBuffer:8*1024*1024,env:{...process.env,CARGO_NET_OFFLINE:'true'}}));}
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
const routeSource=owner=>owner==='platform'?'platform/communication/routes.rs':owner+'/workspace/module.rs';
const ordered=routes=>routes.toSorted((a,b)=>(a.group+' '+a.path).localeCompare(b.group+' '+b.path));
function definitionOwners(kind,name){
 const pattern=new RegExp('\\b'+kind+'\\s+'+name+'\\b','u');
 // 夹具使用统一斜线；源码读取仍保留原生平台路径。
 return files(onchina).filter(file=>file.endsWith('.rs')&&pattern.test(readFileSync(file,'utf8'))).map(file=>relative(onchina,file).split(sep).join('/'));
}

test('五个Cargo包、测试根和真实入口与结构夹具一致',()=>{
 assert.deepEqual(readdirSync(onchina,{withFileTypes:true}).filter(entry=>entry.isDirectory()).map(entry=>entry.name).sort(),layout.parts);
 assert.deepEqual(readdirSync(onchina,{withFileTypes:true}).filter(entry=>entry.isFile()).map(entry=>entry.name).sort(),layout.rootFiles);
 for(const [part,expectedFunctions]of Object.entries(layout.functions)){
  assert.deepEqual(readdirSync(join(onchina,part),{withFileTypes:true}).filter(entry=>entry.isDirectory()&&!ignored.has(entry.name)).map(entry=>entry.name).sort(),expectedFunctions);
 }
 for(const [owner,declared]of Object.entries(layout.frontendPackages)){
  const frontend=join(onchina,declared.path),pkg=JSON.parse(readFileSync(join(frontend,'package.json'),'utf8'));
  const lock=JSON.parse(readFileSync(join(frontend,'package-lock.json'),'utf8'));
  assert.equal(pkg.name,declared.name);assert.equal(lock.name,pkg.name);assert.equal(lock.packages[''].name,pkg.name);
  assert.ok(pkg.exports&&pkg.scripts.build&&pkg.scripts.test&&pkg.scripts.typecheck);
  const subdirectories=readdirSync(frontend,{withFileTypes:true}).filter(entry=>entry.isDirectory()&&!ignored.has(entry.name));
  assert.equal(declared.path.includes('/'),subdirectories.length===0,owner);
 }
 for(const path of layout.requiredFiles)assert.ok(existsSync(join(onchina,path)),path);
 for(const path of layout.absentFiles)assert.equal(existsSync(join(onchina,path)),false,path);
 for(const file of files(onchina)){
  const parts=relative(root,file).split(sep);
  assert.ok(!parts.slice(2,-1).includes('src'),file);
  assert.ok(!parts.slice(3,-1).includes('src'),file);
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
 assert.match(source('platform/modules/lib.rs'),/mod transactions;/u);
 assert.match(source('platform/modules/main.rs'),/onchina::run_backend\(None\)/u);
 for(const owner of Object.keys(layout.moduleTargets).filter(owner=>owner!=='platform')){
  assert.doesNotMatch(source(layout.moduleTargets[owner]),/include!/u,owner);
  assert.match(source(layout.moduleTargets[owner]),/onchina::run_backend/u,owner);
  assert.match(source(owner+'/Cargo.toml'),/onchina = \{ path = "\.\.\/platform" \}/u,owner);
  assert.doesNotMatch(source('platform/Cargo.toml'),new RegExp('onchina-'+owner,'u'));
 }

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
 assert.match(source('platformweb/workspace/index.html'),/src="\/main\.tsx"/u);
 // 包根只保留工程必需文件；两个小前端仍原位且不创建子目录。
 for(const [owner,expectedFiles]of Object.entries(layout.packageRoots)){
  const directory=join(onchina,owner);
  const actual=readdirSync(directory,{withFileTypes:true}).filter(entry=>entry.isFile()).map(entry=>entry.name).sort();
  assert.deepEqual(actual,expectedFiles,owner);
 }
 assert.equal(existsSync(join(onchina,'tsconfig.json')),false);

});

test('公共机构信息面板只定义在平台且原有消费者引用同一实现',()=>{
 const candidates=files(onchina).filter(file=>/\.(?:ts|tsx)$/u.test(file)&&/export function OwnInstitutionInfoPanel\b/u.test(readFileSync(file,'utf8')));
 assert.deepEqual(candidates.map(file=>relative(onchina,file).split(sep).join('/')),[layout.commonPanel]);
 for(const file of layout.commonPanelConsumers){
  const text=source(file);assert.match(text,/import[^;]*OwnInstitutionInfoPanel[^;]*institutions\/InfoPanel['"]/u,file);
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
 const actual=Object.keys(layout.moduleTargets).flatMap(owner=>routeRecords(source(routeSource(owner)),owner));
 assert.deepEqual(ordered(actual),ordered(expected.routes));
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
 assert.equal([...declarations.matchAll(/\.layer\((?:(?:crate|onchina)::)?documents::upload_body_limit\(\)\)/gu)].length,layout.uploadRoutes.length);
 for(const path of layout.uploadRoutes)assert.ok(expected.routes.some(route=>route.path===path&&route.methods.includes('post')),path);
 assert.match(source('platform/transactions/submit.rs'),/super::chain_url::rpc_http_client\(\)/u);
 assert.match(source('registry/projection/mod.rs'),/projection::commit_projection_batch/u);
});
test('宿主生产装配和公开前端不再静态编译其他业务模块',()=>{
 const shared=source('platform/modules/lib.rs');
 assert.doesNotMatch(shared,/#\[path = "\.\.\/(?:registry|legislation|private|judicial)\//u);
 for(const file of files(join(onchina,'platformweb')).filter(file=>/\.(?:ts|tsx)$/u.test(file)&&!file.endsWith('.test.ts'))){
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
 const targets=buildTargets(root,cargoMetadata());
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

test('默认节点保留基准命令开关、完整元数据依赖及生产防护',()=>{
 const manifest=readFileSync(join(root,'node/Cargo.toml'),'utf8');
 assert.match(manifest,/^default = \["std"\]$/mu);
 assert.match(manifest,/^custom-protocol = \["tauri\/custom-protocol"\]$/mu);
 assert.match(manifest,/^frame-benchmarking-cli = \{ workspace = true, default-features = true \}$/mu);
 assert.match(manifest,/"pallet-staking\/try-runtime"/u);
 assert.match(manifest,/"frame-benchmarking-cli\/runtime-benchmarks"/u);
 assert.match(readFileSync(join(root,'node/src/lib.rs'),'utf8'),/#\[cfg\(feature = "runtime-benchmarks"\)\]\s*pub mod benchmarks;/u);
 assert.match(readFileSync(join(root,'node/src/cli.rs'),'utf8'),/#\[cfg\(feature = "runtime-benchmarks"\)\]\s*#\[command\(subcommand\)\]\s*Benchmark\(/u);
 assert.match(readFileSync(join(root,'node/src/command.rs'),'utf8'),/#\[cfg\(feature = "runtime-benchmarks"\)\]\s*Some\(crate::cli::Subcommand::Benchmark/u);
 assert.match(readFileSync(join(root,'node/guard/Cargo.toml'),'utf8'),/^frame-benchmarking = /mu);
 assert.match(readFileSync(join(root,'node/guard/src/policy.rs'),'utf8'),/dyn frame_benchmarking::Benchmark<runtime::Block>/u);
});

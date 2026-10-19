// 中文注释：仅登记结构回归；由产品既有测试流程执行，本轮不运行测试。
import {test} from 'node:test';
import assert from 'node:assert/strict';
import {readFileSync,readdirSync,existsSync,statSync} from 'node:fs';
import {resolve,dirname,join,relative} from 'node:path';

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
 return files(onchina).filter(file=>file.endsWith('.rs')&&pattern.test(readFileSync(file,'utf8'))).map(file=>relative(onchina,file));
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
  const parts=relative(root,file).split('/');
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
});

test('公共机构信息面板只定义在平台且原有消费者引用同一实现',()=>{
 const candidates=files(onchina).filter(file=>/\.(?:ts|tsx)$/u.test(file)&&/export function OwnInstitutionInfoPanel\b/u.test(readFileSync(file,'utf8')));
 assert.deepEqual(candidates.map(file=>relative(onchina,file)),[layout.commonPanel]);
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

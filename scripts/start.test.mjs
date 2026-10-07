import assert from 'node:assert/strict';
import test from 'node:test';
import {mkdtempSync,mkdirSync,writeFileSync,rmSync,realpathSync,symlinkSync} from 'node:fs';
import {join} from 'node:path';
import { testRoot as tmpdir } from './build.mjs';
import {start,startArtifact,startDeclaration} from './start.mjs';
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

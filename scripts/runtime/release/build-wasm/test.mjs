import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import test from 'node:test';
import { runInNewContext } from 'node:vm';

test('citizenchain.wasm.release的build-wasm远端Job物理独立', () => {
  const source = readFileSync(new URL('./execute.mjs', import.meta.url), 'utf8');
  assert.ok(source.includes('{"pipeline":"citizenchain.wasm.release","job":"build-wasm"}'));
  assert.match(source, /function runExactWorkflowStep\(index\)/u);
  assert.match(source, /function requireExactRemoteJobEnvironment\(\)/u);
});

// WASM版本仍使用spec-version，产品只接受完整公民链。
test('WASM版本验真使用完整产品而非拆分身份',()=>{
 const source=readFileSync(new URL('./execute.mjs',import.meta.url),'utf8');
 assert.ok(source.includes('--product-id citizenchain --target wasm'));
 assert.doesNotMatch(source,/--product-id citizenchain-runtime/u);
});

// 只执行真实候选校验函数；合成输入不派发Job、不读取或生成Runtime文件。
test('WASM候选正常边界与软件版本、错误标签、坏锚点全部失败',()=>{
 const source=readFileSync(new URL('./execute.mjs',import.meta.url),'utf8');
 const body=source.match(/function validateCandidate\(\) \{([\s\S]*?)\n\}/u)?.[1];
 assert.ok(body);
 const input={SOURCE_SHA:'a'.repeat(40),CI_RUN_ID:'1',SOFTWARE_VERSION:'',
  SPEC_VERSION:'1',CHAIN_SPEC_VERSION:'0',GENESIS_HASH:'0x'+'b'.repeat(64),
  FINALIZED_HEAD:'c'.repeat(64),VERSION_TAG:'citizenchain-wasm-v1'};
 const verify=value=>runInNewContext(body,{process:{env:value}});
 assert.doesNotThrow(()=>verify(input));
 for(const [key,value] of [['SOURCE_SHA','bad'],['CI_RUN_ID','0'],['SOFTWARE_VERSION','1.0.0'],
  ['SPEC_VERSION','0'],['CHAIN_SPEC_VERSION','-1'],['GENESIS_HASH','bad'],['FINALIZED_HEAD','bad'],
  ['VERSION_TAG','citizenchain-wasm-v2']]) assert.throws(()=>verify({...input,[key]:value}),/WASM Release候选/u);
});

import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import test from 'node:test';

test('citizenchain.wasm.ci的build-wasm远端Job物理独立', () => {
  const source = readFileSync(new URL('./execute.mjs', import.meta.url), 'utf8');
  assert.ok(source.includes('{"pipeline":"citizenchain.wasm.ci","job":"build-wasm"}'));
  assert.match(source, /function runExactWorkflowStep\(index\)/u);
  assert.match(source, /function requireExactRemoteJobEnvironment\(\)/u);
});

// Cargo直接消费当前流程的输出环境，根target永久保持普通目录。
test('远端缓存不得用链接覆盖产品根target', async t => {
  const { mkdtempSync, mkdirSync, rmSync, lstatSync } = await import('node:fs');
  const { join } = await import('node:path');
  const { testRoot } = await import('../../../build.mjs');
  const { cacheIdentity, wireCacheLinks } = await import('./execute.mjs');
  const fixture = mkdtempSync(join(testRoot('wasm'), 'cargo-target-boundary-'));
  t.after(() => rmSync(fixture, { recursive: true }));
  const workspace = join(fixture, 'workspace'), work = join(fixture, 'owned');
  mkdirSync(workspace); mkdirSync(work); mkdirSync(join(workspace, 'target'));
  const identity = cacheIdentity({ repository: 'crcfrcn/citizenchain', product: 'citizenchain-runtime',
    platform: 'wasm', architecture: 'wasm', component: 'build-wasm', runnerOs: 'linux',
    runnerArch: 'arm64', toolchainFingerprint: 'a'.repeat(64) });
  assert.throws(() => wireCacheLinks(identity, work, 'cargo-target', workspace, 'target=cargo-target'), /根target不能建立缓存链接/u);
  assert.throws(() => wireCacheLinks(identity, work, 'cargo-target', workspace, 'nested/target=cargo-target'), /根target不能建立缓存链接/u);
  const result = wireCacheLinks(identity, work, 'cargo-target', workspace, '');
  assert.ok(result.successPaths[0].startsWith(work + '/'));
  assert.equal(lstatSync(join(workspace, 'target')).isSymbolicLink(), false);
});

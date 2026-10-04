// 直接调用本产品Build；工具替身只验证调用与失败条件，不代表真实编译验收。
import test from 'node:test';
import assert from 'node:assert/strict';
import { spawnSync } from 'node:child_process';
import { mkdtempSync, mkdirSync, writeFileSync, readFileSync, readdirSync, copyFileSync, chmodSync, rmSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { fileURLToPath } from 'node:url';
const root = fileURLToPath(new URL('..', import.meta.url));
const entry = join(root, 'scripts', 'build-local.sh');
test('错误参数与未知平台在执行工具前拒绝', () => {
  for (const args of [[], ['unknown', '/tmp/input', '/tmp/output'], ['windows', 'relative', 'relative']]) {
    assert.notEqual(spawnSync('/bin/bash', [entry, ...args], { env: {} }).status, 0);
  }
});
test('源码外输出、WASM隔离和首条失败收口', t => {
  const work = mkdtempSync(join(tmpdir(), 'citizenchain-build-unit-'));
  t.after(() => rmSync(work, { recursive: true, force: true }));
  const tool = join(work, 'cargo'), calls = join(work, 'calls');
  writeFileSync(tool, '#!/bin/bash\nprintf "%s\\n" "$CARGO_TARGET_DIR" "$@" >> "$CALLS"\nprintenv WASM_FILE >> "$CALLS"\nexit "$TOOL_STATUS"\n');
  chmodSync(tool, 0o700);
  for (const status of [0, 23]) {
    writeFileSync(calls, '');
    const run = spawnSync('/bin/bash', [entry, 'windows', work], {
      encoding: 'utf8', env: { ...process.env, CARGO: tool, RUSTC: tool, CALLS: calls, WASM_FILE: 'forbidden', TOOL_STATUS: String(status) },
    });
    assert.equal(run.status, status);
    const args = readFileSync(calls, 'utf8').split('\n');
    assert.ok(args.includes(join(work, 'cargo-target')));
    assert.ok(!args.includes('forbidden'));
    assert.ok(args.includes('check'));
    assert.equal(args.includes('test'), status === 0);
    if (status === 0) assert.ok(args.includes('--no-run'));
  }
  assert.notEqual(spawnSync('/bin/bash', [entry, 'windows', root], { env: { CARGO: tool, RUSTC: tool } }).status, 0);
});

// 所有前端直接导入都必须有自己的声明；本仓file依赖须与默认npm ci的链接模型一致。
test('三个前端源码直接导入和本仓锁条目完整', () => {
  for (const project of ['node/frontend', 'onchina/frontend', 'crates/scanner-react']) {
    const directory = join(root, project);
    const manifest = JSON.parse(readFileSync(join(directory, 'package.json'), 'utf8'));
    const declared = { ...manifest.dependencies, ...manifest.devDependencies, ...manifest.peerDependencies };
    function visit(path) {
      for (const entry of readdirSync(path, { withFileTypes: true })) {
        if (['node_modules', 'dist'].includes(entry.name)) continue;
        const file = join(path, entry.name);
        if (entry.isDirectory()) visit(file);
        else if (/\.[cm]?tsx?$/u.test(entry.name)) {
          const source = readFileSync(file, 'utf8');
          for (const match of source.matchAll(/(?:from\s+|import\s*\()(['"])([^'"]+)\1/gu)) {
            const value = match[2];
            if (value.startsWith('.') || value.startsWith('node:')) continue;
            const name = value.startsWith('@') ? value.split('/').slice(0, 2).join('/') : value.split('/')[0];
            assert.ok(Object.hasOwn(declared, name), file + '未直接声明' + name);
          }
        }
      }
    }
    visit(directory);
    const lock = JSON.parse(readFileSync(join(directory, 'package-lock.json'), 'utf8'));
    assert.deepEqual(lock.packages[''].dependencies, manifest.dependencies);
    if (declared['@gmb/scanner-react']) {
      const local = lock.packages['node_modules/@gmb/scanner-react'];
      assert.equal(local.link, true);
      const target = lock.packages[local.resolved];
      const scanner = JSON.parse(readFileSync(join(root, 'crates/scanner-react/package.json'), 'utf8'));
      assert.equal(target.version, scanner.version);
      assert.deepEqual(target.dependencies, scanner.dependencies);
    }
  }
});

import {test} from 'node:test';
import assert from 'node:assert/strict';
import {lstatSync, mkdirSync, mkdtempSync, readFileSync, rmSync, symlinkSync, writeFileSync} from 'node:fs';
import {join, resolve} from 'node:path';
import {stagePostgresResources, platformCargoEnvironment} from './build.mjs';

function fixture(t) {
  const parent = resolve(import.meta.dirname, '../target/test');
  mkdirSync(parent, {recursive: true});
  const work = mkdtempSync(join(parent, 'postgres-stage-'));
  t.after(() => rmSync(work, {recursive: true, force: true}));
  const source = join(work, 'source'), output = join(work, 'output');
  for (const name of ['bin', 'lib', 'share']) mkdirSync(join(source, name), {recursive: true});
  return {work, source, output};
}

test('PostgreSQL dylib别名被复制为普通文件且保持内容', t => {
  const {source, output} = fixture(t);
  writeFileSync(join(source, 'lib/libpq.5.dylib'), 'synthetic-library');
  symlinkSync('libpq.5.dylib', join(source, 'lib/libpq.dylib'));
  stagePostgresResources(source, output);
  const alias = join(output, 'lib/libpq.dylib');
  assert.equal(lstatSync(alias).isSymbolicLink(), false);
  assert.equal(readFileSync(alias, 'utf8'), 'synthetic-library');
});

test('PostgreSQL资源链接不得读取来源目录以外的文件', t => {
  const {work, source, output} = fixture(t);
  writeFileSync(join(work, 'outside'), 'outside');
  symlinkSync(join(work, 'outside'), join(source, 'lib/outside'));
  assert.throws(() => stagePostgresResources(source, output), /链接越界/u);
});

test('PostgreSQL资源目录循环被拒绝', t => {
  const {source, output} = fixture(t);
  symlinkSync('.', join(source, 'lib/cycle'));
  assert.throws(() => stagePostgresResources(source, output), /目录循环/u);
});

test('macOS原生构建关闭会生成未对齐过程宏的符号剥离', () => {
  const environment = {CARGO_PROFILE_RELEASE_STRIP: 'debuginfo'};
  assert.equal(platformCargoEnvironment('macos', '/tmp/synthetic-build', environment).CARGO_PROFILE_RELEASE_STRIP, 'none');
  assert.equal(platformCargoEnvironment('linux-amd', '/tmp/synthetic-build', environment).CARGO_PROFILE_RELEASE_STRIP, 'debuginfo');
});

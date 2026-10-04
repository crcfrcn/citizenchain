import assert from 'node:assert/strict';
import { readFileSync, readdirSync } from 'node:fs';
import { dirname, resolve } from 'node:path';
import { spawnSync } from 'node:child_process';
import test from 'node:test';
import { fileURLToPath } from 'node:url';

test('citizenchain.macos.ci的build-desktop远端Job物理独立', () => {
  const source = readFileSync(new URL('./execute.mjs', import.meta.url), 'utf8');
  assert.ok(source.includes('{"pipeline":"citizenchain.macos.ci","job":"build-desktop"}'));
  assert.match(source, /function runExactWorkflowStep\(index\)/u);
  assert.match(source, /function requireExactRemoteJobEnvironment\(\)/u);
});

test('CitizenChain四端只使用产品锁定的官方protoc 35.0', () => {
  const here = dirname(fileURLToPath(import.meta.url));
  const scripts = resolve(here, '../../../..');
  const dependencyPath = resolve(scripts, 'dependencies.mjs');
  const dependencySource = readFileSync(dependencyPath, 'utf8');
  const contract = JSON.parse(readFileSync(resolve(scripts, 'dependencies.json'), 'utf8'));
  assert.equal(contract.schema, 1);
  assert.equal(contract.tools.protoc.version, '35.0');
  assert.equal(contract.tools.protoc.source,
    'https://github.com/protocolbuffers/protobuf/releases/tag/v35.0');
  assert.deepEqual(Object.keys(contract.tools.protoc.archives).sort(),
    ['linux-amd', 'linux-arm', 'macos', 'windows']);
  const expected = {
    macos: ['protoc-35.0-osx-aarch_64.zip',
      '45444963204757fd3e2fbe304bc1fdadfb488d8556ff099c4cc06575eab88976', 'bin/protoc'],
    'linux-arm': ['protoc-35.0-linux-aarch_64.zip',
      '36b518ac14d90351cc6598228ed2bbe5afe4e357b1af470b07e0ec1609875de2', 'bin/protoc'],
    'linux-amd': ['protoc-35.0-linux-x86_64.zip',
      'a45cda0989c17dd950db55f6fbe1e5814c50fda08e87aa422980ac1f89dddbbc', 'bin/protoc'],
    windows: ['protoc-35.0-win64.zip',
      'd1cede9e308cc3eb072392af1c02ccae4bdd3d2f374ec2970dbd8cdfdaa91363', 'bin/protoc.exe'],
  };
  for (const [platform, [archive, sha256, executable]] of Object.entries(expected)) {
    assert.deepEqual(contract.tools.protoc.archives[platform], {
      url: `https://github.com/protocolbuffers/protobuf/releases/download/v35.0/${archive}`,
      sha256,
      executable,
    });
  }
  assert.match(dependencySource, /version[.]stdout[.]trim\(\) !== `libprotoc \$\{expectedVersion\}`/u);
  assert.match(dependencySource, /release-assets[.]githubusercontent[.]com/u);
  assert.match(dependencySource, /工具工作目录禁止符号链接/u);

  const invalid = spawnSync(process.execPath, [dependencyPath], { encoding: 'utf8' });
  assert.notEqual(invalid.status, 0);
  assert.match(invalid.stderr, /CitizenChain工具参数无效/u);
  const sourceWork = spawnSync(process.execPath,
    [dependencyPath, 'prepare', 'protoc', 'macos', resolve(scripts, '..')], { encoding: 'utf8' });
  assert.notEqual(sourceWork.status, 0);
  assert.match(sourceWork.stderr, /不得写入源码目录/u);

  const nodeRoot = resolve(scripts, 'node');
  const pathLookup = new RegExp(['command', '-v', 'protoc'].join(' '), 'u');
  const systemPackage = ['protobuf', 'compiler'].join('-');
  const jobs = [
    'ci/linux-amd/build-desktop/execute.mjs', 'ci/linux-amd/verify/execute.mjs',
    'ci/linux-arm/build-desktop/execute.mjs', 'ci/linux-arm/verify/execute.mjs',
    'ci/macos/build-desktop/execute.mjs', 'ci/macos/verify/execute.mjs',
    'ci/windows/build-desktop/execute.mjs', 'ci/windows/verify/execute.mjs',
    'release/linux-amd/build-desktop/execute.mjs',
    'release/linux-arm/build-desktop/execute.mjs',
    'release/macos/build-desktop/execute.mjs',
    'release/windows/build-desktop/execute.mjs',
  ];
  for (const relative of jobs) {
    const source = readFileSync(resolve(nodeRoot, relative), 'utf8');
    assert.match(source, /dependencies[.]mjs prepare protoc/u, `${relative}缺少产品protoc准备`);
    assert.match(source, /PROTOC=%s|PROTOC=\$protoc_executable/u, `${relative}缺少准确PROTOC注入`);
    assert.match(source, /RUNNER_TEMP\/citizenchain-protoc/u, `${relative}未隔离protoc工作目录`);
    assert.doesNotMatch(source, pathLookup, `${relative}仍从PATH选择protoc`);
    assert.doesNotMatch(source, /protoc[^\n]{0,160}GITHUB_PATH/u,
      `${relative}仍把protoc写入PATH`);
  }
  for (const relative of [
    'runtime/ci/build-wasm/execute.mjs',
    'runtime/release/build-wasm/execute.mjs',
  ]) {
    const source = readFileSync(resolve(scripts, relative), 'utf8');
    assert.match(source, /dependencies[.]mjs prepare protoc/u, `${relative}缺少产品protoc准备`);
    assert.match(source, /PROTOC=%s/u, `${relative}缺少准确PROTOC注入`);
    assert.ok(source.includes('os.environ[\\"PROTOC\\"]')
      || source.includes('os.environ["PROTOC"]'), `${relative}环境摘要未使用准确PROTOC`);
    assert.doesNotMatch(source, pathLookup, `${relative}仍从PATH选择protoc`);
    assert.doesNotMatch(source, /protoc[^\n]{0,160}GITHUB_PATH/u,
      `${relative}仍把protoc写入PATH`);
  }
  for (const flow of ['ci', 'release']) {
    for (const platform of readdirSync(resolve(nodeRoot, flow))) {
      const index = resolve(nodeRoot, flow, platform, 'index.mjs');
      let source;
      try { source = readFileSync(index, 'utf8'); } catch { continue; }
      assert.equal(source.includes(systemPackage), false,
        `${flow}/${platform}仍保留系统protoc包`);
      assert.doesNotMatch(source, pathLookup, `${flow}/${platform}仍从PATH选择protoc`);
    }
  }
  for (const flow of ['ci', 'release']) {
    const source = readFileSync(resolve(scripts, 'runtime', flow, 'index.mjs'), 'utf8');
    assert.equal(source.includes(systemPackage), false,
      `runtime/${flow}仍保留系统protoc包`);
    assert.doesNotMatch(source, pathLookup, `runtime/${flow}仍从PATH选择protoc`);
  }
});


// 独立链仓真实取得官网所有者白皮书；不以存在同名邻仓判定成功。
test('Node白皮书唯一Git输入的正常、失败与路径边界', async () => {
  const { prepareWhitepaperSource } = await import('../../../../dependencies.mjs');
  const { mkdtempSync, realpathSync, mkdirSync, symlinkSync, writeFileSync, rmSync } = await import('node:fs');
  const { join } = await import('node:path'); const { tmpdir } = await import('node:os');
  const base = realpathSync(mkdtempSync(join(tmpdir(), 'node-whitepaper-')));
  try {
    assert.throws(() => prepareWhitepaperSource('relative'), /绝对路径/u);
    assert.throws(() => prepareWhitepaperSource(new URL('../../../../../', import.meta.url).pathname), /不得写入源码/u);
    const actual = join(base, 'actual'); mkdirSync(actual);
    const linked = join(base, 'linked'); symlinkSync(actual, linked, 'dir');
    assert.throws(() => prepareWhitepaperSource(linked), /符号链接/u);
    const input = prepareWhitepaperSource(join(base, 'input'));
    assert.ok(readFileSync(join(input, 'src/whitepaper.md')).length);
    assert.equal(input, prepareWhitepaperSource(join(base, 'input')));
    writeFileSync(join(input, 'src/whitepaper.md'), 'changed');
    assert.throws(() => prepareWhitepaperSource(join(base, 'input')), /来源或内容已改变/u);
  } finally { rmSync(base, { recursive: true, force: true }); }
});

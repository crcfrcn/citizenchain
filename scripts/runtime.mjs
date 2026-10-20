#!/usr/bin/env node
// Runtime 唯一 GitHub 构建入口。正式候选按链上版本各加1，原源码和测试金标不回写。
import {copyFileSync, existsSync, lstatSync, mkdirSync, readFileSync, realpathSync, rmSync, writeFileSync, appendFileSync} from 'node:fs';
import {dirname, isAbsolute, join, relative, resolve} from 'node:path';
import {fileURLToPath} from 'node:url';
import {zstdDecompressSync} from 'node:zlib';
import {product, artifactNames, runtimeVersion} from './config.mjs';
import {assertBuildWork, checkWork, platformCargoEnvironment, runBuildProcess} from './build.mjs';

const root = realpathSync(resolve(import.meta.dirname, '..'));
const maximumVersion = 0xffffffff;
const maximumWasm = 50 * 1024 * 1024; // 沿用 SDK 解压边界，不增加生产链业务限制。
const compressedPrefix = Buffer.from([82, 188, 83, 118, 70, 219, 142, 5]);
const fail = message => { throw Error(message); };
const hash = value => typeof value === 'string' && /^0x[0-9a-f]{64}$/u.test(value);
function version(value) {
  if (!Number.isInteger(value) || value < 0 || value > maximumVersion) fail('Runtime版本必须是u32整数');
  return value;
}

export function nextRuntimeVersion(current) {
  const spec = version(current.specVersion), transaction = version(current.transactionVersion);
  if (spec === maximumVersion || transaction === maximumVersion) fail('Runtime版本加1溢出');
  return {specVersion: spec + 1, transactionVersion: transaction + 1};
}

// 仅生成临时编译输入。限定 VERSION 内两个数字字面量，不能替换业务代码或其它常量。
export function patchRuntimeVersion(source, target) {
  version(target.specVersion); version(target.transactionVersion);
  const declaration = /pub const VERSION:\s*RuntimeVersion\s*=\s*RuntimeVersion\s*\{[\s\S]*?\n\};/gu;
  const matches = [...source.matchAll(declaration)];
  if (matches.length !== 1) fail('Runtime VERSION声明必须唯一');
  let body = matches[0][0];
  for (const [field, value] of [['spec_version', target.specVersion], ['transaction_version', target.transactionVersion]]) {
    const pattern = new RegExp('(\\b' + field + '\\s*:\\s*)[0-9]+(\\s*,)', 'gu');
    if ([...body.matchAll(pattern)].length !== 1) fail('Runtime版本字段必须是唯一数字字面量');
    body = body.replace(pattern, (_, prefix, suffix) => prefix + value + suffix);
  }
  return source.slice(0, matches[0].index) + body + source.slice(matches[0].index + matches[0][0].length);
}

// 读取实际产物的 runtime_version 自定义段，不执行候选代码，不以文件名或Tag推断版本。
export function readRuntimeWasmVersion(input) {
  let bytes = Buffer.from(input);
  if (!bytes.length || bytes.length > maximumWasm) fail('Runtime WASM大小无效');
  if (bytes.subarray(0, 8).equals(compressedPrefix)) {
    bytes = zstdDecompressSync(bytes.subarray(8), {maxOutputLength: maximumWasm});
  }
  if (!bytes.subarray(0, 8).equals(Buffer.from([0, 97, 115, 109, 1, 0, 0, 0]))) fail('Runtime WASM头无效');
  let at = 8, section;
  const leb = limit => {
    let result = 0, shift = 0, byte;
    do {
      if (at >= limit || shift > 28) fail('WASM长度编码无效');
      byte = bytes[at++];
      if (shift === 28 && (byte & 0xf0)) fail('WASM长度溢出');
      result += (byte & 0x7f) * 2 ** shift; shift += 7;
    } while (byte & 0x80);
    // WASM允许u32宽度内的LEB128补零；长度、末字节高位和段边界仍严格检查。
    return result;
  };
  while (at < bytes.length) {
    const id = bytes[at++], size = leb(bytes.length), end = at + size;
    if (end > bytes.length) fail('WASM段越界');
    if (id === 0) {
      const length = leb(end);
      if (at + length > end) fail('WASM段名越界');
      const name = new TextDecoder('utf-8', {fatal: true}).decode(bytes.subarray(at, at + length));
      at += length;
      if (name === 'runtime_version') {
        if (section) fail('Runtime版本段重复');
        section = bytes.subarray(at, end);
      }
    }
    at = end;
  }
  if (!section) fail('WASM缺少Runtime版本段');
  at = 0; bytes = section;
  const take = length => {
    if (!Number.isSafeInteger(length) || length < 0 || at + length > bytes.length) fail('Runtime版本段越界');
    const value = bytes.subarray(at, at + length); at += length; return value;
  };
  const u32 = () => take(4).readUInt32LE();
  const compact = () => {
    const first = take(1)[0], mode = first & 3;
    if (mode === 0) return first >> 2;
    if (mode === 1) {
      const value = (first + take(1)[0] * 256) >>> 2;
      if (value < 64) fail('Runtime长度编码非规范'); return value;
    }
    if (mode === 2) {
      const tail = take(3), value = Math.floor((first + tail[0] * 256 + tail[1] * 65536 + tail[2] * 16777216) / 4);
      if (value < 16384) fail('Runtime长度编码非规范'); return value;
    }
    if ((first >> 2) !== 0) fail('Runtime长度超过u32');
    const value = u32(); if (value < 2 ** 30) fail('Runtime长度编码非规范'); return value;
  };
  const text = () => new TextDecoder('utf-8', {fatal: true}).decode(take(compact()));
  const specName = text(); text(); u32(); // impl_name、authoring_version保持原字节，不参与加号。
  const specVersion = u32(); u32(); // impl_version。
  take(compact() * 12); // SCALE ApisVec；自定义段通常为空，API声明另有官方段。
  const transactionVersion = u32(); take(1); // system_version。
  if (at !== bytes.length || specName !== product.id) fail('Runtime版本段或链名称不符');
  return {specName, specVersion, transactionVersion};
}

export function verifyRuntimeWasm(input, target) {
  const actual = readRuntimeWasmVersion(input);
  if (actual.specVersion !== version(target.specVersion) || actual.transactionVersion !== version(target.transactionVersion)) fail('实际WASM版本与本轮目标不符');
  return actual;
}

// HTTPS/WSS只使用平台正常证书验证；失败不重试、不降级，也不输出连接配置。
async function rpcRequest(endpoint, method, params, {fetchImpl = fetch, WebSocketType = WebSocket} = {}) {
  let url;
  try { url = new URL(endpoint); } catch { fail('缺少有效的Runtime链RPC地址'); }
  if (!['https:', 'wss:'].includes(url.protocol) || url.username || url.password || url.hash) fail('Runtime RPC只允许可信HTTPS/WSS地址');
  const request = JSON.stringify({jsonrpc: '2.0', id: 1, method, params});
  let raw;
  if (url.protocol === 'https:') {
    const response = await fetchImpl(url, {method: 'POST', redirect: 'error', credentials: 'omit',
      headers: {'content-type': 'application/json'}, body: request, signal: AbortSignal.timeout(15000)});
    if (!response.ok || !response.body) fail('Runtime链RPC读取失败');
    const parts = []; let size = 0;
    for await (const part of response.body) {
      size += part.length; if (size > 64 * 1024) fail('Runtime链RPC响应过大'); parts.push(part);
    }
    raw = Buffer.concat(parts).toString('utf8');
  } else {
    raw = await new Promise((accept, reject) => {
      const socket = new WebSocketType(url), timer = setTimeout(() => finish(Error('Runtime链RPC超时')), 15000);
      let done = false;
      function finish(error, value) {
        if (done) return; done = true; clearTimeout(timer);
        try { socket.close(); } catch { /* 连接失败也不触发重试。 */ }
        if (error) reject(error); else accept(value);
      }
      socket.addEventListener('open', () => socket.send(request));
      socket.addEventListener('message', event => {
        if (typeof event.data !== 'string' || Buffer.byteLength(event.data) > 64 * 1024) finish(Error('Runtime链RPC响应无效'));
        else finish(null, event.data);
      });
      socket.addEventListener('error', () => finish(Error('Runtime链RPC连接失败')));
      socket.addEventListener('close', () => finish(Error('Runtime链RPC连接提前关闭')));
    });
  }
  let value;
  try { value = JSON.parse(raw); } catch { fail('Runtime链RPC不是有效JSON'); }
  if (value?.jsonrpc !== '2.0' || value.id !== 1 || value.error || !Object.hasOwn(value, 'result')) fail('Runtime链RPC返回错误');
  return value.result;
}

export async function fetchRuntimeBase(rpc, expectedGenesis) {
  if (!hash(expectedGenesis)) fail('冻结创世身份无效');
  const genesisHash = await rpc('chain_getBlockHash', [0]);
  if (genesisHash !== expectedGenesis) fail('Runtime构建连接的链身份不符');
  const blockHash = await rpc('chain_getFinalizedHead', []);
  if (!hash(blockHash)) fail('Runtime finalized区块无效');
  const value = await rpc('state_getRuntimeVersion', [blockHash]);
  if (value?.specName !== product.id) fail('链上Runtime名称不符');
  const specVersion = version(value.specVersion), transactionVersion = version(value.transactionVersion);
  return {genesisHash, blockHash, specName: value.specName, specVersion, transactionVersion};
}

// 派发与GitHub编译共用此入口，导入本文件不会读链或创建编译现场。
export async function readRuntimeTarget(endpoint, {request = rpcRequest} = {}) {
  const frozen = readFileSync(join(root, 'runtime/primitives/src/genesis.rs'), 'utf8').match(/pub const GENESIS_HASH:\s*\[u8;\s*32\]\s*=\s*hex_literal::hex!\("([a-f0-9]{64})"\)/u);
  if (!frozen) fail('无法读取既有冻结链身份');
  const rpc = async(method, params) => {
    try { return await request(endpoint, method, params); }
    catch { fail('无法读取可信Runtime链RPC，请核对地址配置、证书及网络'); }
  };
  const base = await fetchRuntimeBase(rpc, '0x' + frozen[1]);
  return {base, target: nextRuntimeVersion(base)};
}

export function assertRuntimeTarget(target, spec, transaction) {
  if (target.specVersion !== Number(runtimeVersion(spec)) || target.transactionVersion !== Number(runtimeVersion(transaction))) {
    fail('链上Runtime基线或候选输入已变化，禁止使用错误名称编译或发布');
  }
  return target;
}

// WASM使用自己的整数双版本，不能经过节点构建入口的SemVer守卫；目录安全检查仍复用原实现。
export function assertRuntimeWork(work, env = process.env) {
  if (env.PLATFORM !== 'wasm' || env.GITHUB_ACTIONS !== 'true' || env.GITHUB_REPOSITORY !== product.repository
      || env.GITHUB_EVENT_NAME !== 'workflow_dispatch' || env.GITHUB_REF !== 'refs/heads/main'
      || env.GITHUB_WORKFLOW_REF !== `${product.repository}/.github/workflows/${product.platforms.wasm.workflow}@refs/heads/main`
      || !/^[a-f0-9]{40}$/u.test(env.GITHUB_SHA || '') || env.CITIZENCHAIN_SOURCE_SHA !== env.GITHUB_SHA
      || work !== join(root, 'target/build/wasm')) fail('Runtime编译只接受本产品准确WASM的GitHub main任务');
  runtimeVersion(env.CITIZENCHAIN_RELEASE_VERSION); runtimeVersion(env.CITIZENCHAIN_RUNTIME_TRANSACTION_VERSION);
  return checkWork(work);
}

// 只复制Git已跟踪的普通文件；产物、凭据、node_modules和临时现场不进入编译输入。
export function copyRuntimeSource(project, destination, files) {
  if (existsSync(destination)) fail('Runtime临时源码目录已存在');
  mkdirSync(destination);
  for (const name of files) {
    const path = resolve(project, name), within = relative(project, path);
    if (!within || within.startsWith('..') || isAbsolute(within) || /^(?:target|\.git|node_modules)(?:[\\/]|$)/u.test(within)
        || realpathSync(path) !== path || !lstatSync(path).isFile()) fail('Runtime编译输入不是仓库普通文件');
    const output = join(destination, within); mkdirSync(dirname(output), {recursive: true}); copyFileSync(path, output);
  }
}

async function runtime(action) {
  if (!['test', 'build'].includes(action)) fail('Runtime入口仅接受test或build');
  const platform = process.env.PLATFORM, work = join(root, 'target/build', platform || '');
  if (platform === 'wasm') assertRuntimeWork(work);
  else assertBuildWork(work, platform);
  const declared = product.platforms[platform];
  if (action === 'build' && platform !== 'wasm') fail('正式Runtime候选只由WASM工作流构建');
  if (process.platform !== declared.host || process.arch !== declared.arch || process.versions.node !== product.tools.node) fail('Runtime构建宿主或Node版本不符');
  mkdirSync(work, {recursive: true});
  const lock = join(work, '.runtime-lock');
  try { mkdirSync(lock); } catch { fail('本轮Runtime任务已占用，禁止并发构建'); }
  try {
    if (existsSync(join(work, '.retain-runtime-work'))) fail('Runtime工具退出未确认，保留现场');
    const env = platformCargoEnvironment(platform, work, process.env);
    delete env.SKIP_WASM_BUILD;
    env.WASM_BUILD_FROM_SOURCE = '1'; env.CITIZENCHAIN_PRODUCTION_WASM_BUILD = '1';
    const run = async(command, args, cwd = root, options) => {
      try { return await runBuildProcess(command, args, env, cwd, options); }
      catch (error) {
        if (error.retainWork) writeFileSync(join(work, '.retain-runtime-work'), '工具退出未确认，保留本轮现场\n');
        throw error;
      }
    };
    if (!(await run(env.CARGO, ['--version'], root, {capture: true})).stdout.startsWith('cargo ' + product.tools.rust + ' ')
        || (await run(env.PROTOC, ['--version'], root, {capture: true})).stdout.trim() !== 'libprotoc ' + product.tools.protoc) fail('Runtime工具版本不符');
    const cargo = (directory, args) => run(env.CARGO, ['--config', join(directory, 'config.toml'), ...args], directory);
    if (action === 'test') {
      // 保留原测试及测试WASM，版本0等金标不改；测试夹具绝不作为正式升级产物交付。
      await cargo(root, ['clean', '-p', 'citizenchain', '--locked']);
      await cargo(root, ['test', '--locked', '--release', ...(platform === 'wasm' ? ['-p', 'citizenchain'] : ['--workspace']), '--all-targets']);
      return;
    }
    const sha = (await run('git', ['rev-parse', 'HEAD'], root, {capture: true})).stdout.trim();
    if (sha !== process.env.CITIZENCHAIN_SOURCE_SHA || (await run('git', ['status', '--porcelain', '--untracked-files=no'], root, {capture: true})).stdout.trim()) fail('Runtime候选必须来自准确且未修改的检出');
    const {base, target} = await readRuntimeTarget(process.env.CITIZENCHAIN_RUNTIME_RPC_URL);
    assertRuntimeTarget(target, process.env.CITIZENCHAIN_RELEASE_VERSION, process.env.CITIZENCHAIN_RUNTIME_TRANSACTION_VERSION);
    const source = join(work, 'source'), artifact = join(work, 'artifact');
    if (existsSync(artifact) || existsSync(source) || existsSync(join(work, '.retain-runtime-work'))) fail('本轮Runtime现场已存在，禁止覆盖或混用');
    try {
      const files = (await run('git', ['ls-files', '-z'], root, {capture: true})).stdout.split('\0').filter(Boolean);
      copyRuntimeSource(root, source, files);
      const input = join(source, 'runtime/src/lib.rs');
      writeFileSync(input, patchRuntimeVersion(readFileSync(input, 'utf8'), target));
      const latest = await readRuntimeTarget(process.env.CITIZENCHAIN_RUNTIME_RPC_URL);
      assertRuntimeTarget(latest.target, String(target.specVersion), String(target.transactionVersion));
      env.CITIZENCHAIN_SOURCE_ROOT = source;
      await cargo(source, ['build', '--locked', '--release', '-p', 'citizenchain', '--no-default-features', '--features', 'std']);
      const filesToPublish = artifactNames('wasm', String(target.specVersion));
      for (const name of filesToPublish) {
        const path = join(env.CARGO_TARGET_DIR, 'release/wbuild/citizenchain', name);
        if (!lstatSync(path).isFile() || lstatSync(path).isSymbolicLink()) fail('Runtime产物不是普通文件');
        verifyRuntimeWasm(readFileSync(path), target);
      }
      mkdirSync(artifact);
      for (const name of filesToPublish) copyFileSync(join(env.CARGO_TARGET_DIR, 'release/wbuild/citizenchain', name), join(artifact, name));
      const record = {source_sha: sha, genesis_hash: base.genesisHash, base_block_hash: base.blockHash, spec_name: base.specName,
        base_spec_version: base.specVersion, base_transaction_version: base.transactionVersion,
        spec_version: target.specVersion, transaction_version: target.transactionVersion};
      if (process.env.GITHUB_OUTPUT) appendFileSync(process.env.GITHUB_OUTPUT, 'version=' + target.specVersion + '\nruntime=' + JSON.stringify(record) + '\n');
      process.stdout.write('Runtime候选版本：' + target.specVersion + ' / ' + target.transactionVersion + '\n');
      process.stdout.write('完整Runtime产物：' + artifact + '\n');
    } finally {
      // 只有普通退出可清理；信号/超时不能证明后代工具已停止。
      if (!existsSync(join(work, '.retain-runtime-work')) && existsSync(source)) rmSync(source, {recursive: true, force: true});
    }
  } finally {
    if (!existsSync(join(work, '.retain-runtime-work'))) rmSync(lock, {recursive: true, force: true});
  }
}

// 只检查命令分发身份；导入本模块不会读取链、构建或创建现场。
const testFile = Boolean(process.env.NODE_TEST_CONTEXT) && process.argv[1] === import.meta.filename && process.argv.length === 2;
if (!testFile && process.argv[1] === fileURLToPath(import.meta.url)) {
  if (process.argv.length !== 3) fail('Runtime只接受一个动作参数');
  runtime(process.argv[2]).catch(error => { process.stderr.write('Runtime构建失败：' + error.message + '\n'); process.exitCode = 1; });
}

// 内嵌纯规则/临时夹具回归：不连接真实节点，不执行Cargo；正式构建也不自动运行它们。
if (testFile) {
  const {test} = await import('node:test'), {default: assert} = await import('node:assert/strict');
  const {mkdtempSync} = await import('node:fs'), {tmpdir} = await import('node:os');
  const {zstdCompressSync} = await import('node:zlib');
  const genesis = '0x' + '11'.repeat(32), head = '0x' + '22'.repeat(32);
  const declaration = 'pub const VERSION: RuntimeVersion = RuntimeVersion {\n    spec_version: 0,\n    transaction_version: 0,\n};\nconst OTHER: u32 = 0;\n';
  const u32 = value => {const bytes = Buffer.alloc(4); bytes.writeUInt32LE(value); return bytes;};
  const leb = value => {const bytes = []; do {let byte = value & 127; value = Math.floor(value / 128); if (value) byte |= 128; bytes.push(byte);} while (value); return Buffer.from(bytes);};
  const text = value => Buffer.concat([Buffer.from([Buffer.byteLength(value) << 2]), Buffer.from(value)]);
  const fixture = (spec, transaction, specName = product.id, padded = false) => {
    const length = value => {
      const encoded = [...leb(value)];
      if (padded) while (encoded.length < 5) {encoded[encoded.length - 1] |= 128; encoded.push(0);}
      return Buffer.from(encoded);
    };
    const body = Buffer.concat([text(specName), text(product.id), u32(0), u32(spec), u32(0), Buffer.from([0]), u32(transaction), Buffer.from([0])]);
    const name = Buffer.from('runtime_version'), section = Buffer.concat([length(name.length), name, body]);
    return Buffer.concat([Buffer.from([0, 97, 115, 109, 1, 0, 0, 0, 0]), length(section.length), section]);
  };
  test('以链上各自版本加1，0有效，非法值和溢出拒绝', () => {
    assert.deepEqual(nextRuntimeVersion({specVersion: 7, transactionVersion: 3}), {specVersion: 8, transactionVersion: 4});
    assert.deepEqual(nextRuntimeVersion({specVersion: 0, transactionVersion: 0}), {specVersion: 1, transactionVersion: 1});
    for (const value of [-1, 1.5, '1', undefined, maximumVersion, maximumVersion + 1]) {
      assert.throws(() => nextRuntimeVersion({specVersion: value, transactionVersion: 0}));
      assert.throws(() => nextRuntimeVersion({specVersion: 0, transactionVersion: value}));
    }
  });
  test('临时版本只取链基线，同一基线重试不累加，声明异常拒绝', () => {
    const target = nextRuntimeVersion({specVersion: 7, transactionVersion: 3}), patched = patchRuntimeVersion(declaration, target);
    assert.equal(patched, declaration.replace('spec_version: 0', 'spec_version: 8').replace('transaction_version: 0', 'transaction_version: 4'));
    assert.equal(patchRuntimeVersion(patched, target), patched);
    assert.throws(() => patchRuntimeVersion(declaration + declaration, target));
    assert.throws(() => patchRuntimeVersion(declaration.replace('transaction_version', 'missing'), target));
  });
  test('实际原始/压缩WASM版本一致，缺失、重复、损坏与尾字节拒绝', () => {
    const raw = fixture(8, 4), expected = {specName: product.id, specVersion: 8, transactionVersion: 4};
    assert.deepEqual(readRuntimeWasmVersion(raw), expected);
    assert.deepEqual(readRuntimeWasmVersion(Buffer.concat([compressedPrefix, zstdCompressSync(raw)])), expected);
    for (const invalid of [Buffer.alloc(0), Buffer.from('invalid'), raw.subarray(0, 8), raw.subarray(0, -1),
      Buffer.concat([raw, Buffer.from([0])]), Buffer.concat([raw, raw.subarray(8)]), fixture(8, 4, 'wrong-chain')]) assert.throws(() => readRuntimeWasmVersion(invalid));
  });
  test('最终产物拒绝同号、降号、跳号或仅一个版本正确', () => {
    const target = nextRuntimeVersion({specVersion: 7, transactionVersion: 3});
    assert.equal(verifyRuntimeWasm(fixture(8, 4), target).specVersion, 8);
    for (const [spec, transaction] of [[7, 3], [6, 4], [9, 4], [8, 3], [8, 5]]) {
      assert.throws(() => verifyRuntimeWasm(fixture(spec, transaction), target));
    }
  });
  test('WASM合法五字节LEB128补零可验真，超长、溢出和截断仍拒绝', () => {
    const raw = fixture(8, 4, product.id, true), target = {specVersion: 8, transactionVersion: 4};
    assert.equal(verifyRuntimeWasm(raw, target).specVersion, 8);
    assert.equal(verifyRuntimeWasm(Buffer.concat([compressedPrefix, zstdCompressSync(raw)]), target).transactionVersion, 4);
    for (const length of [[128], [128, 128, 128, 128, 128, 0], [255, 255, 255, 255, 16]]) {
      assert.throws(() => readRuntimeWasmVersion(Buffer.concat([raw.subarray(0, 9), Buffer.from(length)])));
    }
  });
  test('派发与编译共用目标，基线变化、缺版本及SemVer输入拒绝', async () => {
    const frozen = readFileSync(join(root, 'runtime/primitives/src/genesis.rs'), 'utf8')
      .match(/pub const GENESIS_HASH:\s*\[u8;\s*32\]\s*=\s*hex_literal::hex!\("([a-f0-9]{64})"\)/u);
    assert.ok(frozen);
    const request = async(_endpoint, method) => method === 'chain_getBlockHash' ? '0x' + frozen[1]
      : method === 'chain_getFinalizedHead' ? head : {specName: product.id, specVersion: 7, transactionVersion: 3};
    const first = await readRuntimeTarget('https://fixture.invalid', {request});
    assert.deepEqual(first.target, {specVersion: 8, transactionVersion: 4});
    assert.deepEqual((await readRuntimeTarget('https://fixture.invalid', {request})).target, first.target);
    assert.deepEqual(assertRuntimeTarget(first.target, '8', '4'), first.target);
    for (const [spec, transaction] of [['9', '4'], ['8', '5'], ['8.0.0', '4'], ['08', '4'], ['8', undefined]]) {
      assert.throws(() => assertRuntimeTarget(first.target, spec, transaction));
    }
  });
  test('WASM整数双版本通过自身GitHub身份守卫，错误任务、目录和SemVer仍拒绝', () => {
    const work = join(root, 'target/build/wasm'), sha = 'a'.repeat(40);
    const env = {PLATFORM: 'wasm', GITHUB_ACTIONS: 'true', GITHUB_REPOSITORY: product.repository,
      GITHUB_EVENT_NAME: 'workflow_dispatch', GITHUB_REF: 'refs/heads/main',
      GITHUB_WORKFLOW_REF: `${product.repository}/.github/workflows/wasm.yml@refs/heads/main`,
      GITHUB_SHA: sha, CITIZENCHAIN_SOURCE_SHA: sha, CITIZENCHAIN_RELEASE_VERSION: '2', CITIZENCHAIN_RUNTIME_TRANSACTION_VERSION: '1'};
    assert.equal(assertRuntimeWork(work, env), work);
    for (const changed of [{GITHUB_ACTIONS: ''}, {GITHUB_EVENT_NAME: 'repository_dispatch'}, {GITHUB_REF: 'refs/heads/other'},
      {GITHUB_REPOSITORY: 'foreign/repository'}, {GITHUB_WORKFLOW_REF: 'foreign/workflow'}, {CITIZENCHAIN_SOURCE_SHA: 'b'.repeat(40)},
      {CITIZENCHAIN_RELEASE_VERSION: '2.0.0'}, {CITIZENCHAIN_RUNTIME_TRANSACTION_VERSION: undefined}]) {
      assert.throws(() => assertRuntimeWork(work, {...env, ...changed}));
    }
    assert.throws(() => assertRuntimeWork(join(root, 'target/build/other'), env));
  });
  test('准确finalized块绑定双版本，错误链、缺字段与网络失败不填默认值', async () => {
    const calls = [], rpc = async(method, params) => {
      calls.push([method, params]);
      if (method === 'chain_getBlockHash') return genesis;
      if (method === 'chain_getFinalizedHead') return head;
      return {specName: product.id, specVersion: 7, transactionVersion: 3};
    };
    assert.deepEqual(await fetchRuntimeBase(rpc, genesis), {genesisHash: genesis, blockHash: head, specName: product.id, specVersion: 7, transactionVersion: 3});
    assert.deepEqual(calls.at(-1), ['state_getRuntimeVersion', [head]]);
    await assert.rejects(fetchRuntimeBase(rpc, head));
    await assert.rejects(fetchRuntimeBase(async(method, params) => method === 'state_getRuntimeVersion' ? {specName: product.id, specVersion: 7} : rpc(method, params), genesis));
    await assert.rejects(fetchRuntimeBase(async() => {throw Error('fixture network failure');}, genesis));
    await assert.rejects(fetchRuntimeBase(async(method, params) => method === 'state_getRuntimeVersion'
      ? {specName: product.id, specVersion: 7, transactionVersion: '3'} : rpc(method, params), genesis));
  });
  test('RPC只允许HTTPS/WSS，严格匹配响应，不执行真实网络请求', async () => {
    let calls = 0;
    const fetchImpl = async(_url, options) => {
      calls++; assert.equal(options.redirect, 'error'); assert.equal(options.credentials, 'omit');
      const request = JSON.parse(options.body);
      assert.equal(request.method, 'chain_getFinalizedHead');
      return Response.json({jsonrpc: '2.0', id: 1, result: head});
    };
    for (const endpoint of ['', 'http://fixture.invalid', 'ws://fixture.invalid', 'https://user:password@fixture.invalid']) {
      await assert.rejects(rpcRequest(endpoint, 'chain_getFinalizedHead', [], {fetchImpl}));
    }
    assert.equal(calls, 0);
    assert.equal(await rpcRequest('https://fixture.invalid', 'chain_getFinalizedHead', [], {fetchImpl}), head);
    await assert.rejects(rpcRequest('https://fixture.invalid', 'chain_getFinalizedHead', [], {
      fetchImpl: async() => Response.json({jsonrpc: '2.0', id: 2, result: head}),
    }));
    await assert.rejects(rpcRequest('https://fixture.invalid', 'chain_getFinalizedHead', [], {
      fetchImpl: async() => Response.json({jsonrpc: '2.0', id: 1, error: {code: -1}}),
    }));
    class Socket {
      handlers = new Map();
      constructor() {queueMicrotask(() => this.handlers.get('open')());}
      addEventListener(name, action) {this.handlers.set(name, action);}
      send(body) {
        assert.equal(JSON.parse(body).method, 'chain_getFinalizedHead');
        queueMicrotask(() => this.handlers.get('message')({data: JSON.stringify({jsonrpc: '2.0', id: 1, result: head})}));
      }
      close() {}
    }
    assert.equal(await rpcRequest('wss://fixture.invalid', 'chain_getFinalizedHead', [], {WebSocketType: Socket}), head);
  });
  test('编译输入只在临时目录生成，仓库Runtime和Node原字节保持', () => {
    const directory = realpathSync(mkdtempSync(join(tmpdir(), 'citizenchain-runtime-')));
    try {
      const project = join(directory, 'project'), output = join(directory, 'source');
      mkdirSync(join(project, 'runtime/src'), {recursive: true}); mkdirSync(join(project, 'node'), {recursive: true});
      writeFileSync(join(project, 'runtime/src/lib.rs'), declaration); writeFileSync(join(project, 'node/original'), 'unchanged');
      copyRuntimeSource(project, output, ['runtime/src/lib.rs', 'node/original']);
      const input = join(output, 'runtime/src/lib.rs'); writeFileSync(input, patchRuntimeVersion(readFileSync(input, 'utf8'), {specVersion: 8, transactionVersion: 4}));
      assert.equal(readFileSync(join(project, 'runtime/src/lib.rs'), 'utf8'), declaration);
      assert.equal(readFileSync(join(project, 'node/original'), 'utf8'), 'unchanged');
      assert.notEqual(readFileSync(input, 'utf8'), declaration);
      assert.throws(() => copyRuntimeSource(project, output, []));
      assert.throws(() => copyRuntimeSource(project, join(directory, 'invalid'), ['../outside']));
    } finally {rmSync(directory, {recursive: true, force: true});}
  });
  test('新入口在本机拒绝真实构建，旧WASM入口不转发', () => {
    const {spawnSync} = process.getBuiltinModule('node:child_process');
    const environment = {...process.env, NODE_TEST_CONTEXT: '', GITHUB_ACTIONS: ''};
    const rejected = spawnSync(process.execPath, [import.meta.filename, 'build'], {env: environment, encoding: 'utf8'});
    assert.equal(rejected.status, 1); assert.match(rejected.stderr, /GitHub/);
    const old = spawnSync(process.execPath, [join(import.meta.dirname, 'build.mjs'), 'wasm'], {env: environment, encoding: 'utf8'});
    assert.equal(old.status, 1); assert.match(old.stderr, /scripts\/runtime.mjs build/);
  });
  test('五个工作流只调用唯一Runtime入口，正式产物记录双版本', () => {
    for (const platform of Object.values(product.platforms)) {
      const workflow = readFileSync(join(root, '.github/workflows', platform.workflow), 'utf8');
      assert.ok(workflow.includes('node scripts/runtime.mjs test'));
      assert.ok(!workflow.includes('export WASM_BUILD_FROM_SOURCE='));
      assert.ok(!workflow.includes('cargo --config "$GITHUB_WORKSPACE/config.toml" test'));
    }
    const workflow = readFileSync(join(root, '.github/workflows/wasm.yml'), 'utf8');
    assert.equal(workflow.split('node scripts/runtime.mjs build').length - 1, 1);
    assert.ok(!workflow.includes('node scripts/build.mjs wasm'));
    assert.ok(workflow.indexOf('node scripts/runtime.mjs test') < workflow.indexOf('node scripts/runtime.mjs build'));
    assert.ok(workflow.includes('needs.build.outputs.runtime'));
    assert.ok(workflow.includes('runtime:JSON.parse(process.env.RUNTIME_BUILD)'));
  });
}

#!/usr/bin/env node
// Runtime编译入口；正式候选只修改临时编译输入，原源码和创世输入不回写。
import {copyFileSync, existsSync, lstatSync, mkdirSync, readFileSync, realpathSync, rmSync, writeFileSync,
  openSync, closeSync, fstatSync, readSync, constants as fileConstants} from 'node:fs';
import {dirname, isAbsolute, join, relative, resolve} from 'node:path';
import {fileURLToPath} from 'node:url';
import {zstdDecompressSync} from 'node:zlib';
import {homedir} from 'node:os';
import {execFileSync} from 'node:child_process';
import {X509Certificate} from 'node:crypto';
import {request as httpsRequest} from 'node:https';
import * as cfg from './config.mjs';
import {checkWork, platformCargoEnvironment, runBuildProcess} from './build.mjs';

const root = realpathSync(resolve(import.meta.dirname, '..'));
const maximumVersion = 0xffffffff;
const maximumWasm = 50 * 1024 * 1024; // 沿用 SDK 解压边界，不增加生产链业务限制。
const compressedPrefix = Buffer.from([82, 188, 83, 118, 70, 219, 142, 5]);
const fail = message => { throw Error(message); };
const hash = value => typeof value === 'string' && value.length === 66 && /^0x[0-9a-f]{64}$/u.test(value);
function version(value) {
  if (!Number.isInteger(value) || value < 0 || value > maximumVersion) fail('Runtime版本必须是u32整数');
  return value;
}

export function nextRuntimeVersion(current) {
  const spec = version(current.specVersion), transaction = version(current.transactionVersion);
  if (spec === maximumVersion || transaction === maximumVersion) fail('Runtime版本加1溢出');
  return {specVersion: spec + 1, transactionVersion: transaction + 1};
}

// 本机现有RPC入口只归Runtime文件；不是新增配置项，也不交给GitHub连接。
const localRpc = 'https://127.0.0.1:9944';
const certificateName = 'rpc-certificate.hex', maximumRpcBytes = 64 * 1024;
const certificateFiles = {lstatSync, realpathSync, openSync, closeSync, fstatSync, readSync};

// 固定读取当前macOS正式节点的公开交付，不搜索开发目录、不新增环境或配置项。
function runtimeCertificateDirectory() {
  if (process.platform !== 'darwin') fail('本机公开证书取数入口仅适用于macOS正式节点');
  return join(homedir(), 'Library/Application Support/gmb');
}

function assertCertificateAcl(output, privatePath) {
  const lines = output.trimEnd().split('\n');
  if (!lines[0]) fail('公开证书路径ACL无法确认');
  for (const line of lines.slice(1)) {
    const match = /^\s*[0-9]+: (.+)$/u.exec(line);
    if (!match || privatePath) fail('公开证书目录或文件存在附加ACL');
    const entry = /^((?:user|group):.+) (allow|deny) ([a-z_,]+)$/u.exec(match[1]);
    if (!entry) fail('公开证书路径ACL身份或权限无法确认');
    if (entry[2] === 'deny') continue;
    if (!entry[3].split(',').every(value =>
      ['read', 'list', 'execute', 'search', 'readattr', 'readextattr', 'readsecurity', 'synchronize'].includes(value.trim()))) {
      fail('公开证书祖先目录允许额外写入');
    }
  }
}

function inspectCertificateAcl(path, privatePath) {
  if (process.platform !== 'darwin') fail('无法检查macOS公开证书ACL');
  let output;
  try { output = execFileSync('/bin/ls', ['-lde', path], {env: {LC_ALL: 'C'}, encoding: 'utf8',
    maxBuffer: maximumRpcBytes, timeout: 2500, stdio: ['ignore', 'pipe', 'pipe']}); }
  catch { fail('无法检查公开证书ACL'); }
  assertCertificateAcl(output, privatePath);
}

function certificateFileMetadata(value, uid) {
  if (!value.isFile() || value.uid !== BigInt(uid) || (value.mode & 0o7777n) !== 0o600n
      || value.nlink !== 1n || value.size === 0n || value.size > BigInt(maximumRpcBytes)) {
    fail('公开证书文件类型、所有者或权限无效');
  }
}

function sameCertificateFile(left, right) {
  return ['dev', 'ino', 'size', 'mtimeNs', 'ctimeNs'].every(key => left[key] === right[key]);
}

function certificateDirectory(directory, files, inspectAcl, uid) {
  if (!isAbsolute(directory) || resolve(directory) !== directory || files.realpathSync(directory) !== directory) {
    fail('公开证书目录不能经过链接');
  }
  let path = directory, identity;
  for (;;) {
    const value = files.lstatSync(path, {bigint: true});
    if (!value.isDirectory() || ![0n, BigInt(uid)].includes(value.uid) || (value.mode & 0o022n) !== 0n
        || path === directory && (value.uid !== BigInt(uid) || (value.mode & 0o7777n) !== 0o700n)) {
      fail('公开证书目录所有者或权限无效');
    }
    inspectAcl(path, path === directory);
    if (path === directory) identity = value;
    const parent = dirname(path); if (parent === path) return identity;
    path = parent;
  }
}

// 只读获取本轮公开证书，保持文件身份和TLS检查。
export function readRuntimeCertificate(directory = runtimeCertificateDirectory(), {files = certificateFiles, inspectAcl = inspectCertificateAcl} = {}) {
  if (typeof process.geteuid !== 'function' || !fileConstants.O_NOFOLLOW || !fileConstants.O_NONBLOCK) {
    fail('公开证书文件安全校验不可用');
  }
  const uid = process.geteuid(), identity = certificateDirectory(directory, files, inspectAcl, uid);
  const path = join(directory, certificateName);
  let fd;
  try {
    fd = files.openSync(path, fileConstants.O_RDONLY | fileConstants.O_NOFOLLOW | fileConstants.O_NONBLOCK);
    const before = files.fstatSync(fd, {bigint: true}); certificateFileMetadata(before, uid);
    inspectAcl(path, true);
    const named = files.lstatSync(path, {bigint: true}); certificateFileMetadata(named, uid);
    if (!sameCertificateFile(before, named)) fail('公开证书打开期间文件已变化');
    const bytes = Buffer.alloc(Number(before.size) + 1);
    let size = 0;
    for (;;) {
      const count = files.readSync(fd, bytes, size, bytes.length - size, size);
      if (!count) break;
      size += count; if (size === bytes.length) fail('公开证书读取期间大小已变化');
    }
    const after = files.fstatSync(fd, {bigint: true}), current = files.lstatSync(path, {bigint: true});
    certificateFileMetadata(after, uid); certificateFileMetadata(current, uid);
    const parent = certificateDirectory(directory, files, inspectAcl, uid);
    if (identity.dev !== parent.dev || identity.ino !== parent.ino || size !== Number(before.size)
        || !sameCertificateFile(before, after) || !sameCertificateFile(before, current)) {
      fail('公开证书读取期间文件已变化');
    }
    inspectAcl(path, true);
    const text = bytes.subarray(0, size).toString('ascii');
    if (size % 2 || !/^[0-9a-f]+$/u.test(text)) fail('公开证书必须是完整DER十六进制');
    // ascii解码不能丢弃高位后把非法字节当作十六进制。
    if (!Buffer.from(text, 'ascii').equals(bytes.subarray(0, size))) fail('公开证书编码无效');
    let certificate;
    const der = Buffer.from(text, 'hex');
    try { certificate = new X509Certificate(der); } catch { fail('公开证书不是有效X.509'); }
    if (!certificate.raw.equals(der)) fail('公开证书只能包含一张完整X.509');
    return certificate.toString();
  } finally { if (fd !== undefined) files.closeSync(fd); }
}

export function runtimeVersion(value) {
  if (typeof value !== 'string' || value.trim() !== value || !/^(0|[1-9][0-9]*)$/u.test(value)
      || Number(value) > maximumVersion) fail('Runtime版本必须是规范u32十进制整数');
  return value;
}

function frozenGenesis() {
  const frozen = readFileSync(join(root, 'runtime/primitives/src/genesis.rs'), 'utf8')
    .match(/pub const GENESIS_HASH:\s*\[u8;\s*32\]\s*=\s*hex_literal::hex!\("([a-f0-9]{64})"\)/u);
  if (!frozen) fail('无法读取既有冻结链身份');
  return '0x' + frozen[1];
}

// 只接收本轮读取的五个字段；Runner校验快照，不把目标减1冒充实际基线。
export function parseRuntimeBase(input, expectedGenesis = frozenGenesis()) {
  let base;
  try { base = typeof input === 'string' ? JSON.parse(input) : input; }
  catch { fail('Runtime基线不是有效JSON'); }
  if (!base || typeof base !== 'object' || Array.isArray(base)
      || Object.keys(base).sort().join(',') !== 'blockHash,genesisHash,specName,specVersion,transactionVersion'
      || !hash(expectedGenesis) || base.genesisHash !== expectedGenesis || !hash(base.blockHash)
      || base.specName !== cfg.product.id) fail('Runtime基线字段或冻结链身份不符');
  return {base, target: nextRuntimeVersion(base)};
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
  if (at !== bytes.length || specName !== cfg.product.id) fail('Runtime版本段或链名称不符');
  return {specName, specVersion, transactionVersion};
}

export function verifyRuntimeWasm(input, target) {
  const actual = readRuntimeWasmVersion(input);
  if (actual.specVersion !== version(target.specVersion) || actual.transactionVersion !== version(target.transactionVersion)) fail('实际WASM版本与本轮目标不符');
  return actual;
}

// 本机请求只装载本轮公开证书；不使用全局Agent、代理、客户端私钥或关闭身份校验。
function requestLocalRuntime(url, body, certificate, requestImpl) {
  if (url.protocol !== 'https:' || url.hostname !== '127.0.0.1' || url.pathname !== '/' || url.search) {
    fail('本机公开证书只允许回环HTTPS RPC');
  }
  return new Promise((accept, reject) => {
    const request = requestImpl(url, {method: 'POST', agent: false, ca: certificate,
      rejectUnauthorized: true, signal: AbortSignal.timeout(15000),
      headers: {'content-type': 'application/json'}}, response => {
      void (async() => {
        try {
          if (response.statusCode !== 200) fail('本机Runtime RPC状态无效');
          const parts = []; let size = 0;
          for await (const part of response) {
            size += part.length; if (size > maximumRpcBytes) fail('Runtime链RPC响应过大');
            parts.push(part);
          }
          accept(Buffer.concat(parts).toString('utf8'));
        } catch (error) { response.destroy(); reject(error); }
      })();
    });
    request.on('error', reject);
    request.end(body);
  });
}

// 远端HTTPS/WSS仍沿用系统信任；本机使用明确交付的公开证书，失败不重试、不降级。
export async function rpcRequest(endpoint, method, params, {fetchImpl = fetch, WebSocketType = WebSocket,
  certificate, httpsImpl = httpsRequest} = {}) {
  let url;
  try { url = new URL(endpoint); } catch { fail('缺少有效的Runtime链RPC地址'); }
  if (!['https:', 'wss:'].includes(url.protocol) || url.username || url.password || url.hash) fail('Runtime RPC只允许可信HTTPS/WSS地址');
  const request = JSON.stringify({jsonrpc: '2.0', id: 1, method, params});
  let raw;
  if (certificate !== undefined) {
    raw = await requestLocalRuntime(url, request, certificate, httpsImpl);
  } else if (url.origin === localRpc || url.hostname === '127.0.0.1') {
    fail('本机Runtime RPC缺少本轮公开证书');
  } else if (url.protocol === 'https:') {
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
  if (value?.specName !== cfg.product.id) fail('链上Runtime名称不符');
  const specVersion = version(value.specVersion), transactionVersion = version(value.transactionVersion);
  return {genesisHash, blockHash, specName: value.specName, specVersion, transactionVersion};
}

// 只在本机派发前读取一次证书并固定到本轮三个RPC；导入不读文件，Runner仅消费版本基线。
export async function readRuntimeTarget(endpoint = localRpc, {request, readCertificate = readRuntimeCertificate, httpsImpl = httpsRequest} = {}) {
  const certificate = request ? undefined : readCertificate();
  const send = request || ((url, method, params) => rpcRequest(url, method, params, {certificate, httpsImpl}));
  const rpc = async(method, params) => {
    try { return await send(endpoint, method, params); }
    catch { fail('无法读取本机Runtime RPC，请确认节点运行及现有可信连接可用'); }
  };
  const base = await fetchRuntimeBase(rpc, frozenGenesis());
  return {base, target: nextRuntimeVersion(base)};
}

// 本机按钮所需的输入由产品自身准备；通用调度只转发这份数据。
export async function automationInputs({readTarget = readRuntimeTarget,
  readRevision = () => execFileSync('git', ['-C', root, 'rev-parse', 'HEAD'], {encoding: 'utf8', stdio: ['ignore', 'pipe', 'pipe']}).trim()} = {}) {
  const {base, target} = await readTarget();
  return {version: String(target.specVersion), transaction_version: String(target.transactionVersion),
    runtime_base: JSON.stringify(base), source_sha: readRevision()};
}

export function assertRuntimeTarget(target, spec, transaction) {
  if (target.specVersion !== Number(runtimeVersion(spec)) || target.transactionVersion !== Number(runtimeVersion(transaction))) {
    fail('Runtime基线与候选输入不一致，禁止使用错误名称编译或发布');
  }
  return target;
}

// WASM编译使用整数双版本和固定输出目录；GitHub身份归工作流。
export function assertRuntimeWork(work, env = process.env) {
  if (env.PLATFORM !== 'wasm' || work !== join(root, 'target/build/wasm')) fail('Runtime编译平台或输出目录无效');
  runtimeVersion(env.CITIZENCHAIN_RELEASE_VERSION);
  runtimeVersion(env.CITIZENCHAIN_RUNTIME_TRANSACTION_VERSION);
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
  if (action !== 'build') fail('Runtime入口仅接受build');
  const platform = 'wasm', work = join(root, 'target/build/wasm');
  assertRuntimeWork(work);
  const declared = cfg.product.platforms[platform];
  if (process.platform !== declared.host || process.arch !== declared.arch || process.versions.node !== cfg.product.tools.node) fail('Runtime构建宿主或Node版本不符');
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
    if (!(await run(env.CARGO, ['--version'], root, {capture: true})).stdout.startsWith('cargo ' + cfg.product.tools.rust + ' ')
        || (await run(env.PROTOC, ['--version'], root, {capture: true})).stdout.trim() !== 'libprotoc ' + cfg.product.tools.protoc) fail('Runtime工具版本不符');
    const cargo = (directory, args) => run(env.CARGO, ['--config', join(directory, 'config.toml'), ...args], directory);
    const {base, target} = parseRuntimeBase(process.env.CITIZENCHAIN_RUNTIME_BASE);
    assertRuntimeTarget(target, process.env.CITIZENCHAIN_RELEASE_VERSION, process.env.CITIZENCHAIN_RUNTIME_TRANSACTION_VERSION);
    const source = join(work, 'source'), artifact = join(work, 'artifact');
    if (existsSync(artifact) || existsSync(source) || existsSync(join(work, '.retain-runtime-work'))) fail('本轮Runtime现场已存在，禁止覆盖或混用');
    try {
      const files = (await run('git', ['ls-files', '-z'], root, {capture: true})).stdout.split('\0').filter(Boolean);
      copyRuntimeSource(root, source, files);
      const input = join(source, 'runtime/src/lib.rs');
      writeFileSync(input, patchRuntimeVersion(readFileSync(input, 'utf8'), target));
      env.CITIZENCHAIN_SOURCE_ROOT = source;
      await cargo(source, ['build', '--locked', '--release', '-p', 'runtime', '--no-default-features', '--features', 'std']);
      const filesToPublish = cfg.product.platforms.wasm.files;
      for (const name of filesToPublish) {
        const path = join(env.CARGO_TARGET_DIR, 'release/wbuild/runtime', name);
        if (!lstatSync(path).isFile() || lstatSync(path).isSymbolicLink()) fail('Runtime产物不是普通文件');
        verifyRuntimeWasm(readFileSync(path), target);
      }
      mkdirSync(artifact);
      for (const name of filesToPublish) copyFileSync(join(env.CARGO_TARGET_DIR, 'release/wbuild/runtime', name), join(artifact, name));
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

// 仅清理本轮WASM现场；工具退出未确认时保留，不创建独立清理入口。
export function cleanWork(env = process.env) {
  const work = join(root, 'target/build/wasm');
  assertRuntimeWork(work, env);
  if (resolve(env.CITIZENCHAIN_WORK_DIR || '') !== work) fail('现场不符');
  if (existsSync(join(work, '.retain-runtime-work'))) fail('Runtime工具退出未确认，保留现场');
  rmSync(work, {recursive: true, force: true});
}

// 直接执行时只分发 Runtime 构建动作；导入模块不会启动构建。
if (process.argv[1] === fileURLToPath(import.meta.url)) {
  if (process.argv.length !== 3) fail('Runtime只接受一个动作参数');
  runtime(process.argv[2]).catch(error => { process.stderr.write('Runtime构建失败：' + error.message + '\n'); process.exitCode = 1; });
}

#!/usr/bin/env node
// Runtime 唯一 GitHub 构建入口。正式候选按链上版本各加1，原源码和测试金标不回写。
import {copyFileSync, existsSync, lstatSync, mkdirSync, readFileSync, realpathSync, rmSync, writeFileSync, appendFileSync} from 'node:fs';
import {dirname, isAbsolute, join, relative, resolve} from 'node:path';
import {fileURLToPath} from 'node:url';
import {zstdDecompressSync} from 'node:zlib';
import * as cfg from './config.mjs';
import {assertBuildWork, checkWork, platformCargoEnvironment, runBuildProcess} from './build.mjs';

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

export function assertRuntimeProof(proof) {
  const runtime = proof.runtime;
  if (!runtime || runtime.source_sha !== proof.source_sha || runtime.spec_name !== cfg.product.id
      || !hash(runtime.genesis_hash) || !hash(runtime.base_block_hash)
      || ![runtime.base_spec_version, runtime.base_transaction_version].every(value => Number.isInteger(value) && value >= 0 && value < maximumVersion)
      || runtime.spec_version !== runtime.base_spec_version + 1 || runtime.transaction_version !== runtime.base_transaction_version + 1
      || String(runtime.spec_version) !== proof.version) fail('WASM来源双版本与显示号不符');
}

export function assertRuntimeRunName(proof, run) {
  if (run.display_title !== cfg.releaseName('wasm', proof.version)) fail('WASM原Run名称与实际版本不符');
}

export function runtimeLaunchRequest({version: spec, transactionVersion, sha, base} = {}) {
  if (!/^[a-f0-9]{40}$/u.test(sha || '')) fail('派发源码无效');
  const snapshot = parseRuntimeBase(base);
  assertRuntimeTarget(snapshot.target, spec, transactionVersion);
  return {path: 'actions/workflows/' + cfg.product.platforms.wasm.workflow + '/dispatches', method: 'POST',
    body: {ref: 'main', inputs: {version: runtimeVersion(spec), transaction_version: runtimeVersion(transactionVersion),
      source_sha: sha, runtime_base: JSON.stringify(snapshot.base)}}};
}

export function runtimeSettleRequest(source) {
  const run = cfg.assertRun('wasm', source), prefix = cfg.product.title + '·WASM·v';
  if (!run.display_title?.startsWith(prefix)) fail('原Run没有准确交付版本');
  return {path: 'dispatches', method: 'POST', body: {event_type: 'citizenchain-finalize-wasm', client_payload: {
    platform: 'wasm', version: runtimeVersion(run.display_title.slice(prefix.length)),
    run_id: run.id, run_attempt: run.run_attempt, source_sha: run.head_sha}}};
}

// 控制台沿用原launch({read,dispatch})公开接口，只提供传输；本文件独立拥有WASM流程。
export async function describeAction({flow, platform}) {
  if (flow !== 'release' || platform !== 'wasm') fail('Runtime入口只接受WASM流程');
  return {repository: cfg.product.repository, launch: async({read, dispatch}) => {
    const rows = [], releases = [];
    for (let page = 1;; page++) {
      const data = await read(cfg.automationRequest('wasm', 'runs').path + '&per_page=100&page=' + page);
      rows.push(...data.workflow_runs); if (data.workflow_runs.length < 100) break;
    }
    for (let page = 1;; page++) {
      const data = await read('releases?per_page=100&page=' + page);
      releases.push(...data); if (data.length < 100) break;
    }
    if (rows.some(run => run.status !== 'completed')) fail('WASM已有活动编译Run');
    const {base, target} = await readRuntimeTarget(), version = String(target.specVersion);
    if (releases.some(release => release.tag_name === cfg.releaseName('wasm', version))) fail('该Runtime版本已有交付或未收尾资产，禁止重复发布');
    const head = await read('git/ref/heads/main');
    const request = runtimeLaunchRequest({version, transactionVersion: String(target.transactionVersion), sha: head.object.sha, base});
    const receipt = await dispatch({workflow: cfg.product.platforms.wasm.workflow, ...request.body});
    return {runID: receipt.workflow_run_id};
  }};
}

function assertWorkflow(context, event) {
  if (context?.repo?.owner + '/' + context?.repo?.repo !== cfg.product.repository || context.ref !== 'refs/heads/main'
      || context.workflow !== '公民链·WASM' || context.eventName !== event
      || !Number.isSafeInteger(context.runId) || context.runId <= 0 || !/^[a-f0-9]{40}$/u.test(context.sha || '')) {
    fail('WASM工作流身份或事件不符');
  }
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
  if (value?.specName !== cfg.product.id) fail('链上Runtime名称不符');
  const specVersion = version(value.specVersion), transactionVersion = version(value.transactionVersion);
  return {genesisHash, blockHash, specName: value.specName, specVersion, transactionVersion};
}

// 只在本机派发前读取；GitHub构建消费基线快照，导入本文件不读链或创建现场。
export async function readRuntimeTarget(endpoint = localRpc, {request = rpcRequest} = {}) {
  const rpc = async(method, params) => {
    try { return await request(endpoint, method, params); }
    catch { fail('无法读取本机Runtime RPC，请确认节点运行及现有可信连接可用'); }
  };
  const base = await fetchRuntimeBase(rpc, frozenGenesis());
  return {base, target: nextRuntimeVersion(base)};
}

export function assertRuntimeTarget(target, spec, transaction) {
  if (target.specVersion !== Number(runtimeVersion(spec)) || target.transactionVersion !== Number(runtimeVersion(transaction))) {
    fail('Runtime基线与候选输入不一致，禁止使用错误名称编译或发布');
  }
  return target;
}

// WASM使用自己的整数双版本，不能经过节点构建入口的SemVer守卫；目录安全检查仍复用原实现。
export function assertRuntimeWork(work, env = process.env) {
  if (env.PLATFORM !== 'wasm' || env.GITHUB_ACTIONS !== 'true' || env.GITHUB_REPOSITORY !== cfg.product.repository
      || env.GITHUB_EVENT_NAME !== 'workflow_dispatch' || env.GITHUB_REF !== 'refs/heads/main'
      || env.GITHUB_WORKFLOW_REF !== `${cfg.product.repository}/.github/workflows/${cfg.product.platforms.wasm.workflow}@refs/heads/main`
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
  const declared = cfg.product.platforms[platform];
  if (action === 'build' && platform !== 'wasm') fail('正式Runtime候选只由WASM工作流构建');
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
    if (action === 'test') {
      // 保留原测试及测试WASM，版本0等金标不改；测试夹具绝不作为正式升级产物交付。
      await cargo(root, ['clean', '-p', 'citizenchain', '--locked']);
      await cargo(root, ['test', '--locked', '--release', ...(platform === 'wasm' ? ['-p', 'citizenchain'] : ['--workspace']), '--all-targets']);
      return;
    }
    const sha = (await run('git', ['rev-parse', 'HEAD'], root, {capture: true})).stdout.trim();
    if (sha !== process.env.CITIZENCHAIN_SOURCE_SHA || (await run('git', ['status', '--porcelain', '--untracked-files=no'], root, {capture: true})).stdout.trim()) fail('Runtime候选必须来自准确且未修改的检出');
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
      await cargo(source, ['build', '--locked', '--release', '-p', 'citizenchain', '--no-default-features', '--features', 'std']);
      const filesToPublish = cfg.artifactNames('wasm', String(target.specVersion));
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

// 既有工作流正文迁入本文件；权限、先后顺序及失败保护保持。
export async function prepare({github, context, core}) {
  assertWorkflow(context, 'workflow_dispatch');
  const p='wasm',repo=context.repo;
  const run=(await github.rest.actions.getWorkflowRun({...repo,run_id:context.runId})).data;
  cfg.assertRun(p,run,context.runId);
  if(run.head_sha!==process.env.CITIZENCHAIN_SOURCE_SHA||run.head_sha!==context.sha)throw Error('检出源码偏离批准');
  const runs=await github.paginate(github.rest.actions.listWorkflowRuns,{...repo,workflow_id:cfg.product.platforms[p].workflow,branch:'main',event:'workflow_dispatch',per_page:100});
  if(runs.some(row=>row.id<context.runId&&row.status!=='completed'))throw Error('本平台已有原编译任务，禁止重复编译');
  const releases=await github.paginate(github.rest.repos.listReleases,{...repo,per_page:100});
  const {target}=parseRuntimeBase(process.env.CITIZENCHAIN_RUNTIME_BASE);
  assertRuntimeTarget(target,process.env.CITIZENCHAIN_RELEASE_VERSION,process.env.CITIZENCHAIN_RUNTIME_TRANSACTION_VERSION);
  const expected=String(target.specVersion);
  if(run.display_title!==cfg.releaseName(p,expected))throw Error('WASM Run名称偏离链上加1版本');
  const success=cfg.retainedBuildRuns(p,runs).success;
  if(success){const matches=releases.filter(value=>{try{return cfg.readReleaseProof(p,value).run_id===success.id;}catch{return false;}});
   if(matches.length!==1)throw Error('最新成功没有唯一正式交付');const release=matches[0];cfg.assertRelease(p,release,success);
   const ref=(await github.rest.git.getRef({...repo,ref:'tags/'+release.tag_name})).data;
   if(ref.object.type!=='commit'||ref.object.sha!==success.head_sha)throw Error('最新成功Tag提交不符');}
  const tag=cfg.releaseName(p,expected);
  if(releases.some(release=>release.tag_name===tag))throw Error('候选版本仍有未收尾资产，请恢复原Run清理');
  try{await github.rest.git.getRef({...repo,ref:'tags/'+tag});throw Error('候选Tag已存在，请先核对原Run');}catch(error){if(error.status!==404)throw error;}
  core.info('候选已确认：'+tag);
}

// 既有工作流正文迁入本文件；权限、先后顺序及失败保护保持。
export async function recordAssets({github, context, core}) {
  assertWorkflow(context, 'workflow_dispatch');
  const p='wasm',repo=context.repo;
  const {createReadStream,lstatSync,readdirSync}=await import('node:fs');
  const {createHash}=await import('node:crypto');
  const directory=process.env.GITHUB_WORKSPACE+'/target/build/'+p+'/artifact';
  const names=cfg.artifactNames(p,process.env.CITIZENCHAIN_RELEASE_VERSION);
  if(JSON.stringify(readdirSync(directory).sort())!==JSON.stringify([...names].sort()))throw Error('完整产物集合不符');
  const assets=[];
  for(const name of names){const path=directory+'/'+name,info=lstatSync(path);if(!info.isFile()||info.isSymbolicLink()||info.size<=0)throw Error('产物不是非空普通文件');
   const hash=createHash('sha256');for await(const bytes of createReadStream(path))hash.update(bytes);
   assets.push({name,label:cfg.artifactLabel(p,process.env.CITIZENCHAIN_RELEASE_VERSION,name),size:info.size,sha256:hash.digest('hex')});}
  core.setOutput('assets',JSON.stringify(assets));
}

// 既有工作流正文迁入本文件；权限、先后顺序及失败保护保持。
export async function publish({github, context, core}) {
  assertWorkflow(context, 'workflow_dispatch');
  const p='wasm',repo=context.repo;
  const {readFileSync,writeFileSync,mkdirSync,createReadStream}=await import('node:fs');
  const {createHash}=await import('node:crypto');
  const {execFileSync}=await import('node:child_process');
  const runtime=JSON.parse(process.env.RUNTIME_BUILD);
  assertRuntimeTarget({specVersion:runtime.spec_version,transactionVersion:runtime.transaction_version},process.env.CITIZENCHAIN_RELEASE_VERSION,process.env.CITIZENCHAIN_RUNTIME_TRANSACTION_VERSION);
  if(process.env.BUILT_VERSION!==process.env.CITIZENCHAIN_RELEASE_VERSION)throw Error('实际构建版本与发布名称不符');
  const tag=cfg.releaseName(p,process.env.CITIZENCHAIN_RELEASE_VERSION),assets=JSON.parse(process.env.SOURCE_ASSETS);
  const names=cfg.artifactNames(p,process.env.CITIZENCHAIN_RELEASE_VERSION);
  if(JSON.stringify(assets.map(asset=>asset.name).sort())!==JSON.stringify([...names].sort()))throw Error('生产Run产物声明不完整');
  const hashFile=async path=>{const hash=createHash('sha256');for await(const bytes of createReadStream(path))hash.update(bytes);return hash.digest('hex');};
  const source=process.env.RUNNER_TEMP+'/artifact';
  for(const asset of assets){const bytes=readFileSync(source+'/'+asset.name);if(bytes.length!==asset.size||await hashFile(source+'/'+asset.name)!==asset.sha256)throw Error('Actions附件偏离生产Run原始字节');
   verifyRuntimeWasm(bytes,{specVersion:runtime.spec_version,transactionVersion:runtime.transaction_version});}
  const proof={schema:1,repository:cfg.product.repository,product_id:cfg.product.id,platform:p,workflow:'.github/workflows/'+cfg.product.platforms[p].workflow,
   run_id:context.runId,run_attempt:Number(process.env.GITHUB_RUN_ATTEMPT),source_sha:context.sha,version:process.env.CITIZENCHAIN_RELEASE_VERSION,tag,assets,runtime};
  const body='<!-- automation:'+JSON.stringify(proof)+' -->\n';
  cfg.readReleaseProof(p,{body,tag_name:tag,name:tag,target_commitish:context.sha});
  const notes=process.env.RUNNER_TEMP+'/release-notes.txt';writeFileSync(notes,body);
  const gh=args=>execFileSync('gh',args,{stdio:['ignore','pipe','pipe'],encoding:'utf8',maxBuffer:4*1024*1024});
  gh(['release','create',tag,...assets.map(asset=>source+'/'+asset.name+'#'+asset.label),'--repo',cfg.product.repository,'--target',context.sha,'--title',tag,'--notes-file',notes,'--draft','--latest=false']);
  let draft=(await github.paginate(github.rest.repos.listReleases,{...repo,per_page:100})).filter(value=>value.tag_name===tag);
  if(draft.length!==1)throw Error('本轮草稿不唯一');
  const run=(await github.rest.actions.getWorkflowRun({...repo,run_id:context.runId})).data;
  cfg.assertRelease(p,draft[0],run,{draft:true});
  const downloaded=process.env.RUNNER_TEMP+'/release-download';mkdirSync(downloaded);
  gh(['release','download',tag,'--repo',cfg.product.repository,'--dir',downloaded]);
  for(const asset of assets)if(!readFileSync(downloaded+'/'+asset.name).equals(readFileSync(source+'/'+asset.name)))throw Error('正式附件回读字节不符');
  gh(['release','edit',tag,'--repo',cfg.product.repository,'--draft=false','--latest=false']);
  const published=(await github.rest.repos.getRelease({...repo,release_id:draft[0].id})).data;
  cfg.assertRelease(p,published,run);
  const ref=(await github.rest.git.getRef({...repo,ref:'tags/'+tag})).data;
  if(ref.object.type!=='commit'||ref.object.sha!==context.sha)throw Error('正式Tag提交不符');
  for(const asset of published.assets)if(new URL(asset.browser_download_url).href!=='https://github.com/'+cfg.product.repository+'/releases/download/'+encodeURIComponent(tag)+'/'+encodeURIComponent(asset.name))throw Error('正式附件下载地址不符');
  core.info('正式Tag、Release及完整附件已确认：'+tag);
}

// 既有工作流正文迁入本文件；权限、先后顺序及失败保护保持。
export async function requestFinalize({github, context, core}) {
  assertWorkflow(context, 'workflow_dispatch');
  const p='wasm',repo=context.repo;
  cfg.runtimeVersion(process.env.CITIZENCHAIN_RELEASE_VERSION);
  // 只请求本平台收尾，不派发另一次编译；POST 不自动重试。
  await github.rest.repos.createDispatchEvent({...repo,event_type:'citizenchain-finalize-'+p,
   client_payload:{platform:p,version:process.env.CITIZENCHAIN_RELEASE_VERSION,run_id:context.runId,
   run_attempt:Number(process.env.GITHUB_RUN_ATTEMPT),source_sha:context.sha}});
  core.info('已请求原Run终态收尾；结果由独立收尾Run如实记录');
}

// 既有工作流正文迁入本文件；权限、先后顺序及失败保护保持。
export async function finalize({github, context, core}) {
  assertWorkflow(context, 'repository_dispatch');
  const p='wasm',repo=context.repo;
  const input=context.payload.client_payload;
  if(!input||input.platform!==p||!Number.isSafeInteger(input.run_id)||!Number.isSafeInteger(input.run_attempt))throw Error('收尾输入无效');
  cfg.runtimeVersion(input.version);
  const original=async()=>{const run=(await github.rest.actions.getWorkflowRun({...repo,run_id:input.run_id})).data;cfg.assertRun(p,run,input.run_id);
   if(run.head_sha!==input.source_sha||run.run_attempt!==input.run_attempt||run.display_title!==cfg.releaseName(p,input.version))throw Error('原Run、attempt或版本变化');return run;};
  let source=await original();
  for(let count=0;source.status!=='completed'&&count<120;count++){await new Promise(done=>setTimeout(done,5000));source=await original();}
  if(source.status!=='completed')throw Error('原Run尚未completed，禁止替换终态历史');
  const rows=()=>github.paginate(github.rest.actions.listWorkflowRuns,{...repo,workflow_id:cfg.product.platforms[p].workflow,branch:'main',event:'workflow_dispatch',per_page:100});
  const getRelease=id=>github.rest.repos.getRelease({...repo,release_id:id}).then(value=>value.data).catch(error=>{if(error.status===404)return null;throw error;});
  const getTag=tag=>github.rest.git.getRef({...repo,ref:'tags/'+tag}).then(value=>value.data).catch(error=>{if(error.status===404)return null;throw error;});
  const keep=cfg.retainedBuildRuns(p,await rows());
  const releases=await github.paginate(github.rest.repos.listReleases,{...repo,per_page:100});
  const owned=releases.filter(value=>value.tag_name.startsWith(cfg.product.title+'·'+cfg.product.platforms[p].name+'·v'));
  const proven=owned.map(value=>({release:value,proof:cfg.readReleaseProof(p,value)}));
  let latest;
  if(keep.success){const matches=proven.filter(value=>value.proof.run_id===keep.success.id);if(matches.length!==1)throw Error('最新成功没有唯一正式交付，保留旧记录');
   latest=matches[0];cfg.assertRelease(p,latest.release,keep.success);
   const ref=await getTag(latest.proof.tag);if(!ref||ref.object.type!=='commit'||ref.object.sha!==keep.success.head_sha)throw Error('最新成功Tag不符，保留旧记录');}
  for(const item of proven){
   if(item.release.id===latest?.release.id)continue;
   const run=(await github.rest.actions.getWorkflowRun({...repo,run_id:item.proof.run_id})).data;cfg.assertRun(p,run,item.proof.run_id);
   if(run.status!=='completed')continue;
   if(cfg.retainedBuildRuns(p,await rows()).success?.id!==keep.success?.id)throw Error('最新成功已变化，需恢复最新原Run收尾');
   const current=await getRelease(item.release.id);if(!current)continue;
   const proof=cfg.assertRelease(p,current,run,{partial:run.conclusion!=='success',draft:true});
   if(current.immutable)throw Error('旧Release不可变，未修改仓库设置');
   const ref=await getTag(proof.tag);
   if(ref){if(ref.object.type!=='commit'||ref.object.sha!==run.head_sha)throw Error('旧Tag提交变化');
    await github.rest.git.deleteRef({...repo,ref:'tags/'+proof.tag});if(await getTag(proof.tag))throw Error('Tag删除未确认');}
   await github.rest.repos.deleteRelease({...repo,release_id:current.id});if(await getRelease(current.id))throw Error('Release删除未确认');
  }
  for(const old of keep.obsolete){
   const fresh=cfg.retainedBuildRuns(p,await rows());if(fresh.success?.id!==keep.success?.id)throw Error('最新成功已变化，需恢复最新原Run收尾');
   if(!fresh.obsolete.some(run=>run.id===old.id))continue;
   let current;try{current=(await github.rest.actions.getWorkflowRun({...repo,run_id:old.id})).data;}catch(error){if(error.status===404)continue;throw error;}
   cfg.assertRun(p,current,old.id);if(current.status!=='completed'||current.run_attempt!==old.run_attempt||current.head_sha!==old.head_sha||current.conclusion!==old.conclusion)throw Error('删除前旧Run终态变化');
   await github.rest.actions.deleteWorkflowRun({...repo,run_id:old.id});
   try{await github.rest.actions.getWorkflowRun({...repo,run_id:old.id});throw Error('旧Run删除未确认');}catch(error){if(error.status!==404)throw error;}
  }
  const remaining=await rows();if(cfg.retainedBuildRuns(p,remaining).obsolete.length)throw Error('编译终态集合尚未收口，需要恢复原Run收尾');
  // 业务清理已核验后删除旧成功收尾；只留最近失败与当前活动收尾，不制造第二条编译成功记录。
  const maintenance=await github.paginate(github.rest.actions.listWorkflowRuns,{...repo,workflow_id:cfg.product.platforms[p].workflow,event:'repository_dispatch',per_page:100});
  const terminal=maintenance.filter(run=>run.status==='completed').sort((a,b)=>b.run_number-a.run_number);
  const failure=terminal.find(run=>run.conclusion!=='success'),retained=new Set(failure?[failure.id]:[]);
  for(const run of terminal)if(!retained.has(run.id)){const current=(await github.rest.actions.getWorkflowRun({...repo,run_id:run.id})).data;
   if(current.event!=='repository_dispatch'||current.path!=='.github/workflows/'+cfg.product.platforms[p].workflow||current.head_branch!=='main'||current.status!=='completed'||current.run_attempt!==run.run_attempt||current.head_sha!==run.head_sha||current.conclusion!==run.conclusion)throw Error('旧收尾Run状态变化');
   await github.rest.actions.deleteWorkflowRun({...repo,run_id:run.id});
   try{await github.rest.actions.getWorkflowRun({...repo,run_id:run.id});throw Error('旧收尾Run删除未确认');}catch(error){if(error.status!==404)throw error;}}
  await core.summary.addHeading('原编译Run终态清理完成').addLink(String(input.run_id),'https://github.com/'+cfg.product.repository+'/actions/runs/'+input.run_id).addRaw('\n已保留最新成功、最新失败编译Run；活动Run保持。').write();
}

// 仅清理本轮WASM现场；工具退出未确认时保留，不创建独立清理入口。
export function cleanWork(env = process.env) {
  const work = join(root, 'target/build/wasm');
  assertRuntimeWork(work, env);
  if (resolve(env.CITIZENCHAIN_WORK_DIR || '') !== work) fail('现场不符');
  if (existsSync(join(work, '.retain-runtime-work'))) fail('Runtime工具退出未确认，保留现场');
  rmSync(work, {recursive: true, force: true});
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
  const {product, artifactNames, artifactLabel, releaseName, assertRelease, readReleaseProof, retainedBuildRuns, workflowRecords, automationRequest, nextDeliveryVersion} = cfg;
  const {mkdtempSync} = await import('node:fs'), {tmpdir} = await import('node:os');
  const {zstdCompressSync} = await import('node:zlib');
  const genesis = '0x' + '11'.repeat(32), head = '0x' + '22'.repeat(32);
  const declaration = 'pub const VERSION: RuntimeVersion = RuntimeVersion {\n    spec_version: 0,\n    transaction_version: 0,\n};\nconst OTHER: u32 = 0;\n';
  const u32 = value => {const bytes = Buffer.alloc(4); bytes.writeUInt32LE(value); return bytes;};
  const leb = value => {const bytes = []; do {let byte = value & 127; value = Math.floor(value / 128); if (value) byte |= 128; bytes.push(byte);} while (value); return Buffer.from(bytes);};
  const text = value => Buffer.concat([Buffer.from([Buffer.byteLength(value) << 2]), Buffer.from(value)]);
  const fixture = (spec, transaction, specName = cfg.product.id, padded = false) => {
    const length = value => {
      const encoded = [...leb(value)];
      if (padded) while (encoded.length < 5) {encoded[encoded.length - 1] |= 128; encoded.push(0);}
      return Buffer.from(encoded);
    };
    const body = Buffer.concat([text(specName), text(cfg.product.id), u32(0), u32(spec), u32(0), Buffer.from([0]), u32(transaction), Buffer.from([0])]);
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
    const raw = fixture(8, 4), expected = {specName: cfg.product.id, specVersion: 8, transactionVersion: 4};
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
    const raw = fixture(8, 4, cfg.product.id, true), target = {specVersion: 8, transactionVersion: 4};
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
      : method === 'chain_getFinalizedHead' ? head : {specName: cfg.product.id, specVersion: 7, transactionVersion: 3};
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
    const env = {PLATFORM: 'wasm', GITHUB_ACTIONS: 'true', GITHUB_REPOSITORY: cfg.product.repository,
      GITHUB_EVENT_NAME: 'workflow_dispatch', GITHUB_REF: 'refs/heads/main',
      GITHUB_WORKFLOW_REF: `${cfg.product.repository}/.github/workflows/wasm.yml@refs/heads/main`,
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
      return {specName: cfg.product.id, specVersion: 7, transactionVersion: 3};
    };
    assert.deepEqual(await fetchRuntimeBase(rpc, genesis), {genesisHash: genesis, blockHash: head, specName: cfg.product.id, specVersion: 7, transactionVersion: 3});
    assert.deepEqual(calls.at(-1), ['state_getRuntimeVersion', [head]]);
    await assert.rejects(fetchRuntimeBase(rpc, head));
    await assert.rejects(fetchRuntimeBase(async(method, params) => method === 'state_getRuntimeVersion' ? {specName: cfg.product.id, specVersion: 7} : rpc(method, params), genesis));
    await assert.rejects(fetchRuntimeBase(async() => {throw Error('fixture network failure');}, genesis));
    await assert.rejects(fetchRuntimeBase(async(method, params) => method === 'state_getRuntimeVersion'
      ? {specName: cfg.product.id, specVersion: 7, transactionVersion: '3'} : rpc(method, params), genesis));
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
    for (const platform of Object.values(cfg.product.platforms)) {
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
    assert.ok(workflow.includes('await runtime.publish({github,context,core})'));
    assert.ok(workflow.includes('CITIZENCHAIN_RUNTIME_BASE'));
    assert.ok(!workflow.includes('CITIZENCHAIN_RUNTIME_RPC_URL'));
  });
  // WASM夹具独立于节点软件版本，目标2/1来自链基线1/0，不强求两个数字相等。
  const observedBase={genesisHash:frozenGenesis(),blockHash:'0x'+'22'.repeat(32),specName:product.id,specVersion:1,transactionVersion:0};
  const wasmRun=(id,conclusion='success')=>({id,run_number:id,run_attempt:1,event:'workflow_dispatch',head_branch:'main',
    repository:{full_name:product.repository},path:'.github/workflows/wasm.yml',head_sha:'a'.repeat(40),
    html_url:`https://github.com/${product.repository}/actions/runs/${id}`,status:'completed',conclusion,display_title:releaseName('wasm','2')});
  function wasmRelease(source,value='2',transaction=1){
    const hash='f'.repeat(64),assets=artifactNames('wasm',value).map((name,index)=>({id:200+index,name,
      label:artifactLabel('wasm',value,name),size:3,state:'uploaded',digest:'sha256:'+hash,
      url:`https://api.github.com/repos/${product.repository}/releases/assets/${200+index}`}));
    const runtime={source_sha:source.head_sha,genesis_hash:'0x'+'11'.repeat(32),base_block_hash:'0x'+'22'.repeat(32),
      spec_name:product.id,base_spec_version:Number(value)-1,base_transaction_version:transaction-1,spec_version:Number(value),transaction_version:transaction};
    const proof={schema:1,repository:product.repository,product_id:product.id,platform:'wasm',workflow:source.path,
      version:value,tag:source.display_title,run_id:source.id,run_attempt:source.run_attempt,source_sha:source.head_sha,runtime,
      assets:assets.map(({name,label,size})=>({name,label,size,sha256:hash}))};
    return {id:100+source.id,name:proof.tag,tag_name:proof.tag,target_commitish:source.head_sha,draft:false,prerelease:false,published_at:'2026-10-19T00:00:00Z',
      body:'<!-- automation:'+JSON.stringify(proof)+' -->',assets};
  }
  test('WASM只接受整数版本，名称、附件及原Run收尾不走SemVer',()=>{
    assert.equal(runtimeVersion('0'),'0');assert.equal(runtimeVersion('4294967295'),'4294967295');
    for(const value of ['2.3.4','02','2\n',' 2','-1','1.5','4294967296',2,null]){
      assert.throws(()=>releaseName('wasm',value));
      assert.throws(()=>automationRequest('wasm','launch',{version:value,transactionVersion:'1',sha:'a'.repeat(40),base:observedBase}));
    }
    assert.throws(()=>nextDeliveryVersion('wasm',[],[],'1.0.0'));
    const launch=automationRequest('wasm','launch',{version:'2',transactionVersion:'1',sha:'a'.repeat(40),base:observedBase});
    assert.deepEqual(launch.body.inputs,{version:'2',transaction_version:'1',source_sha:'a'.repeat(40),runtime_base:JSON.stringify(observedBase)});
    assert.throws(()=>automationRequest('wasm','launch',{version:'2',sha:'a'.repeat(40),base:observedBase}));
    const settle=automationRequest('wasm','settle',{run:wasmRun(42,'failure')});
    assert.deepEqual(settle.body.client_payload,{platform:'wasm',version:'2',run_id:42,run_attempt:1,source_sha:'a'.repeat(40)});
    assert.equal(artifactLabel('wasm','2','citizenchain.compact.compressed.wasm'),'公民链·WASM·v2.compact.compressed.wasm');
  });
  test('WASM正式交付绑定显示号、双版本、源码、原Run及完整三附件',()=>{
    const source=wasmRun(42),value=wasmRelease(source);
    assert.equal(assertRelease('wasm',value,source).version,'2');
    const rewrite=mutate=>{const changed=structuredClone(value),proof=readReleaseProof('wasm',changed);mutate(proof);
      changed.body='<!-- automation:'+JSON.stringify(proof)+' -->';return changed;};
    for(const mutate of [proof=>delete proof.runtime,proof=>{proof.version='2.0.0';},
      proof=>{proof.runtime.spec_version=3;},proof=>{proof.runtime.transaction_version=2;},
      proof=>{proof.runtime.base_transaction_version=-1;},proof=>{proof.runtime.base_spec_version=0xffffffff;},
      proof=>{proof.runtime.source_sha='b'.repeat(40);},proof=>{proof.runtime.spec_name='wrong-chain';},
      proof=>{proof.runtime.genesis_hash='invalid';}])assert.throws(()=>assertRelease('wasm',rewrite(mutate),source));
    assert.throws(()=>assertRelease('wasm',value,{...source,run_attempt:2}));
    assert.throws(()=>assertRelease('wasm',value,{...source,display_title:'公民链·WASM·v3'}));
    assert.throws(()=>assertRelease('wasm',{...value,assets:value.assets.slice(1)},source));
    const rows=[wasmRun(1),wasmRun(2,'failure'),source,{...wasmRun(43),status:'in_progress'},
      {...wasmRun(99),event:'repository_dispatch'}];
    const keep=retainedBuildRuns('wasm',rows);
    assert.equal(keep.success.id,42);assert.equal(keep.failure.id,2);assert.deepEqual(keep.obsolete.map(row=>row.id),[1]);
    assert.equal(workflowRecords('wasm',rows).length,3);
  });
  test('WASM派发复用本机读链加1，传递完整基线及目标双版本',async()=>{
    const {readFile}=await import('node:fs/promises');
    const frozen=(await readFile(new URL('../runtime/primitives/src/genesis.rs',import.meta.url),'utf8'))
      .match(/pub const GENESIS_HASH:\s*\[u8;\s*32\]\s*=\s*hex_literal::hex!\("([a-f0-9]{64})"\)/u);
    assert.ok(frozen);const previous=globalThis.fetch,calls=[];let missing=false,releases=[],runs=[];
    globalThis.fetch=async(url,options)=>{
      assert.equal(String(url),localRpc+'/');const rpc=JSON.parse(options.body);calls.push(rpc.method);
      const result=rpc.method==='chain_getBlockHash'?'0x'+frozen[1]:rpc.method==='chain_getFinalizedHead'?'0x'+'22'.repeat(32)
        :{specName:product.id,specVersion:7,...(missing?{}:{transactionVersion:3})};
      return Response.json({jsonrpc:'2.0',id:1,result});
    };
    try{
      const target=await describeAction({flow:'release',platform:'wasm'}),requests=[];
      const read=async path=>{
        if(path.startsWith('actions/workflows/wasm.yml/runs'))return {workflow_runs:runs};
        if(path.startsWith('releases?'))return releases;
        if(path==='git/ref/heads/main')return {object:{sha:'a'.repeat(40)}};
        throw Error('未预期读取：'+path);
      };
      const dispatch=async request=>{requests.push(request);return {workflow_run_id:42};};
      assert.deepEqual(await target.launch({read,dispatch}),{runID:42});
      const {runtime_base,...inputs}=requests[0].inputs;
      assert.deepEqual({...requests[0],inputs},{workflow:'wasm.yml',ref:'main',inputs:{version:'8',transaction_version:'4',source_sha:'a'.repeat(40)}});
      assert.deepEqual(JSON.parse(runtime_base),{genesisHash:'0x'+frozen[1],blockHash:'0x'+'22'.repeat(32),specName:product.id,specVersion:7,transactionVersion:3});
      missing=true;await assert.rejects(target.launch({read,dispatch}));assert.equal(requests.length,1);
      missing=false;releases=[{tag_name:'公民链·WASM·v8'}];
      await assert.rejects(target.launch({read,dispatch}));assert.equal(requests.length,1);
      releases=[];runs=[{status:'in_progress'}];
      await assert.rejects(target.launch({read,dispatch}));assert.equal(requests.length,1);
      assert.deepEqual(calls.slice(0,3),['chain_getBlockHash','chain_getFinalizedHead','state_getRuntimeVersion']);
    }finally{globalThis.fetch=previous;}
  });
  // 直接调用WASM收尾函数的内存替身；不读真实GitHub、不派发、不构建、不删除真实记录。
  async function wasmCleanupFixture(options={}){
    const old={...wasmRun(1),display_title:releaseName('wasm','1')},success=wasmRun(10),failed={...wasmRun(11,'failure'),display_title:releaseName('wasm','3')};
    const builds=[old,wasmRun(2,'failure'),success,failed,{...wasmRun(12),status:'in_progress',conclusion:null}];
    const maintenance=[{...wasmRun(20),event:'repository_dispatch'}, {...wasmRun(21,'failure'),event:'repository_dispatch'},
      {...wasmRun(22),event:'repository_dispatch'}, {...wasmRun(30),event:'repository_dispatch',status:'in_progress',conclusion:null}];
    const partial=wasmRelease(failed,'3',2);partial.draft=true;partial.assets=[];
    const releases=[wasmRelease(old,'1'),wasmRelease(success),partial];
    if(options.unknownRelease)releases[0].body='来源不明';
    const tags=new Map(releases.map(value=>[value.tag_name,{object:{type:'commit',sha:success.head_sha}}])),calls=[];
    const absent=()=>{throw Object.assign(Error('合成404'),{status:404});};
    const actions={listWorkflowRuns:()=>{},getWorkflowRun:async({run_id})=>{
      const row=[...builds,...maintenance].find(value=>value.id===run_id);if(!row)return absent();
      return {data:structuredClone(options.drift&&run_id===2?{...row,run_attempt:2}:row)};
    },deleteWorkflowRun:async({run_id})=>{
      calls.push(['deleteRun',run_id]);assert.ok(![10,11,12,21,30].includes(run_id));
      if(options.failDelete)throw Object.assign(Error('合成403'),{status:403});
      if(!options.unconfirmedDelete){const rows=builds.some(row=>row.id===run_id)?builds:maintenance;rows.splice(rows.findIndex(row=>row.id===run_id),1);}
    }};
    const repos={listReleases:()=>{},getRelease:async({release_id})=>{
      const row=releases.find(value=>value.id===release_id);return row?{data:structuredClone(row)}:absent();
    },deleteRelease:async({release_id})=>{
      calls.push(['deleteRelease',release_id]);assert.notEqual(release_id,110);
      if(options.failRelease)throw Object.assign(Error('合成403'),{status:403});
      releases.splice(releases.findIndex(row=>row.id===release_id),1);
    }};
    const git={getRef:async({ref})=>{const row=tags.get(ref.slice(5));return row?{data:structuredClone(row)}:absent();},
      deleteRef:async({ref})=>{calls.push(['deleteTag',ref]);assert.notEqual(ref,'tags/'+success.display_title);tags.delete(ref.slice(5));}};
    const github={rest:{actions,repos,git},paginate:async(method,parameters)=>structuredClone(method===actions.listWorkflowRuns
      ?parameters.event==='repository_dispatch'?maintenance:builds:releases)};
    let summaries=0;const summary={addHeading(){return this;},addLink(){return this;},addRaw(){return this;},async write(){summaries++;}};
    const context={repo:{owner:'crcfrcn',repo:'citizenchain'},ref:'refs/heads/main',workflow:'公民链·WASM',eventName:'repository_dispatch',runId:30,sha:failed.head_sha,payload:{client_payload:{platform:'wasm',version:'3',run_id:11,run_attempt:1,source_sha:failed.head_sha}}};
    let error;try{await finalize({github,core:{summary},context});}
    catch(failure){error=failure;}
    return {error,calls,builds,maintenance,releases,tags,summaries};
  }
  test('WASM真实收尾正文清除旧版本及失败草稿，保留成功失败、活动Run和最近失败收尾',async()=>{
    const result=await wasmCleanupFixture();assert.equal(result.error,undefined);assert.equal(result.summaries,1);
    assert.deepEqual(result.builds.map(row=>row.id),[10,11,12]);assert.deepEqual(result.maintenance.map(row=>row.id),[21,30]);
    assert.deepEqual(result.releases.map(row=>row.id),[110]);assert.deepEqual([...result.tags.keys()],['公民链·WASM·v2']);
    assert.deepEqual(result.calls.slice(0,3),[['deleteTag','tags/公民链·WASM·v1'],['deleteRelease',101],['deleteTag','tags/公民链·WASM·v3']]);
  });
  test('WASM收尾拒绝API失败、删除未确认、attempt漂移和未知来源，不误删保留项',async()=>{
    for(const [options,expected] of [[{failDelete:true},/合成403/],[{unconfirmedDelete:true},/旧Run删除未确认/],
      [{drift:true},/删除前旧Run终态变化/],[{unknownRelease:true},/正式版本来源证明必须唯一/],[{failRelease:true},/合成403/]]){
      const result=await wasmCleanupFixture(options);assert.ok(result.error);assert.match(result.error.message,expected);assert.equal(result.summaries,0);
      assert.ok(result.builds.some(row=>row.id===10));assert.ok(result.builds.some(row=>row.id===11));
      assert.ok(result.releases.some(row=>row.id===110));assert.ok(result.tags.has('公民链·WASM·v2'));
      if(options.unknownRelease)assert.equal(result.calls.length,0);
    }
  });

  test('Runtime基线完整传递，字段、链身份、缺失版本及溢出拒绝', () => {
    const base={genesisHash:genesis,blockHash:head,specName:product.id,specVersion:7,transactionVersion:3};
    assert.deepEqual(parseRuntimeBase(JSON.stringify(base),genesis),{base,target:{specVersion:8,transactionVersion:4}});
    for(const invalid of [undefined,null,'{',[],{...base,extra:1},{...base,genesisHash:head},
      {...base,blockHash:'invalid'},{...base,blockHash:head+'\n'},{...base,specName:'wrong'},{...base,transactionVersion:undefined},
      {...base,specVersion:maximumVersion},{...base,transactionVersion:-1}]){
      assert.throws(()=>parseRuntimeBase(invalid,genesis));
    }
    assert.throws(()=>runtimeLaunchRequest({version:'3',transactionVersion:'1',sha:'a'.repeat(40),base:observedBase}));
  });
  test('配置入口只转调Runtime，创建声明不读链或派发', async () => {
    const previous=globalThis.fetch;
    globalThis.fetch=async()=>{throw Error('声明阶段不能访问网络');};
    try{
      const action=await cfg.describeAction({flow:'release',platform:'wasm'});
      assert.equal(action.repository,product.repository);assert.equal(typeof action.launch,'function');
      await assert.rejects(cfg.describeAction({flow:'invalid',platform:'wasm'}));
    }finally{globalThis.fetch=previous;}
  });
  test('Runner准备只消费本轮基线，输入变化拒绝且不查询RPC', async () => {
    const names=['CITIZENCHAIN_RUNTIME_BASE','CITIZENCHAIN_RELEASE_VERSION','CITIZENCHAIN_RUNTIME_TRANSACTION_VERSION','CITIZENCHAIN_SOURCE_SHA'];
    const saved=names.map(name=>process.env[name]),previous=globalThis.fetch;
    const source={...wasmRun(42),status:'in_progress',conclusion:null};
    const context={repo:{owner:'crcfrcn',repo:'citizenchain'},ref:'refs/heads/main',workflow:'公民链·WASM',
      eventName:'workflow_dispatch',runId:42,sha:source.head_sha};
    const actions={getWorkflowRun:async()=>({data:source}),listWorkflowRuns:()=>{}},repos={listReleases:()=>{}};
    const github={rest:{actions,repos,git:{getRef:async()=>{throw Object.assign(Error('合成404'),{status:404});}}},
      paginate:async method=>method===actions.listWorkflowRuns?[source]:[]};
    let messages=0;const core={info:()=>{messages++;}};
    globalThis.fetch=async()=>{throw Error('Runner不得查询本机RPC');};
    Object.assign(process.env,{CITIZENCHAIN_RUNTIME_BASE:JSON.stringify(observedBase),CITIZENCHAIN_RELEASE_VERSION:'2',
      CITIZENCHAIN_RUNTIME_TRANSACTION_VERSION:'1',CITIZENCHAIN_SOURCE_SHA:source.head_sha});
    try{
      await prepare({github,context,core});assert.equal(messages,1);
      process.env.CITIZENCHAIN_RUNTIME_BASE=JSON.stringify({...observedBase,specVersion:2});
      await assert.rejects(prepare({github,context,core}),/基线与候选输入不一致/);
      await assert.rejects(prepare({github,context:{...context,eventName:'repository_dispatch'},core}),/工作流身份或事件不符/);
    }finally{
      globalThis.fetch=previous;
      names.forEach((name,index)=>{if(saved[index]===undefined)delete process.env[name];else process.env[name]=saved[index];});
    }
  });

}

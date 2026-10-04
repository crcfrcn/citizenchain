#!/usr/bin/env node
import { spawnSync as runExactProcess } from 'node:child_process';
import { createHash } from 'node:crypto';
import {
  appendFileSync,
  existsSync,
  lstatSync,
  mkdirSync,
  readlinkSync,
  rmSync,
  symlinkSync,
  writeFileSync,
} from 'node:fs';
import path from 'node:path';
import { pathToFileURL } from 'node:url';

// 缓存身份使用固定语义前缀，不把内部实现误当成版本化协议。
export const CI_CACHE_SCHEMA = 'ci';

function required(value, label) {
  const normalized = String(value ?? '').trim();
  if (!normalized) throw new Error(`缺少${label}`);
  return normalized;
}

function token(value, label) {
  const normalized = required(value, label).toLowerCase();
  // GitHub 作业名允许下划线；仍禁止路径分隔符、空白和越界长度。
  if (!/^[a-z0-9][a-z0-9._-]{0,63}$/.test(normalized)) {
    throw new Error(`${label}不是安全缓存标识`);
  }
  return normalized;
}

function positiveInteger(value, label) {
  const normalized = required(value, label);
  if (!/^[1-9][0-9]*$/.test(normalized)) throw new Error(`${label}必须是正整数`);
  return normalized;
}

function repositoryIdentity(value) {
  const normalized = required(value, '仓库身份');
  if (!/^[A-Za-z0-9_.-]+\/[A-Za-z0-9_.-]+$/.test(normalized)) {
    throw new Error('仓库身份必须使用owner/repository');
  }
  return {
    api: normalized,
    key: normalized.toLowerCase().replace('/', '.'),
  };
}

export function cacheIdentity(input) {
  const repository = repositoryIdentity(input.repository);
  const toolchain = required(input.toolchainFingerprint, '工具链指纹').toLowerCase();
  if (!/^[0-9a-f]{64}$/.test(toolchain)) throw new Error('工具链指纹必须是SHA-256');
  const identity = Object.freeze({
    repository: repository.api,
    repositoryKey: repository.key,
    product: token(input.product, '产品'),
    platform: token(input.platform, '平台'),
    architecture: token(input.architecture, '架构'),
    component: token(input.component, 'CI组件'),
    runnerOs: token(input.runnerOs, 'Runner系统'),
    runnerArch: token(input.runnerArch, 'Runner架构'),
    toolchainFingerprint: toolchain,
  });
  const logicalKey = [
    CI_CACHE_SCHEMA,
    identity.product,
    identity.platform,
    identity.architecture,
    identity.component,
    identity.runnerOs,
    identity.runnerArch,
  ].join('-');
  const baseKey = `${logicalKey}-${toolchain.slice(0, 16)}`;
  if (baseKey.length > 400) throw new Error('缓存身份超过安全长度');
  return Object.freeze({ ...identity, logicalKey, baseKey });
}

export function cacheKeys(identity, runId, attempt) {
  const run = positiveInteger(runId, 'GitHub Run ID');
  const runAttempt = positiveInteger(attempt, 'GitHub Run Attempt');
  return Object.freeze({
    successPrefix: `${identity.baseKey}-success-`,
    failurePrefix: `${identity.baseKey}-failure-`,
    successKey: `${identity.baseKey}-success-${run}-${runAttempt}`,
    failureKey: `${identity.baseKey}-failure-${run}-${runAttempt}`,
  });
}

export function parseCacheKey(identity, key) {
  const parsed = parseLogicalCacheKey(identity, key);
  return parsed?.toolchain === identity.toolchainFingerprint.slice(0, 16) ? parsed : null;
}

export function parseLogicalCacheKey(identity, key) {
  const prefix = `${identity.logicalKey}-`;
  if (!String(key).startsWith(prefix)) return null;
  const remainder = String(key).slice(prefix.length);
  const toolchain = remainder.slice(0, 16);
  if (!/^[0-9a-f]{16}$/.test(toolchain) || remainder[16] !== '-') return null;
  const stateAndRun = remainder.slice(17);
  for (const state of ['success', 'failure']) {
    const statePrefix = `${state}-`;
    if (!stateAndRun.startsWith(statePrefix)) continue;
    const match = stateAndRun.slice(statePrefix.length).match(/^([1-9][0-9]*)-([1-9][0-9]*)$/);
    if (!match) return null;
    return Object.freeze({ toolchain, state, runId: match[1], attempt: match[2] });
  }
  return null;
}

function compareCache(left, right) {
  for (const field of ['runId', 'attempt', 'id']) {
    const difference = BigInt(left[field]) - BigInt(right[field]);
    if (difference !== 0n) return difference > 0n ? 1 : -1;
  }
  return 0;
}

function recognizedCaches(identity, caches, ref, currentToolchainOnly = false) {
  const rows = [];
  for (const cache of caches) {
    if (ref && cache.ref !== ref) continue;
    const parsed = parseLogicalCacheKey(identity, cache.key);
    if (!parsed || !/^[1-9][0-9]*$/.test(String(cache.id ?? ''))) continue;
    if (currentToolchainOnly
        && parsed.toolchain !== identity.toolchainFingerprint.slice(0, 16)) continue;
    rows.push({ ...cache, ...parsed, id: String(cache.id) });
  }
  return rows;
}

export function selectLatestCache(identity, caches, state = 'success', ref = '') {
  if (!['success', 'failure'].includes(state)) throw new Error('缓存状态无效');
  const rows = recognizedCaches(identity, caches, ref, true)
    .filter((cache) => cache.state === state);
  rows.sort(compareCache);
  return rows.at(-1) ?? null;
}

export function planCachePrune(identity, caches, ref = '') {
  const rows = recognizedCaches(identity, caches, ref);
  const retained = new Set();
  for (const state of ['success', 'failure']) {
    const candidates = rows.filter((cache) => cache.state === state).sort(compareCache);
    const latest = candidates.at(-1);
    if (latest) retained.add(latest.id);
  }
  return Object.freeze({
    retain: rows.filter((cache) => retained.has(cache.id)),
    remove: rows.filter((cache) => !retained.has(cache.id)),
  });
}

function pathImplementation(runnerOs) {
  return runnerOs === 'windows' ? path.win32 : path.posix;
}

export function cachePathPlan(identity, runnerTemp, entries) {
  const pathApi = pathImplementation(identity.runnerOs);
  const temp = required(runnerTemp, 'Runner临时目录');
  if (!pathApi.isAbsolute(temp)) throw new Error('Runner临时目录必须是绝对路径');
  const names = String(entries ?? '')
    .split(/[\n,]/)
    .map((entry) => entry.trim())
    .filter(Boolean);
  if (names.length === 0) throw new Error('至少需要一个成功缓存路径');
  if (new Set(names).size !== names.length) throw new Error('成功缓存路径不能重复');
  for (const name of names) {
    if (!/^[a-z0-9][a-z0-9._-]*(\/[a-z0-9][a-z0-9._-]*)*$/.test(name)) {
      throw new Error(`缓存相对路径无效：${name}`);
    }
  }
  const digest = createHash('sha256').update(identity.baseKey).digest('hex').slice(0, 20);
  const rootName = `${identity.product}-${identity.platform}-${identity.component}-${digest}`;
  const root = pathApi.resolve(temp, 'ci-cache', rootName);
  const expectedParent = pathApi.resolve(temp, 'ci-cache');
  const relative = pathApi.relative(expectedParent, root);
  if (!relative || relative.startsWith('..') || pathApi.isAbsolute(relative)) {
    throw new Error('缓存根目录逃出Runner临时目录');
  }
  return Object.freeze({
    root,
    successPaths: names.map((name) => pathApi.join(root, ...name.split('/'))),
    failurePath: pathApi.join(root, 'failure-diagnostic'),
  });
}

function relativeEntries(value, label) {
  const entries = String(value ?? '').split(/[\n,]/).map((entry) => entry.trim()).filter(Boolean);
  for (const entry of entries) {
    if (!/^[a-z0-9][a-z0-9._-]*(\/[a-z0-9][a-z0-9._-]*)*$/.test(entry)) {
      throw new Error(`${label}相对路径无效：${entry}`);
    }
  }
  return entries;
}

function resolvedChild(pathApi, parent, relative, label) {
  const target = pathApi.resolve(parent, ...relative.split('/'));
  const child = pathApi.relative(parent, target);
  if (!child || child.startsWith('..') || pathApi.isAbsolute(child)) {
    throw new Error(`${label}逃出允许根`);
  }
  return target;
}

export function wireCacheLinks(identity, runnerTemp, entries, workspace, links) {
  const pathApi = pathImplementation(identity.runnerOs);
  const plan = cachePathPlan(identity, runnerTemp, entries);
  const workspaceRoot = required(workspace, 'GitHub工作区');
  if (!pathApi.isAbsolute(workspaceRoot)) throw new Error('GitHub工作区必须是绝对路径');
  const rows = String(links ?? '').split(/\n/).map((entry) => entry.trim()).filter(Boolean);
  for (const row of rows) {
    const separator = row.indexOf('=');
    if (separator <= 0) throw new Error(`缓存目录链接无效：${row}`);
    const sourceRelative = row.slice(0, separator);
    const cacheRelative = row.slice(separator + 1);
    relativeEntries(sourceRelative, '工作区生成目录');
    relativeEntries(cacheRelative, '受控缓存目录');
    const source = resolvedChild(pathApi, workspaceRoot, sourceRelative, '工作区生成目录');
    const target = resolvedChild(pathApi, plan.root, cacheRelative, '受控缓存目录');
    mkdirSync(pathApi.dirname(source), { recursive: true });
    mkdirSync(target, { recursive: true });
    if (existsSync(source)) {
      const status = lstatSync(source);
      if (status.isSymbolicLink()) {
        const linked = pathApi.resolve(pathApi.dirname(source), readlinkSync(source));
        if (linked === target) continue;
      }
      throw new Error(`工作区生成目录已存在且不是准确缓存链接：${sourceRelative}`);
    }
    symlinkSync(target, source, identity.runnerOs === 'windows' ? 'junction' : 'dir');
  }
  return plan;
}

export function sanitizeCacheFinals(identity, runnerTemp, entries, finals) {
  const pathApi = pathImplementation(identity.runnerOs);
  const plan = cachePathPlan(identity, runnerTemp, entries);
  for (const relative of relativeEntries(finals, '最终候选')) {
    rmSync(resolvedChild(pathApi, plan.root, relative, '最终候选'), {
      recursive: true,
      force: true,
    });
  }
}

function identityFromEnvironment(environment) {
  return cacheIdentity({
    repository: environment.GITHUB_REPOSITORY,
    product: environment.CI_CACHE_PRODUCT,
    platform: environment.CI_CACHE_PLATFORM,
    architecture: environment.CI_CACHE_ARCHITECTURE,
    component: environment.CI_CACHE_COMPONENT,
    runnerOs: environment.RUNNER_OS,
    runnerArch: environment.RUNNER_ARCH,
    toolchainFingerprint: environment.CI_CACHE_TOOLCHAIN_FINGERPRINT,
  });
}

function githubHeaders(tokenValue) {
  return {
    Accept: 'application/vnd.github+json',
    Authorization: `Bearer ${required(tokenValue, 'GitHub Actions令牌')}`,
    'X-GitHub-Api-Version': '2022-11-28',
    'User-Agent': 'ci-cache',
  };
}

async function githubRequest(url, tokenValue, options = {}) {
  const response = await fetch(url, {
    ...options,
    headers: { ...githubHeaders(tokenValue), ...(options.headers ?? {}) },
  });
  if (!response.ok) throw new Error(`GitHub缓存API失败：${response.status}`);
  if (response.status === 204) return null;
  return response.json();
}

async function listRepositoryCaches(repository, tokenValue) {
  const caches = [];
  for (let page = 1; ; page += 1) {
    const endpoint = `https://api.github.com/repos/${repository}/actions/caches?per_page=100&page=${page}`;
    const result = await githubRequest(endpoint, tokenValue);
    const rows = Array.isArray(result?.actions_caches) ? result.actions_caches : [];
    caches.push(...rows);
    if (rows.length < 100) break;
  }
  return caches;
}

async function deleteRepositoryCache(repository, cacheId, tokenValue) {
  await githubRequest(
    `https://api.github.com/repos/${repository}/actions/caches/${cacheId}`,
    tokenValue,
    { method: 'DELETE' },
  );
}

function output(name, value, environment) {
  const target = environment.GITHUB_OUTPUT;
  if (!target) return;
  const text = String(value);
  if (text.includes('\n')) {
    const delimiter = `CI_CACHE_${name.toUpperCase()}_EOF`;
    if (text.includes(delimiter)) throw new Error('GitHub多行输出包含保留分隔符');
    appendFileSync(target, `${name}<<${delimiter}\n${text}\n${delimiter}\n`);
  } else {
    appendFileSync(target, `${name}=${text}\n`);
  }
}

function persistEnvironment(name, value, environment) {
  const target = environment.GITHUB_ENV;
  if (!target) return;
  const text = String(value ?? '');
  if (text.includes('\n')) {
    const delimiter = `CI_CACHE_ENV_${name}_EOF`;
    if (text.includes(delimiter)) throw new Error('GitHub环境变量包含保留分隔符');
    appendFileSync(target, `${name}<<${delimiter}\n${text}\n${delimiter}\n`);
  } else {
    appendFileSync(target, `${name}=${text}\n`);
  }
}

function commandContext(environment) {
  requireExactRemoteJobEnvironment();
  const identity = identityFromEnvironment(environment);
  const keys = cacheKeys(identity, environment.GITHUB_RUN_ID, environment.GITHUB_RUN_ATTEMPT);
  const paths = cachePathPlan(identity, environment.RUNNER_TEMP, environment.CI_CACHE_PATHS);
  const ref = required(environment.GITHUB_REF, 'GitHub Ref');
  const tokenValue = environment.GH_TOKEN || environment.GITHUB_TOKEN;
  return { identity, keys, paths, ref, tokenValue };
}

async function prepare(environment) {
  const context = commandContext(environment);
  const caches = await listRepositoryCaches(context.identity.repository, context.tokenValue);
  const latest = selectLatestCache(context.identity, caches, 'success', context.ref);
  for (const directory of [...context.paths.successPaths, context.paths.failurePath]) {
    mkdirSync(directory, { recursive: true });
  }
  const restoreKey = latest?.key ?? `${context.keys.successPrefix}none`;
  output('cache_root', context.paths.root, environment);
  output('success_paths', context.paths.successPaths.join('\n'), environment);
  output('failure_paths', context.paths.failurePath, environment);
  output('restore_key', restoreKey, environment);
  output('success_key', context.keys.successKey, environment);
  output('failure_key', context.keys.failureKey, environment);
  for (const name of [
    'CI_CACHE_PRODUCT', 'CI_CACHE_PLATFORM', 'CI_CACHE_ARCHITECTURE',
    'CI_CACHE_COMPONENT', 'CI_CACHE_TOOLCHAIN_FINGERPRINT', 'CI_CACHE_PATHS',
    'CI_CACHE_LINKS', 'CI_CACHE_FINALS', 'CI_CACHE_WORKFLOW', 'CI_CACHE_JOB',
  ]) persistEnvironment(name, environment[name] ?? '', environment);
  persistEnvironment('CI_INCREMENTAL_ROOT', context.paths.root, environment);
  const pathByName = new Map(
    relativeEntries(environment.CI_CACHE_PATHS, '成功缓存').map(
      (name, index) => [name, context.paths.successPaths[index]],
    ),
  );
  const environmentPaths = {
    'cargo-home': 'CARGO_HOME',
    'cargo-target': 'CARGO_TARGET_DIR',
    'dart-pub': 'PUB_CACHE',
    gradle: 'GRADLE_USER_HOME',
    cocoapods: 'CP_HOME_DIR',
    npm: 'npm_config_cache',
    xdg: 'XDG_CACHE_HOME',
  };
  for (const [cacheName, environmentName] of Object.entries(environmentPaths)) {
    if (pathByName.has(cacheName)) persistEnvironment(environmentName, pathByName.get(cacheName), environment);
  }
  if (pathByName.has('cargo-target')) persistEnvironment('CARGO_INCREMENTAL', '1', environment);
  if (pathByName.has('cargo-home') && environment.GITHUB_PATH) {
    appendFileSync(environment.GITHUB_PATH, `${path.join(pathByName.get('cargo-home'), 'bin')}\n`);
  }
  if (!environment.GITHUB_OUTPUT) {
    process.stdout.write(`${JSON.stringify({
      cacheRoot: context.paths.root,
      restoreKey,
      successKey: context.keys.successKey,
      failureKey: context.keys.failureKey,
    })}\n`);
  }
}

function wire(environment) {
  const context = commandContext(environment);
  wireCacheLinks(
    context.identity,
    environment.RUNNER_TEMP,
    environment.CI_CACHE_PATHS,
    environment.GITHUB_WORKSPACE,
    environment.CI_CACHE_LINKS,
  );
}

function sanitize(environment) {
  const context = commandContext(environment);
  sanitizeCacheFinals(
    context.identity,
    environment.RUNNER_TEMP,
    environment.CI_CACHE_PATHS,
    environment.CI_CACHE_FINALS,
  );
}

function writeTerminalRecord(environment) {
  const context = commandContext(environment);
  const state = token(environment.CI_CACHE_TERMINAL_STATE, '终态');
  if (!['success', 'failure'].includes(state)) throw new Error('终态只能是success或failure');
  const sourceSha = required(environment.GITHUB_SHA, 'GitHub源码SHA').toLowerCase();
  if (!/^[0-9a-f]{40}$/.test(sourceSha)) throw new Error('GitHub源码SHA无效');
  const directory = state === 'success' ? context.paths.successPaths[0] : context.paths.failurePath;
  mkdirSync(directory, { recursive: true });
  const record = {
    schema: CI_CACHE_SCHEMA,
    state,
    repository: context.identity.repository,
    product: context.identity.product,
    platform: context.identity.platform,
    architecture: context.identity.architecture,
    component: context.identity.component,
    runner_os: context.identity.runnerOs,
    runner_arch: context.identity.runnerArch,
    source_sha: sourceSha,
    run_id: positiveInteger(environment.GITHUB_RUN_ID, 'GitHub Run ID'),
    run_attempt: positiveInteger(environment.GITHUB_RUN_ATTEMPT, 'GitHub Run Attempt'),
    workflow: token(environment.CI_CACHE_WORKFLOW, 'Workflow'),
    job: token(environment.CI_CACHE_JOB, 'Job'),
  };
  const receipt = path.join(directory, `${state}.json`);
  // 成功目录可能来自上一份成功缓存；新成功只替换旧成功回执，不累积代次文件。
  rmSync(receipt, { force: true });
  writeFileSync(
    receipt,
    `${JSON.stringify(record, null, 2)}\n`,
    { flag: 'wx' },
  );
}

async function prune(environment) {
  const context = commandContext(environment);
  const state = token(environment.CI_CACHE_TERMINAL_STATE, '终态');
  if (!['success', 'failure'].includes(state)) throw new Error('终态只能是success或failure');
  const currentKey = state === 'success' ? context.keys.successKey : context.keys.failureKey;
  const caches = await listRepositoryCaches(context.identity.repository, context.tokenValue);
  const currentExists = caches.some(
    (cache) => cache.key === currentKey && cache.ref === context.ref,
  );
  if (!currentExists) throw new Error('新缓存槽尚未确认存在，拒绝删除历史缓存');
  const plan = planCachePrune(context.identity, caches, context.ref);
  for (const cache of plan.remove) {
    await deleteRepositoryCache(context.identity.repository, cache.id, context.tokenValue);
  }
  process.stdout.write(
    `CI缓存收口完成：保留${plan.retain.length}个，删除${plan.remove.length}个\n`,
  );
}


// 本文件只执行 citizenchain.linux-amd.ci 的 build-desktop Job；阶段编号由本仓唯一 Workflow 固定，禁止接收其它身份。
export const EXACT_REMOTE_JOB_IDENTITY = Object.freeze({"pipeline":"citizenchain.linux-amd.ci","job":"build-desktop"});

function requireExactRemoteJobEnvironment() {
  const expected = 'crcfrcn/citizenchain';
  if (!expected || process.env.GITHUB_REPOSITORY !== expected) {
    throw new Error('准确远端Job仓库身份无效');
  }
}
const workflowSteps = Object.freeze({"0":{"shell":"bash","source":"node \"$GITHUB_WORKSPACE/scripts/node/ci/linux-amd/build-desktop/execute.mjs\" prepare"},"1":{"shell":"bash","source":"node \"$GITHUB_WORKSPACE/scripts/node/ci/linux-amd/build-desktop/execute.mjs\" wire"},"2":{"shell":"bash","source":"node \"$GITHUB_WORKSPACE/scripts/node/ci/linux-amd/build-desktop/execute.mjs\" sanitize"},"3":{"shell":"bash","source":"set -euo pipefail\ntest \"$(git rev-parse HEAD)\" = \"$GMB_SOURCE_SHA\"\n"},"4":{"shell":"bash","source":"set -euo pipefail\nif [ \"${RUNNER_ARCH}\" != \"${EXPECTED_RUNNER_ARCH}\" ]; then\n  echo \"::error::runner 架构不符合 ${EXPECTED_RUNNER_ARCH}: 当前为 ${RUNNER_ARCH}\"\n  exit 1\nfi\n"},"5":{"shell":"bash","source":"set -euo pipefail\nnode <<'NODE'\nconst fs = require('fs');\nconst targetSource = process.env.CITIZENCHAIN_CI_BUNDLE_TARGETS;\nconst bundleTargets = (targetSource || '')\n  .split(',')\n  .map((target) => target.trim())\n  .filter(Boolean);\nif (bundleTargets.length === 0) {\n  throw new Error('缺少本 matrix 的 Tauri bundle targets。');\n}\n// 候选版本已在当前 runner 工作区应用；这里只设置本 matrix 的 bundle 目标。\nconst tauriPath = 'node/tauri.conf.json';\nconst tauri = JSON.parse(fs.readFileSync(tauriPath, 'utf8'));\nconst version = tauri.version;\ntauri.bundle = tauri.bundle || {};\n// 中文注释：CI 只生成用户安装包；正式 updater 候选只属于独立 Release workflow。\ntauri.bundle.targets = bundleTargets;\n// 中文注释：GitHub 只构建无私钥候选产物；updater 签名统一由本机原生安全进程在 Touch ID 后完成。\ndelete tauri.bundle.createUpdaterArtifacts;\n// 中文注释：CI 不创建正式版本 Tag，诊断安装包保留源码中的正式 updater 配置。\nfs.writeFileSync(tauriPath, `${JSON.stringify(tauri, null, 2)}\n`);\nconsole.log(`桌面端版本(源码): ${version}`);\nconsole.log(`Tauri bundle targets: ${bundleTargets.join(',')}`);\nNODE\n"},"6":{"shell":"bash","source":"node \"$GITHUB_WORKSPACE/scripts/node/ci/linux-amd/index.mjs\" linux-deps"},"7":{"shell":"bash","source":"case \"$RUNNER_OS/$RUNNER_ARCH\" in\n  Linux/ARM64) platform=linux-arm ;;\n  Linux/X64) platform=linux-amd ;;\n  *) echo 'CitizenChain Linux protoc宿主不受支持' >&2; exit 1 ;;\nesac\nprotoc_executable=\"$(node scripts/dependencies.mjs prepare protoc \"$platform\" \"$RUNNER_TEMP/citizenchain-protoc/$platform\")\"\n{\n  echo \"LLVM_CONFIG_PATH=$(command -v llvm-config)\"\n  echo \"LIBCLANG_PATH=$(llvm-config --libdir)\"\n  echo \"PROTOC=$protoc_executable\"\n} >> \"$GITHUB_ENV\"\n"},"8":{"shell":"bash","source":"npm --prefix node/frontend ci\nnpm --prefix node/frontend run build\n"},"9":{"shell":"bash","source":"# 中文注释：安装包只携带节点软件；首次启动按冻结 chainspec 本地初始化并联网同步。\nrm -rf node/resources/genesis-state\n"},"10":{"shell":"bash","source":"node frontend/node_modules/@tauri-apps/cli/tauri.js build --ci -- --locked --config \"$GITHUB_WORKSPACE/config.toml\""},"11":{"shell":"bash","source":"set -euo pipefail\nmkdir -p citizenchain-release\ndeb_file=\"$(find target/release/bundle/deb -name '*.deb' | head -1)\"\nif [ -z \"$deb_file\" ]; then\n  echo \"::error::未找到 Linux deb 安装包。\"\n  exit 1\nfi\nactual_deb_arch=\"$(dpkg-deb -f \"$deb_file\" Architecture)\"\nif [ \"$actual_deb_arch\" != \"$EXPECTED_DEB_ARCH\" ]; then\n  echo \"::error::deb 架构不符合 ${EXPECTED_DEB_ARCH}: 当前为 ${actual_deb_arch}\"\n  exit 1\nfi\ncp \"$deb_file\" \"citizenchain-release/${INSTALLER_NAME}\"\nls -lh citizenchain-release\n"},"12":{"shell":"bash","source":"node \"$GITHUB_WORKSPACE/scripts/node/ci/linux-amd/build-desktop/execute.mjs\" sanitize\nnode \"$GITHUB_WORKSPACE/scripts/node/ci/linux-amd/build-desktop/execute.mjs\" record\n"},"13":{"shell":"bash","source":"node \"$GITHUB_WORKSPACE/scripts/node/ci/linux-amd/build-desktop/execute.mjs\" prune"},"14":{"shell":"bash","source":"node \"$GITHUB_WORKSPACE/scripts/node/ci/linux-amd/build-desktop/execute.mjs\" sanitize\nnode \"$GITHUB_WORKSPACE/scripts/node/ci/linux-amd/build-desktop/execute.mjs\" record\n"},"15":{"shell":"bash","source":"node \"$GITHUB_WORKSPACE/scripts/node/ci/linux-amd/build-desktop/execute.mjs\" prune"}});

function runExactWorkflowStep(index) {
  requireExactRemoteJobEnvironment();
  if (!/^(?:0|[1-9][0-9]*)$/.test(String(index || '')) || !Object.hasOwn(workflowSteps, String(index))) {
    throw new Error('准确远端Job阶段无效');
  }
  const step = workflowSteps[String(index)];
  const command = step.shell === 'pwsh' ? 'pwsh' : (process.platform === 'win32' ? 'bash' : '/bin/bash');
  const args = step.shell === 'pwsh'
    ? ['-NoLogo', '-NoProfile', '-NonInteractive', '-Command', step.source]
    : ['--noprofile', '--norc', '-e', '-o', 'pipefail', '-c', step.source];
  const result = runExactProcess(command, args, { cwd: process.cwd(), env: process.env, stdio: 'inherit' });
  if (result.error) throw new Error('准确远端Job阶段无法启动');
  if (result.status !== 0) process.exitCode = Number.isInteger(result.status) ? result.status : 1;
}

async function main() {
  const command = process.argv[2];
  if (command === 'workflow-step') return runExactWorkflowStep(process.argv[3]);
  if (command === 'prepare') return prepare(process.env);
  if (command === 'wire') return wire(process.env);
  if (command === 'sanitize') return sanitize(process.env);
  if (command === 'record') return writeTerminalRecord(process.env);
  if (command === 'prune') return prune(process.env);
  throw new Error('用法：ci-cache.mjs <workflow-step|prepare|wire|sanitize|record|prune>');
}

const invokedPath = process.argv[1] ? pathToFileURL(path.resolve(process.argv[1])).href : '';
if (invokedPath === import.meta.url) {
  main().catch((error) => {
    console.error(error instanceof Error ? error.message : String(error));
    process.exitCode = 1;
  });
}

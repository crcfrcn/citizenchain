#!/usr/bin/env node
import {withFixedWork,remoteStep,fixedWork} from '../target.mjs';
const directEntry=process.argv[1]===import.meta.filename&&!process.execArgv.some(v=>/^(?:-e|-p|--eval|--print)(?:=|$)/u.test(v));
const inlineTestEntry=directEntry&&Boolean(process.env.NODE_TEST_CONTEXT)&&process.argv.length===2;
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
import {remoteEnvironment as productRemoteEnvironment,temporaryRoot} from '../build.mjs';
import {mkdtempSync as actionDirectory,writeFileSync as writeAction,rmSync as removeAction} from 'node:fs';
import {join as joinAction} from 'node:path';
// 同一平台流程只有一个实现；Job保持原有身份、步骤、取消和缓存隔离。
export const JOB_IDENTITIES=Object.freeze({"build-wasm":{"pipeline":"citizenchain.wasm.ci","job":"build-wasm"}});
export const JOB_STEPS=Object.freeze({"build-wasm":{"0":{"shell":"bash","source":"node \"$GITHUB_WORKSPACE/scripts/ci/wasm.mjs\" build-wasm prepare"},"1":{"shell":"bash","source":"node \"$GITHUB_WORKSPACE/scripts/ci/wasm.mjs\" build-wasm wire"},"2":{"shell":"bash","source":"node \"$GITHUB_WORKSPACE/scripts/ci/wasm.mjs\" build-wasm sanitize"},"3":{"shell":"bash","source":"test \"$(git rev-parse HEAD)\" = \"$GMB_SOURCE_SHA\"\npython3 - <<'PY'\nimport os\nimport re\nfrom pathlib import Path\nruntime_path = Path(\"runtime/src/lib.rs\")\ntest_path = Path(\"runtime/src/tests/cases.rs\")\nruntime_text = runtime_path.read_text(encoding=\"utf-8\")\ntest_text = test_path.read_text(encoding=\"utf-8\")\nruntime_pattern = re.compile(r\"(?m)^(s*spec_version:s*)(d+)(s*,s*)$\")\ntest_pattern = re.compile(r\"assert_eq!(VERSION.spec_version,s*(d+));\")\nruntime_matches = list(runtime_pattern.finditer(runtime_text))\ntest_matches = list(test_pattern.finditer(test_text))\nif len(runtime_matches) != 1 or len(test_matches) != 1:\n    raise SystemExit(\"Runtime spec_version 真源或测试断言不唯一\")\nsource_version = int(runtime_matches[0].group(2))\ntest_version = int(test_matches[0].group(1))\nif source_version != test_version:\n    raise SystemExit(\"Runtime 源码与测试断言的 spec_version 不一致\")\nif source_version < 0 or source_version > 2**32 - 1:\n    raise SystemExit(\"Runtime 源码 spec_version 无效\")\nsummary = Path(os.environ[\"GITHUB_STEP_SUMMARY\"])\nwith summary.open(\"a\", encoding=\"utf-8\") as out:\n    out.write(\"## CitizenChain WASM 构建来源\n\n\")\n    out.write(\"- 构建用途：纯 CI 验证\n\")\n    out.write(f\"- 源码 spec_version：`{source_version}`\n\")\n    out.write(f\"- 源码提交：`{os.environ['GMB_SOURCE_SHA']}`\n\")\n    out.write(\"- CI 不读取正式链、不分配正式版本、不创建 Tag\n\")\nPY\n"},"4":{"shell":"bash","source":"node \"$GITHUB_WORKSPACE/scripts/ci/wasm.mjs\" linux-deps\ncase \"$RUNNER_OS/$RUNNER_ARCH\" in\n  Linux/ARM64) platform=linux-arm ;;\n  Linux/X64) platform=linux-amd ;;\n  *) echo 'CitizenChain Runtime protoc宿主不受支持' >&2; exit 1 ;;\nesac\nprotoc_executable=\"$(node scripts/resources.mjs protoc \"$platform\" \"$RUNNER_TEMP/citizenchain-protoc/$platform\")\"\nprintf 'PROTOC=%s\\n' \"$protoc_executable\" >> \"$GITHUB_ENV\""},"5":{"shell":"bash","source":"cargo --config \"$GITHUB_WORKSPACE/config.toml\" metadata --locked --no-deps --format-version 1 >/dev/null\npython3 - <<'PY'\nimport hashlib\nimport os\nimport subprocess\nfrom pathlib import Path\ndef command_output(*command: str) -> str:\n    return subprocess.check_output(\n        command,\n        text=True,\n        stderr=subprocess.STDOUT,\n    ).strip()\nlock_data = Path(\"Cargo.lock\").read_bytes()\nsummary = Path(os.environ[\"GITHUB_STEP_SUMMARY\"])\nwith summary.open(\"a\", encoding=\"utf-8\") as out:\n    out.write(\"\n## CitizenChain WASM 固定构建环境\n\n\")\n    out.write(\"- runner label：`ubuntu-24.04`\n\")\n    out.write(f\"- runner OS / arch：`{os.environ['RUNNER_OS']} / {os.environ['RUNNER_ARCH']}`\n\")\n    out.write(f\"- runner image OS：`{os.environ.get('ImageOS', 'unknown')}`\n\")\n    out.write(f\"- runner image version：`{os.environ.get('ImageVersion', 'unknown')}`\n\")\n    out.write(\n        \"- checkout action：\"\n        \"`fbc6f3992d24b796d5a048ff273f7fcc4a7b6c09` (`v5.1.0`)\n\"\n    )\n    out.write(\n        \"- rust-toolchain action：\"\n        f\"`{os.environ['CITIZENCHAIN_RUST_INSTALLER_SHA']}` \"\n        f\"(`{os.environ['CITIZENCHAIN_RUST_VERSION']}`)\n\"\n    )\n    out.write(f\"- Cargo.lock SHA-256：`{hashlib.sha256(lock_data).hexdigest()}`\n\")\n    for label, command in (\n        (\"rustc\", (\"rustc\", \"--version\", \"--verbose\")),\n        (\"cargo\", (\"cargo\", \"--version\", \"--verbose\")),\n        (\"protoc\", (os.environ[\"PROTOC\"], \"--version\")),\n        (\"clang\", (\"clang\", \"--version\")),\n    ):\n        out.write(f\"\n### {label}\n\n```\n{command_output(*command)}\n```\n\")\nPY\n"},"6":{"shell":"bash","source":"if cargo --config \"$GITHUB_WORKSPACE/config.toml\" tree --locked -p citizenchain -e features --no-default-features --features std \\n  | grep -F 'citizenchain feature \"runtime-benchmarks\"'; then\n  echo \"正式 WASM 依赖图禁止启用 runtime-benchmarks\" >&2\n  exit 1\nfi\n"},"7":{"shell":"bash","source":"cargo --config \"$GITHUB_WORKSPACE/config.toml\" clippy --locked -p citizenchain --no-default-features --features std -- -D warnings\n"},"8":{"shell":"bash","source":"cargo --config \"$GITHUB_WORKSPACE/config.toml\" build --locked --release -p citizenchain --no-default-features --features std\n"},"9":{"shell":"bash","source":"python3 - <<'PY'\nimport hashlib\nimport os\nfrom pathlib import Path\noutput_dir = (Path(os.environ[\"CARGO_TARGET_DIR\"]) / \"release/wbuild/citizenchain\")\nfilenames = (\n    \"citizenchain.wasm\",\n    \"citizenchain.compact.wasm\",\n    \"citizenchain.compact.compressed.wasm\",\n)\nsummary = Path(os.environ[\"GITHUB_STEP_SUMMARY\"])\nrows = []\nfor filename in filenames:\n    path = output_dir / filename\n    if not path.is_file() or path.stat().st_size == 0:\n        raise SystemExit(f\"WASM 产物不存在或为空：{path}\")\n    data = path.read_bytes()\n    digest = hashlib.blake2b(data, digest_size=32).hexdigest()\n    rows.append((filename, len(data), digest))\nwith summary.open(\"a\", encoding=\"utf-8\") as out:\n    out.write(\"\n## CitizenChain WASM 产物\n\n\")\n    out.write(\"| 文件 | 字节数 | Blake2-256 |\n\")\n    out.write(\"| --- | ---: | --- |\n\")\n    for filename, size, digest in rows:\n        out.write(f\"| `{filename}` | {size} | `0x{digest}` |\n\")\nPY\n"},"10":{"shell":"bash","source":"npm --prefix node/frontend ci\nnpm --prefix node/frontend run build\n"},"11":{"shell":"bash","source":"cargo --config \"$GITHUB_WORKSPACE/config.toml\" test --locked -p node \\n  current_wasm_passes_candidate_runtime_policy_behavior_probes -- --nocapture\n"},"12":{"shell":"bash","source":"node \"$GITHUB_WORKSPACE/scripts/ci/wasm.mjs\" build-wasm sanitize\nnode \"$GITHUB_WORKSPACE/scripts/ci/wasm.mjs\" build-wasm record\n"},"13":{"shell":"bash","source":"node \"$GITHUB_WORKSPACE/scripts/ci/wasm.mjs\" build-wasm prune"},"14":{"shell":"bash","source":"node \"$GITHUB_WORKSPACE/scripts/ci/wasm.mjs\" build-wasm sanitize\nnode \"$GITHUB_WORKSPACE/scripts/ci/wasm.mjs\" build-wasm record\n"},"15":{"shell":"bash","source":"node \"$GITHUB_WORKSPACE/scripts/ci/wasm.mjs\" build-wasm prune"}}});
export const ACTION_SOURCES=Object.freeze({"linux-deps":"#!/usr/bin/env bash\n\nset -euo pipefail\n\n# 中文注释：GitHub Ubuntu Runner 默认优先使用 Azure 区域镜像，该镜像偶发卡住时\n# Acquire::Retries 无法保证整条 apt-get 命令及时退出。这里为当前命令提供独立官方源，\n# 不改写 Runner 的全局 sources 文件，也不访问与 CitizenChain 构建无关的第三方仓库。\n# shellcheck disable=SC1091\nsource /etc/os-release\n\nif [[ \"${ID:-}\" != \"ubuntu\" || -z \"${VERSION_CODENAME:-}\" ]]; then\n  echo \"::error::只支持带 VERSION_CODENAME 的 Ubuntu Runner\"\n  exit 1\nfi\n\ncitizenchain_arch=\"$(dpkg --print-architecture)\"\ncitizenchain_source=\"$(mktemp)\"\ntrap 'rm -f \"${citizenchain_source}\"' EXIT\n\ncase \"${citizenchain_arch}\" in\n  amd64)\n    citizenchain_archive=\"https://archive.ubuntu.com/ubuntu\"\n    citizenchain_security=\"https://security.ubuntu.com/ubuntu\"\n    ;;\n  arm64)\n    citizenchain_archive=\"https://ports.ubuntu.com/ubuntu-ports\"\n    citizenchain_security=\"${citizenchain_archive}\"\n    ;;\n  *)\n    echo \"::error::不支持的 Ubuntu 架构：${citizenchain_arch}\"\n    exit 1\n    ;;\nesac\n\n{\n  echo \"deb [arch=${citizenchain_arch} signed-by=/usr/share/keyrings/ubuntu-archive-keyring.gpg] ${citizenchain_archive} ${VERSION_CODENAME} main restricted universe multiverse\"\n  echo \"deb [arch=${citizenchain_arch} signed-by=/usr/share/keyrings/ubuntu-archive-keyring.gpg] ${citizenchain_archive} ${VERSION_CODENAME}-updates main restricted universe multiverse\"\n  echo \"deb [arch=${citizenchain_arch} signed-by=/usr/share/keyrings/ubuntu-archive-keyring.gpg] ${citizenchain_archive} ${VERSION_CODENAME}-backports main restricted universe multiverse\"\n  echo \"deb [arch=${citizenchain_arch} signed-by=/usr/share/keyrings/ubuntu-archive-keyring.gpg] ${citizenchain_security} ${VERSION_CODENAME}-security main restricted universe multiverse\"\n} > \"${citizenchain_source}\"\n\ncitizenchain_apt_options=(\n  -o \"Dir::Etc::sourcelist=${citizenchain_source}\"\n  -o \"Dir::Etc::sourceparts=-\"\n  -o Acquire::Retries=2\n  -o Acquire::http::Timeout=20\n  -o Acquire::https::Timeout=20\n  -o Acquire::Languages=none\n)\n\ncitizenchain_packages=(\n  clang\n  llvm\n  llvm-dev\n  libclang-dev\n  libpam0g-dev\n  libssl-dev\n  libwebkit2gtk-4.1-dev\n  libgtk-3-dev\n  libayatana-appindicator3-dev\n  librsvg2-dev\n  pkg-config\n  patchelf\n  file\n)\n\nrun_apt() {\n  local citizenchain_label=\"$1\"\n  local citizenchain_timeout=\"$2\"\n  shift 2\n\n  local citizenchain_attempt\n  local citizenchain_status=1\n  for citizenchain_attempt in 1 2 3; do\n    echo \"${citizenchain_label}：第 ${citizenchain_attempt}/3 次\"\n    if timeout --signal=TERM --kill-after=15s \"${citizenchain_timeout}\" \"$@\"; then\n      return 0\n    else\n      citizenchain_status=$?\n    fi\n\n    if [[ \"${citizenchain_attempt}\" -lt 3 ]]; then\n      # 中文注释：重试整条 APT 事务，避免单个 Acquire 重试耗尽后直接终止产品流水线。\n      sleep \"$((citizenchain_attempt * 10))\"\n    fi\n  done\n\n  echo \"::error::${citizenchain_label}连续三次失败，最后退出码为 ${citizenchain_status}\"\n  return \"${citizenchain_status}\"\n}\n\nrun_apt \\\n  \"更新 CitizenChain Linux 官方软件源\" \\\n  4m \\\n  sudo env DEBIAN_FRONTEND=noninteractive apt-get \\\n    \"${citizenchain_apt_options[@]}\" update\n\nrun_apt \\\n  \"安装 CitizenChain Linux 系统依赖\" \\\n  8m \\\n  sudo env DEBIAN_FRONTEND=noninteractive apt-get \\\n    \"${citizenchain_apt_options[@]}\" install -y \\\n    \"${citizenchain_packages[@]}\"\n\n# 中文注释：APT 成功退出后再回读包状态和关键命令，禁止缺少依赖时进入 Rust/前端构建。\nfor citizenchain_package in \"${citizenchain_packages[@]}\"; do\n  if [[ \"$(dpkg-query -W -f='${Status}' \"${citizenchain_package}\" 2>/dev/null || true)\" != \"install ok installed\" ]]; then\n    echo \"::error::CitizenChain Linux 依赖未安装：${citizenchain_package}\"\n    exit 1\n  fi\ndone\n\nfor citizenchain_command in clang llvm-config pkg-config patchelf file; do\n  if ! command -v \"${citizenchain_command}\" >/dev/null 2>&1; then\n    echo \"::error::CitizenChain Linux 构建命令不可用：${citizenchain_command}\"\n    exit 1\n  fi\ndone\n\necho \"CitizenChain Linux 系统依赖已通过官方 Ubuntu 源安装并回读验证。\"\n"});
export function forJob(job){if(!Object.hasOwn(JOB_IDENTITIES,job))throw Error('准确远端Job身份无效');const EXACT_REMOTE_JOB_IDENTITY=JOB_IDENTITIES[job];const workflowSteps=JOB_STEPS[job];
// 缓存身份使用固定语义前缀，不把内部实现误当成版本化协议。
const CI_CACHE_SCHEMA = 'ci';

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

function cacheIdentity(input) {
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

function cacheKeys(identity, runId, attempt) {
  const run = positiveInteger(runId, 'GitHub Run ID');
  const runAttempt = positiveInteger(attempt, 'GitHub Run Attempt');
  return Object.freeze({
    successPrefix: `${identity.baseKey}-success-`,
    failurePrefix: `${identity.baseKey}-failure-`,
    successKey: `${identity.baseKey}-success-${run}-${runAttempt}`,
    failureKey: `${identity.baseKey}-failure-${run}-${runAttempt}`,
  });
}

function parseCacheKey(identity, key) {
  const parsed = parseLogicalCacheKey(identity, key);
  return parsed?.toolchain === identity.toolchainFingerprint.slice(0, 16) ? parsed : null;
}

function parseLogicalCacheKey(identity, key) {
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

function selectLatestCache(identity, caches, state = 'success', ref = '') {
  if (!['success', 'failure'].includes(state)) throw new Error('缓存状态无效');
  const rows = recognizedCaches(identity, caches, ref, true)
    .filter((cache) => cache.state === state);
  rows.sort(compareCache);
  return rows.at(-1) ?? null;
}

function planCachePrune(identity, caches, ref = '') {
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

function cachePathPlan(identity, runnerTemp, entries) {
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
  const root = pathApi.resolve(temp, 'cache');
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

function tempRootForView(value){return value;}
function wireCacheLinks(identity, runnerTemp, entries, workspace, links) {
  const pathApi = pathImplementation(identity.runnerOs);
  const plan = cachePathPlan(identity, runnerTemp, entries);
  const workspaceRoot = required(workspace, 'GitHub工作区');
  if (!pathApi.isAbsolute(workspaceRoot)) throw new Error('GitHub工作区必须是绝对路径');
  const rows = String(links ?? '').split(/\n/).map((entry) => entry.trim()).filter(Boolean);
  for (const row of rows) {
    const separator = row.indexOf('=');
    if (separator <= 0) throw new Error(`缓存目录链接无效：${row}`);
    const sourceRelative = row.slice(0, separator);
    if (sourceRelative === 'target' || sourceRelative.endsWith('/target')) throw new Error('产品根target不能建立缓存链接');
    const cacheRelative = row.slice(separator + 1);
    relativeEntries(sourceRelative, '工作区生成目录');
    relativeEntries(cacheRelative, '受控缓存目录');
    const source = resolvedChild(pathApi, pathApi.join(tempRootForView(runnerTemp),'source'), sourceRelative, '工作区生成目录');
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

function sanitizeCacheFinals(identity, runnerTemp, entries, finals) {
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
  environment = productRemoteEnvironment(environment);
  requireExactRemoteJobEnvironment();
  const identity = identityFromEnvironment(environment);
  const keys = cacheKeys(identity, environment.GITHUB_RUN_ID, environment.GITHUB_RUN_ATTEMPT);
  const paths = cachePathPlan(identity, environment.RUNNER_TEMP, environment.CI_CACHE_PATHS);
  const ref = required(environment.GITHUB_REF, 'GitHub Ref');
  const tokenValue = environment.GH_TOKEN || environment.GITHUB_TOKEN;
  return { identity, keys, paths, ref, tokenValue };
}

async function prepare(environment) {
  environment = productRemoteEnvironment(environment);
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
  environment = productRemoteEnvironment(environment);
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



function requireExactRemoteJobEnvironment() {
  const expected = 'crcfrcn/citizenchain';
  if (!expected || process.env.GITHUB_REPOSITORY !== expected) {
    throw new Error('准确远端Job仓库身份无效');
  }
}
function runExactWorkflowStep(index){requireExactRemoteJobEnvironment();if(!/^(?:0|[1-9][0-9]*)$/.test(String(index||''))||!Object.hasOwn(workflowSteps,String(index)))throw Error('准确远端Job阶段无效');const declaredStep=workflowSteps[String(index)],bound=remoteStep(declaredStep.source,process.env),step={...declaredStep,source:bound.source},command=step.shell==='pwsh'?'pwsh':process.platform==='win32'?'bash':'/bin/bash',args=step.shell==='pwsh'?['-NoLogo','-NoProfile','-NonInteractive','-Command',step.source]:['--noprofile','--norc','-e','-o','pipefail','-c',step.source];const result=runExactProcess(command,args,{cwd:bound.cwd,env:bound.env,stdio:'inherit'});if(result.error)throw Error('准确远端Job阶段无法启动');if(result.status!==0)process.exitCode=Number.isInteger(result.status)?result.status:1;}
return {EXACT_REMOTE_JOB_IDENTITY,runExactWorkflowStep,requireExactRemoteJobEnvironment,CI_CACHE_SCHEMA,cacheIdentity,cacheKeys,parseCacheKey,parseLogicalCacheKey,selectLatestCache,planCachePrune,cachePathPlan,wireCacheLinks,sanitizeCacheFinals,prepare,wire,sanitize,writeTerminalRecord,prune};}
async function action(command,args){if(!Object.hasOwn(ACTION_SOURCES,command))throw Error('准确动作无效');const dir=joinAction(temporaryRoot(undefined,'tmp'),'action'),shell=['linux-deps','guardrails'].includes(command),file=joinAction(dir,shell?'step.sh':'step.mjs');mkdirSync(dir,{recursive:true});writeAction(file,ACTION_SOURCES[command],{mode:0o700});try{const result=runExactProcess(shell?'/bin/bash':process.execPath,[file,...args],{cwd:process.env.GITHUB_WORKSPACE||process.cwd(),env:process.env,stdio:'inherit'});if(result.error)throw result.error;process.exitCode=result.status??1;}finally{removeAction(dir,{recursive:true,force:true});}}
async function main(){return withFixedWork('build',()=>mainTask(),{retain:process.env.GITHUB_ACTIONS==='true'});}
async function mainTask(){if(process.env.GITHUB_REPOSITORY!=='crcfrcn/citizenchain')throw Error('准确远端Job仓库身份无效');Object.assign(process.env,productRemoteEnvironment());const [job,command,...args]=process.argv.slice(2);if(Object.hasOwn(ACTION_SOURCES,job))return action(job,[command,...args].filter(v=>v!==undefined));const owner=forJob(job);if(command==='workflow-step')return owner.runExactWorkflowStep(args[0]);const method={prepare:'prepare',wire:'wire',sanitize:'sanitize',record:'writeTerminalRecord',prune:'prune'}[command];if(!method||!owner[method])throw Error('准确远端Job动作无效');return owner[method](process.env);}
if(!inlineTestEntry&&directEntry){void main().catch(e=>{console.error(e.message);process.exitCode=1;});}
// 内嵌回归位于正式实现之后，仅直接测试本文件时注册。
if(inlineTestEntry){
void(async()=>{
const {default:assert} = await import('node:assert/strict');
const { readFileSync } = await import('node:fs');
const {default:test} = await import('node:test');

test('citizenchain.wasm.ci的build-wasm远端Job物理独立', () => {
  const source = readFileSync(import.meta.filename, 'utf8');
  assert.ok(source.includes('{"pipeline":"citizenchain.wasm.ci","job":"build-wasm"}'));
  assert.match(source, /function runExactWorkflowStep\(index\)/u);
  assert.match(source, /function requireExactRemoteJobEnvironment\(\)/u);
});

// Cargo直接消费当前流程的输出环境，根target永久保持普通目录。
test('远端缓存不得用链接覆盖产品根target', async t => {
  const { mkdtempSync, mkdirSync, rmSync, lstatSync } = await import('node:fs');
  const { join } = await import('node:path');
  const { testRoot } = await import('../build.mjs');
  const { cacheIdentity, wireCacheLinks } = forJob('build-wasm');
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

})();
}

#!/usr/bin/env node
import {citizenChainRelease} from './release-wasm.mjs';
import {remoteStep,withFixedWorkSync} from '../../scripts/build.mjs';
// 本仓本目标的完整自动化只由同名Workflow调用；版本与产物均在GitHub生成。
import { createHash } from 'node:crypto';
import { spawnSync, execFileSync } from 'node:child_process';
import { appendFileSync, copyFileSync, createReadStream, existsSync, lstatSync, mkdirSync, readFileSync, readdirSync, rmSync, writeFileSync } from 'node:fs';
import { basename, dirname, isAbsolute, join, resolve } from 'node:path';
import { fileURLToPath, pathToFileURL } from 'node:url';

export const owner = Object.freeze({"product": "citizenchain", "platform": "linux-amd", "repository": "crcfrcn/citizenchain", "version_source": {"kind": "json", "path": "node/tauri.conf.json"}, "required_assets": [], "asset_locations": ["$RUNNER_TEMP/citizenchain-release"], "asset_patterns": ["*.deb", "*.AppImage", "*.dmg", "*.exe", "*.msi", "*.tar.gz", "*.zip", "*.json", "*.sig"], "required_patterns": ["*.deb", "*.AppImage", "*.AppImage.sig", "citizenchain-node-latest-LinuxAMD.json"]});
const commands = Object.freeze({
  "1": {
    "shell": "bash",
    "source": "set -euo pipefail\ntest \"$(git rev-parse HEAD)\" = \"$SOURCE_SHA\"\nnode \"$GITHUB_WORKSPACE/.github/workflows/release-linux-amd.mjs\" action node-version apply \"$SOFTWARE_VERSION\""
  },
  "2": {
    "shell": "bash",
    "source": "set -euo pipefail\nif [ \"${RUNNER_ARCH}\" != \"${EXPECTED_RUNNER_ARCH}\" ]; then\n  echo \"::error::runner 架构不符合 ${EXPECTED_RUNNER_ARCH}: 当前为 ${RUNNER_ARCH}\"\n  exit 1\nfi"
  },
  "3": {
    "shell": "bash",
    "source": "set -euo pipefail\nnode <<'NODE'\nconst fs = require('fs');\nconst targetSource = process.env.CITIZENCHAIN_MANUAL_BUNDLE_TARGETS;\nconst bundleTargets = (targetSource || '')\n  .split(',')\n  .map((target) => target.trim())\n  .filter(Boolean);\nif (bundleTargets.length === 0) {\n  throw new Error('缺少本 matrix 的 Tauri bundle targets。');\n}\n// 本次版本已在当前 runner 工作区应用；这里只设置本 matrix 的 bundle 目标。\nconst tauriPath = 'node/tauri.conf.json';\nconst tauri = JSON.parse(fs.readFileSync(tauriPath, 'utf8'));\nconst version = tauri.version;\ntauri.bundle = tauri.bundle || {};\n// 中文注释：Release 生成安装包与 updater 正式文件；正式签名在同一 runner 的后续步骤完成。\ntauri.bundle.targets = bundleTargets;\n// 中文注释：显式关闭 Tauri 自动 updater 归档；本 workflow 按现有平台文件格式签名并生成清单。\ndelete tauri.bundle.createUpdaterArtifacts;\n// 更新检查由节点按当前平台解析本仓成功Run和Release。\ntauri.plugins = tauri.plugins || {};\ntauri.plugins.updater = tauri.plugins.updater || {};\ntauri.plugins.updater.endpoints = [];\nfs.writeFileSync(tauriPath, `${JSON.stringify(tauri, null, 2)}\n`);\nconsole.log(`桌面端版本(源码): ${version}`);\nconsole.log(`Tauri bundle targets: ${bundleTargets.join(',')}`);\nNODE"
  },
  "4": {
    "shell": "bash",
    "source": "node \"$GITHUB_WORKSPACE/.github/workflows/release-linux-amd.mjs\" action linux-deps linux-deps"
  },
  "5": {
    "shell": "bash",
    "source": "case \"$RUNNER_OS/$RUNNER_ARCH\" in\n  Linux/ARM64) platform=linux-arm ;;\n  Linux/X64) platform=linux-amd ;;\n  *) echo 'CitizenChain Linux protoc宿主不受支持' >&2; exit 1 ;;\nesac\nprotoc_executable=\"$(node .github/workflows/release-wasm.mjs protoc \"$platform\")\"\n{\n  echo \"LLVM_CONFIG_PATH=$(command -v llvm-config)\"\n  echo \"LIBCLANG_PATH=$(llvm-config --libdir)\"\n  echo \"PROTOC=$protoc_executable\"\n} >> \"$GITHUB_ENV\""
  },
  "6": {
    "shell": "bash",
    "source": "node \"$GITHUB_WORKSPACE/.github/workflows/release-linux-amd.mjs\" action node-version lock \"$SOFTWARE_VERSION\""
  },
  "7": {
    "shell": "bash",
    "source": "npm --prefix node/frontend ci\nnpm --prefix node/frontend run build"
  },
  "8": {
    "shell": "bash",
    "source": "# 中文注释：安装包只携带节点软件；首次启动按冻结 chainspec 本地初始化并联网同步。\nrm -rf node/resources/genesis-state"
  },
  "9": {
    "shell": "bash",
    "source": "node frontend/node_modules/@tauri-apps/cli/tauri.js build --ci -- --locked --config \"$GITHUB_WORKSPACE/config.toml\""
  },
  "10": {
    "shell": "bash",
    "source": "set -euo pipefail\nmkdir -p \"$RUNNER_TEMP/citizenchain-release\"\nversioned_installer=\"${INSTALLER_NAME%.deb}-v${SOFTWARE_VERSION}.deb\"\nversioned_updater=\"${UPDATER_ASSET_NAME%.AppImage}-v${SOFTWARE_VERSION}.AppImage\"\ndeb_file=\"$(find ${CARGO_TARGET_DIR}/release/bundle/deb -name '*.deb' | head -1)\"\nif [ -z \"$deb_file\" ]; then\n  echo \"::error::未找到 Linux deb 安装包。\"\n  exit 1\nfi\nactual_deb_arch=\"$(dpkg-deb -f \"$deb_file\" Architecture)\"\nif [ \"$actual_deb_arch\" != \"$EXPECTED_DEB_ARCH\" ]; then\n  echo \"::error::deb 架构不符合 ${EXPECTED_DEB_ARCH}: 当前为 ${actual_deb_arch}\"\n  exit 1\nfi\ncp \"$deb_file\" \"$RUNNER_TEMP/citizenchain-release/${versioned_installer}\"\nappimage_file=\"\"\nif [ -d ${CARGO_TARGET_DIR}/release/bundle/appimage ]; then\n  appimage_file=\"$(find ${CARGO_TARGET_DIR}/release/bundle/appimage -name '*.AppImage' | head -1)\"\nfi\nif [ -z \"$appimage_file\" ]; then\n  echo \"::error::Release 缺少 Linux updater AppImage。\"\n  exit 1\nfi\ncp \"$appimage_file\" \"$RUNNER_TEMP/citizenchain-release/${versioned_updater}\"\n(cd node && node frontend/node_modules/@tauri-apps/cli/tauri.js signer sign \"$RUNNER_TEMP/citizenchain-release/${versioned_updater}\")\nsignature=\"$(tr -d '\n' < \"$RUNNER_TEMP/citizenchain-release/${versioned_updater}.sig\")\"\ntest -n \"$signature\"\nnode - \"$versioned_updater\" \"$signature\" <<'NODE'\nconst fs = require('node:fs');\nconst [asset, signature] = process.argv.slice(2);\nfs.writeFileSync(process.env.RUNNER_TEMP + '/citizenchain-release/citizenchain-node-latest-LinuxAMD.json', `${JSON.stringify({\n  version: process.env.SOFTWARE_VERSION,\n  notes: `CitizenChain LinuxAMD v${process.env.SOFTWARE_VERSION}。`,\n  pub_date: new Date().toISOString(),\n  platforms: { 'linux-x86_64': {\n    signature,\n    url: `https://github.com/${process.env.GITHUB_REPOSITORY}/releases/download/${process.env.VERSION_TAG}/${asset}`,\n  } },\n}, null, 2)}\n`);\nNODE\nls -lh \"$RUNNER_TEMP/citizenchain-release\""
  }
});
const actions = Object.freeze({
  "node-version": {
    "source": "#!/usr/bin/env node\n\nimport { readFileSync, writeFileSync } from 'node:fs';\nimport { spawnSync } from 'node:child_process';\nimport { pathToFileURL } from 'node:url';\n\nconst VERSION_PATTERN = /^\\d+\\.(?:0|[1-9]\\d?)\\.(?:0|[1-9]\\d?)$/;\nconst CARGO_PATH = 'Cargo.toml';\nconst LOCK_PATH = 'Cargo.lock';\nconst TAURI_PATH = 'node/tauri.conf.json';\n\nfunction required(condition, message) {\n  if (!condition) throw new Error(message);\n}\n\nexport function applyVersion(version) {\n  required(VERSION_PATTERN.test(version), '公民链节点本次版本无效');\n  const tauri = JSON.parse(readFileSync(TAURI_PATH, 'utf8'));\n  tauri.version = version;\n  writeFileSync(TAURI_PATH, `${JSON.stringify(tauri, null, 2)}\\n`);\n\n  let cargo = readFileSync(CARGO_PATH, 'utf8');\n  const pattern = /(\\[workspace\\.package\\][\\s\\S]*?\\nversion\\s*=\\s*\")[^\"]+(\"\\s*)/;\n  required(pattern.test(cargo), 'CitizenChain workspace.package 版本真源无效');\n  cargo = cargo.replace(pattern, `$1${version}$2`);\n  writeFileSync(CARGO_PATH, cargo);\n}\n\nfunction packageBlocks(text) {\n  // 中文注释：Windows runner 会把检出文件转换为 CRLF，而 Cargo 重写锁文件时可能恢复 LF；\n  // 先统一换行再做逐块比对，避免把完全相同的 Cargo.lock 误判为缺少 package。\n  const normalized = text.replaceAll('\\r\\n', '\\n');\n  const marker = '[[package]]\\n';\n  const first = normalized.indexOf(marker);\n  required(first >= 0, 'Cargo.lock 缺少 package');\n  const prefix = normalized.slice(0, first);\n  const blocks = normalized.slice(first).split(/(?=^\\[\\[package\\]\\]\\n)/m);\n  return { prefix, blocks };\n}\n\nfunction packageField(block, field) {\n  return new RegExp(`^${field} = \"([^\"]+)\"$`, 'm').exec(block)?.[1] ?? null;\n}\n\nexport function validateLockChange(before, after, version) {\n  const left = packageBlocks(before);\n  const right = packageBlocks(after);\n  required(left.prefix === right.prefix && left.blocks.length === right.blocks.length,\n    'Cargo.lock 发生了非 workspace 版本变化');\n  let changes = 0;\n  for (let index = 0; index < left.blocks.length; index += 1) {\n    const oldBlock = left.blocks[index];\n    const newBlock = right.blocks[index];\n    if (oldBlock === newBlock) continue;\n    const oldName = packageField(oldBlock, 'name');\n    const newName = packageField(newBlock, 'name');\n    const oldVersion = packageField(oldBlock, 'version');\n    const newVersion = packageField(newBlock, 'version');\n    required(oldName && oldName === newName && oldVersion && newVersion === version,\n      'Cargo.lock workspace 包身份或本次版本无效');\n    required(packageField(oldBlock, 'source') === null && packageField(newBlock, 'source') === null,\n      `Cargo.lock 禁止修改远端依赖：${oldName}`);\n    const normalize = (block) => block.replace(/^version = \"[^\"]+\"$/m, 'version = \"<workspace>\"');\n    required(normalize(oldBlock) === normalize(newBlock),\n      `Cargo.lock 除 workspace 版本外发生变化：${oldName}`);\n    required(oldVersion !== newVersion, `Cargo.lock 出现无效版本变化：${oldName}`);\n    changes += 1;\n  }\n  // 中文注释：CI 只验证源码版本，不推进正式版本；源码 manifest 与 Cargo.lock 已经一致时，\n  // `cargo update --workspace` 合法地不产生差异，随后 metadata --locked 仍会验证锁定一致性。\n  return changes;\n}\n\nfunction runCargo(args) {\n  const result = spawnSync('cargo', args, {\n    cwd: process.env.GITHUB_WORKSPACE || process.cwd(), encoding: 'utf8', stdio: ['ignore', 'pipe', 'pipe'],\n  });\n  if (result.error) throw result.error;\n  if (result.status !== 0) throw new Error(String(result.stderr || result.stdout).trim());\n}\n\nexport function lockVersion(version) {\n  required(VERSION_PATTERN.test(version), '公民链节点本次版本无效');\n  const cargo = readFileSync(CARGO_PATH, 'utf8');\n  const tauri = JSON.parse(readFileSync(TAURI_PATH, 'utf8'));\n  const escapedVersion = version.replaceAll('.', '\\\\.');\n  required(new RegExp(`\\\\[workspace\\\\.package\\\\][\\\\s\\\\S]*?\\\\nversion\\\\s*=\\\\s*\"${escapedVersion}\"`).test(cargo)\n    && tauri.version === version, '锁文件同步前的节点本次版本不一致');\n  const before = readFileSync(LOCK_PATH, 'utf8');\n  // 中文注释：全新 runner 可能尚未缓存 Cargo.lock 已钉死的 Git 源，因此这里允许 Cargo\n  // 取得锁文件所需源码；紧随其后的逐块校验仍只允许本地 workspace 包版本发生变化。\n  runCargo(['update', '--workspace']);\n  const after = readFileSync(LOCK_PATH, 'utf8');\n  validateLockChange(before, after, version);\n  runCargo(['metadata', '--locked', '--offline', '--no-deps', '--format-version', '1']);\n}\n\nif (process.argv[1] && import.meta.url === pathToFileURL(process.argv[1]).href) {\n  try {\n    const [command, version] = process.argv.slice(2);\n    if (command === 'apply') applyVersion(version);\n    else if (command === 'lock') lockVersion(version);\n    else throw new Error('公民链节点版本命令无效');\n  } catch (error) {\n    console.error(error.message);\n    process.exitCode = 1;\n  }\n}\n",
    "shell": "node"
  },
  "linux-deps": {
    "source": "#!/usr/bin/env bash\n\nset -euo pipefail\n\n# 中文注释：GitHub Ubuntu Runner 默认优先使用 Azure 区域镜像，该镜像偶发卡住时\n# Acquire::Retries 无法保证整条 apt-get 命令及时退出。这里为当前命令提供独立官方源，\n# 不改写 Runner 的全局 sources 文件，也不访问与 CitizenChain 构建无关的第三方仓库。\n# shellcheck disable=SC1091\nsource /etc/os-release\n\nif [[ \"${ID:-}\" != \"ubuntu\" || -z \"${VERSION_CODENAME:-}\" ]]; then\n  echo \"::error::只支持带 VERSION_CODENAME 的 Ubuntu Runner\"\n  exit 1\nfi\n\ncitizenchain_arch=\"$(dpkg --print-architecture)\"\ncitizenchain_source=\"$(mktemp)\"\ntrap 'rm -f \"${citizenchain_source}\"' EXIT\n\ncase \"${citizenchain_arch}\" in\n  amd64)\n    citizenchain_archive=\"https://archive.ubuntu.com/ubuntu\"\n    citizenchain_security=\"https://security.ubuntu.com/ubuntu\"\n    ;;\n  arm64)\n    citizenchain_archive=\"https://ports.ubuntu.com/ubuntu-ports\"\n    citizenchain_security=\"${citizenchain_archive}\"\n    ;;\n  *)\n    echo \"::error::不支持的 Ubuntu 架构：${citizenchain_arch}\"\n    exit 1\n    ;;\nesac\n\n{\n  echo \"deb [arch=${citizenchain_arch} signed-by=/usr/share/keyrings/ubuntu-archive-keyring.gpg] ${citizenchain_archive} ${VERSION_CODENAME} main restricted universe multiverse\"\n  echo \"deb [arch=${citizenchain_arch} signed-by=/usr/share/keyrings/ubuntu-archive-keyring.gpg] ${citizenchain_archive} ${VERSION_CODENAME}-updates main restricted universe multiverse\"\n  echo \"deb [arch=${citizenchain_arch} signed-by=/usr/share/keyrings/ubuntu-archive-keyring.gpg] ${citizenchain_archive} ${VERSION_CODENAME}-backports main restricted universe multiverse\"\n  echo \"deb [arch=${citizenchain_arch} signed-by=/usr/share/keyrings/ubuntu-archive-keyring.gpg] ${citizenchain_security} ${VERSION_CODENAME}-security main restricted universe multiverse\"\n} > \"${citizenchain_source}\"\n\ncitizenchain_apt_options=(\n  -o \"Dir::Etc::sourcelist=${citizenchain_source}\"\n  -o \"Dir::Etc::sourceparts=-\"\n  -o Acquire::Retries=2\n  -o Acquire::http::Timeout=20\n  -o Acquire::https::Timeout=20\n  -o Acquire::Languages=none\n)\n\ncitizenchain_packages=(\n  clang\n  llvm\n  llvm-dev\n  libclang-dev\n  libpam0g-dev\n  libssl-dev\n  libwebkit2gtk-4.1-dev\n  libgtk-3-dev\n  libayatana-appindicator3-dev\n  librsvg2-dev\n  pkg-config\n  patchelf\n  file\n)\n\nrun_apt() {\n  local citizenchain_label=\"$1\"\n  local citizenchain_timeout=\"$2\"\n  shift 2\n\n  local citizenchain_attempt\n  local citizenchain_status=1\n  for citizenchain_attempt in 1 2 3; do\n    echo \"${citizenchain_label}：第 ${citizenchain_attempt}/3 次\"\n    if timeout --signal=TERM --kill-after=15s \"${citizenchain_timeout}\" \"$@\"; then\n      return 0\n    else\n      citizenchain_status=$?\n    fi\n\n    if [[ \"${citizenchain_attempt}\" -lt 3 ]]; then\n      # 中文注释：重试整条 APT 事务，避免单个 Acquire 重试耗尽后直接终止产品流水线。\n      sleep \"$((citizenchain_attempt * 10))\"\n    fi\n  done\n\n  echo \"::error::${citizenchain_label}连续三次失败，最后退出码为 ${citizenchain_status}\"\n  return \"${citizenchain_status}\"\n}\n\nrun_apt \\\n  \"更新 CitizenChain Linux 官方软件源\" \\\n  4m \\\n  sudo env DEBIAN_FRONTEND=noninteractive apt-get \\\n    \"${citizenchain_apt_options[@]}\" update\n\nrun_apt \\\n  \"安装 CitizenChain Linux 系统依赖\" \\\n  8m \\\n  sudo env DEBIAN_FRONTEND=noninteractive apt-get \\\n    \"${citizenchain_apt_options[@]}\" install -y \\\n    \"${citizenchain_packages[@]}\"\n\n# 中文注释：APT 成功退出后再回读包状态和关键命令，禁止缺少依赖时进入 Rust/前端构建。\nfor citizenchain_package in \"${citizenchain_packages[@]}\"; do\n  if [[ \"$(dpkg-query -W -f='${Status}' \"${citizenchain_package}\" 2>/dev/null || true)\" != \"install ok installed\" ]]; then\n    echo \"::error::CitizenChain Linux 依赖未安装：${citizenchain_package}\"\n    exit 1\n  fi\ndone\n\nfor citizenchain_command in clang llvm-config pkg-config patchelf file; do\n  if ! command -v \"${citizenchain_command}\" >/dev/null 2>&1; then\n    echo \"::error::CitizenChain Linux 构建命令不可用：${citizenchain_command}\"\n    exit 1\n  fi\ndone\n\necho \"CitizenChain Linux 系统依赖已通过官方 Ubuntu 源安装并回读验证。\"\n",
    "shell": "bash"
  }
});
const shaPattern = /^[0-9a-f]{40}$/u;
const fail = message => { throw new Error(message); };
const root = fileURLToPath(new URL('../../', import.meta.url));
const firstProjectStep = 7;
const workflowPath = `.github/workflows/release-${owner.platform}.yml`;
const prefix = `${owner.product}-${owner.platform}-v`;

export function context(environment = process.env) {
  const number = name => {
    const value = environment[name];
    if (!/^[1-9][0-9]*$/u.test(value || '') || !Number.isSafeInteger(Number(value))) fail('GitHub运行坐标无效');
    return Number(value);
  };
  if (environment.GITHUB_ACTIONS !== 'true' || environment.GITHUB_REPOSITORY !== owner.repository
    || environment.GITHUB_REF !== 'refs/heads/main' || environment.GITHUB_EVENT_NAME !== 'workflow_dispatch'
    || !shaPattern.test(environment.GITHUB_SHA || '')
    || environment.GITHUB_WORKFLOW_REF !== `${owner.repository}/${workflowPath}@refs/heads/main`) fail('所属GitHub运行身份无效');
  return { repository: owner.repository, product_id: owner.product, platform: owner.platform,
    source_sha: environment.GITHUB_SHA, run_id: number('GITHUB_RUN_ID'),
    run_number: number('GITHUB_RUN_NUMBER'), run_attempt: number('GITHUB_RUN_ATTEMPT'), workflow: workflowPath };
}

export async function request(path, { method = 'GET', body, raw = false, size, fetch: send = globalThis.fetch } = {}) {
  const token = process.env.GH_TOKEN || process.env.GITHUB_TOKEN;
  if (!token || /[\s\u0000-\u001f\u007f]/u.test(token)) fail('缺少GitHub任务令牌');
  const url = path.startsWith('https://') ? new URL(path) : new URL(`https://api.github.com/repos/${owner.repository}/${path}`);
  if (!['api.github.com', 'uploads.github.com'].includes(url.hostname) || url.protocol !== 'https:' || url.username || url.password || !url.pathname.startsWith(`/repos/${owner.repository}/`)) fail('GitHub接口地址无效');
  const headers = { Authorization: `Bearer ${token}`, Accept: raw ? 'application/octet-stream' : 'application/vnd.github+json',
    'X-GitHub-Api-Version': '2026-03-10', 'User-Agent': owner.product };
  if (body !== undefined) headers['Content-Type'] = body?.pipe ? 'application/octet-stream' : 'application/json';
  if(body?.pipe){if(!Number.isSafeInteger(size)||size<=0)fail('资产上传长度无效');headers['Content-Length']=String(size);}
  let response = await send(url, { method, headers, redirect: raw ? 'manual' : 'error', signal: AbortSignal.timeout(300_000),
    ...(body === undefined ? {} : { body: body?.pipe ? body : JSON.stringify(body), ...(body?.pipe ? { duplex: 'half' } : {}) }) });
  if(raw&&response.status===302){
    const location=new URL(response.headers.get('location'));
    if(location.protocol!=='https:'||location.username||location.password)fail('正式资产回读地址无效');
    response=await send(location,{method:'GET',redirect:'error',credentials:'omit',signal:AbortSignal.timeout(300_000)});
  }
  if (response.status === 404) return null;
  if (!response.ok) fail(`GitHub接口失败：${response.status}，操作未确认`);
  if (raw) return response;
  return response.status === 204 ? {} : response.json();
}

export async function pages(path, field = null, api = request) {
  const rows = [];
  for (let page = 1; ; page++) {
    const data = await api(`${path}${path.includes('?') ? '&' : '?'}per_page=100&page=${page}`);
    const values = field ? data?.[field] : data;
    if (!Array.isArray(values)) fail('GitHub分页数据无效');
    rows.push(...values);
    if (values.length < 100) return rows;
  }
}

function seedVersion() {
  const source = owner.version_source;
  if (source.kind === 'sequence') return '0.0.0';
  const text = readFileSync(join(root, source.path), 'utf8');
  if (source.kind === 'json') return String(JSON.parse(text).version);
  if (source.kind === 'spec') {
    const matches = [...text.matchAll(/^\s*spec_version:\s*(\d+)\s*,\s*$/gm)];
    if (matches.length !== 1) fail('Runtime版本真源不唯一');
    return matches[0][1];
  }
  if (source.kind === 'cargo') {
    const value = /^\[package\][\s\S]*?^version\s*=\s*"(\d+\.\d+\.\d+)"/mu.exec(text)?.[1];
    if (!value) fail('本仓Cargo版本真源无效');
    return value;
  }
  const value = /^version:\s*(\d+\.\d+\.\d+)(?:\+\d+)?\s*$/mu.exec(text)?.[1];
  if (!value) fail('本仓软件版本真源无效');
  return value;
}

export function nextVersion(seed, versions, protocol = false, runNumber = 1) {
  if (protocol) {
    if (!/^\d+$/u.test(seed) || versions.some(value => !/^\d+$/u.test(value))) fail('协议版本无效');
    const value = Math.max(Number(seed), ...versions.map(Number)) + (versions.length ? 1 : 0);
    if (!Number.isSafeInteger(value) || value < 1 || value > 0xffffffff) fail('协议版本越界');
    return String(value);
  }
  const parse = value => {
    const match = /^(0|[1-9]\d*)\.(0|[1-9]\d?)\.(0|[1-9]\d?)$/u.exec(value);
    if (!match) fail('软件版本无效');
    const parts=match.slice(1).map(Number);if(parts.some(value=>!Number.isSafeInteger(value)))fail('软件版本越界');return parts;
  };
  const values = [seed, ...versions].map(parse).sort((a,b) => a[0]-b[0] || a[1]-b[1] || a[2]-b[2]);
  let [major, minor, patch] = values.at(-1);
  if (versions.length) { if (++patch > 99) { patch = 0; if (++minor > 99) { minor = 0; major++; } } }
  if(!Number.isSafeInteger(runNumber)||runNumber<1)fail('版本运行序号无效');
  const initial=parse(seed),floor=BigInt(initial[0])*10000n+BigInt(initial[1])*100n+BigInt(initial[2])+BigInt(runNumber-1);
  const historical=BigInt(major)*10000n+BigInt(minor)*100n+BigInt(patch);
  if(floor>historical){major=Number(floor/10000n);minor=Number(floor/100n%100n);patch=Number(floor%100n);}
  if(![major,minor,patch].every(Number.isSafeInteger))fail('软件版本越界');
  return `${major}.${minor}.${patch}`;
}

function output(name, value, file = process.env.GITHUB_OUTPUT) {
  if (!file || /[\r\n]/u.test(String(value))) fail('GitHub步骤输出无效');
  appendFileSync(file, `${name}=${value}\n`);
}

export async function prepare() {
  const identity = context();
  const releases = await pages('releases');
  const versions = [];
  for (const release of releases) {
    if (release.draft || release.prerelease || !String(release.tag_name).startsWith(prefix)) continue;
    const notes = (await citizenChainRelease(release,owner.platform,tag=>request('git/ref/tags/'+encodeURIComponent(tag))));
    if (!notes || notes.platform !== owner.platform) continue;
    const run = await request(`actions/runs/${notes.run_id}`);
    if (run?.status === 'completed' && run.conclusion === 'success' && run.path === workflowPath) versions.push(notes.version);
  }
  const version = nextVersion(seedVersion(), versions, owner.version_source.kind === 'spec', identity.run_number);
  const tag = `${prefix}${version}-r${identity.run_id}-a${identity.run_attempt}`;
  for (const [name,value] of Object.entries({version, tag, source_sha:identity.source_sha,
    run_id:identity.run_id, run_attempt:identity.run_attempt, run_number:identity.run_number})) output(name,value);
}

function runVersion() {
  const identity = context();
  const version = process.env.RELEASE_VERSION;
  const tag = process.env.RELEASE_TAG;
  nextVersion(version, [], owner.version_source.kind === 'spec');
  if (tag !== `${prefix}${version}-r${identity.run_id}-a${identity.run_attempt}`) fail('本次版本与Tag不一致');
  return {...identity, version, tag};
}

export function job() {
  const identity = runVersion();
  if (execFileSync('git', ['rev-parse','HEAD'], {cwd:root,encoding:'utf8'}).trim() !== identity.source_sha) fail('检出源码不符');
  const work = join(process.env.RUNNER_TEMP, owner.product, owner.platform, String(identity.run_id), String(identity.run_attempt), process.env.GITHUB_JOB);
  mkdirSync(work,{recursive:true});
  const variables = {RELEASE_WORK:work, RELEASE_ASSETS_DIR:join(work,'assets'), SOURCE_SHA:identity.source_sha,
    SOFTWARE_VERSION:identity.version, VERSION_TAG:identity.tag, BUILD_NUMBER:String(identity.run_number),
    CARGO_HOME:join(work,'cargo-home'), CARGO_TARGET_DIR:join(work,'cargo'), PUB_CACHE:join(work,'pub'),
    GRADLE_USER_HOME:join(work,'gradle'), npm_config_cache:join(work,'npm'), XDG_CACHE_HOME:join(work,'cache'),
    TMPDIR:join(work,'tmp'), TMP:join(work,'tmp'), TEMP:join(work,'tmp')};
  for (const path of ['cargo-home','cargo','pub','gradle','npm','cache','tmp','assets']) mkdirSync(join(work,path),{recursive:true});
  for (const [name,value] of Object.entries(variables)) { process.env[name]=value; output(name,value,process.env.GITHUB_ENV); }
}

export function step(key) {
  runVersion();
  const value = commands[key];
  if (!value || !['bash','pwsh'].includes(value.shell)) fail('本目标构建步骤无效');
  const run = bound => {
    const directory = join(process.env.RELEASE_WORK,'commands');mkdirSync(directory,{recursive:true});
    const file = join(directory, value.shell === 'pwsh' ? 'step.ps1' : 'step.sh');
    writeFileSync(file, value.shell === 'bash' ? 'set -euo pipefail\n'+bound.source : "$ErrorActionPreference = 'Stop'\n"+bound.source,{mode:0o700});
    const result = spawnSync(value.shell === 'pwsh' ? 'pwsh' : 'bash', value.shell === 'pwsh' ? ['-NoProfile','-File',file] : [file],
      {cwd:bound.cwd,env:bound.env,stdio:'inherit'});
    rmSync(file,{force:true});
    if (result.error || result.status !== 0) fail(`本仓构建步骤失败：${key}`);
  };
  if(Number(key)<firstProjectStep)return run({source:value.source,cwd:process.cwd(),env:process.env});
  return withFixedWorkSync('build/'+owner.platform,()=>{
    const bound=remoteStep(value.source,process.env,value.shell);
    // Workflow 的 Tauri 阶段从 node 目录启动；工程视图保持相同相对目录。
    if(process.cwd()===join(root,'node'))bound.cwd=join(bound.cwd,'node');
    run(bound);
  },{retain:true});
}

export function action(name, args) {
  runVersion();
  const value = actions[name];if (!value) fail('本目标动作无效');
  const directory = join(process.env.RELEASE_WORK,'commands');mkdirSync(directory,{recursive:true});
  const file = join(directory,value.shell === 'bash'?'action.sh':'action.mjs');
  const source=value.source.replace(/__PRODUCT_URL__([^'"\s]+)__END_URL__/gu,(_,path)=>pathToFileURL(join(root,path)).href);
  writeFileSync(file,source,{mode:0o700});
  const result=spawnSync(value.shell==='bash'?'bash':process.execPath,[file,...args],{cwd:root,env:process.env,stdio:'inherit'});
  rmSync(file,{force:true});if(result.error||result.status!==0)fail(`本仓动作失败：${name}`);
}

function regular(path) {
  const stat=lstatSync(path);if(!stat.isFile()||stat.isSymbolicLink()||stat.size<=0)fail('正式产物不是非空普通文件');return stat;
}
async function digestFile(path) { const hash=createHash('sha256');for await(const bytes of createReadStream(path))hash.update(bytes);return hash.digest('hex'); }
function assetName(name) { if(!name||name!==basename(name)||/[\u0000-\u001f\u007f]/u.test(name))fail('正式资产文件名无效');return name; }

export async function collect(paths) {
  const identity=runVersion(),destination=process.env.RELEASE_ASSETS_DIR;
  if(!destination||!paths.length)fail('本仓没有完整产物');mkdirSync(destination,{recursive:true});
  const files=[];
  for(const path of paths){const file=resolve(path),stat=regular(file),name=assetName(basename(file));
    if(files.some(row=>row.name===name))fail('正式资产重名');
    const target=join(destination,name);if(file!==target)copyFileSync(file,target);
    files.push({name,size:stat.size,sha256:await digestFile(target)});
  }
  if(owner.required_assets.some(name=>!files.some(row=>row.name===name)))fail('本目标必要产物缺失');
  const metadata={schema:1,...identity,assets:files};
  writeFileSync(join(destination,'automation.json'),JSON.stringify(metadata,null,2)+'\n');
  output('assets',destination);return metadata;
}

export async function collectProduced() {
  const paths=[],seen=new Set();
  const expand=value=>value.replace(/\$\{([A-Z_]+)\}|\$([A-Z_]+)/gu,(_,a,b)=>process.env[a||b]||'');
  for(const location of owner.asset_locations){
    const path=expand(location);if(!path||!existsSync(path))continue;
    const candidates=lstatSync(path).isDirectory()?readdirSync(path).map(name=>join(path,name)):[path];
    for(const candidate of candidates){if(!lstatSync(candidate).isFile()||seen.has(resolve(candidate)))continue;
      const name=basename(candidate);if(!owner.asset_patterns.some(pattern=>new RegExp('^'+pattern.replace(/[.+?^${}()|[\]\\]/gu,'\\$&').replaceAll('*','.*')+'$','u').test(name)))continue;
      seen.add(resolve(candidate));paths.push(candidate);
    }
  }
  if(owner.required_patterns.some(pattern=>!paths.some(path=>new RegExp('^'+pattern.replace(/[.+?^${}()|[\]\\]/gu,'\\$&').replaceAll('*','.*')+'$','u').test(basename(path)))))fail('本目标完整正式资产缺失');
  return collect(paths);
}



export async function publish(directory) {
  const identity=runVersion();const metadata=JSON.parse(readFileSync(join(directory,'automation.json'),'utf8'));
  if(Object.entries(identity).some(([key,value])=>metadata[key]!==value)||!Array.isArray(metadata.assets)||!metadata.assets.length)fail('完整产物身份无效');
  const files=metadata.assets;
  if(readdirSync(directory).sort().join('\0')!==[...files.map(value=>value.name),'automation.json'].sort().join('\0'))fail('产物目录与完整资产集合不符');
  for(const file of files){const path=join(directory,assetName(file.name));if(regular(path).size!==file.size||await digestFile(path)!==file.sha256)fail('正式产物在交付前改变');}
  if(await request(`git/ref/tags/${encodeURIComponent(identity.tag)}`)!==null)fail('本次Tag已经存在');
  await request('git/refs',{method:'POST',body:{ref:`refs/tags/${identity.tag}`,sha:identity.source_sha}});
  const release=await request('releases',{method:'POST',body:{tag_name:identity.tag,target_commitish:identity.source_sha,
    name:`${owner.product} · ${owner.platform} · ${identity.version}`,draft:false,prerelease:false,make_latest:'false',
    body:`${owner.product} · ${owner.platform} · ${identity.version}\nSource: ${identity.source_sha}\nRun: ${identity.run_id} / ${identity.run_attempt}`}});
  if(!Number.isSafeInteger(release?.id)||!release.upload_url)fail('正式Release创建未确认');
  for(const file of files){const url=new URL(release.upload_url.replace(/\{.*$/u,''));url.searchParams.set('name',file.name);
    const asset=await request(url.href,{method:'POST',body:createReadStream(join(directory,file.name)),size:file.size});
    if(asset?.name!==file.name||asset.size!==file.size||asset.state!=='uploaded')fail('正式资产上传未确认');
    const response=await request(asset.url,{raw:true});if(!response?.body)fail('正式资产回读失败');
    const hash=createHash('sha256');let size=0;for await(const bytes of response.body){hash.update(bytes);size+=bytes.length;if(size>file.size)fail('正式资产回读超过声明大小');}
    if(size!==file.size||hash.digest('hex')!==file.sha256)fail('GitHub资产逐件回读不一致');
  }
  const readback=await request(`releases/${release.id}`);if((await citizenChainRelease(readback,owner.platform,tag=>request('git/ref/tags/'+encodeURIComponent(tag))))?.run_id!==identity.run_id||readback.draft||readback.prerelease
    ||readback.assets?.length!==files.length)fail('完整正式Release回查失败');
  for(const file of files){const asset=readback.assets.find(value=>value.name===file.name);if(!asset||asset.state!=='uploaded'||asset.size!==file.size||asset.digest!==`sha256:${file.sha256}`)fail('完整正式资产证明回查失败');}
  output('verified','true');output('release_id',release.id);output('tag',identity.tag);
}

function ownedRun(run) {
  // 每个目标只处理自身现行Workflow；文件缺失不能证明历史任务归属。
  return Number.isSafeInteger(run?.id)&&run.id>0&&run.path===workflowPath
    &&run.head_branch==='main'&&run.event==='workflow_dispatch'
    &&(!run.repository||run.repository.full_name===owner.repository);
}

export function cleanupPlan(runs,current,result) {
  if(!['success','failed'].includes(result)||!ownedRun(current)||!Number.isFinite(Date.parse(current.created_at)))fail('清理所属任务身份无效');
  const earlier=run=>Date.parse(run.created_at)<Date.parse(current.created_at)
    ||Date.parse(run.created_at)===Date.parse(current.created_at)&&run.id<current.id;
  return runs.filter(run=>ownedRun(run)&&run.id!==current.id&&run.status==='completed'&&earlier(run)
    &&(run.conclusion==='success'?'success':'failed')===result).sort((a,b)=>a.id-b.id);
}

async function remove(path,api) { await api(path,{method:'DELETE'});const readPath=path.replace(/^git\/refs\//u,'git/ref/');if(await api(readPath)!==null)fail('删除回查仍存在，清理失败'); }
async function removeRunRelease(run,releases,api) {
  for(const release of releases){
    const metadata=(await citizenChainRelease(release,owner.platform,tag=>api('git/ref/tags/'+encodeURIComponent(tag))));
    if(!metadata||metadata.run_id!==run.id)continue;
    if(metadata.source_sha!==run.head_sha)fail('正式Release与所属Run不一致');
    const tag=metadata.tag;
    const again=await api(`actions/runs/${run.id}`);
    if(again&&again.id!==Number(process.env.GITHUB_RUN_ID)
      &&(again.status!=='completed'||again.run_attempt!==run.run_attempt||again.conclusion!==run.conclusion))fail('所属任务已变化，停止清理');
    await remove(`releases/${release.id}`,api);
    const beforeTag=await api(`actions/runs/${run.id}`);
    if(beforeTag&&beforeTag.id!==Number(process.env.GITHUB_RUN_ID)
      &&(beforeTag.status!=='completed'||beforeTag.run_attempt!==run.run_attempt||beforeTag.conclusion!==run.conclusion))fail('所属任务已变化，停止清理');
    await remove(`git/refs/tags/${encodeURIComponent(tag)}`,api);
  }
}
export async function cleanup(result,identity=context(),api=request) {
  const current=await api(`actions/runs/${identity.run_id}`);
  const plan=cleanupPlan(await pages('actions/runs','workflow_runs',api),current,result);
  const releases=await pages('releases',null,api),removed=[];
  for(const row of plan){const run=await api(`actions/runs/${row.id}`);if(!run){removed.push(row.id);continue;}
    if(run.run_attempt!==row.run_attempt||cleanupPlan([run],current,result).length!==1)continue;
    await removeRunRelease(run,releases,api);
    // 失败若只形成Tag也按它的准确Run坐标处理，不能留下同类孤立产物。
    const tags=await api(`git/matching-refs/tags/${prefix}`);
    if(!Array.isArray(tags))fail('所属Tag集合无效');
    for(const reference of tags){
      const tag=String(reference.ref||'').slice('refs/tags/'.length);
      if(!String(reference.ref||'').startsWith('refs/tags/'+prefix)
        ||!new RegExp(`-r${run.id}-a[1-9][0-9]*$`,'u').test(tag)||Number(tag.slice(tag.lastIndexOf('-a')+2))>run.run_attempt)continue;
      if(reference.object?.type!=='commit'||reference.object.sha!==run.head_sha)fail('所属Tag来源已改变，停止清理');
      const again=await api(`actions/runs/${run.id}`);
      if(!again||cleanupPlan([again],current,result).length!==1)fail('所属任务已改变，停止清理');
      await remove(`git/refs/tags/${encodeURIComponent(tag)}`,api);
    }
    for(const asset of await pages(`actions/runs/${run.id}/artifacts`,'artifacts',api)){
      if(!Number.isSafeInteger(asset.id)||asset.id<=0)fail('所属Artifact坐标无效');
      const again=await api(`actions/runs/${run.id}`);if(!again||again.status!=='completed'||again.run_attempt!==run.run_attempt||again.conclusion!==run.conclusion)fail('历史任务已变化，停止清理');
      await remove(`actions/artifacts/${asset.id}`,api);
    }
    const final=await api(`actions/runs/${run.id}`);
    if(final&&(final.run_attempt!==run.run_attempt||cleanupPlan([final],current,result).length!==1))fail('历史任务状态改变，停止清理');
    if(final)await remove(`actions/runs/${run.id}`,api);removed.push(run.id);
  }
  return removed;
}

export function precedingResult(needs) {
  if(!needs||typeof needs!=='object'||Array.isArray(needs)||!Object.keys(needs).length)fail('前置任务结果缺失');
  return Object.values(needs).every(value=>value?.result==='success')?'success':'failed';
}
async function discardCurrent(identity,api) {
  for(const release of await pages('releases',null,api)){
    const metadata=(await citizenChainRelease(release,owner.platform,tag=>api('git/ref/tags/'+encodeURIComponent(tag))));
    if(metadata?.run_id===identity.run_id&&metadata.run_attempt===identity.run_attempt)
      await removeRunRelease({id:identity.run_id,head_sha:identity.source_sha},[release],api);
  }
  const tag=process.env.RELEASE_TAG;
  if(tag&&tag.startsWith(prefix)&&tag.endsWith(`-r${identity.run_id}-a${identity.run_attempt}`)){
    const path=`git/refs/tags/${encodeURIComponent(tag)}`;
    if(await api(path.replace(/^git\/refs\//u,'git/ref/'))!==null)await remove(path,api);
  }
}
export async function finish(needs=JSON.parse(process.env.RELEASE_NEEDS||'null'),api=request,identity=context()) {
  const result=precedingResult(needs),errors=[];
  const attempt=async action=>{try{return await action();}catch(error){errors.push(error);return null;}};
  let removed;
  if(result==='success') {
    removed=await attempt(()=>cleanup('success',identity,api));
    if(errors.length) {
      await attempt(()=>discardCurrent(identity,api));
      await attempt(()=>cleanup('failed',identity,api));
    }
  } else {
    // 本次撤销失败也必须尝试清理同目标旧失败；各项真实错误均保留。
    await attempt(()=>discardCurrent(identity,api));
    removed=await attempt(()=>cleanup('failed',identity,api));
  }
  if(errors.length)throw new AggregateError(errors,'本目标最后处理失败：'+errors.map(error=>error.message).join('；'));
  if(process.env.GITHUB_STEP_SUMMARY)appendFileSync(process.env.GITHUB_STEP_SUMMARY,`本目标${result==='success'?'成功':'失败'}；已清理同类旧Run：${removed.join('、')||'无'}。\n`);
  if(result==='failed')fail('前置任务未全部成功');
}

// 本目标实际构建与组包接口。
async function productCommand() { return false; }

const direct=process.argv[1]&&resolve(process.argv[1])===fileURLToPath(import.meta.url);
const testing=direct&&Boolean(process.env.NODE_TEST_CONTEXT)&&process.argv.length===2;
if(direct&&!testing){
  try{const [command,...args]=process.argv.slice(2);
    if(command==='prepare')await prepare();else if(command==='job')job();else if(command==='step')step(args[0]);
    else if(command==='action')action(args[0],args.slice(1));else if(command==='collect')await collect(args);
    else if(command==='collect-produced')await collectProduced();else if(command==='publish')await publish(args[0]);else if(command==='finish')await finish();
    else if(!await productCommand(command,args))fail('自动化命令无效');
  }catch(error){console.error(error.message);process.exitCode=1;}
}

if(testing){
  const {default:assert}=await import('node:assert/strict');const {default:test}=await import('node:test');
  test('前端构建进入本产品工程视图且成功失败均收尾',()=>{
    assert.match(commands[String(firstProjectStep)].source,/npm --prefix node\/frontend run build/);
    assert.match(commands['5'].source,/protoc "\$platform"\)/);
    const workflow=readFileSync(join(root,workflowPath),'utf8');
    assert.ok(workflow.includes('    - name: 清理本轮节点工程视图\n      if: always()\n      run: node scripts/build.mjs finish '+owner.platform+' "$GITHUB_RUN_ID"'));
  });

  test('节点版本动作只交付内嵌脚本接受的apply与lock参数',()=>{
    const sources=Object.values(commands).map(command=>command.source);
    for(const verb of ['apply','lock']){
      const matched=sources.filter(source=>source.includes(' action node-version '+verb+' "$SOFTWARE_VERSION"'));
      assert.equal(matched.length,1,verb+'动作入口必须唯一且参数准确');
    }
    assert.equal(sources.some(source=>source.includes('action node-version node-version')),false);
  });
  test('旧入口不能成为任一现行平台的清理归属证明',()=>{
    const current={id:9,path:workflowPath,head_branch:'main',event:'workflow_dispatch',created_at:'2026-01-02T00:00:00Z'};
    const old={...current,id:1,status:'completed',conclusion:'success',created_at:'2026-01-01T00:00:00Z'};
    for(const path of ['.github/workflows/release.yml',`.github/workflows/${owner.product}-${owner.platform}-ci.yml`,'.github/workflows/deleted.yml'])
      assert.deepEqual(cleanupPlan([{...old,path}],current,'success'),[]);
  });
  test('撤销当前产物失败仍处理旧失败且最终失败',async()=>{
    const current={id:9,path:workflowPath,head_branch:'main',event:'workflow_dispatch',created_at:'2026-01-02T00:00:00Z'};
    let releases=0,history=0;
    const api=async path=>{
      if(path.startsWith('releases?')){if(++releases===1)throw Error('撤销中断');return [];}
      if(path==='actions/runs/9')return current;
      if(path.startsWith('actions/runs?')){history++;return {workflow_runs:[]};}
      throw Error('未声明请求');
    };
    await assert.rejects(finish({build:{result:'failure'}},api,{run_id:9}),/撤销中断/);
    assert.equal(history,1);assert.equal(releases,2);
  });
  test('实际交付文件完整进入正式证明，必要件缺失或为空时失败',async()=>{
    const {mkdtempSync}=await import('node:fs');const {tmpdir}=await import('node:os');
    const directory=mkdtempSync(join(tmpdir(),'release-assets-')),previous={...process.env};
    try {
      const version='1.0.1';Object.assign(process.env,{GITHUB_ACTIONS:'true',GITHUB_REPOSITORY:owner.repository,GITHUB_REF:'refs/heads/main',GITHUB_EVENT_NAME:'workflow_dispatch',GITHUB_SHA:'a'.repeat(40),GITHUB_WORKFLOW_REF:`${owner.repository}/${workflowPath}@refs/heads/main`,GITHUB_RUN_ID:'9',GITHUB_RUN_ATTEMPT:'1',GITHUB_RUN_NUMBER:'2',RELEASE_VERSION:version,RELEASE_TAG:`${prefix}${version}-r9-a1`,RUNNER_TEMP:directory,RELEASE_DIR:join(directory,'input'),RELEASE_ASSETS_DIR:join(directory,'output'),GITHUB_OUTPUT:join(directory,'github-output')});
      const input=owner.product==='citizenchain'?join(directory,'citizenchain-release'):process.env.RELEASE_DIR;mkdirSync(input);
      const names=["公民链AMD.deb", "node.AppImage", "node.AppImage.sig", "citizenchain-node-latest-LinuxAMD.json"];
      for(const name of names)writeFileSync(join(input,name),'synthetic asset');
      if(owner.product!=='citizenchain'){mkdirSync(join(input,'payload'));writeFileSync(join(input,'payload','internal.apk'),'inside ZIP');}
      const result=await collectProduced();assert.deepEqual(result.assets.map(row=>row.name).sort(),[...names].sort());
      rmSync(join(input,names[0]));await assert.rejects(collectProduced(),/完整正式资产缺失/);
      writeFileSync(join(input,names[0]),'');await assert.rejects(collectProduced(),/非空普通文件/);
    }finally{for(const key of Object.keys(process.env))if(!(key in previous))delete process.env[key];Object.assign(process.env,previous);rmSync(directory,{recursive:true,force:true});}
  });
  test('清理旧失败Run同时回收其多个Attempt的准确孤立Tag',async()=>{
    const old={id:2,run_attempt:2,path:workflowPath,head_branch:'main',event:'workflow_dispatch',head_sha:'a'.repeat(40),status:'completed',conclusion:'failure',created_at:'2026-01-01T00:00:00Z'},current={...old,id:9,status:'in_progress',created_at:'2026-01-02T00:00:00Z'};
    const deleted=new Set(),tags=[1,2].map(attempt=>({ref:`refs/tags/${prefix}1.0.0-r2-a${attempt}`,object:{type:'commit',sha:old.head_sha}}));
    const api=async(path,options={})=>{
      if(options.method==='DELETE'){deleted.add(path);return {};}
      if(deleted.has(path)||deleted.has(path.replace('git/ref/','git/refs/')))return null;
      if(path.startsWith('actions/runs?'))return {workflow_runs:[old,current]};
      if(path.startsWith('releases?')||path.includes('/artifacts?'))return path.includes('/artifacts?')?{artifacts:[]}:[];
      if(path.startsWith('git/matching-refs/'))return tags;
      if(path==='actions/runs/2')return old;if(path==='actions/runs/9')return current;
      throw Error('未声明的请求');
    };
    assert.deepEqual(await cleanup('failed',{run_id:9},api),[2]);assert.equal([...deleted].filter(path=>path.startsWith('git/refs/')).length,2);
  });
  test('全部前置成功才成功，其余结论一律失败',()=>{
    assert.equal(precedingResult({build:{result:'success'},publish:{result:'success'}}),'success');
    for(const result of ['failure','cancelled','skipped','timed_out',undefined])assert.equal(precedingResult({build:{result}}),'failed');
    assert.throws(()=>precedingResult({}));
  });
  test('当前Run尚在运行也能清理同目标旧结果，保护其它目标和活动任务',()=>{
    const row=(id,conclusion='success',status='completed',path=workflowPath)=>({id,conclusion,status,path,head_branch:'main',event:'workflow_dispatch',created_at:new Date(1700000000000+id*1000).toISOString()});
    const current=row(6,null,'in_progress');const rows=[row(1),row(2,'failure'),row(3,'success','in_progress'),row(4,'success','completed','.github/workflows/release-other.yml'),current,row(7)];
    assert.deepEqual(cleanupPlan(rows,current,'success').map(row=>row.id),[1]);
    assert.deepEqual(cleanupPlan(rows,current,'failed').map(row=>row.id),[2]);
  });
  test('软件版本进位与协议版本边界',()=>{
    assert.equal(nextVersion('1.0.0',['1.99.99']),'2.0.0');assert.equal(nextVersion('9',['9'],true),'10');
    assert.throws(()=>nextVersion(String(0xffffffff),[String(0xffffffff)],true));
  });
  test('历史完整分页不截断超过1000条记录',async()=>{
    const rows=Array.from({length:1005},(_,id)=>({id}));const api=async path=>rows.slice((Number(/page=(\d+)$/u.exec(path)[1])-1)*100,Number(/page=(\d+)$/u.exec(path)[1])*100);
    assert.equal((await pages('releases',null,api)).length,1005);
  });
  test('失败清理只删除所属旧失败产物及Run，成功和活动任务独立保留',async()=>{
    const row=(id,conclusion,status='completed')=>({id,run_attempt:1,conclusion,status,path:workflowPath,head_branch:'main',event:'workflow_dispatch',repository:{full_name:owner.repository},head_sha:'a'.repeat(40),created_at:new Date(1700000000000+id*1000).toISOString()});
    const current=row(10,null,'in_progress'),rows=[row(1,'success'),row(2,'failure'),row(3,null,'in_progress'),current];
    const gone=new Set(),removed=[];
    const api=async(path,options={})=>{
      if(options.method==='DELETE'){removed.push(path);gone.add(path);return {};}
      if(gone.has(path))return null;
      if(path.startsWith('actions/runs?'))return {workflow_runs:rows};
      if(path.startsWith('releases?')||path.startsWith('git/matching-refs/'))return [];
      if(path.startsWith('actions/runs/2/artifacts?'))return {artifacts:[{id:20}]};
      if(path==='actions/artifacts/20')return {id:20};
      const match=/^actions\/runs\/(\d+)$/u.exec(path);if(match)return rows.find(row=>row.id===Number(match[1]))??null;
      throw Error('未声明的模拟接口：'+path);
    };
    assert.deepEqual(await cleanup('failed',{run_id:10},api),[2]);
    assert.deepEqual(removed,['actions/artifacts/20','actions/runs/2']);
  });

  test('GitHub运行序号保证成功历史清理后版本不会回到初始值',()=>{
    assert.equal(nextVersion('1.0.0',[],false,4),'1.0.3');
    assert.equal(nextVersion('1.99.99',[],false,2),'2.0.0');
    assert.equal(nextVersion('1.0.0',['3.0.0'],false,4),'3.0.1');
    assert.throws(()=>nextVersion('1.0.0',[],false,0));
  });

}

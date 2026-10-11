#!/usr/bin/env node
export async function citizenChainRelease(release,platform,readTag){
 if(!['macos','windows','linux-arm','linux-amd','wasm'].includes(platform))fail('公民链没有该正式发布目标');
 const start=owner.product+'-'+platform+'-v',tag=release?.tag_name;
 if(typeof tag!=='string'||!tag.startsWith(start))return null;
 const tail=tag.slice(start.length),parts=/^(.*)-r([1-9][0-9]*)-a([1-9][0-9]*)$/u.exec(tail);
 if(!parts)fail('公民链正式Tag运行坐标损坏');
 const version=parts[1],run_id=Number(parts[2]),run_attempt=Number(parts[3]);
 if(!Number.isSafeInteger(run_id)||!Number.isSafeInteger(run_attempt))fail('公民链正式Run越界');
 if(platform==='wasm'){
  if(!/^[1-9][0-9]*$/u.test(version)||BigInt(version)>4294967295n)fail('Runtime正式spec_version越界');
 }else if(!/^[0-9]+\.[0-9]+\.[0-9]+$/u.test(version))fail('节点正式软件版本错误');
 const reference=await readTag(tag);
 if(reference?.ref!=='refs/tags/'+tag||reference.object?.type!=='commit'||!/^[a-f0-9]{40}$/u.test(reference.object.sha||''))fail('公民链正式Release缺少准确Tag提交');
 return {version,tag,run_id,run_attempt,source_sha:reference.object.sha};
}
// 本仓本目标的完整自动化只由同名Workflow调用；版本与产物均在GitHub生成。
import { createHash } from 'node:crypto';
import { spawnSync, execFileSync } from 'node:child_process';
import { appendFileSync, copyFileSync, createReadStream, existsSync, lstatSync, mkdirSync, readFileSync, readdirSync, rmSync, writeFileSync } from 'node:fs';
import { basename, dirname, isAbsolute, join, resolve, sep } from 'node:path';
import { fileURLToPath, pathToFileURL } from 'node:url';

export const owner = Object.freeze({"product": "citizenchain", "platform": "wasm", "repository": "crcfrcn/citizenchain", "version_source": {"kind": "spec", "path": "runtime/src/lib.rs"}, "required_assets": ["citizenchain.wasm", "citizenchain.compact.wasm", "citizenchain.compact.compressed.wasm"], "asset_locations": ["$CARGO_TARGET_DIR/release/wbuild/citizenchain"], "asset_patterns": ["citizenchain.wasm", "citizenchain.compact.wasm", "citizenchain.compact.compressed.wasm"], "required_patterns": ["citizenchain.wasm", "citizenchain.compact.wasm", "citizenchain.compact.compressed.wasm"]});
const commands = Object.freeze({
  "1": {
    "shell": "bash",
    "source": "python3 - <<'PY'\nimport os,re\nfrom pathlib import Path\nruntime=Path('runtime/src/lib.rs'); tests=Path('runtime/src/tests/cases.rs')\nvalue=int(os.environ['SPEC_VERSION'])\nif not 0<value<2**32: raise SystemExit('Runtime spec_version越界')\nsource,count=re.subn(r'(?m)^(\\s*spec_version:\\s*)\\d+(\\s*,\\s*)$',lambda m:m[1]+str(value)+m[2],runtime.read_text())\nif count!=1: raise SystemExit('Runtime版本真源不唯一')\nchecks,count=re.subn(r'assert_eq!\\(VERSION.spec_version,\\s*\\d+\\);',f'assert_eq!(VERSION.spec_version, {value});',tests.read_text())\nif count!=1: raise SystemExit('Runtime版本测试断言不唯一')\nruntime.write_text(source); tests.write_text(checks)\nPY"
  },
  "2": {
    "shell": "bash",
    "source": "node \"$GITHUB_WORKSPACE/.github/workflows/release-wasm.mjs\" action linux-deps linux-deps\ncase \"$RUNNER_OS/$RUNNER_ARCH\" in\n  Linux/ARM64) platform=linux-arm ;;\n  Linux/X64) platform=linux-amd ;;\n  *) echo 'CitizenChain Runtime protoc宿主不受支持' >&2; exit 1 ;;\nesac\nprotoc_executable=\"$(node .github/workflows/release-wasm.mjs protoc \"$platform\" \"$GITHUB_WORKSPACE/target/build/protoc/$platform\")\"\nprintf 'PROTOC=%s\\n' \"$protoc_executable\" >> \"$GITHUB_ENV\""
  },
  "3": {
    "shell": "bash",
    "source": "cargo --config \"$GITHUB_WORKSPACE/config.toml\" metadata --locked --no-deps --format-version 1 >/dev/null\npython3 - <<'PY'\nimport hashlib\nimport os\nimport subprocess\nfrom pathlib import Path\ndef command_output(*command: str) -> str:\n    return subprocess.check_output(\n        command,\n        text=True,\n        stderr=subprocess.STDOUT,\n    ).strip()\nlock_data = Path(\"Cargo.lock\").read_bytes()\nsummary = Path(os.environ[\"GITHUB_STEP_SUMMARY\"])\nwith summary.open(\"a\", encoding=\"utf-8\") as out:\n    out.write(\"\\n## CitizenChain WASM 固定构建环境\\n\\n\")\n    out.write(\"- runner label：`ubuntu-24.04`\\n\")\n    out.write(f\"- runner OS / arch：`{os.environ['RUNNER_OS']} / {os.environ['RUNNER_ARCH']}`\\n\")\n    out.write(f\"- runner image OS：`{os.environ.get('ImageOS', 'unknown')}`\\n\")\n    out.write(f\"- runner image version：`{os.environ.get('ImageVersion', 'unknown')}`\\n\")\n    out.write(\n        \"- checkout action：\"\n        \"`fbc6f3992d24b796d5a048ff273f7fcc4a7b6c09` (`v5.1.0`)\\n\"\n    )\n    out.write(\n        \"- rust-toolchain action：\"\n        f\"`{os.environ['CITIZENCHAIN_RUST_INSTALLER_SHA']}` \"\n        f\"(`{os.environ['CITIZENCHAIN_RUST_VERSION']}`)\\n\"\n    )\n    out.write(f\"- Cargo.lock SHA-256：`{hashlib.sha256(lock_data).hexdigest()}`\\n\")\n    for label, command in (\n        (\"rustc\", (\"rustc\", \"--version\", \"--verbose\")),\n        (\"cargo\", (\"cargo\", \"--version\", \"--verbose\")),\n        (\"protoc\", (os.environ[\"PROTOC\"], \"--version\")),\n        (\"clang\", (\"clang\", \"--version\")),\n    ):\n        out.write(f\"\\n### {label}\\n\\n```\\n{command_output(*command)}\\n```\\n\")\nPY"
  },
  "4": {
    "shell": "bash",
    "source": "if cargo --config \"$GITHUB_WORKSPACE/config.toml\" tree --locked -p citizenchain -e features --no-default-features --features std \\\n  | grep -F 'citizenchain feature \"runtime-benchmarks\"'; then\n  echo \"正式 WASM 依赖图禁止启用 runtime-benchmarks\" >&2\n  exit 1\nfi"
  },
  "5": {
    "shell": "bash",
    "source": "cargo --config \"$GITHUB_WORKSPACE/config.toml\" clippy --locked -p citizenchain --no-default-features --features std -- -D warnings"
  },
  "6": {
    "shell": "bash",
    "source": "cargo --config \"$GITHUB_WORKSPACE/config.toml\" build --locked --release -p citizenchain --no-default-features --features std"
  },
  "7": {
    "shell": "bash",
    "source": "python3 - <<'PY'\nimport hashlib\nimport os\nfrom pathlib import Path\noutput_dir = (Path(os.environ[\"CARGO_TARGET_DIR\"]) / \"release/wbuild/citizenchain\")\nfilenames = (\n    \"citizenchain.wasm\",\n    \"citizenchain.compact.wasm\",\n    \"citizenchain.compact.compressed.wasm\",\n)\nsummary = Path(os.environ[\"GITHUB_STEP_SUMMARY\"])\nrows = []\nfor filename in filenames:\n    path = output_dir / filename\n    if not path.is_file() or path.stat().st_size == 0:\n        raise SystemExit(f\"WASM 产物不存在或为空：{path}\")\n    data = path.read_bytes()\n    digest = hashlib.blake2b(data, digest_size=32).hexdigest()\n    rows.append((filename, len(data), digest))\nwith summary.open(\"a\", encoding=\"utf-8\") as out:\n    out.write(\"\\n## CitizenChain WASM 产物\\n\\n\")\n    out.write(\"| 文件 | 字节数 | Blake2-256 |\\n\")\n    out.write(\"| --- | ---: | --- |\\n\")\n    for filename, size, digest in rows:\n        out.write(f\"| `{filename}` | {size} | `0x{digest}` |\\n\")\nPY"
  },
  "8": {
    "shell": "bash",
    "source": "npm --prefix node/frontend ci\nnpm --prefix node/frontend run build"
  },
  "9": {
    "shell": "bash",
    "source": "cargo --config \"$GITHUB_WORKSPACE/config.toml\" test --locked -p node \\\n  current_wasm_passes_candidate_runtime_policy_behavior_probes -- --nocapture"
  }
});
const actions = Object.freeze({
  "linux-deps": {
    "source": "#!/usr/bin/env bash\n\nset -euo pipefail\n\n# 中文注释：GitHub Ubuntu Runner 默认优先使用 Azure 区域镜像，该镜像偶发卡住时\n# Acquire::Retries 无法保证整条 apt-get 命令及时退出。这里为当前命令提供独立官方源，\n# 不改写 Runner 的全局 sources 文件，也不访问与 CitizenChain 构建无关的第三方仓库。\n# shellcheck disable=SC1091\nsource /etc/os-release\n\nif [[ \"${ID:-}\" != \"ubuntu\" || -z \"${VERSION_CODENAME:-}\" ]]; then\n  echo \"::error::只支持带 VERSION_CODENAME 的 Ubuntu Runner\"\n  exit 1\nfi\n\ncitizenchain_arch=\"$(dpkg --print-architecture)\"\ncitizenchain_source=\"$(mktemp)\"\ntrap 'rm -f \"${citizenchain_source}\"' EXIT\n\ncase \"${citizenchain_arch}\" in\n  amd64)\n    citizenchain_archive=\"https://archive.ubuntu.com/ubuntu\"\n    citizenchain_security=\"https://security.ubuntu.com/ubuntu\"\n    ;;\n  arm64)\n    citizenchain_archive=\"https://ports.ubuntu.com/ubuntu-ports\"\n    citizenchain_security=\"${citizenchain_archive}\"\n    ;;\n  *)\n    echo \"::error::不支持的 Ubuntu 架构：${citizenchain_arch}\"\n    exit 1\n    ;;\nesac\n\n{\n  echo \"deb [arch=${citizenchain_arch} signed-by=/usr/share/keyrings/ubuntu-archive-keyring.gpg] ${citizenchain_archive} ${VERSION_CODENAME} main restricted universe multiverse\"\n  echo \"deb [arch=${citizenchain_arch} signed-by=/usr/share/keyrings/ubuntu-archive-keyring.gpg] ${citizenchain_archive} ${VERSION_CODENAME}-updates main restricted universe multiverse\"\n  echo \"deb [arch=${citizenchain_arch} signed-by=/usr/share/keyrings/ubuntu-archive-keyring.gpg] ${citizenchain_archive} ${VERSION_CODENAME}-backports main restricted universe multiverse\"\n  echo \"deb [arch=${citizenchain_arch} signed-by=/usr/share/keyrings/ubuntu-archive-keyring.gpg] ${citizenchain_security} ${VERSION_CODENAME}-security main restricted universe multiverse\"\n} > \"${citizenchain_source}\"\n\ncitizenchain_apt_options=(\n  -o \"Dir::Etc::sourcelist=${citizenchain_source}\"\n  -o \"Dir::Etc::sourceparts=-\"\n  -o Acquire::Retries=2\n  -o Acquire::http::Timeout=20\n  -o Acquire::https::Timeout=20\n  -o Acquire::Languages=none\n)\n\ncitizenchain_packages=(\n  clang\n  llvm\n  llvm-dev\n  libclang-dev\n  libpam0g-dev\n  libssl-dev\n  libwebkit2gtk-4.1-dev\n  libgtk-3-dev\n  libayatana-appindicator3-dev\n  librsvg2-dev\n  pkg-config\n  patchelf\n  file\n)\n\nrun_apt() {\n  local citizenchain_label=\"$1\"\n  local citizenchain_timeout=\"$2\"\n  shift 2\n\n  local citizenchain_attempt\n  local citizenchain_status=1\n  for citizenchain_attempt in 1 2 3; do\n    echo \"${citizenchain_label}：第 ${citizenchain_attempt}/3 次\"\n    if timeout --signal=TERM --kill-after=15s \"${citizenchain_timeout}\" \"$@\"; then\n      return 0\n    else\n      citizenchain_status=$?\n    fi\n\n    if [[ \"${citizenchain_attempt}\" -lt 3 ]]; then\n      # 中文注释：重试整条 APT 事务，避免单个 Acquire 重试耗尽后直接终止产品流水线。\n      sleep \"$((citizenchain_attempt * 10))\"\n    fi\n  done\n\n  echo \"::error::${citizenchain_label}连续三次失败，最后退出码为 ${citizenchain_status}\"\n  return \"${citizenchain_status}\"\n}\n\nrun_apt \\\n  \"更新 CitizenChain Linux 官方软件源\" \\\n  4m \\\n  sudo env DEBIAN_FRONTEND=noninteractive apt-get \\\n    \"${citizenchain_apt_options[@]}\" update\n\nrun_apt \\\n  \"安装 CitizenChain Linux 系统依赖\" \\\n  8m \\\n  sudo env DEBIAN_FRONTEND=noninteractive apt-get \\\n    \"${citizenchain_apt_options[@]}\" install -y \\\n    \"${citizenchain_packages[@]}\"\n\n# 中文注释：APT 成功退出后再回读包状态和关键命令，禁止缺少依赖时进入 Rust/前端构建。\nfor citizenchain_package in \"${citizenchain_packages[@]}\"; do\n  if [[ \"$(dpkg-query -W -f='${Status}' \"${citizenchain_package}\" 2>/dev/null || true)\" != \"install ok installed\" ]]; then\n    echo \"::error::CitizenChain Linux 依赖未安装：${citizenchain_package}\"\n    exit 1\n  fi\ndone\n\nfor citizenchain_command in clang llvm-config pkg-config patchelf file; do\n  if ! command -v \"${citizenchain_command}\" >/dev/null 2>&1; then\n    echo \"::error::CitizenChain Linux 构建命令不可用：${citizenchain_command}\"\n    exit 1\n  fi\ndone\n\necho \"CitizenChain Linux 系统依赖已通过官方 Ubuntu 源安装并回读验证。\"\n",
    "shell": "bash"
  }
});
const shaPattern = /^[0-9a-f]{40}$/u;
const fail = message => { throw new Error(message); };
const root = fileURLToPath(new URL('../../', import.meta.url));
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

const CHAIN_HASH_PATTERN = /^0x[0-9a-f]{64}$/;
const CHAIN_RPC_METHODS = new Set([
  'chain_getFinalizedHead',
  'chain_getBlockHash',
  'state_getRuntimeVersion',
]);
const CHAIN_RPC_TIMEOUT_MS = 8000;
const CHAIN_RPC_MAX_RESPONSE_BYTES = 128 * 1024;

function requireProtectedChainRpcConfig({ chainUrl, accessClientId, accessClientSecret }) {
  let parsedUrl;
  try {
    parsedUrl = new URL(String(chainUrl || '').trim());
  } catch {
    throw new Error('国储会正式链 RPC 地址无效');
  }
  if (parsedUrl.origin !== 'https://chain.crcfrcn.com'
    || parsedUrl.username || parsedUrl.password || parsedUrl.hash) {
    throw new Error('国储会正式链 RPC 必须使用 chain.crcfrcn.com 受保护的 HTTPS 地址');
  }
  const clientId = String(accessClientId || '').trim();
  const clientSecret = String(accessClientSecret || '').trim();
  if (!clientId || !clientSecret) throw new Error('国储会正式链 Access 服务令牌未配置');
  return { url: parsedUrl.toString(), clientId, clientSecret };
}

async function readBoundedJson(response) {
  const declaredLength = Number.parseInt(response.headers.get('content-length') || '', 10);
  if (Number.isFinite(declaredLength) && declaredLength > CHAIN_RPC_MAX_RESPONSE_BYTES) {
    await response.body?.cancel();
    throw new Error('国储会正式链 RPC 响应超过大小限制');
  }
  if (!response.body) throw new Error('国储会正式链 RPC 返回空响应');
  const reader = response.body.getReader();
  const chunks = [];
  let total = 0;
  for (;;) {
    const { done, value } = await reader.read();
    if (done) break;
    total += value.byteLength;
    if (total > CHAIN_RPC_MAX_RESPONSE_BYTES) {
      await reader.cancel();
      throw new Error('国储会正式链 RPC 响应超过大小限制');
    }
    chunks.push(value);
  }
  const body = Buffer.concat(chunks.map((chunk) => Buffer.from(chunk)), total).toString('utf8');
  try {
    return JSON.parse(body);
  } catch {
    throw new Error('国储会正式链 RPC 返回无效 JSON');
  }
}

async function callProtectedChainRpc(config, method, params, id, options) {
  if (!CHAIN_RPC_METHODS.has(method)) fail('正式链RPC方法无效');
  const fetchImpl=options.fetchImpl||globalThis.fetch;
  let response;
  try {response=await fetchImpl(config.url,{method:'POST',redirect:'manual',signal:AbortSignal.timeout(options.timeoutMs??CHAIN_RPC_TIMEOUT_MS),
    headers:{accept:'application/json','content-type':'application/json','CF-Access-Client-Id':config.clientId,'CF-Access-Client-Secret':config.clientSecret},
    body:JSON.stringify({jsonrpc:'2.0',id,method,params})});} catch {fail('正式链RPC连接失败');}
  if(response.status>=300&&response.status<400){await response.body?.cancel();fail('正式链拒绝重定向');}
  if(!response.ok){await response.body?.cancel();fail('正式链RPC请求失败');}
  const payload=await readBoundedJson(response);
  if(!payload||Array.isArray(payload)||payload.jsonrpc!=='2.0'||payload.id!==id)fail('正式链RPC返回无效响应');
  if(payload.error!=null||!Object.hasOwn(payload,'result'))fail('正式链RPC没有成功结果');
  return payload.result;
}

// WASM Release 通过国储会 Access + Tunnel 私有 RPC 读取 finalized Runtime 版本；P2P 30333
// 不承载 JSON-RPC，本机 9944 是否运行也不影响 Release。三个请求均为固定只读方法。
export async function readRuntimeBuildTarget(chainConfig, options = {}) {
  const config = requireProtectedChainRpcConfig(chainConfig);
  const finalizedHead = await callProtectedChainRpc(
    config, 'chain_getFinalizedHead', [], 1, options,
  );
  if (typeof finalizedHead !== 'string' || !CHAIN_HASH_PATTERN.test(finalizedHead)) {
    throw new Error('国储会正式链 finalized hash 无效');
  }
  const genesisHash = await callProtectedChainRpc(
    config, 'chain_getBlockHash', [0], 2, options,
  );
  if (typeof genesisHash !== 'string' || !CHAIN_HASH_PATTERN.test(genesisHash)) {
    throw new Error('国储会正式链 genesis hash 无效');
  }
  const runtimeVersion = await callProtectedChainRpc(
    config, 'state_getRuntimeVersion', [finalizedHead], 3, options,
  );
  if (!runtimeVersion || typeof runtimeVersion !== 'object' || Array.isArray(runtimeVersion)) {
    throw new Error('国储会正式链 RuntimeVersion 无效');
  }
  const specVersion = runtimeVersion.specVersion;
  if (!Number.isSafeInteger(specVersion) || specVersion < 0 || specVersion > 0xffffffff) {
    throw new Error('国储会正式链 spec_version 无效');
  }
  const specName=runtimeVersion.specName;
  if(specName!=='citizenchain')fail('正式链Runtime名称无效');
  return {
    specVersion,
    genesisHash,
    chainName: specName,
    finalizedHead,
  };
}

// 正式链目标和版本由本仓GitHub运行读取并计算，不接受控制台交付的版本。
export function runtimeVersion(seed, versions, chainVersion) {
  const values=[seed,...versions,String(chainVersion)];
  if(values.some(value=>!/^\d+$/u.test(value)) || !Number.isSafeInteger(chainVersion) || chainVersion<0)fail('Runtime版本无效');
  const version=Math.max(Number(seed),...versions.map(Number),chainVersion+1);
  if(!Number.isSafeInteger(version)||version<1||version>0xffffffff)fail('协议版本越界');
  return String(version);
}

// 创世身份只读取本仓Runtime唯一冻结常量，不再维护GitHub配置副本。
function frozenGenesisHash(source=readFileSync(join(root,'runtime/primitives/src/genesis.rs'),'utf8')) {
  const matches=[...source.matchAll(/\bpub const GENESIS_HASH:\s*\[u8;\s*32\]\s*=\s*hex_literal::hex!\("([0-9a-f]{64})"\);/gu)];
  if(matches.length!==1||/^0{64}$/u.test(matches[0][1]))fail('Runtime冻结创世哈希真源无效');
  return '0x'+matches[0][1];
}
function verifiedGenesisHash(actual,source) {
  if(actual!==frozenGenesisHash(source))fail('本仓正式链创世身份不一致');
  return actual;
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
  const chain=await readRuntimeBuildTarget({chainUrl:process.env.CHAIN_URL,accessClientId:process.env.CHAIN_ID,accessClientSecret:process.env.CHAIN_SECRET});
  verifiedGenesisHash(chain.genesisHash);
  const version = runtimeVersion(seedVersion(), versions, chain.specVersion);
  output('chain_spec_version',chain.specVersion);output('genesis_hash',chain.genesisHash);output('finalized_head',chain.finalizedHead);
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
  const chain={spec_version:Number(process.env.CHAIN_SPEC_VERSION),genesis_hash:process.env.GENESIS_HASH,finalized_head:process.env.FINALIZED_HEAD};
  if(!Number.isSafeInteger(chain.spec_version)||chain.spec_version<0||Number(version)<=chain.spec_version||!CHAIN_HASH_PATTERN.test(chain.genesis_hash||'')||!CHAIN_HASH_PATTERN.test(chain.finalized_head||''))fail('WASM链目标证明无效');
  return {...identity, version, tag,chain};
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
  const directory = join(process.env.RELEASE_WORK,'commands');mkdirSync(directory,{recursive:true});
  const file = join(directory, value.shell === 'pwsh' ? 'step.ps1' : 'step.sh');
  writeFileSync(file, value.shell === 'bash' ? 'set -euo pipefail\n'+value.source : "$ErrorActionPreference = 'Stop'\n"+value.source,{mode:0o700});
  const result = spawnSync(value.shell === 'pwsh' ? 'pwsh' : 'bash', value.shell === 'pwsh' ? ['-NoProfile','-File',file] : [file],
    {cwd:process.cwd(),env:process.env,stdio:'inherit'});
  rmSync(file,{force:true});
  if (result.error || result.status !== 0) fail(`本仓构建步骤失败：${key}`);
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
  if(Object.entries(identity).some(([key,value])=>JSON.stringify(metadata[key])!==JSON.stringify(value))||!Array.isArray(metadata.assets)||!metadata.assets.length)fail('完整产物身份无效');
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

// 官方protoc原件和安装只归本仓自动化；Build不准备Runner工具。
const protocLock=Object.freeze({"version":"35.0","source":"https://github.com/protocolbuffers/protobuf/releases/tag/v35.0","archives":{"macos":{"url":"https://github.com/protocolbuffers/protobuf/releases/download/v35.0/protoc-35.0-osx-aarch_64.zip","sha256":"45444963204757fd3e2fbe304bc1fdadfb488d8556ff099c4cc06575eab88976","executable":"bin/protoc"},"linux-arm":{"url":"https://github.com/protocolbuffers/protobuf/releases/download/v35.0/protoc-35.0-linux-aarch_64.zip","sha256":"36b518ac14d90351cc6598228ed2bbe5afe4e357b1af470b07e0ec1609875de2","executable":"bin/protoc"},"linux-amd":{"url":"https://github.com/protocolbuffers/protobuf/releases/download/v35.0/protoc-35.0-linux-x86_64.zip","sha256":"a45cda0989c17dd950db55f6fbe1e5814c50fda08e87aa422980ac1f89dddbbc","executable":"bin/protoc"},"windows":{"url":"https://github.com/protocolbuffers/protobuf/releases/download/v35.0/protoc-35.0-win64.zip","sha256":"d1cede9e308cc3eb072392af1c02ccae4bdd3d2f374ec2970dbd8cdfdaa91363","executable":"bin/protoc.exe"}}});
export async function prepareProtoc(platform,work){
 const expected=protocLock.archives[platform];
 if(!expected||!['macos','windows','linux-arm','linux-amd'].includes(platform))fail('自动化protoc平台未登记');
 if(typeof work!=='string'||!isAbsolute(work)||resolve(work)!==work
  ||!work.startsWith(join(root,'target/build/protoc')+sep)||work!==join(root,'target/build/protoc',platform))fail('自动化protoc工作根越界');
 const fs=await import('node:fs');
 const candidate=fs.lstatSync(work,{throwIfNoEntry:false});
 if(candidate&&(!candidate.isDirectory()||candidate.isSymbolicLink()))fail('自动化protoc工作根无效');
 fs.mkdirSync(work,{recursive:true});
 const archive=join(work,basename(new URL(expected.url).pathname)),pending=archive+'.pending';
 if(!expected.url.startsWith('https://github.com/protocolbuffers/protobuf/releases/download/v'+protocLock.version+'/')
  ||!/^[a-f0-9]{64}$/u.test(expected.sha256)||expected.executable!==(platform==='windows'?'bin/protoc.exe':'bin/protoc'))fail('自动化protoc锁无效');
 if(!fs.existsSync(archive)){
  const response=await fetch(expected.url,{redirect:'follow',signal:AbortSignal.timeout(300000)});
  const final=new URL(response.url);
  if(!response.ok||!response.body||final.protocol!=='https:'||!['github.com','release-assets.githubusercontent.com'].includes(final.hostname))fail('自动化protoc官方来源无效');
  const chunks=[];let bytes=0;
  for await(const chunk of response.body){bytes+=chunk.length;if(bytes>128*1024*1024)fail('自动化protoc归档超限');chunks.push(Buffer.from(chunk));}
  const value=Buffer.concat(chunks);
  if(createHash('sha256').update(value).digest('hex')!==expected.sha256)fail('自动化protoc归档摘要不符');
  fs.writeFileSync(pending,value,{flag:'wx',mode:0o600});fs.renameSync(pending,archive);
 }
 if(createHash('sha256').update(fs.readFileSync(archive)).digest('hex')!==expected.sha256)fail('自动化protoc缓存漂移');
 const output=join(work,'payload');if(fs.existsSync(output))fail('自动化protoc输出已存在');
 const members=execFileSync('unzip',['-Z1',archive],{encoding:'utf8'}).trim().split(/\r?\n/u);
 if(!members.length||members.some(name=>!name||name.startsWith('/')||name.split('/').some(part=>part==='..'||part==='.')||/[\x00-\x1f]/u.test(name)))fail('自动化protoc归档成员越界');
 fs.mkdirSync(output,{mode:0o700});
 const unpack=spawnSync('unzip',['-q',archive,'-d',output],{stdio:'inherit'});
 if(unpack.error||unpack.status!==0)fail('自动化protoc解包失败');
 const executable=join(output,expected.executable),file=fs.lstatSync(executable,{throwIfNoEntry:false});
 if(!file?.isFile()||file.isSymbolicLink())fail('自动化protoc可执行文件无效');
 fs.chmodSync(executable,0o700);
 const version=spawnSync(executable,['--version'],{encoding:'utf8'});
 if(version.error||version.status!==0||version.stdout.trim()!=='libprotoc '+protocLock.version)fail('自动化protoc版本无效');
 return executable;
}
const direct=process.argv[1]&&resolve(process.argv[1])===fileURLToPath(import.meta.url);
const testing=direct&&Boolean(process.env.NODE_TEST_CONTEXT)&&process.argv.length===2;
if(direct&&!testing){
  try{const [command,...args]=process.argv.slice(2);
    if(command==='protoc'){if(args.length!==2)fail('自动化protoc参数无效');process.stdout.write(await prepareProtoc(args[0],args[1]));}else if(command==='prepare')await prepare();else if(command==='job')job();else if(command==='step')step(args[0]);
    else if(command==='action')action(args[0],args.slice(1));else if(command==='collect')await collect(args);
    else if(command==='collect-produced')await collectProduced();else if(command==='publish')await publish(args[0]);else if(command==='finish')await finish();
    else if(!await productCommand(command,args))fail('自动化命令无效');
  }catch(error){console.error(error.message);process.exitCode=1;}
}

if(testing){
  const {default:assert}=await import('node:assert/strict');const {default:test}=await import('node:test');
 const hash = '0x' + '1'.repeat(64);
 const config = {chainUrl:'https://chain.crcfrcn.com',accessClientId:'fixture',accessClientSecret:'fixture'};
 test('WASM创世预期只读取Runtime唯一冻结常量',()=>{
  const declaration='pub const GENESIS_HASH: [u8; 32] = hex_literal::hex!("'+'1'.repeat(64)+'");';
  assert.equal(frozenGenesisHash(declaration),hash);
  assert.equal(verifiedGenesisHash(hash,declaration),hash);
  assert.throws(()=>verifiedGenesisHash('0x'+'2'.repeat(64),declaration),/本仓正式链创世身份不一致/);
  assert.match(frozenGenesisHash(),/^0x[0-9a-f]{64}$/u);
  for(const source of ['',declaration+'\n'+declaration,'pub const GENESIS_HASH: [u8; 32] = hex_literal::hex!("'+'0'.repeat(64)+'");'])
    assert.throws(()=>frozenGenesisHash(source),/Runtime冻结创世哈希真源无效/);
 });
 test('WASM自动化读取同一finalized锚点的版本和真实创世身份',async()=>{
  const requests=[];
  const result=await readRuntimeBuildTarget(config,{fetchImpl:async(url,options)=>{
   assert.equal(url,config.chainUrl+'/');assert.equal(options.redirect,'manual');
   const request=JSON.parse(options.body);requests.push(request);
   const result=request.id===1?hash:request.id===2?'0x'+'2'.repeat(64):{specVersion:7,specName:'citizenchain'};
   return Response.json({jsonrpc:'2.0',id:request.id,result});
  }});
  assert.deepEqual(result,{specVersion:7,genesisHash:'0x'+'2'.repeat(64),chainName:'citizenchain',finalizedHead:hash});
  assert.deepEqual(requests.map(x=>[x.method,x.params]),[['chain_getFinalizedHead',[]],['chain_getBlockHash',[0]],['state_getRuntimeVersion',[hash]]]);
 });
 test('WASM自动化拒绝错误链入口、响应身份和溢出版本',async()=>{
  await assert.rejects(readRuntimeBuildTarget({...config,chainUrl:'https://example.com'}),/受保护/);
  await assert.rejects(readRuntimeBuildTarget(config,{fetchImpl:async()=>Response.json({jsonrpc:'2.0',id:99,result:hash})}),/无效响应/);
  await assert.rejects(readRuntimeBuildTarget(config,{fetchImpl:async(_,options)=>{
   const {id}=JSON.parse(options.body);return Response.json({jsonrpc:'2.0',id,result:id<3?hash:{specVersion:2**32}});
  }}),/spec_version/);
 });
  test('协议版本由链上及本仓成功事实生成，溢出失败',()=>{
    assert.equal(runtimeVersion('0',[],0),'1');assert.equal(runtimeVersion('0',['3'],2),'3');
    assert.equal(runtimeVersion('0',['3'],3),'4');assert.throws(()=>runtimeVersion('0',[],0xffffffff));
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

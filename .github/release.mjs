import {execFileSync} from 'node:child_process';
import {createHash} from 'node:crypto';
import {appendFileSync, createReadStream, lstatSync, mkdirSync, readFileSync, readdirSync, realpathSync, writeFileSync} from 'node:fs';
import {dirname, join} from 'node:path';
import {product} from '../scripts/config.mjs';
import {nodeTools, npmCLI, runBuildProcess} from '../scripts/build.mjs';
import {assertRuntimeTarget, parseRuntimeBase} from '../scripts/runtime.mjs';

const root = dirname(import.meta.dirname);

async function artifactBytes(path) {
  const info = lstatSync(path);
  if (!info.isFile() || info.size <= 0) throw Error('产物不是非空普通文件');
  const hash = createHash('sha256');
  for await (const chunk of createReadStream(path)) hash.update(chunk);
  return {size: info.size, sha256: hash.digest('hex')};
}

// 发布作业在写入 GitHub 前校验本轮下载的实际产物。
export async function publish({context, github, core}) {
  const platform = process.env.PLATFORM;
  const version = process.env.CITIZENCHAIN_RELEASE_VERSION;
  const declared = product.platforms[platform];
  const tag = product.title + '·' + declared.name + '·v' + version;
  const assets = JSON.parse(process.env.SOURCE_ASSETS);
  const names = declared.files || [declared.artifact.replace('{version}', version)];
  const source = join(process.env.RUNNER_TEMP, 'artifact');

  if (context.sha !== process.env.CITIZENCHAIN_SOURCE_SHA || context.ref !== 'refs/heads/main') {
    throw Error('发布源码身份不符');
  }
  if (JSON.stringify(assets.map(asset => asset.name).sort()) !== JSON.stringify([...names].sort())) {
    throw Error('正式产物集合不完整');
  }
  for (const asset of assets) {
    const {size, sha256} = await artifactBytes(join(source, asset.name));
    if (size !== asset.size || sha256 !== asset.sha256) throw Error('正式产物字节不符');
  }

  const proof = {
    schema: 1, repository: product.repository, product_id: product.id, platform,
    workflow: '.github/workflows/' + declared.workflow, run_id: context.runId,
    run_attempt: Number(process.env.GITHUB_RUN_ATTEMPT), source_sha: context.sha, version, tag, assets,
  };
  if (platform === 'wasm') {
    const runtime = JSON.parse(process.env.RUNTIME_BUILD);
    if (String(runtime.spec_version) !== version || process.env.BUILT_VERSION !== version || runtime.source_sha !== context.sha) {
      throw Error('WASM实际版本不符');
    }
    proof.runtime = runtime;
  }

  const notes = join(process.env.RUNNER_TEMP, 'release-notes.txt');
  writeFileSync(notes, '<!-- automation:' + JSON.stringify(proof) + ' -->\n');
  execFileSync('gh', ['release', 'create', tag, ...assets.map(asset => join(source, asset.name) + '#' + asset.label),
    '--repo', product.repository, '--target', context.sha, '--title', tag, '--notes-file', notes,
    '--draft', '--latest=false'], {stdio: ['ignore', 'pipe', 'pipe']});

  const releases = await github.paginate(github.rest.repos.listReleases, {...context.repo, per_page: 100});
  const drafts = releases.filter(release => release.tag_name === tag);
  if (drafts.length !== 1) throw Error('本轮草稿不唯一');
  const draft = drafts[0];
  if (!draft.draft || draft.prerelease || draft.assets.length !== assets.length) {
    throw Error('本轮草稿或附件状态不符');
  }
  for (const asset of draft.assets) {
    const original = assets.find(value => value.name === asset.name);
    if (!original || asset.state !== 'uploaded' || asset.size !== original.size
        || asset.digest !== 'sha256:' + original.sha256 || asset.label !== original.label) {
      throw Error('正式附件回读不符');
    }
  }
  const {data: release} = await github.rest.repos.updateRelease({
    ...context.repo, release_id: draft.id, draft: false,
  });
  if (release.draft || release.prerelease || !release.published_at) throw Error('正式Release发布未确认');
  const {data: ref} = await github.rest.git.getRef({...context.repo, ref: 'tags/' + tag});
  if (ref.object.type !== 'commit' || ref.object.sha !== context.sha) throw Error('正式Tag提交不符');
  core.info('正式交付已确认：' + tag);
}

export async function version(platform) {
  // 每个平台只从自己的成功正式Release确定下一版；失败和重试不消耗版本。
  const {name, workflow} = product.platforms[platform];
  const prefix = product.title + '·' + name + '·v';
  const valid=value=>/^(0|[1-9][0-9]*)\.(0|[1-9][0-9]*)\.(0|[1-9][0-9]*)$/u.test(value)
    &&value.split('.').every(part=>BigInt(part)<2n**64n);
  let version=JSON.parse(readFileSync('node/tauri.conf.json','utf8')).version;
  const releases=JSON.parse(execFileSync('gh',['api','--paginate','--slurp','repos/'+product.repository+'/releases?per_page=100'],{encoding:'utf8',maxBuffer:16*1024*1024})).flat()
    .filter(row=>row.draft===false&&row.prerelease===false&&typeof row.tag_name==='string'&&row.tag_name.startsWith(prefix))
    .sort((a,b)=>String(b.published_at).localeCompare(String(a.published_at))||b.id-a.id);
  for(const release of releases){
    const records=String(release.body||'').split('\n').filter(line=>line.startsWith('<!-- automation:')&&line.endsWith(' -->'));
    if(records.length!==1)continue;
    const proof=JSON.parse(records[0].slice('<!-- automation:'.length,-' -->'.length));
    if(proof.repository!==product.repository||proof.product_id!==product.id||proof.platform!==platform)continue;
    if(proof.schema!==1||proof.workflow!=='.github/workflows/'+workflow||!valid(proof.version)
      ||proof.tag!==release.tag_name||release.tag_name!==prefix+proof.version
      ||!Number.isSafeInteger(proof.run_id)||proof.run_id<=0)throw Error('本平台正式版本记录无效');
    const run=JSON.parse(execFileSync('gh',['api','repos/'+product.repository+'/actions/runs/'+proof.run_id],{encoding:'utf8'}));
    if(run.status!=='completed'||run.conclusion!=='success')continue;
    if(run.path!==proof.workflow||run.head_branch!=='main'||run.event!=='workflow_dispatch'
      ||run.head_sha!==proof.source_sha||run.run_attempt!==proof.run_attempt)throw Error('正式版本与成功Run不符');
    const parts=proof.version.split('.');parts[2]=String(BigInt(parts[2])+1n);version=parts.join('.');
    break;
  }
  if(!valid(version))throw Error('本轮节点版本无效');
  appendFileSync(process.env.GITHUB_OUTPUT,'version='+version+'\n');
}

export async function wasmVersion({context,github,core}) {
  if (context.ref !== 'refs/heads/main' || context.sha !== process.env.CITIZENCHAIN_SOURCE_SHA) throw Error('WASM源码身份不符');
  const version = process.env.CITIZENCHAIN_RELEASE_VERSION;
  const {target} = parseRuntimeBase(process.env.CITIZENCHAIN_RUNTIME_BASE);
  assertRuntimeTarget(target, version, process.env.CITIZENCHAIN_RUNTIME_TRANSACTION_VERSION);
  const runs=await github.paginate(github.rest.actions.listWorkflowRuns,{...context.repo,workflow_id:product.platforms.wasm.workflow,branch:'main',event:'workflow_dispatch',per_page:100});
  if(runs.some(run=>run.id!==context.runId&&run.status!=='completed'&&run.run_number<(runs.find(row=>row.id===context.runId)?.run_number||0)))throw Error('WASM已有活动编译Run');
  const tag=product.title+'·WASM·v'+version;
  const releases=await github.paginate(github.rest.repos.listReleases,{...context.repo,per_page:100});
  // 编译候选可能尚未上链；同目标版本的旧Release和Tag先删除，再重新编译。
  for(const release of releases.filter(value=>value.tag_name===tag)){
    await github.rest.repos.deleteRelease({...context.repo,release_id:release.id});
    core.info('已删除目标版本旧Release：'+tag);
  }
  // 仅接受404或GitHub明确返回的422引用不存在，其它删除错误使本轮失败。
  try{
    await github.rest.git.deleteRef({...context.repo,ref:'tags/'+tag});
    core.info('已删除目标版本旧Tag：'+tag);
  }catch(error){
    if(error.status!==404&&(error.status!==422||error.response?.data?.message!=='Reference does not exist'))throw error;
  }
  core.info('WASM候选已确认：'+tag);
}

export async function recordAssets({context,core}) {
  const p=process.env.PLATFORM,version=process.env.CITIZENCHAIN_RELEASE_VERSION,declared=product.platforms[p];
  const names=declared.files||[declared.artifact.replace('{version}',version)];
  const tag=product.title+'·'+declared.name+'·v'+version,directory=process.env.GITHUB_WORKSPACE+'/target/build/'+p+'/artifact';
  if(JSON.stringify(readdirSync(directory).sort())!==JSON.stringify([...names].sort()))throw Error('完整产物集合不符');
  const assets=[];
  for(const name of names){
    const label=tag+(p==='wasm'?name.slice('runtime'.length):declared.extension);
    assets.push({name,label,...await artifactBytes(join(directory,name))});
  }
  core.setOutput('assets',JSON.stringify(assets));
  if(p!=='wasm')return;
  const base=JSON.parse(process.env.CITIZENCHAIN_RUNTIME_BASE);
  core.setOutput('version',version);
  core.setOutput('runtime',JSON.stringify({source_sha:context.sha,genesis_hash:base.genesisHash,base_block_hash:base.blockHash,spec_name:base.specName,
    base_spec_version:base.specVersion,base_transaction_version:base.transactionVersion,
    spec_version:Number(version),transaction_version:Number(process.env.CITIZENCHAIN_RUNTIME_TRANSACTION_VERSION)}));
}

function runShell(script) {
  return runBuildProcess('bash',['-euo','pipefail','-c',script],process.env,root);
}

const testEnvironment = `export NODE="$(node -p process.execPath)" CITIZENCHAIN_BUILD_PLATFORM="$PLATFORM"
unset SKIP_WASM_BUILD WASM_FILE DOCS_RS
for key in $(compgen -v); do
  case "$key" in APPLE_*|*_TOKEN|*_PASSWORD|*_PRIVATE_KEY|*_SECRET|NODE_OPTIONS|NODE_PATH|NODE_TEST_CONTEXT|SKIP_*_WASM_BUILD) unset "$key" ;; esac
done`;

export async function test() {
  mkdirSync(process.env.CITIZENCHAIN_WORK_DIR,{recursive:true});
  if (process.env.PLATFORM === 'wasm') return runShell(testEnvironment+'\n'+`cargo --config config.toml clean -p runtime --locked
cargo --config config.toml test --locked --release -p runtime --all-targets`);
  const {node,npm} = nodeTools();
  for (const lock of product.locks.filter(file=>file.endsWith('/package-lock.json'))) {
    await runBuildProcess(node,[npm,'ci','--include=dev'],process.env,join(root,dirname(lock)));
  }
  return runShell(testEnvironment+'\n'+`node --test --test-concurrency=1 onchina/platform/tests/structure.test.mjs node/frontend/tests/metamask.test.mjs
cargo test --manifest-path crates/protocol/Cargo.toml --locked
npm run check --prefix crates/scanner
npm test --prefix crates/scanner
npm run test:local-doc --prefix node/frontend
npm run test:types --prefix node/frontend
npm test --prefix node/frontend
npm run typecheck --prefix onchina/platformweb
npm run typecheck --prefix onchina/registryweb
npm run typecheck --prefix onchina/legislationweb
npm run typecheck --prefix onchina/private/frontend
npm run typecheck --prefix onchina/judicial/frontend
npm test --prefix onchina/platformweb
npm test --prefix onchina/registryweb
npm test --prefix onchina/legislationweb
npm test --prefix onchina/private/frontend
npm test --prefix onchina/judicial/frontend
export TAURI_CONFIG="$(node -e 'console.log(JSON.stringify({version:process.env.CITIZENCHAIN_RELEASE_VERSION,build:{beforeBuildCommand:null,frontendDist:process.env.GITHUB_WORKSPACE+"/node/frontend"}}))')"
# 节点默认测试不生成SDK或产品WASM；真实整链验收归Runtime专项。
export SKIP_WASM_BUILD=1
if [ "$PLATFORM" = macos ]; then export CARGO_PROFILE_RELEASE_STRIP=none; fi
if [ "$PLATFORM" = windows ]; then export PATH="$(cygpath -u "$OPENSSL_DIR")/bin:$PATH"; fi
# SDK跳过构建时不会覆盖旧嵌入文件；先清理缺少wbuild资源的旧生成输出。
cargo --config config.toml clean --release -p runtime -p revive-dev-runtime -p substrate-test-runtime -p frame-storage-access-test-runtime --locked
cargo --config config.toml test --locked --release --workspace --exclude sc-consensus-grandpa --all-targets`);
}

export async function testModules() {
  const directory = realpathSync(join(process.env.CITIZENCHAIN_WORK_DIR,'onchina-modules'));
  const ids = readdirSync(directory,{withFileTypes:true}).filter(entry=>entry.isDirectory()).map(entry=>entry.name);
  const env = {...process.env,ONCHINA_TEST_MODULE_ROOT:directory,ONCHINA_TEST_MODULE_IDS:JSON.stringify(ids)};
  await runBuildProcess(process.execPath,['--test','--test-name-pattern','实际模块产物','onchina/platform/tests/structure.test.mjs'],env,root);
  await runBuildProcess(process.execPath,[npmCLI(),'test','--prefix','onchina/platformweb','--','-t','实际IIFE装载'],env,root);
}

const protoc = `curl --fail --location --proto =https --proto-redir =https --retry 3 --output "$RUNNER_TEMP/protoc.zip" "https://github.com/protocolbuffers/protobuf/releases/download/v$protoc_version/protoc-$protoc_version-$protoc_arch.zip"
mkdir -p "$RUNNER_TEMP/protoc"
unzip -q "$RUNNER_TEMP/protoc.zip" -d "$RUNNER_TEMP/protoc"
echo "PROTOC=$RUNNER_TEMP/protoc/bin/protoc" >> "$GITHUB_ENV"`;

export async function prepareTools() {
  const {PLATFORM:platform} = process.env;
  if (platform === 'windows') {
    return runBuildProcess('pwsh',['-NoProfile','-NonInteractive','-Command',`$ErrorActionPreference = 'Stop'
$zip = Join-Path $env:RUNNER_TEMP 'protoc.zip'
Invoke-WebRequest 'https://github.com/protocolbuffers/protobuf/releases/download/v${product.tools.protoc}/protoc-${product.tools.protoc}-win64.zip' -OutFile $zip
$dir = Join-Path $env:RUNNER_TEMP 'protoc'
Expand-Archive $zip -DestinationPath $dir
if (-not (Test-Path (Join-Path $env:PGROOT 'bin\\initdb.exe'))) { throw 'Runner 缺少 PostgreSQL 17 原件' }
# 复用Runner现有x64 MSVC OpenSSL开发库；只声明位置，不另下载或安装。
$openssl = Join-Path $env:ProgramFiles 'OpenSSL'
$opensslInclude = Join-Path $openssl 'include'
$opensslLib = Join-Path $openssl 'lib\\VC\\x64\\MD'
$opensslBin = Join-Path $openssl 'bin'
foreach ($file in @((Join-Path $opensslInclude 'openssl\\ssl.h'), (Join-Path $opensslLib 'libssl.lib'), (Join-Path $opensslLib 'libcrypto.lib'), (Join-Path $opensslBin 'libssl-3-x64.dll'), (Join-Path $opensslBin 'libcrypto-3-x64.dll'))) {
  if (-not (Test-Path -LiteralPath $file -PathType Leaf)) { throw "Runner预装OpenSSL开发原件缺失：$file" }
}
"OPENSSL_DIR=$openssl" | Out-File -FilePath $env:GITHUB_ENV -Encoding utf8 -Append
"OPENSSL_INCLUDE_DIR=$opensslInclude" | Out-File -FilePath $env:GITHUB_ENV -Encoding utf8 -Append
"OPENSSL_LIB_DIR=$opensslLib" | Out-File -FilePath $env:GITHUB_ENV -Encoding utf8 -Append
"OPENSSL_STATIC=0" | Out-File -FilePath $env:GITHUB_ENV -Encoding utf8 -Append
"PROTOC=$(Join-Path $dir 'bin\\protoc.exe')" | Out-File -FilePath $env:GITHUB_ENV -Encoding utf8 -Append
# 使用Runner现有Visual Studio官方可再分发CRT，应用本地部署，不要求客户另装VC运行库。
$vswhere = Join-Path \${env:ProgramFiles(x86)} 'Microsoft Visual Studio\\Installer\\vswhere.exe'
$vs = & $vswhere -latest -products '*' -requires Microsoft.VisualStudio.Component.VC.Tools.x86.x64 -property installationPath
if ($LASTEXITCODE -ne 0 -or -not $vs) { throw 'Runner缺少官方MSVC位置' }
$version = (Get-Content (Join-Path $vs 'VC\\Auxiliary\\Build\\Microsoft.VCRedistVersion.default.txt') -Raw).Trim()
$crt = @(Get-ChildItem -Directory (Join-Path $vs "VC\\Redist\\MSVC\\$version\\x64") -Filter 'Microsoft.VC*.CRT')
if ($crt.Count -ne 1 -or -not (Test-Path (Join-Path $crt[0].FullName 'vcruntime140.dll'))) { throw '官方x64 CRT原件不唯一或缺件' }
"CITIZENCHAIN_VC_CRT_DIR=$($crt[0].FullName)" | Out-File -FilePath $env:GITHUB_ENV -Encoding utf8 -Append
"CITIZENCHAIN_PG_DIST=$env:PGROOT" | Out-File -FilePath $env:GITHUB_ENV -Encoding utf8 -Append
"LIBCLANG_PATH=$env:PROGRAMFILES\\LLVM\\bin" | Out-File -FilePath $env:GITHUB_ENV -Encoding utf8 -Append`],process.env,root);
  }
  const compiler = platform === 'macos' ? 'osx-aarch_64' :
    platform === 'linux-arm' ? 'linux-aarch_64' : 'linux-x86_64';
  const settings = 'protoc_version='+product.tools.protoc+'\nprotoc_arch='+compiler+'\n';
  if (platform === 'macos') return runShell(settings+`# 按产品声明选择Runner内已安装的Xcode，仅设置本轮编译环境。
xcode_expected="$(node --input-type=module -e "import {product} from './scripts/config.mjs'; console.log(product.tools.xcode)")"
developer="$(cd "/Applications/Xcode_$xcode_expected.app/Contents/Developer" && pwd -P)"
export DEVELOPER_DIR="$developer"
# 完整收取版本输出，避免grep提前关闭管道令xcodebuild写入Broken pipe。
xcode_version="$(xcodebuild -version)"
printf '%s\\n' "$xcode_version"
test "\${xcode_version%%$'\\n'*}" = "Xcode $xcode_expected"
brew install bash openssl@3 bison flex
export PATH="$(brew --prefix bison)/bin:$(brew --prefix flex)/bin:$PATH"
export CPPFLAGS="-I$(brew --prefix openssl@3)/include"
export LDFLAGS="-L$(brew --prefix openssl@3)/lib"
# 与本机安装共用产品入口：先复用准确安装/缓存，缺件才从原官方来源准备。
node scripts/build.mjs postgres macos >> "$GITHUB_ENV"
`+protoc+'\n'+`libclang="$developer/Toolchains/XcodeDefault.xctoolchain/usr/lib"
test -f "$libclang/libclang.dylib"
# 编译期发现与构建程序运行时加载分别交付，保留已有搜索路径或Cargo官方默认目录。
{
  echo "LIBCLANG_PATH=$libclang"
  echo "DYLD_FALLBACK_LIBRARY_PATH=$libclang:\${DYLD_FALLBACK_LIBRARY_PATH:-$HOME/lib:/usr/local/lib:/usr/lib}"
  echo "DEVELOPER_DIR=$developer"
  echo "CC=$developer/Toolchains/XcodeDefault.xctoolchain/usr/bin/clang"
} >> "$GITHUB_ENV"`);
  const packages = platform === 'wasm' ? 'llvm' : 'build-essential patchelf bison flex'+(platform === 'linux-arm'?' curl wget':'');
  const postgres = platform === 'wasm' ? '' : `printf 'int main(void){return 0;}\\n' | cc -x c - -lpam -lxdo -o "$RUNNER_TEMP/citizenchain-link-preflight"
pkg-config --exists webkit2gtk-4.1 gtk+-3.0 openssl
pg_version="$(node --input-type=module -e "import {product} from './scripts/config.mjs'; console.log(product.tools.postgres)")"
curl --fail --location --proto =https --proto-redir =https --retry 3 --output "$RUNNER_TEMP/postgres.tar.bz2" "https://ftp.postgresql.org/pub/source/v$pg_version/postgresql-$pg_version.tar.bz2"
tar -xjf "$RUNNER_TEMP/postgres.tar.bz2" -C "$RUNNER_TEMP"
(
  cd "$RUNNER_TEMP/postgresql-$pg_version"
  ./configure --prefix="$RUNNER_TEMP/pg-dist" --without-readline --without-icu --without-zlib --with-openssl
  make -j4
  make install
  cp COPYRIGHT "$RUNNER_TEMP/pg-dist/COPYRIGHT"
)
# 系统cp跟随上游库别名生成物理副本，编译输入不带指向临时原件的链接。
cp -RL "$RUNNER_TEMP/pg-dist" "$RUNNER_TEMP/pg-runtime"
echo "CITIZENCHAIN_PG_DIST=$RUNNER_TEMP/pg-runtime" >> "$GITHUB_ENV"
echo "LIBCLANG_PATH=$(llvm-config --libdir)" >> "$GITHUB_ENV"
`;
  return runShell(settings+`sudo apt-get update
sudo apt-get install -y clang llvm-dev libclang-dev libssl-dev libwebkit2gtk-4.1-dev libgtk-3-dev libayatana-appindicator3-dev librsvg2-dev libpam0g-dev libxdo-dev pkg-config file unzip`+ ' '+packages+'\n'+postgres+'\n'+protoc);
}

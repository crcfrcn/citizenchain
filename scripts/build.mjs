import {createSecureContext} from 'node:tls';
import {spawnSync} from 'node:child_process';
import {chmodSync, copyFileSync, cpSync, existsSync, lstatSync, mkdirSync, mkdtempSync, readFileSync, readdirSync, realpathSync, renameSync, rmSync, writeFileSync} from 'node:fs';
import {basename, dirname, isAbsolute, join, relative, resolve, sep, delimiter} from 'node:path';
import {fileURLToPath} from 'node:url';
import {product} from './config.mjs';

export function developmentTLS(certificate, privateKey) {
  if (!certificate || !privateKey) throw new Error('缺少TLS证书或私钥路径');
  try {
    const options = { cert: readFileSync(certificate), key: readFileSync(privateKey), minVersion: 'TLSv1.3' };
    createSecureContext(options);
    return options;
  } catch { throw new Error('TLS证书、私钥或配对无效'); }
}

const root = realpathSync(resolve(import.meta.dirname, '..'));
const fail = message => { throw Error(message); };
function inside(parent, path) {
  const value = relative(parent, path);
  return value !== '' && value !== '..' && !value.startsWith('..' + sep) && !isAbsolute(value);
}
export function checkWork(work) {
  if (!isAbsolute(work || '') || resolve(work) !== work || !inside(join(root, 'target'), work)) fail('编译输出必须属于本产品 target');
  let parent = work;
  while (!existsSync(parent)) parent = dirname(parent);
  if (realpathSync(parent) !== parent || !lstatSync(parent).isDirectory()) fail('编译输出不能经过链接');
  return work;
}
// 编译只检查平台、版本和本轮输出目录；GitHub身份由工作流核对。
function buildVersion(value) {
  if (typeof value !== 'string' || !/^(0|[1-9][0-9]*)\.(0|[1-9][0-9]*)\.(0|[1-9][0-9]*)$/u.test(value)
      || value.split('.').some(part => BigInt(part) >= 2n ** 64n)) fail('编译版本必须是规范X.Y.Z');
  return value;
}
export function assertBuildWork(work, platform, env = process.env) {
  if (!product.platforms[platform] || work !== join(root, 'target/build', platform)) fail('编译平台或输出目录无效');
  buildVersion(env.CITIZENCHAIN_RELEASE_VERSION);
  return checkWork(work);
}
// 只执行真实编译工具；继承输出或捕获模块 --describe 的有界公开结果。
export async function runBuildProcess(command, args, env, cwd, {capture = false, timeout, diagnostics = false} = {}) {
  const value = spawnSync(command, args, {env, cwd, shell: false, encoding: 'utf8',
    // 本机长编译诊断走同一公开日志流，避免安装器有界stderr仅留下前段而丢失最终错误。
    stdio: capture ? ['ignore', 'pipe', 'pipe'] : diagnostics ? ['ignore', 'inherit', 1] : 'inherit', maxBuffer: 16 * 1024 * 1024, timeout});
  if (value.error || value.status !== 0 || value.signal) {
    const error = Error(`${basename(command)} 执行失败，退出码 ${value.status}`);
    // 信号、超时或输出管道异常不能证明后代工具已停止，调用方须保留其现场。
    error.retainWork = Boolean(value.signal || value.error && value.error.code !== 'ENOENT');
    throw error;
  }
  return {stdout: value.stdout || '', stderr: value.stderr || ''};
}
export function nodeTools(node = process.execPath) {
  // 控制台内置Node不含npm；复用设备上同一声明版本的完整官方Node安装。
  const nodes = [node, ...String(process.env.PATH || '').split(delimiter).filter(isAbsolute).map(bin => join(bin, process.platform === 'win32' ? 'node.exe' : 'node')),
    join(process.env.HOME || '', '.nvm/versions/node/v' + product.tools.node + '/bin/node')];
  for (const candidate of [...new Set(nodes)]) {
    if (!isAbsolute(candidate) || !existsSync(candidate)) continue;
    const bin = dirname(realpathSync(candidate));
    const path = process.platform === 'win32' ? join(bin, 'node_modules/npm/bin/npm-cli.js') : join(bin, '../lib/node_modules/npm/bin/npm-cli.js');
    if (!existsSync(path)) continue;
    const version = spawnSync(candidate, ['--version'], {encoding: 'utf8'});
    if (version.status === 0 && version.stdout.trim() === 'v' + product.tools.node) return {node: realpathSync(candidate), npm: realpathSync(path)};
  }
  fail('缺少声明版本Node ' + product.tools.node + '随附的npm');
}
export function npmCLI(node = process.execPath) { return nodeTools(node).npm; }
export function platformCargoEnvironment(platform, work, environment) {
  const env = {...environment, NODE: environment.NODE || process.execPath, CARGO: environment.CARGO || 'cargo', CARGO_INCREMENTAL: '1',
    CARGO_TARGET_DIR: join(work, 'cargo-target'), CITIZENCHAIN_SOURCE_ROOT: root, CITIZENCHAIN_WORK_DIR: work,
    CITIZENCHAIN_BUILD_PLATFORM: platform};
  for (const key of Object.keys(env)) if (key.startsWith('APPLE_') || /(?:^|_)(?:TOKEN|PASSWORD|PRIVATE_KEY|SECRET)$/u.test(key) || ['GH_TOKEN', 'GITHUB_TOKEN', 'NODE_OPTIONS', 'NODE_PATH', 'NODE_TEST_CONTEXT', 'WASM_FILE', 'DOCS_RS', 'SKIP_WASM_BUILD'].includes(key) || /^SKIP_.+_WASM_BUILD$/u.test(key)) delete env[key];
  // 节点/模块不构建WASM，SDK依赖的WASM构建保持关闭。
  env.SKIP_WASM_BUILD = '1';
  // rust-lang/rust#157750：macOS 27 会拒绝 Rust 剥离调试信息后未对齐的过程宏 dylib。
  if (platform === 'macos') env.CARGO_PROFILE_RELEASE_STRIP = 'none';
  if(platform==='windows'){
    const searchPath=environment.PATH||environment.Path;
    for(const key of Object.keys(env))if(key.toUpperCase()==='PATH')delete env[key];
    env.PATH=(isAbsolute(env.OPENSSL_DIR||'')?join(env.OPENSSL_DIR,'bin')+delimiter:'')+searchPath;
  }
  return env;
}
export function stageWindowsOpenSsl(_resources, env, directories) {
  if(!isAbsolute(env.CITIZENCHAIN_VC_CRT_DIR||'')||!existsSync(join(env.CITIZENCHAIN_VC_CRT_DIR,'vcruntime140.dll')))fail('缺少官方x64可再分发CRT');
  for(const name of readdirSync(env.CITIZENCHAIN_VC_CRT_DIR).filter(name=>name.endsWith('.dll')))for(const directory of directories)copyFileSync(join(env.CITIZENCHAIN_VC_CRT_DIR,name),join(directory,name));
  if (!isAbsolute(env.OPENSSL_DIR || '')) fail('缺少 Windows OpenSSL 开发库');
  for (const name of ['libssl-3-x64.dll', 'libcrypto-3-x64.dll']) {
    const source = join(env.OPENSSL_DIR, 'bin', name), bytes = readFileSync(source), at = bytes.length >= 64 ? bytes.readUInt32LE(60) : -1;
    if (bytes.toString('ascii', 0, 2) !== 'MZ' || at < 0 || at > bytes.length - 24 || bytes.readUInt32LE(at) !== 0x4550 || bytes.readUInt16LE(at + 4) !== 0x8664) fail('OpenSSL DLL 架构无效');
    for (const directory of directories) copyFileSync(source, join(directory, name));
  }
}
// Node 25.2.1 的目录 cpSync 即使启用 dereference 仍会保留部分 dylib 链接。
// 显式解析官方 PostgreSQL 产物内的链接并复制文件，禁止越界与目录循环。
export function stagePostgresResources(source, destination) {
  const sourceRoot = realpathSync(source);
  function copy(path, output, ancestors = new Set()) {
    const resolved = realpathSync(path);
    if (resolved !== sourceRoot && !inside(sourceRoot, resolved)) fail('PostgreSQL运行资源链接越界');
    const info = lstatSync(resolved);
    if (info.isDirectory()) {
      if (ancestors.has(resolved)) fail('PostgreSQL运行资源存在目录循环');
      const parents = new Set([...ancestors, resolved]);
      mkdirSync(output, {recursive: true});
      for (const name of readdirSync(resolved)) copy(join(resolved, name), join(output, name), parents);
    } else if (info.isFile()) {
      copyFileSync(resolved, output);
    } else fail('PostgreSQL运行资源必须是普通文件或目录');
  }
  for (const name of ['bin', 'lib', 'share']) copy(join(sourceRoot, name), join(destination, name));
}
// Homebrew保留上游版本并附加发行标识；只接受完整版本，不能以17匹配17.11。
export function postgresVersion(output) {
  return /^postgres \(PostgreSQL\) (\d+\.\d+(?:\.\d+)?)(?: \(Homebrew\))?$/u.exec(output.trim())?.[1];
}
// 本机安装与macOS Workflow共用资源准备；已有准确版本优先，缺件才访问原官方来源。
export async function preparePostgres(work, environment, {run = runBuildProcess, candidates, cache = join(root, 'target/cache/macos', 'postgresql-' + product.tools.postgres)} = {}) {
  const version = product.tools.postgres;
  const invoke = (file, args, cwd = work) => run(file, args, environment, cwd, {capture: true});
  const complete = path => ['bin/postgres', 'bin/initdb', 'lib', 'share/postgresql/postgres.bki', 'COPYRIGHT'].every(name => existsSync(join(path, name)));
  const matches = async path => complete(path) && postgresVersion((await invoke(join(path, 'bin/postgres'), ['--version'])).stdout) === version;
  if (environment.CITIZENCHAIN_PG_DIST) {
    const specified = environment.CITIZENCHAIN_PG_DIST;
    if (!isAbsolute(specified) || !await matches(specified)) fail('指定PostgreSQL目录不完整或版本不符');
    return realpathSync(specified);
  }
  const installed = candidates ?? [
    ...String(environment.PATH || '').split(delimiter).filter(isAbsolute).map(bin => dirname(bin)),
    '/opt/homebrew/opt/postgresql@' + version.split('.')[0],
    '/usr/local/opt/postgresql@' + version.split('.')[0],
  ];
  for (const path of [...new Set([...installed, cache])]) {
    if (await matches(path)) { process.stderr.write('复用PostgreSQL ' + version + '：' + path + '\n'); return realpathSync(path); }
  }
  if (existsSync(cache)) fail('PostgreSQL缓存已存在但不可复用，请检查后重试');
  mkdirSync(dirname(cache), {recursive: true});
  const lock = cache + '.lock';
  mkdirSync(lock); // 同一产品缓存只允许一个准备者，不能覆盖其它活动任务。
  let scratch;
  try {
    scratch = mkdtempSync(join(work, 'postgres-'));
    const archive = cache + '.tar.bz2';
    if (!existsSync(archive)) {
      const partial = join(scratch, 'postgres.tar.bz2');
      await invoke('/usr/bin/curl', ['--fail', '--location', '--proto', '=https', '--proto-redir', '=https', '--output', partial,
        'https://ftp.postgresql.org/pub/source/v' + version + '/postgresql-' + version + '.tar.bz2']);
      renameSync(partial, archive);
    }
    await invoke('/usr/bin/tar', ['-xjf', archive, '-C', scratch]);
    const source = join(scratch, 'postgresql-' + version), distribution = join(scratch, 'dist');
    // 固定资源目录，不让上游按临时路径中的postgres字样改变安装布局。
    await invoke(join(source, 'configure'), ['--prefix=' + distribution, '--datadir=' + join(distribution, 'share/postgresql'), '--without-readline', '--without-icu', '--without-zlib', '--with-openssl'], source);
    await invoke('/usr/bin/make', ['-j4'], source);
    await invoke('/usr/bin/make', ['install'], source);
    copyFileSync(join(source, 'COPYRIGHT'), join(distribution, 'COPYRIGHT'));
    if (!await matches(distribution)) fail('新准备的PostgreSQL不完整或版本不符');
    if (existsSync(cache)) fail('PostgreSQL缓存已存在但不可复用，请检查后重试');
    renameSync(distribution, cache);
    return cache;
  } finally { if (scratch) rmSync(scratch, {recursive: true, force: true}); rmSync(lock, {recursive: true}); }
}
function walk(path) {
  return readdirSync(path).flatMap(name => {
    const file = join(path, name), info = lstatSync(file);
    if (info.isSymbolicLink()) fail('打包运行资源不能带链接');
    return info.isDirectory() ? walk(file) : [file];
  });
}
function oneBundle(path, extension) {
  const files = readdirSync(path).filter(name => name.endsWith(extension));
  if (files.length !== 1) fail('本平台必须生成唯一完整安装包');
  return join(path, files[0]);
}
// 安装包必需的动态库按实际链接修正，仅处理本轮运行资源，不建立工具库存。
export async function macLibraries(resources, executable, env) {
  const library = join(resources, 'postgres/macos/lib'), queue = [...walk(resources), executable];
  const processed = new Set(), originals = new Map(), sources = new Map(), binaries = [];
  while (queue.length) {
    const file = queue.shift();
    if (processed.has(file)) continue;
    processed.add(file);
    const bytes = readFileSync(file);
    if (bytes.length < 4 || ![0xfeedfacf, 0xcffaedfe, 0xcafebabe, 0xbebafeca].includes(bytes.readUInt32LE(0))) continue;
    if (inside(library, file)) originals.set(file, bytes);
    if (file !== executable) binaries.push(file);
    const ownId = (await runBuildProcess('/usr/bin/otool', ['-D', file], env, root, {capture: true})).stdout.trim().split('\n')[1];
    const links = (await runBuildProcess('/usr/bin/otool', ['-L', file], env, root, {capture: true})).stdout.split('\n').slice(1);
    for (const line of links) {
      const dependency = /^\s+(.+?) \(compatibility version /u.exec(line)?.[1];
      if (!dependency || dependency === ownId || dependency.startsWith('/usr/lib/') || dependency.startsWith('/System/Library/')) continue;
      let source = dependency;
      if (dependency.startsWith('@loader_path/')) {
        if (existsSync(resolve(dirname(file), dependency.slice(13)))) continue;
        // 新复制的官方库仍可能引用原目录中的相邻库，按实际原件递归收齐后再改本轮副本。
        const original = sources.get(file);
        if (!original) fail('随包动态库缺失：' + file + ' -> ' + dependency);
        source = resolve(dirname(original), dependency.slice(13));
      }
      if (!isAbsolute(source) || !existsSync(source)) fail('动态库来源无法明确定位：' + file + ' -> ' + dependency);
      const destination = join(library, basename(source));
      if (!existsSync(destination)) {
        const original = realpathSync(source);
        copyFileSync(original, destination); sources.set(destination, original);
      } else if (!(originals.get(destination) || readFileSync(destination)).equals(readFileSync(realpathSync(source)))) fail('随包动态库重名冲突');
      queue.push(destination);
      const reference = file === executable ? '@executable_path/../Resources/postgres/macos/lib/' + basename(destination) : '@loader_path/' + relative(dirname(file), destination);
      await runBuildProcess('/usr/bin/install_name_tool', ['-change', dependency, reference, file], env, root, {capture: true});
    }
    if (ownId) await runBuildProcess('/usr/bin/install_name_tool', ['-id', '@loader_path/' + basename(file), file], env, root, {capture: true});
  }
  return binaries;
}
async function signResources(files, work, env) {
  const certificate = join(work, 'certificate.p12'), keychain = join(work, 'signing.keychain-db');
  if (!env.APPLE_CERTIFICATE || !env.APPLE_CERTIFICATE_PASSWORD) fail('缺少 Developer ID 签名材料');
  writeFileSync(certificate, Buffer.from(env.APPLE_CERTIFICATE, 'base64'), {mode: 0o600, flag: 'wx'});
  const security = args => runBuildProcess('/usr/bin/security', args, env, root, {capture: true});
  try {
    await security(['create-keychain', '-p', env.APPLE_CERTIFICATE_PASSWORD, keychain]);
    await security(['unlock-keychain', '-p', env.APPLE_CERTIFICATE_PASSWORD, keychain]);
    await security(['import', certificate, '-k', keychain, '-P', env.APPLE_CERTIFICATE_PASSWORD, '-T', '/usr/bin/codesign']);
    await security(['set-key-partition-list', '-S', 'apple-tool:,apple:,codesign:', '-s', '-k', env.APPLE_CERTIFICATE_PASSWORD, keychain]);
    for (const file of files) await runBuildProcess('/usr/bin/codesign', ['--force', '--timestamp', '--options', 'runtime', '--keychain', keychain, '--sign', env.APPLE_SIGNING_IDENTITY, file], env, root, {capture: true});
  } finally {
    rmSync(certificate, {force: true});
    if (existsSync(keychain)) await security(['delete-keychain', keychain]);
  }
}
export async function buildPlatform(platform,{work=join(root,'target/build',platform),local=false,consume}={}) {
  if (platform === 'wasm') fail('Runtime请使用 scripts/runtime.mjs build');
  const declared = product.platforms[platform];
  if(local)checkWork(work);else assertBuildWork(work, platform);
  if (process.platform !== declared.host || process.arch !== declared.arch) fail('必须在声明对应的原生 Runner 编译');
  if (process.versions.node !== product.tools.node) fail('Node 版本不符合产品声明');
  mkdirSync(work, {recursive: true});
  const env = platformCargoEnvironment(platform, work, process.env), run = (command, args, options) => runBuildProcess(command, args, env, root, options);
  const cargoVersion = await run(env.CARGO, ['--version'], {capture: true});
  if (!cargoVersion.stdout.startsWith('cargo ' + product.tools.rust + ' ')) fail('Rust 版本不符合产品声明');
  if ((await run(env.PROTOC, ['--version'], {capture: true})).stdout.trim() !== 'libprotoc ' + product.tools.protoc) fail('protoc 版本不符合产品声明');
  const expected = [declared.artifact.replace('{version}', buildVersion(process.env.CITIZENCHAIN_RELEASE_VERSION))];
  const artifact = join(work, 'artifact');
  if (existsSync(artifact)) fail('本轮产物目录已存在，禁止混用旧包');
  mkdirSync(artifact);
  {
    const {buildPreparedOnChina} = await import('./onchina.mjs');
    const onchina = await buildPreparedOnChina({project: root, platform, work, environment: env, prepareDependencies: local});
    const resources = join(work, 'resources'), pgOs = platform === 'macos' ? 'macos' : platform === 'windows' ? 'windows' : 'linux';
    mkdirSync(join(resources, 'onchina-bin'), {recursive: true});
    copyFileSync(onchina.host, join(resources, 'onchina-bin', basename(onchina.host)));
    cpSync(onchina.modules, join(resources, 'onchina-bin/modules'), {recursive: true});
    cpSync(onchina.frontend, join(resources, 'onchina-frontend/dist'), {recursive: true});
    const pg = process.env.CITIZENCHAIN_PG_DIST;
    if (!isAbsolute(pg || '')) fail('缺少完整 PostgreSQL 原件');
    if (postgresVersion((await run(join(pg, 'bin', platform === 'windows' ? 'postgres.exe' : 'postgres'), ['--version'], {capture: true})).stdout) !== product.tools.postgres) fail('PostgreSQL 版本不符');
    stagePostgresResources(pg, join(resources, 'postgres', pgOs));
    for(const name of ['COPYRIGHT','LICENSE','license.txt'])if(existsSync(join(pg,name)))copyFileSync(join(pg,name),join(resources,'postgres',pgOs,name));
    if (platform === 'windows') stageWindowsOpenSsl(resources, env, [resources, join(resources, 'onchina-bin'), join(resources,'postgres/windows/bin')]);
    else for (const file of walk(resources)) chmodSync(file, lstatSync(file).mode & 0o111 ? 0o755 : 0o644);
    env.CITIZENCHAIN_FRONTEND_DIST = join(work, 'node-frontend');
    await runBuildProcess(env.NODE, [npmCLI(env.NODE), 'run', 'build'], env, join(root, 'node/frontend'));
    const config = {version: process.env.CITIZENCHAIN_RELEASE_VERSION, build: {beforeBuildCommand: null, frontendDist: env.CITIZENCHAIN_FRONTEND_DIST},
      bundle: {resources: {[resources + sep]: '', [join(root, 'icons') + sep]: 'icons/', [join(root, 'onchina/platform/codes/china.sqlite')]: 'china.sqlite'}}};
    if (platform === 'macos' && !local) {
      if (!process.env.APPLE_PROFILE_BASE64 || !process.env.APPLE_SIGNING_IDENTITY || !process.env.APPLE_ID || !process.env.APPLE_PASSWORD || !process.env.APPLE_TEAM_ID) fail('缺少 macOS 正式签名、公证输入');
      const profile = join(work, 'embedded.provisionprofile');
      writeFileSync(profile, Buffer.from(process.env.APPLE_PROFILE_BASE64, 'base64'), {mode: 0o600, flag: 'wx'});
      config.bundle.macOS = {files: {'embedded.provisionprofile': profile}, hardenedRuntime: true, signingIdentity: process.env.APPLE_SIGNING_IDENTITY};
    }
    const tauri = join(root, 'node/frontend/node_modules/@tauri-apps/cli/tauri.js');
    await runBuildProcess(env.NODE, [tauri, 'build', '--config', JSON.stringify(config), '--no-bundle', '--ci', '--', '--locked', '--config', join(root, 'config.toml')], env, join(root, 'node'), {diagnostics: local});
    const signing = {...env, ...Object.fromEntries(Object.entries(process.env).filter(([key]) => key.startsWith('APPLE_')))};
    if (platform === 'macos') {
      const binaries = await macLibraries(resources, join(env.CARGO_TARGET_DIR, 'release/citizenchain'), env);
      if (!local) await signResources(binaries, work, signing);
    }
    if(local){
      if(platform!=='macos')fail('当前本机编译只支持macOS');
      config.bundle.macOS={hardenedRuntime:true,signingIdentity:null};
      await runBuildProcess(env.NODE,[tauri,'bundle','--config',JSON.stringify(config),'--bundles','app','--ci'],env,join(root,'node'));
      const app=oneBundle(join(env.CARGO_TARGET_DIR,'release/bundle/macos'),'.app');
      await consume({schema:1,product_id:product.id,platform,work,artifact:app,tools:{}});return;
    }
    if (platform.startsWith('linux-')) for (const file of walk(join(resources, 'postgres/linux'))) {
      if (readFileSync(file).subarray(0, 4).equals(Buffer.from([127, 69, 76, 70]))) await run('patchelf', ['--set-rpath', '$ORIGIN:$ORIGIN/..:$ORIGIN/../lib', file], {capture: true});
    }
    await run(join(resources, 'postgres', pgOs, 'bin', platform === 'windows' ? 'initdb.exe' : 'initdb'), ['-D', join(work, 'pg-probe'), '-U', 'citizenchain', '--auth-local=trust', '--auth-host=trust', '--encoding=UTF8', '--no-locale']);
    // 按Tauri官方目标保留macOS App用于签名、公证核对，最终仅交付DMG。
    await runBuildProcess(env.NODE, [tauri, 'bundle', '--config', JSON.stringify(config), '--bundles', platform === 'macos' ? 'app,dmg' : declared.bundle, '--ci'], platform === 'macos' ? signing : env, join(root, 'node'));
    const installer = oneBundle(join(env.CARGO_TARGET_DIR, 'release/bundle', declared.bundle), declared.extension);
    if (platform === 'macos') {
      const app = oneBundle(join(env.CARGO_TARGET_DIR, 'release/bundle/macos'), '.app');
      await run('/usr/bin/codesign', ['--verify', '--deep', '--strict', app]);
      const identity = (await run('/usr/bin/codesign', ['-dv', '--verbose=4', app], {capture: true})).stderr;
      if (!identity.includes('Authority=' + process.env.APPLE_SIGNING_IDENTITY) || !identity.includes('TeamIdentifier=' + process.env.APPLE_TEAM_ID) || !/^Timestamp=/mu.test(identity)) fail('App 签名身份、团队或时间戳不符');
      await run('/usr/bin/xcrun', ['stapler', 'validate', app]);
      await run('/usr/bin/hdiutil',['verify',installer]);
    }
    if (platform.startsWith('linux-')) {
      const architecture = platform === 'linux-arm' ? 'arm64' : 'amd64';
      if ((await run('dpkg-deb', ['--field', installer, 'Architecture'], {capture: true})).stdout.trim() !== architecture) fail('DEB 架构不符');
      const installed = join(work, 'installed');
      await run('dpkg-deb', ['--extract', installer, installed]);
      for (const name of ['onchina-bin/onchina', 'onchina-frontend/dist/index.html', 'postgres/linux/bin/initdb', 'postgres/linux/bin/postgres', 'china.sqlite']) if (!lstatSync(join(installed, 'usr/lib/citizenchain', name)).isFile()) fail('DEB 缺少完整运行资源');
    }
    if(platform.startsWith('linux-'))for(const id of readdirSync(onchina.modules))for(const name of ['onchina-'+id,'frontend/module.js','frontend/module.css'])if(!lstatSync(join(work,'installed/usr/lib/citizenchain/onchina-bin/modules',id,name)).isFile())fail('DEB模块完整产物缺失');
    copyFileSync(installer, join(artifact, expected[0]));
  }
  if (JSON.stringify(readdirSync(artifact).sort()) !== JSON.stringify(expected.sort())) fail('完整产物集合不符');
  for (const name of expected) {
    const info = lstatSync(join(artifact, name));
    if (!info.isFile() || info.isSymbolicLink() || info.size === 0) fail('正式产物不是非空普通文件');
  }
  process.stdout.write('完整编译产物：' + artifact + '\n');
}
// 本机编译复用同一编译实现，并向安装器交付本轮产物。
async function compile(platform){
 if(platform!=='macos'||process.platform!=='darwin'||process.arch!=='arm64')fail('本机公民链编译只支持ARM64 macOS');
 const parent=join(root,'target/build');mkdirSync(parent,{recursive:true});checkWork(join(parent,'candidate'));
 const work=mkdtempSync(join(parent,'local-'));
 const environment={...process.env,PATH:process.env.HOME+'/.cargo/bin:'+process.env.HOME+'/.nvm/versions/node/v'+product.tools.node+'/bin:/opt/homebrew/bin:'+(process.env.PATH||'/usr/bin:/bin'),
  NODE:nodeTools().node,CARGO:'cargo',CARGO_HOME:process.env.HOME+'/.cargo',RUSTUP_TOOLCHAIN:product.tools.rust,
  CARGO_NET_OFFLINE:'true',GIT_ALLOW_PROTOCOL:'file',
  CITIZENCHAIN_RELEASE_VERSION:/^version\s*=\s*"([^"]+)"/mu.exec(readFileSync(join(root,'Cargo.toml'),'utf8'))?.[1]};
 buildVersion(environment.CITIZENCHAIN_RELEASE_VERSION);
 let safe=true;
 try{
  const run=(file,args,options)=>runBuildProcess(file,args,environment,work,options);
  const developer=(await run('/usr/bin/xcode-select',['-p'],{capture:true})).stdout.trim();
  environment.DEVELOPER_DIR=developer;environment.LIBCLANG_PATH=join(developer,'Toolchains/XcodeDefault.xctoolchain/usr/lib');
  environment.DYLD_FALLBACK_LIBRARY_PATH=environment.LIBCLANG_PATH+':/usr/lib';
  // 沿用本产品GitHub已经使用的官方来源、版本和编译选项；不安装替代版本。
  const openssl=(await run('/opt/homebrew/bin/brew',['--prefix','openssl@3'],{capture:true})).stdout.trim();
  environment.CPPFLAGS='-I'+join(openssl,'include');environment.LDFLAGS='-L'+join(openssl,'lib');
  for(const name of ['bison','flex'])environment.PATH=(await run('/opt/homebrew/bin/brew',['--prefix',name],{capture:true})).stdout.trim()+'/bin:'+environment.PATH;
  environment.CITIZENCHAIN_PG_DIST=await preparePostgres(work,environment);
  // 本机复用已安装的声明版本，不从GitHub取得工具。
  environment.PROTOC=process.env.PROTOC||'protoc';
  if((await run(environment.PROTOC,['--version'],{capture:true})).stdout.trim()!=='libprotoc '+product.tools.protoc)fail('本机需要已安装的protoc '+product.tools.protoc);
  await runBuildProcess(environment.NODE,[npmCLI(environment.NODE),'ci','--offline','--ignore-scripts','--no-audit','--no-fund'],environment,join(root,'node/frontend'));
  const previous=process.env;process.env=environment;
  try{await buildPlatform(platform,{work,local:true,consume:async value=>{
    process.stdout.write('TATA_COMPILE '+JSON.stringify(value)+'\n');
    for await(const part of process.stdin)if(part.length)fail('编译消费通道只接受EOF');
  }});}finally{process.env=previous;}
 }catch(error){safe=!(error.retainWork || process.env.CITIZENCHAIN_RETAIN_FAILED_BUILD==='1');throw error;}finally{if(safe)rmSync(work,{recursive:true,force:true});}
}
if (process.argv[1] === fileURLToPath(import.meta.url)) {
 const args=process.argv.slice(2);
 const operation=args.length===2&&args[0]==='compile'?compile(args[1]):args.length===2&&args[0]==='postgres'&&args[1]==='macos'
  ? preparePostgres(process.env.RUNNER_TEMP || root,process.env).then(path=>process.stdout.write('CITIZENCHAIN_PG_DIST='+path+'\n'))
  :args.length===1?buildPlatform(args[0]):Promise.reject(Error('编译参数无效'));
 operation.catch(error => { process.stderr.write(error.message + '\n'); process.exitCode = 1; });
}

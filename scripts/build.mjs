#!/usr/bin/env node
// 仅供公民链 GitHub Runner 编译、组装及核对完整产物；不负责自动化管理或发布。
import {spawnSync} from 'node:child_process';
import {chmodSync, copyFileSync, cpSync, existsSync, lstatSync, mkdirSync, readFileSync, readdirSync, realpathSync, rmSync, writeFileSync} from 'node:fs';
import {basename, dirname, isAbsolute, join, relative, resolve, sep, delimiter} from 'node:path';
import {fileURLToPath} from 'node:url';
import {product, artifactNames, deliveryVersion} from './config.mjs';

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
export function assertBuildWork(work, platform, env = process.env) {
  const declared = product.platforms[platform];
  if (!Object.hasOwn(product.platforms,platform) || env.GITHUB_ACTIONS !== 'true' || env.GITHUB_REPOSITORY !== product.repository
      || env.GITHUB_EVENT_NAME !== 'workflow_dispatch' || env.GITHUB_REF !== 'refs/heads/main'
      || env.GITHUB_WORKFLOW_REF !== `${product.repository}/.github/workflows/${declared.workflow}@refs/heads/main`
      || !/^[a-f0-9]{40}$/u.test(env.GITHUB_SHA || '') || env.CITIZENCHAIN_SOURCE_SHA !== env.GITHUB_SHA
      || work !== join(root, 'target/build', platform)) fail('编译只接受本产品准确平台的 GitHub main 任务');
  deliveryVersion(env.CITIZENCHAIN_RELEASE_VERSION);
  checkWork(work);
}
// 只执行真实编译工具；继承输出或捕获模块 --describe 的有界公开结果。
export async function runBuildProcess(command, args, env, cwd, {capture = false, timeout} = {}) {
  const value = spawnSync(command, args, {env, cwd, shell: false, encoding: 'utf8',
    stdio: capture ? ['ignore', 'pipe', 'pipe'] : 'inherit', maxBuffer: 16 * 1024 * 1024, timeout});
  if (value.error || value.status !== 0 || value.signal) {
    const error = Error(`${basename(command)} 执行失败，退出码 ${value.status}`);
    // 信号、超时或输出管道异常不能证明后代工具已停止，调用方须保留其现场。
    error.retainWork = Boolean(value.signal || value.error && value.error.code !== 'ENOENT');
    throw error;
  }
  return {stdout: value.stdout || '', stderr: value.stderr || ''};
}
export function npmCLI(node = process.execPath) {
  const bin = dirname(realpathSync(node));
  const path = process.platform === 'win32' ? join(bin, 'node_modules/npm/bin/npm-cli.js') : join(bin, '../lib/node_modules/npm/bin/npm-cli.js');
  return realpathSync(path);
}
export function platformCargoEnvironment(platform, work, environment) {
  const env = {...environment, NODE: process.execPath, CARGO: environment.CARGO || 'cargo', CARGO_INCREMENTAL: '1',
    CARGO_TARGET_DIR: join(work, 'cargo-target'), CITIZENCHAIN_SOURCE_ROOT: root, CITIZENCHAIN_WORK_DIR: work,
    CITIZENCHAIN_BUILD_PLATFORM: platform};
  for (const key of Object.keys(env)) if (key.startsWith('APPLE_') || /(?:^|_)(?:TOKEN|PASSWORD|PRIVATE_KEY|SECRET)$/u.test(key) || ['GH_TOKEN', 'GITHUB_TOKEN', 'NODE_OPTIONS', 'NODE_PATH', 'NODE_TEST_CONTEXT', 'WASM_FILE', 'DOCS_RS', 'SKIP_WASM_BUILD', 'WASM_BUILD_FROM_SOURCE', 'CITIZENCHAIN_PRODUCTION_WASM_BUILD'].includes(key) || /^SKIP_.+_WASM_BUILD$/u.test(key)) delete env[key];
  // 节点/模块构建不自行编译WASM；Runtime专属入口按其模式显式开启。
  env.SKIP_WASM_BUILD = '1';
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
async function macLibraries(resources, executable, env) {
  const library = join(resources, 'postgres/macos/lib'), queue = [...walk(resources), executable];
  const processed = new Set(), originals = new Map(), binaries = [];
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
      if (dependency.startsWith('@loader_path/')) {
        if (!existsSync(resolve(dirname(file), dependency.slice(13)))) fail('随包动态库缺失');
        continue;
      }
      if (!isAbsolute(dependency) || !existsSync(dependency)) fail('动态库来源无法明确定位');
      const destination = join(library, basename(dependency));
      if (!existsSync(destination)) copyFileSync(realpathSync(dependency), destination);
      else if (!(originals.get(destination) || readFileSync(destination)).equals(readFileSync(realpathSync(dependency)))) fail('随包动态库重名冲突');
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
async function compile(platform) {
  if (platform === 'wasm') fail('Runtime请使用 scripts/runtime.mjs build');
  const work = join(root, 'target/build', platform), declared = product.platforms[platform];
  assertBuildWork(work, platform);
  if (process.platform !== declared.host || process.arch !== declared.arch) fail('必须在声明对应的原生 Runner 编译');
  if (process.versions.node !== product.tools.node) fail('Node 版本不符合产品声明');
  mkdirSync(work, {recursive: true});
  const env = platformCargoEnvironment(platform, work, process.env), run = (command, args, options) => runBuildProcess(command, args, env, root, options);
  const cargoVersion = await run(env.CARGO, ['--version'], {capture: true});
  if (!cargoVersion.stdout.startsWith('cargo ' + product.tools.rust + ' ')) fail('Rust 版本不符合产品声明');
  if ((await run(env.PROTOC, ['--version'], {capture: true})).stdout.trim() !== 'libprotoc ' + product.tools.protoc) fail('protoc 版本不符合产品声明');
  const artifact = join(work, 'artifact');
  if (existsSync(artifact)) fail('本轮产物目录已存在，禁止混用旧包');
  mkdirSync(artifact);
  {
    const {buildPreparedOnChina} = await import('./onchina.mjs');
    const onchina = await buildPreparedOnChina({project: root, platform, work, environment: env, prepareDependencies: false});
    const resources = join(work, 'resources'), pgOs = platform === 'macos' ? 'macos' : platform === 'windows' ? 'windows' : 'linux';
    mkdirSync(join(resources, 'onchina-bin'), {recursive: true});
    copyFileSync(onchina.host, join(resources, 'onchina-bin', basename(onchina.host)));
    cpSync(onchina.modules, join(resources, 'onchina-bin/modules'), {recursive: true});
    cpSync(onchina.frontend, join(resources, 'onchina-frontend/dist'), {recursive: true});
    const pg = process.env.CITIZENCHAIN_PG_DIST;
    if (!isAbsolute(pg || '')) fail('缺少完整 PostgreSQL 原件');
    if ((await run(join(pg, 'bin', platform === 'windows' ? 'postgres.exe' : 'postgres'), ['--version'], {capture: true})).stdout.trim() !== 'postgres (PostgreSQL) ' + product.tools.postgres) fail('PostgreSQL 版本不符');
    for(const name of ['bin','lib','share'])cpSync(join(pg,name),join(resources,'postgres',pgOs,name),{recursive:true,dereference:true});
    for(const name of ['COPYRIGHT','LICENSE','license.txt'])if(existsSync(join(pg,name)))copyFileSync(join(pg,name),join(resources,'postgres',pgOs,name));
    if (platform === 'windows') stageWindowsOpenSsl(resources, env, [resources, join(resources, 'onchina-bin'), join(resources,'postgres/windows/bin')]);
    else for (const file of walk(resources)) chmodSync(file, lstatSync(file).mode & 0o111 ? 0o755 : 0o644);
    env.CITIZENCHAIN_FRONTEND_DIST = join(work, 'node-frontend');
    await runBuildProcess(process.execPath, [npmCLI(), 'run', 'build'], env, join(root, 'node/frontend'));
    const config = {version: process.env.CITIZENCHAIN_RELEASE_VERSION, build: {beforeBuildCommand: null, frontendDist: env.CITIZENCHAIN_FRONTEND_DIST},
      bundle: {resources: {[resources + sep]: '', [join(root, 'icons') + sep]: 'icons/', [join(root, 'onchina/platform/codes/china.sqlite')]: 'china.sqlite'}}};
    if (platform === 'macos') {
      if (!process.env.APPLE_PROFILE_BASE64 || !process.env.APPLE_SIGNING_IDENTITY || !process.env.APPLE_ID || !process.env.APPLE_PASSWORD || !process.env.APPLE_TEAM_ID) fail('缺少 macOS 正式签名、公证输入');
      const profile = join(work, 'embedded.provisionprofile');
      writeFileSync(profile, Buffer.from(process.env.APPLE_PROFILE_BASE64, 'base64'), {mode: 0o600, flag: 'wx'});
      config.bundle.macOS = {files: {'embedded.provisionprofile': profile}, hardenedRuntime: true, signingIdentity: process.env.APPLE_SIGNING_IDENTITY};
    }
    const tauri = join(root, 'node/frontend/node_modules/@tauri-apps/cli/tauri.js');
    await runBuildProcess(process.execPath, [tauri, 'build', '--config', JSON.stringify(config), '--no-bundle', '--ci', '--', '--locked', '--config', join(root, 'config.toml')], env, join(root, 'node'));
    const signing = {...env, ...Object.fromEntries(Object.entries(process.env).filter(([key]) => key.startsWith('APPLE_')))};
    if (platform === 'macos') await signResources(await macLibraries(resources, join(env.CARGO_TARGET_DIR, 'release/citizenchain'), env), work, signing);
    if (platform.startsWith('linux-')) for (const file of walk(join(resources, 'postgres/linux'))) {
      if (readFileSync(file).subarray(0, 4).equals(Buffer.from([127, 69, 76, 70]))) await run('patchelf', ['--set-rpath', '$ORIGIN:$ORIGIN/..:$ORIGIN/../lib', file], {capture: true});
    }
    await run(join(resources, 'postgres', pgOs, 'bin', platform === 'windows' ? 'initdb.exe' : 'initdb'), ['-D', join(work, 'pg-probe'), '-U', 'citizenchain', '--auth-local=trust', '--auth-host=trust', '--encoding=UTF8', '--no-locale']);
    await runBuildProcess(process.execPath, [tauri, 'bundle', '--config', JSON.stringify(config), '--bundles', declared.bundle, '--ci'], platform === 'macos' ? signing : env, join(root, 'node'));
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
    copyFileSync(installer, join(artifact, artifactNames(platform, process.env.CITIZENCHAIN_RELEASE_VERSION)[0]));
  }
  const expected = artifactNames(platform, process.env.CITIZENCHAIN_RELEASE_VERSION);
  if (JSON.stringify(readdirSync(artifact).sort()) !== JSON.stringify(expected.sort())) fail('完整产物集合不符');
  for (const name of expected) {
    const info = lstatSync(join(artifact, name));
    if (!info.isFile() || info.isSymbolicLink() || info.size === 0) fail('正式产物不是非空普通文件');
  }
  process.stdout.write('完整编译产物：' + artifact + '\n');
}
if (process.argv[1] === fileURLToPath(import.meta.url)) {
  if (process.argv.length !== 3) fail('仅接受一个 GitHub 编译平台参数');
  compile(process.argv[2]).catch(error => { process.stderr.write(error.message + '\n'); process.exitCode = 1; });
}

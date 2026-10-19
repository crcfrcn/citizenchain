#!/usr/bin/env node
// 链上中国唯一专属构建实现；只生成所选软件产物，不安装、不发布、不改链。
import {copyFileSync, cpSync, existsSync, lstatSync, mkdirSync, readFileSync, readdirSync, realpathSync, renameSync, rmSync, writeFileSync} from 'node:fs';
import {basename, dirname, isAbsolute, join, relative, resolve, sep} from 'node:path';
import {fileURLToPath, pathToFileURL} from 'node:url';
import {createRequire} from 'node:module';
import {product} from './config.mjs';
import {
  checkWork, runBuildProcess, platformCargoEnvironment, npmCLI, stageWindowsOpenSsl,
} from './build.mjs';

const root = realpathSync(resolve(dirname(fileURLToPath(import.meta.url)), '..'));
const moduleId = value => typeof value === 'string' && /^[a-z][a-z0-9_]{0,63}$/u.test(value);
function inside(parent, child) { const value = relative(parent, child); return value === '' || value !== '..' && !value.startsWith('..' + sep) && !isAbsolute(value); }
function fail(message) { throw new Error(message); }

/** 原生平台只读取产品现有声明，不复制平台或工具需求清单。 */
export function nativePlatform(host = process.platform, arch = process.arch) {
  const entry = Object.entries(product.platforms).find(([id, declared]) => id !== 'wasm' && declared.host === host && declared.arch === arch);
  if (!entry) fail('本产品未声明该链上中国原生平台');
  return entry[0];
}

/** 构建目标只读现有Cargo声明，不维护第二份模块目录清单。 */
export function buildTargets(project = root) {
  const text = readFileSync(join(project, 'onchina/platform/Cargo.toml'), 'utf8');
  const result = new Map();
  for (const block of text.split(/^\[\[bin\]\]\s*$/mu).slice(1)) {
    const body = block.split(/^\[/mu)[0];
    const name = body.match(/^name\s*=\s*"([^"]+)"\s*$/mu)?.[1];
    const path = body.match(/^path\s*=\s*"([^"]+)"\s*$/mu)?.[1];
    if (!name || !path) fail('链上中国构建目标缺少名称或入口');
    const id = name === 'onchina' ? 'platform' : name.startsWith('onchina-') ? name.slice(8) : '';
    if (!moduleId(id) || result.has(id)) fail('链上中国构建目标重复或无效');
    const entry = resolve(project, 'onchina/platform', path);
    if (!inside(join(project, 'onchina'), entry) || !existsSync(entry) || !lstatSync(entry).isFile()) fail('链上中国入口无效');
    result.set(id, {id, name, entry});
  }
  if (!result.has('platform')) fail('链上中国缺少宿主目标');
  return result;
}
export function selectTargets(project, selection = 'all') {
  const targets = buildTargets(project);
  if (selection === 'all') return [...targets.values()];
  if (!targets.has(selection)) fail('不存在指定链上中国模块：' + selection);
  return [targets.get(selection)];
}
export function validateDescription(description, target) {
  if (!description || description.id !== target.id || !/^\d+\.\d+\.\d+$/u.test(description.version)
      || !Number.isSafeInteger(description.interface_version) || description.interface_version !== 1
      || !Array.isArray(description.frontend_exports) || description.frontend_exports.length === 0
      || typeof description.frontend_source !== 'string' || !description.frontend_source.startsWith('frontend/')
      || description.frontend_source.split(/[\\/]/u).some(value => value === '..' || value === '.')) fail('模块构建声明无效');
  return description;
}
/** 专属现场与GitHub正式整包现场分开，默认交付只归所选模块。 */
export function buildPaths(platform, selection = 'all') {
  if (!Object.hasOwn(product.platforms, platform) || platform === 'wasm' || !moduleId(selection)) fail('链上中国构建目标无效');
  const base = join(root, 'target/onchina', platform);
  return {work: join(base, 'work'), output: join(base, selection)};
}

/** 目录本身充当互斥锁：申请失败不清理别人的现场，退出未经确认也不删现场。 */
export async function withOnChinaWork(work, action) {
  checkWork(work);
  mkdirSync(dirname(work), {recursive: true});
  mkdirSync(work);
  let retained = false;
  try { return await action(work); }
  catch (error) { retained = error?.retainWork === true; throw error; }
  finally { if (!retained) rmSync(work, {recursive: true}); }
}

// 正式Runner直接使用准确检出；专属开发构建只使用自己现场中的完整源码副本。
function buildProject(project, work) {
  checkWork(work);
  const value = resolve(project);
  if (realpathSync(value) !== value || (value !== root && value !== join(work, 'source'))) fail('链上中国工程不属于本产品或本次现场');
  return value;
}

async function prepareFrontend(project, environment) {
  for (const packagePath of [join(project, 'crates/scanner'), join(project, 'onchina/platform/frontend')]) {
    await runBuildProcess(environment.NODE, [npmCLI(environment.NODE), 'ci', '--include=dev', ...(environment.npm_config_offline === 'true' ? ['--offline'] : [])], environment, packagePath);
  }
}

function ownedPath(work, path, label) {
  if (!isAbsolute(path) || path === work || !inside(work, path) || inside(join(work, 'source'), path)) fail(label + '必须属于本次构建输出，不能覆盖工程或工作根');
  let parent = path;
  while (!existsSync(parent)) parent = dirname(parent);
  if (realpathSync(parent) !== parent) fail(label + '不能经过符号链接');
  return path;
}
async function describe(executable, environment, project, target) {
  const result = await runBuildProcess(executable, ['--describe'], environment, project, {capture: true, timeout: 10000});
  // 公开执行器返回stdout/stderr对象；解析失败直接终止，不猜测版本或入口。
  const text = result.stdout;
  let description;
  try { description = JSON.parse(text); } catch { fail('无法读取模块真实构建声明'); }
  return validateDescription(description, target);
}
function marker(description) {
  return '/*onchina:' + JSON.stringify({id: description.id, version: description.version, interface_version: description.interface_version}) + '*/\n';
}

/** 单个模块与整套构建使用同一实现；调用方交付本仓已准备的工具和工作目录。 */
export async function buildPreparedOnChina({
  project, platform, work, environment, selection = 'all', frontendOnly = false,
  frontendDestination, moduleDestination, prepareDependencies = true,
}) {
  if (platform !== nativePlatform()) fail('链上中国必须使用声明对应的原生平台');
  project = buildProject(project, work);
  const env = platformCargoEnvironment(platform, work, environment);
  const cargo = env.CARGO;
  const node = env.NODE;
  const targetDirectory = ownedPath(work, env.CARGO_TARGET_DIR, 'Cargo输出');
  env.CARGO_TARGET_DIR = targetDirectory;
  const frontend = join(project, 'onchina/platform/frontend');
  const output = ownedPath(work, frontendDestination || env.ONCHINA_FRONTEND_DIST || join(work, 'onchina-frontend/dist'), '前端输出');
  const modules = ownedPath(work, moduleDestination || env.ONCHINA_MODULE_DIST || join(work, 'onchina-modules'), '模块输出');
  const targets = selectTargets(project, selection);
  if (prepareDependencies) await prepareFrontend(project, env);
  for (const target of targets) {
    if (!frontendOnly) {
      await runBuildProcess(cargo, ['--config', join(project, 'config.toml'), 'build', '--locked', '--release', '-p', 'onchina', '--bin', target.name], env, project);
    }
    const executable = join(targetDirectory, 'release', target.name + (platform === 'windows' ? '.exe' : ''));
    if (target.id !== 'platform' && (!existsSync(executable) || !lstatSync(executable).isFile())) fail('模块声明必须来自已构建的对应程序：' + target.name);
    const description = target.id === 'platform' ? null : await describe(executable, env, project, target);
    const destination = description ? join(modules, target.id, 'frontend') : output;
    mkdirSync(destination, {recursive: true});
    const job = {project, work, frontend, destination, description};
    await runBuildProcess(node, [fileURLToPath(import.meta.url), 'frontend-worker'], {...env, ONCHINA_BUILD_JOB: JSON.stringify(job)}, project);
    if (description) {
      const header = marker(description);
      const script = join(destination, 'module.js');
      if (!existsSync(script)) fail('模块前端缺少module.js');
      writeFileSync(script, header + readFileSync(script, 'utf8'));
      const stylesheet = join(destination, 'module.css');
      writeFileSync(stylesheet, header + (existsSync(stylesheet) ? readFileSync(stylesheet, 'utf8') : ''));
      // 仅重建前端时同样配对已有真实程序，产物验收不从目录猜版本。
      cpSync(executable, join(modules, target.id, basename(executable)));
      if (platform === 'windows') stageWindowsOpenSsl(modules, env, [join(modules, target.id)]);
    }
  }
  const result = {
    host: join(targetDirectory, 'release', 'onchina' + (platform === 'windows' ? '.exe' : '')),
    frontend: output, modules, targets: targets.map(target => target.id),
  };
  await verifyPreparedOnChina({project, work, environment: env, result});
  return result;
}

/** 真正的类型检查和Vite执行只在构建子进程内进行，导入脚本不执行这些操作。 */
async function frontendWorker() {
  let job;
  try { job = JSON.parse(process.env.ONCHINA_BUILD_JOB || ''); } catch { fail('缺少前端构建任务'); }
  buildProject(job.project, job.work);
  if (job.frontend !== join(job.project, 'onchina/platform/frontend')) fail('前端工程不属于本次完整工程');
  ownedPath(job.work, job.destination, '前端输出');
  const require = createRequire(join(job.frontend, 'package.json'));
  const ts = require('typescript');
  const configPath = join(job.frontend, 'tsconfig.json');
  const config = ts.readConfigFile(configPath, ts.sys.readFile);
  if (config.error) fail(ts.flattenDiagnosticMessageText(config.error.messageText, '\n'));
  const parsed = ts.parseJsonConfigFileContent(config.config, ts.sys, job.frontend);
  if (parsed.errors.length) fail(ts.formatDiagnosticsWithColorAndContext(parsed.errors, diagnosticHost(ts)));
  const entry = job.description ? resolve(job.project, 'onchina', job.description.id, job.description.frontend_source) : join(job.frontend, 'main.tsx');
  const roots = [entry, join(job.frontend, 'vite-env.d.ts')];
  const program = ts.createProgram({rootNames: roots, options: {...parsed.options, incremental: false, noEmit: true}});
  const diagnostics = ts.getPreEmitDiagnostics(program);
  if (diagnostics.length) fail(ts.formatDiagnosticsWithColorAndContext(diagnostics, diagnosticHost(ts)));
  // 构建只核对原Node工具配置；原测试仍由既有完整typecheck/test流程核对，避免单模块依赖别的业务测试。
  const nodeConfigPath = join(job.frontend, 'tsconfig.node.json');
  const nodeConfig = ts.readConfigFile(nodeConfigPath, ts.sys.readFile);
  if (nodeConfig.error) fail(ts.flattenDiagnosticMessageText(nodeConfig.error.messageText, '\n'));
  const nodeParsed = ts.parseJsonConfigFileContent(nodeConfig.config, ts.sys, job.frontend);
  if (nodeParsed.errors.length) fail(ts.formatDiagnosticsWithColorAndContext(nodeParsed.errors, diagnosticHost(ts)));
  const nodeProgram = ts.createProgram({rootNames: nodeParsed.fileNames.filter(file => !/\.(?:test|spec)\.[cm]?[jt]sx?$/u.test(file)), options: {...nodeParsed.options, incremental: false, noEmit: true}});
  const nodeDiagnostics = ts.getPreEmitDiagnostics(nodeProgram);
  if (nodeDiagnostics.length) fail(ts.formatDiagnosticsWithColorAndContext(nodeDiagnostics, diagnosticHost(ts)));
  if (job.description) process.env.ONCHINA_MODULE_DESCRIPTION = JSON.stringify(job.description);
  else delete process.env.ONCHINA_MODULE_DESCRIPTION;
  process.env.ONCHINA_FRONTEND_DIST = job.destination;
  const vite = await import(pathToFileURL(require.resolve('vite')).href);
  // 输出已核对属于本轮现场；重建必须清空旧CSS等产物，不覆盖任何源码或其他任务。
  await vite.build({root: job.frontend, configFile: join(job.frontend, 'vite.config.ts'), build: {outDir: job.destination, emptyOutDir: true}});
}
function diagnosticHost(ts) { return {getCanonicalFileName: file => file, getCurrentDirectory: ts.sys.getCurrentDirectory, getNewLine: () => '\n'}; }
/** 编译后的真实产物验收；只登记到既有测试文件，不重新编译或运行全套业务测试。 */
export async function verifyPreparedOnChina({project, work, environment, result}) {
  buildProject(project, work);
  if (result.targets.includes('platform')) {
    const frontend = ownedPath(work, result.frontend, '宿主前端');
    const html = readFileSync(join(frontend, 'index.html'), 'utf8');
    const scripts = [...html.matchAll(/<script\b[^>]*\bsrc=["']([^"']+)["']/gu)];
    if (!scripts.length) fail('宿主前端缺少真实脚本');
    const links = [...html.matchAll(/<link\b[^>]*\bhref=["']([^"']+)["']/gu)];
    for (const match of [...scripts, ...links]) {
      const file = resolve(frontend, match[1]);
      if (!inside(frontend, file) || !lstatSync(file).isFile() || realpathSync(file) !== file) fail('宿主前端脚本越界或缺失');
    }
  }
  if (!result.targets.some(id => id !== 'platform')) return;
  const modules = ownedPath(work, result.modules, '模块产物');
  const ids = result.targets.filter(id => id !== 'platform');
  const all = selectTargets(project).filter(target => target.id !== 'platform').map(target => target.id).sort();
  if (JSON.stringify([...ids].sort()) === JSON.stringify(all) && JSON.stringify(readdirSync(modules).sort()) !== JSON.stringify(all)) fail('完整模块产物混入残留或缺少目标');
  const env = {...environment, ONCHINA_TEST_MODULE_ROOT: modules, ONCHINA_TEST_MODULE_IDS: JSON.stringify(ids)};
  const node = environment.NODE;
  await runBuildProcess(node, ['--test', '--test-name-pattern', '实际模块产物', join(project, 'onchina/platform/modules/structure.test.mjs')], env, project);
  await runBuildProcess(node, [npmCLI(node), 'test', '--', '-t', '实际IIFE装载'], env, join(project, 'onchina/platform/frontend'));
}

export function argumentsOf(args) {
  const values = {action: args[0] || 'build', selection: 'all'};
  for (let i = 1; i < args.length; i += 2) {
    const name = args[i];
    if (!['--platform', '--module', '--output'].includes(name) || !args[i + 1]) fail('链上中国构建参数无效');
    values[name.slice(2)] = args[i + 1];
  }
  values.selection = values.module || 'all';
  if (!['build', 'preview'].includes(values.action)) fail('链上中国构建动作无效');
  values.platform ||= nativePlatform();
  return values;
}

async function cli(args) {
  if (args[0] === 'frontend-worker') { await frontendWorker(); return; }
  const options = argumentsOf(args);
  if (options.platform !== nativePlatform()) fail('链上中国必须使用声明对应的原生平台');
  const targets = selectTargets(root, options.selection);
  const paths = buildPaths(options.platform, options.selection);
  if (options.action === 'preview') {
    if (options.selection !== 'all' || options.output) fail('预览使用完整开发构建的默认宿主前端');
    const frontend = join(root, 'onchina/platform/frontend');
    const output = join(paths.output, 'frontend');
    if (!existsSync(join(output, 'index.html'))) fail('请先执行链上中国开发构建');
    const require = createRequire(join(frontend, 'package.json'));
    const manifestPath = require.resolve('vite/package.json');
    const installed = JSON.parse(readFileSync(manifestPath, 'utf8'));
    const declared = JSON.parse(readFileSync(join(frontend, 'package.json'), 'utf8'));
    if (installed.version !== declared.devDependencies.vite) fail('预览依赖与原声明不符，请按原锁准备依赖');
    await runBuildProcess(process.execPath, [resolve(dirname(manifestPath), installed.bin.vite), 'preview', '--outDir', output], process.env, frontend);
    return;
  }
  const output = options.output ? resolve(options.output) : paths.output;
  if (inside(root, output) && !inside(join(root, 'target'), output) || inside(paths.work, output) || inside(output, paths.work)) fail('交付位置必须位于源码和活动现场之外');
  if (options.output && existsSync(output)) fail('指定交付目录已存在，禁止覆盖');
  await withOnChinaWork(paths.work, async work => {
    const env = platformCargoEnvironment(options.platform, work, process.env);
    // 开发构建不继承运行服务的输出目录或上次验收输入。
    for (const name of ['ONCHINA_FRONTEND_DIST', 'ONCHINA_MODULE_DIST', 'ONCHINA_TEST_MODULE_ROOT', 'ONCHINA_TEST_MODULE_IDS']) delete env[name];
    if (process.versions.node !== product.tools.node) fail('Node版本不符合产品声明');
    const cargo = await runBuildProcess(env.CARGO, ['--version'], env, root, {capture: true});
    if (!cargo.stdout.startsWith('cargo ' + product.tools.rust + ' ')) fail('Rust版本不符合产品声明');
    if (!env.PROTOC) fail('缺少产品声明的官方protoc');
    const protoc = await runBuildProcess(env.PROTOC, ['--version'], env, root, {capture: true});
    if (protoc.stdout.trim() !== 'libprotoc ' + product.tools.protoc) fail('protoc版本不符合产品声明');
    const project = join(work, 'source');
    const listed = await runBuildProcess('git', ['ls-files', '-z'], env, root, {capture: true});
    for (const name of listed.stdout.split('\0').filter(Boolean)) {
      const source = resolve(root, name), destination = resolve(project, name);
      if (!inside(root, source) || !inside(project, destination) || !lstatSync(source).isFile() || realpathSync(source) !== source) fail('源码必须来自本产品普通文件');
      mkdirSync(dirname(destination), {recursive: true}); copyFileSync(source, destination);
    }
    await prepareFrontend(project, env);
    const frontend = join(project, 'onchina/platform/frontend');
    // 完整工程覆盖所有业务；单模块只检查公共装载和所选Rust目标，避免引入其他模块编译。
    await runBuildProcess(env.NODE, ['--test', join(project, 'onchina/platform/modules/structure.test.mjs')], env, project);
    if (options.selection === 'all') await runBuildProcess(env.NODE, [npmCLI(env.NODE), 'run', 'typecheck'], env, frontend);
    await runBuildProcess(env.NODE, [npmCLI(env.NODE), 'test', ...(options.selection === 'all' ? [] : ['--', '-t', '模块'])], env, frontend);
    for (const target of targets) await runBuildProcess(env.CARGO, ['--config', join(project, 'config.toml'), 'test', '--locked', '--release', '-p', 'onchina', '--bin', target.name], env, project);
    const result = await buildPreparedOnChina({project, work, platform: options.platform, environment: env, selection: options.selection, prepareDependencies: false});
    const staged = join(work, 'delivery'); mkdirSync(staged);
    if (result.targets.includes('platform')) {
      copyFileSync(result.host, join(staged, basename(result.host)));
      cpSync(result.frontend, join(staged, 'frontend'), {recursive: true});
      if (options.platform === 'windows') stageWindowsOpenSsl(staged, env, [staged]);
    }
    for (const id of result.targets.filter(id => id !== 'platform')) cpSync(join(result.modules, id), join(staged, 'modules', id), {recursive: true});
    // 默认目录仅存本平台、所选目标的开发产物；显式交付目录始终拒绝覆盖。
    let parent = dirname(output);
    while (!existsSync(parent)) parent = dirname(parent);
    if (realpathSync(parent) !== parent) fail('交付目录不能经过链接');
    mkdirSync(dirname(output), {recursive: true});
    if (!options.output) { checkWork(output); rmSync(output, {recursive: true, force: true}); }
    if (options.output) {
      // 显式目录可位于其他磁盘；先独占创建，再复制，失败只清理自己创建的目录。
      mkdirSync(output);
      try { cpSync(staged, output, {recursive: true, errorOnExist: true, force: false}); }
      catch (error) { rmSync(output, {recursive: true}); throw error; }
    } else { renameSync(staged, output); }
  });
}
if (process.argv[1] === fileURLToPath(import.meta.url)) {
  cli(process.argv.slice(2)).catch(error => { process.stderr.write(String(error.message || error) + '\n'); process.exitCode = 1; });
}

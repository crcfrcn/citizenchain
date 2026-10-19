#!/usr/bin/env node
// 链上中国唯一专属构建实现；只生成所选软件产物，不安装、不发布、不改链。
import {cpSync, existsSync, lstatSync, mkdirSync, readFileSync, realpathSync, writeFileSync} from 'node:fs';
import {basename, dirname, isAbsolute, join, relative, resolve, sep} from 'node:path';
import {fileURLToPath, pathToFileURL} from 'node:url';
import {createRequire} from 'node:module';
import {
  checkWork, fixedWork, withFixedWork, resources, prepare, resourceEnvironment,
  prepareSourceView, runBuildProcess, platformCargoEnvironment, npmCLI, stageWindowsOpenSsl,
} from './build.mjs';

const root = resolve(dirname(fileURLToPath(import.meta.url)), '..');
const moduleId = value => typeof value === 'string' && /^[a-z][a-z0-9_]{0,63}$/u.test(value);
function inside(parent, child) { const value = relative(parent, child); return value === '' || value !== '..' && !value.startsWith('..' + sep) && !isAbsolute(value); }
function fail(message) { throw new Error(message); }

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
function ownedPath(work, path, label) {
  if (!isAbsolute(path) || !inside(work, path)) fail(label + '必须属于本次构建工作目录');
  let parent = path;
  while (!existsSync(parent)) parent = dirname(parent);
  if (realpathSync(parent) !== parent) fail(label + '不能经过符号链接');
  return path;
}
async function describe(executable, environment, project, target) {
  const result = await runBuildProcess(executable, ['--describe'], environment, project, {capture: true, timeout: 10000});
  // 公开执行器返回stdout文本；解析失败直接终止，不能猜测版本或入口。
  const text = typeof result === 'string' ? result : result?.stdout;
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
  checkWork(work);
  if (!['macos', 'windows', 'linux-arm', 'linux-amd'].includes(platform)) fail('链上中国原生平台无效');
  project = ownedPath(work, resolve(project), '链上中国工程');
  if (!lstatSync(project).isDirectory()) fail('链上中国工程必须使用本次已准备的源码视图');
  const env = platformCargoEnvironment(platform, work, {...environment, NODE: environment.NODE || process.execPath});
  const cargo = env.CARGO || 'cargo';
  const node = env.NODE;
  const targetDirectory = ownedPath(work, env.CARGO_TARGET_DIR || join(work, 'work/cargo-target'), 'Cargo输出');
  env.CARGO_TARGET_DIR = targetDirectory;
  const frontend = join(project, 'onchina/platform/frontend');
  const output = ownedPath(work, frontendDestination || env.ONCHINA_FRONTEND_DIST || join(work, 'onchina-frontend/dist'), '前端输出');
  const modules = ownedPath(work, moduleDestination || env.ONCHINA_MODULE_DIST || join(work, 'onchina-modules'), '模块输出');
  const targets = selectTargets(project, selection);
  if (prepareDependencies) {
    for (const packagePath of [join(project, 'crates/scanner'), frontend]) {
      await runBuildProcess(node, [npmCLI(node), 'ci', ...(env.npm_config_offline === 'true' ? ['--offline'] : [])], env, packagePath);
    }
  }
  for (const target of targets) {
    if (!frontendOnly) {
      await runBuildProcess(cargo, ['--config', join(project, 'config.toml'), 'build', '--locked', '--release', '-p', 'onchina', '--bin', target.name], env, project);
    }
    const executable = join(targetDirectory, 'release', target.name + (platform === 'windows' ? '.exe' : ''));
    if (!existsSync(executable) || !lstatSync(executable).isFile()) fail('模块声明必须来自已构建的对应程序：' + target.name);
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
      if (!frontendOnly) {
        cpSync(executable, join(modules, target.id, basename(executable)));
        if (platform === 'windows') stageWindowsOpenSsl(join(work, 'stage'), env, [join(modules, target.id)]);
      }
      // Vite预览使用同一产物，正式宿主按实际装载目录提供资源。
      if (frontendOnly) {
        const preview = join(output, 'modules', target.id);
        mkdirSync(preview, {recursive: true});
        cpSync(destination, preview, {recursive: true});
      }
    }
  }
  return {
    host: join(targetDirectory, 'release', 'onchina' + (platform === 'windows' ? '.exe' : '')),
    frontend: output, modules, targets: targets.map(target => target.id),
  };
}

/** 真正的类型检查和Vite执行只在构建子进程内进行，导入脚本不执行这些操作。 */
async function frontendWorker() {
  let job;
  try { job = JSON.parse(process.env.ONCHINA_BUILD_JOB || ''); } catch { fail('缺少前端构建任务'); }
  checkWork(job.work);
  for (const [label, path] of Object.entries({project: job.project, frontend: job.frontend, destination: job.destination})) ownedPath(job.work, path, label);
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
  await vite.build({root: job.frontend, configFile: join(job.frontend, 'vite.config.ts')});
}
function diagnosticHost(ts) { return {getCanonicalFileName: file => file, getCurrentDirectory: ts.sys.getCurrentDirectory, getNewLine: () => '\n'}; }
function argumentsOf(args) {
  const values = {action: args[0] || 'build', selection: 'all'};
  for (let i = 1; i < args.length; i += 2) {
    const name = args[i];
    if (!['--platform', '--module', '--project', '--output', '--receipt'].includes(name) || !args[i + 1]) fail('链上中国构建参数无效');
    values[name.slice(2)] = args[i + 1];
  }
  values.selection = values.module || 'all';
  values.platform ||= process.platform === 'darwin' ? 'macos' : process.platform === 'win32' ? 'windows' : process.arch === 'arm64' ? 'linux-arm' : 'linux-amd';
  if (!['build', 'prepared', 'frontend'].includes(values.action)) fail('链上中国构建动作无效');
  return values;
}
async function cli(args) {
  if (args[0] === 'frontend-worker') { await frontendWorker(); return; }
  const options = argumentsOf(args);
  if (options.action === 'build' && !options.output) fail('独立构建必须指定--output交付产物');
  selectTargets(root, options.selection);
  if (options.output) {
    const output = resolve(options.output);
    if (inside(root, output) && !inside(join(root, 'target'), output)) fail('构建产物不能写入源码目录');
    if (inside(fixedWork('build/' + options.platform), output)) fail('交付位置不能是本轮待清理工作目录');
  }
  await withFixedWork('build/' + options.platform, async work => {
    let env = {...process.env, NODE: process.execPath};
    let project = options.project;
    if (options.action === 'build') {
      const receipt = options.receipt ? JSON.parse(readFileSync(resolve(options.receipt), 'utf8')) : await resources(options.platform, work, {}, {environment: env, offline: env.CITIZENCHAIN_OFFLINE === 'true'});
      await prepare(options.platform, work, receipt, env);
      env = resourceEnvironment(options.platform, work, receipt, env);
      project = prepareSourceView();
    } else {
      project ||= env.CITIZENCHAIN_PROJECT_ROOT || env.CITIZENCHAIN_BUILD_PROJECT;
      if (!project) fail('请先使用专属build入口准备工程，或交付本仓已准备的工程路径');
    }
    const result = await buildPreparedOnChina({
      project, work, platform: options.platform, environment: env, selection: options.selection,
      frontendOnly: options.action === 'frontend',
    });
    if (options.output) {
      const output = resolve(options.output);
      if (inside(root, output) && !inside(join(root, 'target'), output)) fail('构建产物不能写入源码目录');
      mkdirSync(output, {recursive: true});
      if (existsSync(result.host) && result.targets.includes('platform')) cpSync(result.host, join(output, basename(result.host)), {errorOnExist: true, force: false});
      if (existsSync(result.frontend) && result.targets.includes('platform')) cpSync(result.frontend, join(output, 'frontend'), {recursive: true, errorOnExist: true, force: false});
      for (const id of result.targets.filter(id => id !== 'platform')) cpSync(join(result.modules, id), join(output, 'modules', id), {recursive: true, errorOnExist: true, force: false});
    } else if (options.action === 'build') {
      fail('独立构建必须指定--output交付产物，工作目录不会作为永久交付位置');
    }
  }, {environment: process.env});
}
if (process.argv[1] === fileURLToPath(import.meta.url)) {
  cli(process.argv.slice(2)).catch(error => { process.stderr.write(String(error.message || error) + '\n'); process.exitCode = 1; });
}

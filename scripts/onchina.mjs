#!/usr/bin/env node
import {Agent} from 'node:https';
// 链上中国唯一专属构建实现；只生成所选软件产物，不安装、不发布、不改链。
import {copyFileSync, cpSync, existsSync, lstatSync, mkdirSync, readFileSync, readdirSync, realpathSync, renameSync, rmSync, writeFileSync} from 'node:fs';
import {basename, dirname, isAbsolute, join, relative, resolve, sep} from 'node:path';
import {fileURLToPath, pathToFileURL} from 'node:url';
import {createRequire} from 'node:module';
import {product} from './config.mjs';
import {
  checkWork, runBuildProcess, platformCargoEnvironment, npmCLI, stageWindowsOpenSsl, developmentTLS,
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

/** 目标只接受本轮Cargo metadata的真实工作区包与bin，不维护第二份模块清单。 */
export function buildTargets(project = root, metadata) {
  if (metadata?.version !== 1 || !Array.isArray(metadata.packages) || !Array.isArray(metadata.workspace_members)) fail('缺少真实Cargo元数据');
  const members = new Set(metadata.workspace_members), result = new Map();
  for (const pkg of metadata.packages.filter(pkg => members.has(pkg.id))) {
    const manifest = pkg.manifest_path;
    if (typeof manifest !== 'string' || !inside(join(project, 'onchina'), manifest)) continue;
    const binaries = pkg.targets.filter(target => target.kind.includes('bin'));
    if (!binaries.length) continue;
    if (binaries.length !== 1) fail('链上中国每个软件包必须只有自己的bin');
    const target = binaries[0], name = target.name;
    const id = name === 'onchina' ? 'platform' : name.startsWith('onchina-') ? name.slice(8) : '';
    if (!moduleId(id) || result.has(id)) fail('链上中国构建目标重复或无效');
    const entry = target.src_path;
    if (!isAbsolute(entry) || !inside(dirname(manifest), entry) || !existsSync(entry) || !lstatSync(entry).isFile()) fail('链上中国入口无效');
    const declaration = pkg.metadata?.onchina?.frontend;
    if (typeof declaration !== 'string') fail('链上中国缺少前端工程配对声明');
    const frontend = resolve(dirname(manifest), declaration);
    if (!inside(join(project, 'onchina'), frontend) || realpathSync(frontend) !== frontend) fail('前端工程越界或经过链接');
    const frontendPackage = JSON.parse(readFileSync(join(frontend, 'package.json'), 'utf8'));
    if (frontendPackage.version !== pkg.version || !frontendPackage.exports || [...result.values()].some(value => value.frontend === frontend || value.frontendPackage === frontendPackage.name)) fail('前端工程版本、公开入口或独立归属无效');
    result.set(id, {id, name, entry, package: pkg.name, manifest, frontend, frontendPackage: frontendPackage.name});
  }
  if (!result.has('platform')) fail('链上中国缺少宿主目标');
  return result;
}
export function selectTargets(project, selection = 'all', metadata) {
  const targets = buildTargets(project, metadata);
  if (selection === 'all') return [...targets.values()];
  if (isAbsolute(selection)) {
    const selected = [...targets.values()].filter(target => target.frontend === selection);
    if (selected.length !== 1) fail('不存在指定前端工程');
    return selected;
  }
  if (!targets.has(selection)) fail('不存在指定链上中国模块：' + selection);
  return [targets.get(selection)];
}
export function validateDescription(description, target) {
  if (!description || description.id !== target.id || !/^\d+\.\d+\.\d+$/u.test(description.version)
      || !Number.isSafeInteger(description.interface_version) || description.interface_version !== 2
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

// 构建直接读取本仓源码，产物只写入本次隔离现场。
function buildProject(project, work) {
  checkWork(work);
  const value = resolve(project);
  if (realpathSync(value) !== value || value !== root) fail('链上中国工程不属于本产品或本次现场');
  return value;
}

async function prepareFrontend(project, environment, targets) {
  for (const packagePath of [join(project, 'crates/scanner'), ...targets.map(target => target.frontend)]) {
    await runBuildProcess(environment.NODE, [npmCLI(environment.NODE), 'ci', '--include=dev', ...(environment.npm_config_offline === 'true' ? ['--offline'] : [])], environment, packagePath);
  }
}

function ownedPath(work, path, label) {
  if (!isAbsolute(path) || path === work || !inside(work, path)) fail(label + '必须属于本次构建输出，不能覆盖工程或工作根');
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
  const output = ownedPath(work, frontendDestination || env.ONCHINA_FRONTEND_DIST || join(work, 'onchina-frontend/dist'), '前端输出');
  const modules = ownedPath(work, moduleDestination || env.ONCHINA_MODULE_DIST || join(work, 'onchina-modules'), '模块输出');
  const cargoResult = await runBuildProcess(cargo, ['--config', join(project, 'config.toml'), 'metadata', '--no-deps', '--format-version', '1', '--locked', '--offline'], env, project, {capture: true});
  let metadata;
  try { metadata = JSON.parse(cargoResult.stdout); } catch { fail('Cargo元数据不是有效JSON'); }
  const targets = selectTargets(project, selection, metadata);
  if (prepareDependencies) await prepareFrontend(project, env, targets);
  // 同时准确选择所属package与bin；单模块仅链接平台lib，不编译宿主或其他业务。
  if (!frontendOnly) {
    await runBuildProcess(cargo, ['--config', join(project, 'config.toml'), 'build', '--locked', '--release', '--offline', ...targets.flatMap(target => ['-p', target.package, '--bin', target.name])], env, project);
  }
  for (const target of targets) {
    const executable = join(targetDirectory, 'release', target.name + (platform === 'windows' ? '.exe' : ''));
    if (target.id !== 'platform' && (!existsSync(executable) || !lstatSync(executable).isFile())) fail('模块声明必须来自已构建的对应程序：' + target.name);
    const description = target.id === 'platform' ? null : await describe(executable, env, project, target);
    const destination = description ? join(modules, target.id, 'frontend') : output;
    mkdirSync(destination, {recursive: true});
    // 每个前端是独立工程，分别检查自己的类型与工具配置。
    const job = {project, work, frontend: target.frontend, destination, description};
    await runBuildProcess(node, [fileURLToPath(import.meta.url), 'frontend-worker'], {...env, ONCHINA_BUILD_JOB: JSON.stringify(job)}, target.frontend);
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
  await verifyPreparedOnChina({project, work, result, metadata});
  return result;
}

/** 真正的类型检查和Vite执行只在构建子进程内进行，导入脚本不执行这些操作。 */
/** 使用原生路径边界，避免Windows分隔符或同名前缀目录绕过模块归属。 */
export function insideModule(parent, child, paths = { relative, isAbsolute, sep }) {
  const value = paths.relative(parent, child);
  return value !== '' && value !== '..' && !value.startsWith('..' + paths.sep) && !paths.isAbsolute(value);
}
/** 前端公开入口只由本包标准exports定位；软件接口1的入口标识保持。 */
export function frontendEntry(frontend, descriptor) {
  const declared = JSON.parse(readFileSync(join(frontend, 'package.json'), 'utf8'));
  const source = declared.exports?.['./' + descriptor.frontend_source];
  if (declared.version !== descriptor.version || typeof source !== 'string' || !source.startsWith('./')) throw new Error('前端版本或公开入口声明无效');
  const entry = resolve(frontend, source);
  if (!insideModule(frontend, entry) || !existsSync(entry) || realpathSync(entry) !== entry) throw new Error('模块入口越界或不存在');
  return entry;
}
function moduleBuild(owner, platform, project, descriptor, normalizePath) {
  if (!descriptor) return null;
  if (!/^[a-z][a-z0-9_]{0,63}$/.test(descriptor.id) || !/^\d+\.\d+\.\d+$/.test(descriptor.version)
      || descriptor.interface_version !== 2) throw new Error('模块构建声明无效');
  const entry = frontendEntry(owner, descriptor);
  const packages = {
    react: 'react', 'react-dom/client': 'react_dom_client',
    'react/jsx-runtime': 'react_jsx_runtime', 'react/jsx-dev-runtime': 'react_jsx_dev_runtime',
    antd: 'antd', '@ant-design/icons': 'ant_design_icons', dayjs: 'dayjs',
  };
  // 标准exports定位SDK唯一入口，不假定入口仍在包根。
  const declared = JSON.parse(readFileSync(join(platform, 'package.json'), 'utf8'));
  const publicEntry = resolve(platform, declared.exports['.']);
  const exposed = new Map([...readFileSync(publicEntry, 'utf8')
    .matchAll(/export \* as (platform_[A-Za-z0-9_]+) from ["'](\.\/[^"']+|\.\.\/[^"']+)["']/g)]
    .map(match => [normalizePath(resolve(dirname(publicEntry), match[2])), match[1]]));
  const publicSource = (id, importer) => {
    let path = isAbsolute(id) ? id : id.startsWith('.') && importer ? resolve(dirname(importer), id) : '';
    if (id.startsWith(declared.name + '/')) {
      const source = declared.exports?.['./' + id.slice(declared.name.length + 1)];
      if (typeof source !== 'string') throw new Error('模块引用未公开的平台入口：' + id);
      path = resolve(platform, source);
    }
    if (!path || !insideModule(platform, path)) return null;
    const key = normalizePath(path.replace(/\.tsx?$/, ''));
    if (!exposed.has(key)) throw new Error('模块引用未公开的平台实现：' + relative(platform, path));
    return key;
  };
  const publicImports = {
    name: 'onchina-platform-interface', enforce: 'pre',
    resolveId(id, importer) {
      // SDK总入口直接引用宿主完整对象，与子入口共用同一运行时实例。
      if (id === declared.name) return { id, external: true };
      if (packages[id]) return { id, external: true };
      const source = publicSource(id, importer);
      if (source) return { id: source, external: true };
      if (id.startsWith('@gmb/onchina-')) throw new Error('跨模块运行时调用必须经公开装载入口');
      if (importer && (isAbsolute(id) || id.startsWith('.'))) {
        const path = isAbsolute(id) ? id : resolve(dirname(importer), id);
        if (insideModule(join(project, 'onchina'), path) && !insideModule(owner, path)) throw new Error('跨模块运行时调用必须经公开装载入口');
      }
      return null;
    },
  };
  const metadata = JSON.stringify({ id: descriptor.id, version: descriptor.version, interface_version: descriptor.interface_version });
  return { descriptor, entry, publicImports, output: {
    globals(id) { return id === declared.name ? 'OnChinaPlatform' : 'OnChinaPlatform.' + (packages[id] ?? exposed.get(normalizePath(id))); },
    footer: 'Object.defineProperty(OnChinaModules.' + descriptor.id + ',"__onchina",{value:Object.freeze(' + metadata + ')});',
  }};
}



function frontendProject(frontend) {
  if (!isAbsolute(frontend) || !inside(join(root, 'onchina'), frontend) || realpathSync(frontend) !== frontend || !existsSync(join(frontend, 'package.json'))) fail('前端工程不属于本产品');
  return JSON.parse(readFileSync(join(frontend, 'package.json'), 'utf8'));
}
function publicFile(frontend, key) {
  const source = frontendProject(frontend).exports?.[key];
  if (typeof source !== 'string' || !source.startsWith('./')) fail('缺少前端真实公开入口：' + key);
  const path = resolve(frontend, source);
  if (!insideModule(frontend, path) || !existsSync(path) || realpathSync(path) !== path) fail('前端入口越界或不存在');
  return path;
}
function hostEntry(frontend) {
  const page = publicFile(frontend, './index.html');
  const source = readFileSync(page, 'utf8').match(/<script\b[^>]*\bsrc=["']([^"']+)["']/u)?.[1];
  if (!source) fail('宿主页面缺少真实脚本');
  const entry = resolve(dirname(page), source.replace(/^\/+/, ''));
  if (!insideModule(frontend, entry) || !existsSync(entry) || realpathSync(entry) !== entry) fail('宿主脚本越界或不存在');
  return entry;
}
/** 官方编译选项只有本产品的一份实现，浏览器和测试作用域仍分开。 */
export function frontendCompiler(frontend, testing = false) {
  frontendProject(frontend);
  const require = createRequire(join(frontend, 'package.json')), ts = require('typescript');
  const compilerOptions = { target: 'ES2020', lib: ['ES2020', 'DOM', 'DOM.Iterable'], module: 'ESNext', skipLibCheck: true,
    moduleResolution: 'Bundler', allowImportingTsExtensions: false, resolveJsonModule: true, isolatedModules: true,
    noEmit: true, jsx: 'react-jsx', strict: true, types: [...(testing ? ['node'] : []), 'react', 'react-dom', 'vite/client'] };
  const converted = ts.convertCompilerOptionsFromJson(compilerOptions, frontend);
  if (converted.errors.length) fail(ts.formatDiagnosticsWithColorAndContext(converted.errors, diagnosticHost(ts, frontend)));
  const files = ts.sys.readDirectory(frontend, ['.ts', '.tsx'], ['node_modules'], ['**/*.ts', '**/*.tsx']).filter(file => {
    const test = relative(frontend, file).split(/[\\/]/u).includes('tests') || /\.test\.tsx?$/u.test(file);
    return testing ? test : !test;
  });
  return { ts, options: converted.options, files };
}
export function typecheckFrontend(frontend, entry) {
  for (const testing of entry ? [false] : [false, true]) {
    const { ts, options, files } = frontendCompiler(frontend, testing);
    if (!files.length) continue;
    const host = ts.createCompilerHost(options); host.getCurrentDirectory = () => frontend;
    const program = ts.createProgram({rootNames: entry && !testing ? [entry] : files, options, host});
    const diagnostics = ts.getPreEmitDiagnostics(program);
    if (diagnostics.length) fail(ts.formatDiagnosticsWithColorAndContext(diagnostics, diagnosticHost(ts, frontend)));
  }
}
/** 按当前工程原锁加载Vite/React插件，配置不借用别包依赖库存。 */
export async function frontendConfiguration(frontend, {command = 'build', description = null, destination} = {}) {
  const declared = frontendProject(frontend), require = createRequire(join(frontend, 'package.json'));
  const vite = await import(pathToFileURL(require.resolve('vite')).href);
  const {default: react} = await import(pathToFileURL(require.resolve('@vitejs/plugin-react')).href);
  const sdk = require.resolve('@gmb/onchina-platform');
  let platform = dirname(sdk);
  while (!existsSync(join(platform, 'package.json'))) platform = dirname(platform);
  const project = root, page = description ? null : publicFile(frontend, './index.html');
  const dependencies = Object.keys(declared.dependencies);
  const output = join(project, 'target/onchina', nativePlatform(), 'platform/frontend');
  const module = moduleBuild(frontend, platform, project, description, vite.normalizePath);
  const https = command === 'serve' ? developmentTLS(process.env.ONCHINA_FRONTEND_TLS_CERT_FILE, process.env.ONCHINA_FRONTEND_TLS_KEY_FILE) : undefined;
  // 构建不连接运行中的宿主；开发代理必须显式交付公信 HTTPS 入口，不设旧地址或私有 CA。
  let proxy;
  if (command === 'serve') {
    const target = process.env.ONCHINA_BASE_URL;
    if (!target) fail('开发代理必须通过 ONCHINA_BASE_URL 显式指定链上中国公信 HTTPS 入口');
    const endpoint = new URL(target);
    if (endpoint.protocol !== 'https:' || endpoint.username || endpoint.password || endpoint.search || endpoint.hash) {
      fail('OnChina代理地址必须是无凭据的HTTPS基地址');
    }
    const agent = new Agent({rejectUnauthorized: true, minVersion: 'TLSv1.3'});
    proxy = Object.fromEntries(['/api', '/modules'].map(path => [path, {target, changeOrigin: true, secure: true, agent}]));
  }

  return {
  // OnChina 后端同源托管 dist,base 用相对路径以适配任意内网挂载路径。
  root: module ? frontend : dirname(page),
  configFile: false,
  base: './',
  plugins: [react(), ...(module ? [module.publicImports] : [])],
  // 本仓扫码组件与宿主共用同一React实例，避免链接包加载第二套Hooks运行库。
  resolve: { dedupe: dependencies },
  build: {
    outDir: destination || process.env.ONCHINA_FRONTEND_DIST || output,
    ...(module ? {
      lib: { entry: module.entry, formats: ['iife'], name: 'OnChinaModules.' + module.descriptor.id, fileName: () => 'module.js', cssFileName: 'module' },
      cssCodeSplit: false,
      rolldownOptions: { output: module.output },
    } : {})
  },
  server: {
    fs: { allow: [frontend, platform, join(project, 'icons'), join(project, 'crates/scanner')] },
    https,
    hmr: { protocol: "wss", clientPort: 5179 },
    port: 5179,
    host: 'localhost',
    strictPort: true,
    proxy
  },
  preview: {
    https,
    port: 5179,
    host: 'localhost',
    strictPort: true,
    proxy
  }
  };
}
async function runFrontend(action, args) {
  if (args[0] !== '--frontend' || !args[1]) fail('必须指定前端工程');
  const frontend = resolve(process.cwd(), args[1]), declared = frontendProject(frontend);
  const require = createRequire(join(frontend, 'package.json'));
  if (action === 'typecheck') { if (args.length !== 2) fail('类型检查参数无效'); typecheckFrontend(frontend); return; }
  if (action === 'test') {
    let testNamePattern;
    for (let i = 2; i < args.length; i += 2) {
      if (!['-t', '--testNamePattern'].includes(args[i]) || !args[i + 1]) fail('测试参数无效');
      testNamePattern = new RegExp(args[i + 1]);
    }
    const {startVitest} = await import(pathToFileURL(require.resolve('vitest/node')).href);
    const context = await startVitest('test', [], {root: frontend, config: false, run: true, watch: false,
      environment: 'node', include: ['**/*.test.ts', '**/*.test.tsx'], testNamePattern},
      {esbuild: {jsx: 'automatic'}, resolve: {dedupe: [...Object.keys(declared.dependencies), 'vitest']}});
    if (!context) fail('前端测试未启动');
    try { if (context.state.getUnhandledErrors().length || context.state.getFiles().some(file => file.result?.state === 'fail')) process.exitCode = 1; }
    finally { await context.close(); }
    return;
  }
  if (args.length !== 2) fail('开发或预览参数无效');
  const vite = await import(pathToFileURL(require.resolve('vite')).href);
  const config = await frontendConfiguration(frontend, {command: 'serve'});
  const server = action === 'preview' ? await vite.preview(config) : await vite.createServer(config);
  if (action === 'dev') await server.listen();
  server.printUrls();
}

async function frontendWorker() {
  let job;
  try { job = JSON.parse(process.env.ONCHINA_BUILD_JOB || ''); } catch { fail('缺少前端构建任务'); }
  buildProject(job.project, job.work);
  if (!isAbsolute(job.frontend) || !inside(join(job.project, 'onchina'), job.frontend) || realpathSync(job.frontend) !== job.frontend || !existsSync(join(job.frontend, 'package.json'))) fail('前端工程不属于本次完整工程');
  ownedPath(job.work, job.destination, '前端输出');
  const entry = job.description ? frontendEntry(job.frontend, job.description) : hostEntry(job.frontend);
  typecheckFrontend(job.frontend, entry);
  const require = createRequire(join(job.frontend, 'package.json'));
  const vite = await import(pathToFileURL(require.resolve('vite')).href);
  const configuration = await frontendConfiguration(job.frontend, {description: job.description, destination: job.destination});
  await vite.build({...configuration, build: {...configuration.build, outDir: job.destination, emptyOutDir: true}});
}
function diagnosticHost(ts, frontend) { return {getCanonicalFileName: file => file, getCurrentDirectory: () => frontend, getNewLine: () => '\n'}; }
/** 只检查本轮编译产物的文件、资源路径和模块集合，不启动测试。 */
export async function verifyPreparedOnChina({project, work, result, metadata}) {
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
  const all = selectTargets(project, 'all', metadata).filter(target => target.id !== 'platform').map(target => target.id).sort();
  if (JSON.stringify([...ids].sort()) === JSON.stringify(all) && JSON.stringify(readdirSync(modules).sort()) !== JSON.stringify(all)) fail('完整模块产物混入残留或缺少目标');

}

export function argumentsOf(args) {
  const values = {action: args[0] || 'build', selection: 'all'};
  for (let i = 1; i < args.length; i += 2) {
    const name = args[i];
    if (!['--platform', '--module', '--output', '--frontend'].includes(name) || !args[i + 1]) fail('链上中国构建参数无效');
    values[name.slice(2)] = args[i + 1];
  }
  if (values.frontend && values.module) fail('前端工程与模块选择不能同时指定');
  values.selection = values.module || 'all';
  if (values.action !== 'build') fail('链上中国构建动作无效');
  values.platform ||= nativePlatform();
  return values;
}

async function cli(args) {
  if (args[0] === 'frontend-worker') { await frontendWorker(); return; }
  if (['test', 'typecheck', 'dev', 'preview'].includes(args[0])) { await runFrontend(args[0], args.slice(1)); return; }
  const options = argumentsOf(args);
  if (options.platform !== nativePlatform()) fail('链上中国必须使用声明对应的原生平台');
  const paths = buildPaths(options.platform, options.selection);
  let output = options.output ? resolve(options.output) : paths.output;
  if (inside(root, output) && !inside(join(root, 'target'), output) || inside(paths.work, output) || inside(output, paths.work)) fail('交付位置必须位于源码和活动现场之外');
  if (options.output && existsSync(output)) fail('指定交付目录已存在，禁止覆盖');
  await withOnChinaWork(paths.work, async work => {
    const env = platformCargoEnvironment(options.platform, work, process.env);
    // 开发构建不继承运行服务的输出目录。
    for (const name of ['ONCHINA_FRONTEND_DIST', 'ONCHINA_MODULE_DIST']) delete env[name];
    if (process.versions.node !== product.tools.node) fail('Node版本不符合产品声明');
    const cargo = await runBuildProcess(env.CARGO, ['--version'], env, root, {capture: true});
    if (!cargo.stdout.startsWith('cargo ' + product.tools.rust + ' ')) fail('Rust版本不符合产品声明');
    if (!env.PROTOC) fail('缺少产品声明的官方protoc');
    const protoc = await runBuildProcess(env.PROTOC, ['--version'], env, root, {capture: true});
    if (protoc.stdout.trim() !== 'libprotoc ' + product.tools.protoc) fail('protoc版本不符合产品声明');
    const result = await buildPreparedOnChina({project: root, work, platform: options.platform, environment: env, selection: options.frontend ? resolve(process.cwd(), options.frontend) : options.selection});
    if (options.frontend && !options.output) output = buildPaths(options.platform, result.targets[0]).output;
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

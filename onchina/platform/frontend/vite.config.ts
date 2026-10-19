import {fileURLToPath} from 'node:url';
import { defineConfig, normalizePath, type Plugin } from 'vite';
import react from '@vitejs/plugin-react';
import { tmpdir } from 'node:os';
import { basename, dirname, extname, isAbsolute, join, relative, resolve, sep } from 'node:path';
import { existsSync, readFileSync, readdirSync } from 'node:fs';
import { createSecureContext } from 'node:tls';
import { Agent } from 'node:https';

// 开发及预览必须显式提供证书和私钥；构建静态资源不启动TLS服务。
function developmentTLS() {
  const certificate = process.env.ONCHINA_FRONTEND_TLS_CERT_FILE;
  const privateKey = process.env.ONCHINA_FRONTEND_TLS_KEY_FILE;
  if (!certificate || !privateKey) throw new Error('缺少TLS证书或私钥路径');
  try {
    const options = { cert: readFileSync(certificate), key: readFileSync(privateKey), minVersion: 'TLSv1.3' as const };
    createSecureContext(options);
    return options;
  } catch { throw new Error('TLS证书、私钥或配对无效'); }
}

// 各模块使用独立入口及产物，共用原始依赖锁和宿主公开组件。页面与交互保持。
const frontendRoots = readdirSync(new URL('../../', import.meta.url), { withFileTypes: true })
  .filter(entry => entry.isDirectory())
  .map(entry => fileURLToPath(new URL(`../../${entry.name}/frontend`, import.meta.url)))
  .filter(path => existsSync(path));
const dependencies = Object.keys(JSON.parse(readFileSync(new URL('./package.json', import.meta.url), 'utf8')).dependencies);


// 模块沿用现有组件作为入口，公共对象仅从宿主公开导出取得。
type ModuleBuild = { id: string; version: string; interface_version: number; frontend_source: string };
/** 使用原生路径边界，避免Windows分隔符或同名前缀目录绕过模块归属。 */
export function insideModule(parent: string, child: string, paths: { relative: (from: string, to: string) => string; isAbsolute: (path: string) => boolean; sep: string } = { relative, isAbsolute, sep }): boolean {
  const value = paths.relative(parent, child);
  return value !== '' && value !== '..' && !value.startsWith('..' + paths.sep) && !paths.isAbsolute(value);
}
function moduleBuild() {
  const raw = process.env.ONCHINA_MODULE_DESCRIPTION;
  if (!raw) return null;
  const descriptor = JSON.parse(raw) as ModuleBuild;
  if (!/^[a-z][a-z0-9_]{0,63}$/.test(descriptor.id) || !/^\d+\.\d+\.\d+$/.test(descriptor.version)
      || descriptor.interface_version !== 1) throw new Error('模块构建声明无效');
  const platform = resolve(fileURLToPath(new URL('./', import.meta.url)));
  const owner = resolve(platform, '../../', descriptor.id, 'frontend');
  const entry = resolve(owner, '..', descriptor.frontend_source);
  if (!insideModule(owner, entry) || !existsSync(entry)) throw new Error('模块入口越界或不存在');
  const packages: Record<string, string> = {
    react: 'react', 'react-dom/client': 'react_dom_client',
    'react/jsx-runtime': 'react_jsx_runtime', 'react/jsx-dev-runtime': 'react_jsx_dev_runtime',
    antd: 'antd', '@ant-design/icons': 'ant_design_icons', dayjs: 'dayjs',
  };
  const exposed = new Set([...readFileSync(join(platform, 'main.tsx'), 'utf8')
    .matchAll(/import \* as Shared_([A-Za-z0-9_]+) from ["']\.\/[^"']+["']/g)].map(match => match[1]));
  const publicSource = (id: string, importer?: string) => {
    const path = isAbsolute(id) ? id : id.startsWith('.') && importer ? resolve(dirname(importer), id) : '';
    if (!path || !insideModule(platform, path)) return null;
    const name = basename(path, extname(path));
    if (!exposed.has(name)) throw new Error('模块引用未公开的平台实现：' + name);
    return name;
  };
  const publicImports: Plugin = {
    name: 'onchina-platform-interface',
    enforce: 'pre',
    resolveId(id, importer) {
      if (packages[id]) return { id, external: true };
      const name = publicSource(id, importer);
      if (name) return { id: normalizePath(join(platform, name)), external: true };
      if (importer && (isAbsolute(id) || id.startsWith('.'))) {
        const path = isAbsolute(id) ? id : resolve(dirname(importer), id);
        const onchina = resolve(platform, '../..');
        if (insideModule(onchina, path) && !insideModule(owner, path)) throw new Error('跨模块运行时调用必须经公开装载入口');
      }
      return null;
    },
  };
  const metadata = JSON.stringify({ id: descriptor.id, version: descriptor.version, interface_version: descriptor.interface_version });
  return {
    descriptor, entry, publicImports,
    output: {
      globals(id: string) { return packages[id] ? 'OnChinaPlatform.' + packages[id] : 'OnChinaPlatform.platform_' + basename(id, extname(id)); },
      footer: 'Object.defineProperty(OnChinaModules.' + descriptor.id + ',"__onchina",{value:Object.freeze(' + metadata + ')});',
    },
  };
}
export default defineConfig(({ command }) => {
  const module = moduleBuild();
  const https = command === 'serve' ? developmentTLS() : undefined;
  const target = process.env.ONCHINA_BASE_URL ?? 'https://onchina.local:8964';
  const endpoint = new URL(target);
  if (endpoint.protocol !== 'https:' || endpoint.username || endpoint.password || endpoint.search || endpoint.hash) {
    throw new Error('OnChina代理地址必须是无凭据的HTTPS基地址');
  }
  const caFile = process.env.ONCHINA_TLS_CA_FILE;
  const agent = new Agent({
    ca: caFile === undefined ? undefined : readFileSync(caFile),
    rejectUnauthorized: true,
    minVersion: "TLSv1.3",
  });
  // API与模块资源共用同一受校验的HTTPS入口，开发与预览不维护另一套模块副本。
  const proxy = Object.fromEntries(['/api', '/modules'].map(path => [path, {
    target, changeOrigin: true, secure: true, agent,
  }]));

  return {
  // OnChina 后端同源托管 dist,base 用相对路径以适配任意内网挂载路径。
  base: './',
  plugins: [react(), ...(module ? [module.publicImports] : [])],
  // 本仓扫码组件与宿主共用同一React实例，避免链接包加载第二套Hooks运行库。
  resolve: { dedupe: dependencies },
  build: {
    outDir: process.env.ONCHINA_FRONTEND_DIST || join(tmpdir(), 'citizenchain', 'onchina-frontend'),
    ...(module ? {
      lib: { entry: module.entry, formats: ['iife'] as ('iife')[], name: 'OnChinaModules.' + module.descriptor.id, fileName: () => 'module.js', cssFileName: 'module' },
      cssCodeSplit: false,
      rolldownOptions: { output: module.output },
    } : {})
  },
  server: {
    fs: {allow: [...frontendRoots,fileURLToPath(new URL('../../../icons',import.meta.url)),fileURLToPath(new URL('../../../crates/scanner',import.meta.url))]},
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
});

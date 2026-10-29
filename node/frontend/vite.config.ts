import {fileURLToPath} from 'node:url';
import { defineConfig, type UserConfig } from 'vite';
import react from '@vitejs/plugin-react';
import { tmpdir } from 'node:os';
import { basename, dirname, join } from 'node:path';
import { createRequire } from 'node:module';
import { readFileSync } from 'node:fs';
import { createSecureContext } from 'node:tls';

const host = process.env.TAURI_DEV_HOST;

// 开发及预览必须显式提供证书和私钥；构建静态资源不启动TLS服务。
function developmentTLS() {
  const certificate = process.env.CITIZENCHAIN_NODE_TLS_CERT_FILE;
  const privateKey = process.env.CITIZENCHAIN_NODE_TLS_KEY_FILE;
  if (!certificate || !privateKey) throw new Error('缺少TLS证书或私钥路径');
  try {
    const options = { cert: readFileSync(certificate), key: readFileSync(privateKey), minVersion: 'TLSv1.3' as const };
    createSecureContext(options);
    return options;
  } catch { throw new Error('TLS证书、私钥或配对无效'); }
}

export default defineConfig(({ command }): UserConfig => {
  // 仅Vitest官方测试环境和实际CLI入口同时匹配时加载测试配置。
  // 普通开发/预览即使继承VITEST变量或使用--mode test，仍要求原TLS证书。
  const isTest = process.env.VITEST === 'true' && basename(process.argv[1] ?? '') === 'vitest.mjs';
  const https = command === "serve" && !isTest ? developmentTLS() : undefined;
  const scanner = createRequire(new URL('../../crates/scanner/package.json', import.meta.url));
  return {
  plugins: isTest ? [] : [react()],
  ...(isTest ? {
    test: { environment: 'jsdom', include: ['onchain/transfer.test.tsx'] },
  } : {}),
  // 本仓扫码组件与宿主共用同一React实例，避免链接包加载第二套Hooks运行库。
  resolve: {
    dedupe: ['react', 'react-dom'],
    ...(isTest ? { alias: {
      '@testing-library/react': scanner.resolve('@testing-library/react'),
      react: dirname(scanner.resolve('react/package.json')),
      'react-dom': dirname(scanner.resolve('react-dom/package.json')),
    } } : {}),
  },
  build: {
    outDir: process.env.CITIZENCHAIN_FRONTEND_DIST || join(tmpdir(), 'citizenchain', 'node-frontend')
  },
  // 白皮书由节点运行时读取官网静态正文；公民宪法由链上 runtime API 返回。
  publicDir: false,
  clearScreen: false,
  preview: { https, host: host ?? "127.0.0.1", port: 5173, strictPort: true },
  server: {
    fs: {allow: [fileURLToPath(new URL('.',import.meta.url)),fileURLToPath(new URL('../../icons',import.meta.url)),fileURLToPath(new URL('../../crates/scanner',import.meta.url))]},
    https,
    host: host ?? '127.0.0.1',
    port: 5173,
    strictPort: true,
    hmr: { protocol: 'wss', host: host ?? '127.0.0.1', clientPort: 5173 }
  }
  };
});

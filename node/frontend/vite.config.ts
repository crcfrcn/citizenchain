import {fileURLToPath} from 'node:url';
import { defineConfig, type UserConfig } from 'vite';
import react from '@vitejs/plugin-react';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { createRequire } from 'node:module';

const host = process.env.TAURI_DEV_HOST;

// 开发及预览必须显式提供证书和私钥；构建静态资源不启动TLS服务。
const { developmentTLS } = createRequire(import.meta.url)('../../scripts/build.mjs');

export default defineConfig(({ command }): UserConfig => {
  const https = command === "serve" ? developmentTLS(process.env.CITIZENCHAIN_NODE_TLS_CERT_FILE, process.env.CITIZENCHAIN_NODE_TLS_KEY_FILE) : undefined;
  return {
  plugins: [react()],
  // 本仓扫码组件与宿主共用同一React实例，避免链接包加载第二套Hooks运行库。
  resolve: {
    dedupe: ['react', 'react-dom'],
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

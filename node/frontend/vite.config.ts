import {fileURLToPath} from 'node:url';
import { defineConfig, type UserConfig } from 'vite';
import react from '@vitejs/plugin-react';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
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
  const https = command === "serve" ? developmentTLS() : undefined;
  return {
  plugins: [react()],
  // 本仓扫码组件与宿主共用同一React实例，避免链接包加载第二套Hooks运行库。
  resolve: { dedupe: ['react', 'react-dom'] },
  build: {
    outDir: process.env.CITIZENCHAIN_FRONTEND_DIST || join(tmpdir(), 'citizenchain', 'node-frontend')
  },
  // 白皮书由 citizenchain/scripts/docs.mjs 内置进 bundle;
  // 公民宪法改由链上 runtime API 返回，不再维护静态目录副本。
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

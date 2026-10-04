import { defineConfig } from 'vite';
import react from '@vitejs/plugin-react';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { readFileSync } from 'node:fs';
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

export default defineConfig(({ command }) => {
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

  return {
  // OnChina 后端同源托管 dist,base 用相对路径以适配任意内网挂载路径。
  base: './',
  plugins: [react()],
  // 本仓扫码组件与宿主共用同一React实例，避免链接包加载第二套Hooks运行库。
  resolve: { dedupe: ['react', 'react-dom'] },
  build: {
    outDir: process.env.ONCHINA_FRONTEND_DIST || join(tmpdir(), 'citizenchain', 'onchina-frontend')
  },
  server: {
    https,
    hmr: { protocol: "wss", clientPort: 5179 },
    port: 5179,
    host: 'localhost',
    strictPort: true,
    proxy: {
      '/api': {
        target,
        changeOrigin: true,
        secure: true,
        agent
      }
    }
  },
  preview: {
    https,
    port: 5179,
    host: 'localhost',
    strictPort: true,
    proxy: {
      '/api': {
        target,
        changeOrigin: true,
        secure: true,
        agent
      }
    }
  }
  };
});

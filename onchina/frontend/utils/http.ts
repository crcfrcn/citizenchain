// 前端通用 HTTP 工具。这里只做请求、鉴权头和 401 拦截,
// 不放任何业务 API;业务 API 必须放回所属功能模块目录。

import type { AdminAuth } from '../auth/types';

let onUnauthorized: (() => void) | null = null;
let unauthorizedFired = false;

type ErrorBody = {
  code?: number;
  error_code?: string;
  message?: string;
  trace_id?: string;
};

export class ApiError extends Error {
  readonly status: number;
  readonly code?: number;
  readonly errorCode?: string;
  readonly traceId?: string;

  constructor(status: number, body: ErrorBody | null, fallback: string) {
    super(body?.message ?? fallback);
    this.name = 'ApiError';
    this.status = status;
    this.code = body?.code;
    this.errorCode = body?.error_code;
    this.traceId = body?.trace_id;
  }
}

export class AuthExpiredError extends ApiError {
  constructor(status: number, body: ErrorBody | null) {
    super(status, body, '登录已过期，请重新登录');
    this.name = 'AuthExpiredError';
  }
}

/** AuthProvider 启动时注册回调;卸载时传 null 清除。 */
export function setOnUnauthorized(cb: (() => void) | null) {
  onUnauthorized = cb;
  unauthorizedFired = false;
}

/** 所有请求使用相对路径,由 Vite(开发) / Nginx(生产) 统一代理到后端。 */
export async function adminRequest<T>(
  path: string,
  auth: AdminAuth,
  init?: RequestInit,
): Promise<T> {
  return request<T>(path, {
    ...init,
    headers: {
      ...adminHeaders(auth),
      ...(init?.headers || {}),
    },
  });
}

export async function request<T>(path: string, init?: RequestInit): Promise<T> {
  let resp: Response;
  try {
    resp = await fetch(path, init);
  } catch (error) {
    const msg = error instanceof Error ? error.message : String(error);
    throw new Error(`无法连接服务器：${msg}`);
  }

  const text = await resp.text();
  let body: any = null;
  try {
    body = text ? JSON.parse(text) : null;
  } catch {
    const snippet = text.slice(0, 120);
    throw new Error(
      `服务响应格式错误(${resp.status})：${snippet || 'empty body'}，请确认后端已重启到最新版本`,
    );
  }

  if (resp.status === 401) {
    if (onUnauthorized && !unauthorizedFired) {
      unauthorizedFired = true;
      onUnauthorized();
    }
    // 401 只代表管理员登录态失效;必须抛错中断业务流程,不能返回 undefined。
    throw new AuthExpiredError(resp.status, body);
  }

  if (!resp.ok || !body || body.code !== 0) {
    throw new ApiError(resp.status, body, `request failed (${resp.status})`);
  }
  return body.data as T;
}

export function adminHeaders(auth: AdminAuth): HeadersInit {
  return {
    authorization: `Bearer ${auth.access_token}`,
  };
}

/** 鉴权二进制读取沿用登录失效处理，令牌只进入请求头。 */
export async function adminBlobRequest(
  path: string,
  auth: AdminAuth,
  signal?: AbortSignal,
): Promise<Blob> {
  const resp = await fetch(path, { headers: adminHeaders(auth), cache: 'no-store', signal });
  if (resp.ok) return resp.blob();
  const body: ErrorBody | null = await resp.json().catch(() => null);
  if (resp.status === 401) {
    if (onUnauthorized && !unauthorizedFired) {
      unauthorizedFired = true;
      onUnauthorized();
    }
    throw new AuthExpiredError(resp.status, body);
  }
  throw new ApiError(resp.status, body, `request failed (${resp.status})`);
}

// 公开(免登录)端点的取数变体。`request` 本身不带任何鉴权头(adminRequest 才注入 Bearer),
// 故公开读取直接复用同一相对路径 + {code,message,data} 解包逻辑;仅用语义化别名区分调用意图。
// 大屏只读看板(/api/public/legislation/display/board)经此拉取。
export const publicRequest = request;

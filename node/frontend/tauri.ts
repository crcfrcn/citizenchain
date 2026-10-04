import { invoke } from '@tauri-apps/api/core';

const ERROR_MAX_LENGTH = 500;

export { invoke };

/** 提取错误消息并截断，防止超长或异常内容影响 UI。 */
export function sanitizeError(e: unknown): string {
  const raw = e instanceof Error ? e.message : String(e);
  return raw.length > ERROR_MAX_LENGTH
    ? `${raw.slice(0, ERROR_MAX_LENGTH)}...`
    : raw;
}

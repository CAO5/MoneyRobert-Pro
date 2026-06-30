import { AppError } from '@/types/common';
import { TARO_ENV } from '@/utils/env';

const SENSITIVE_QUERY_KEYS = /^(password|passwd|pwd|token|access_token|refresh_token|secret|api_?key)$/i;

function currentPageUsesHttps(): boolean {
  try {
    return typeof location !== 'undefined' && location.protocol === 'https:';
  } catch {
    return false;
  }
}

function currentPageIsLocalDevelopment(): boolean {
  try {
    return (
      typeof location !== 'undefined' &&
      (location.hostname === 'localhost' || location.hostname === '127.0.0.1')
    );
  } catch {
    return false;
  }
}

/**
 * 所有真实 API 请求都必须走 TLS。
 * 唯一例外是非生产构建访问本机开发服务。
 */
export function assertSecureTransport(url: string): void {
  if (/^https:\/\//i.test(url)) return;

  if (url.startsWith('/')) {
    if (TARO_ENV !== 'h5') {
      throw new AppError('API 地址必须配置为 HTTPS 绝对地址', undefined, 'INSECURE_TRANSPORT');
    }
    if (currentPageUsesHttps()) return;
    if (currentPageIsLocalDevelopment()) return;
  }

  throw new AppError('安全连接不可用，已阻止发送敏感信息', undefined, 'INSECURE_TRANSPORT');
}

/** 密码、Token、密钥禁止放入 URL，避免进入历史、代理及服务器访问日志。 */
export function assertNoSensitiveQuery(params?: Record<string, unknown>): void {
  const unsafeKey = Object.keys(params || {}).find((key) => SENSITIVE_QUERY_KEYS.test(key));
  if (unsafeKey) {
    throw new AppError(
      `敏感字段 ${unsafeKey} 不允许出现在 URL 中`,
      undefined,
      'SENSITIVE_QUERY_BLOCKED',
    );
  }
}

export function buildSafeNavigationQuery(params?: Record<string, string>): string {
  if (!params || Object.keys(params).length === 0) return '';
  assertNoSensitiveQuery(params);
  const query = Object.entries(params)
    .map(([key, value]) => `${encodeURIComponent(key)}=${encodeURIComponent(value)}`)
    .join('&');
  return `?${query}`;
}

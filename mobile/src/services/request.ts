import Taro from '@tarojs/taro';
import {
  ApiResponse,
  AppError,
  getClientPlatform,
  CLIENT_VERSION,
} from '@/types/common';
import {
  TARO_ENV,
  TARO_APP_API_URL,
  TARO_APP_MOCK,
} from '@/utils/env';
import { getActiveLocale } from '@/store/language';
import { assertNoSensitiveQuery, assertSecureTransport } from '@/security/transport';

/**
 * 统一后台 API 基础地址
 * 对接现有 Rust 后端（与桌面 frontend 的 vite proxy 目标一致：localhost:8001）
 * - 小程序端必须配置真实 HTTPS 域名（通过 TARO_APP_API_URL 配置）
 * - H5 端用相对路径 /api/v1，由 ipv4-proxy 转发到后端 8001（同源无 CORS）
 * - 显式设置 TARO_APP_MOCK=true 可强制走 mock（无后端预览场景）
 *
 * 注意：后端实际监听端口为 8001（见 backend/.env 的 APP_SERVER__PORT）
 */
export const API_BASE_URL =
  TARO_ENV === 'h5'
    ? '/api/v1'
    : TARO_APP_API_URL || '';

/**
 * 是否启用 mock 模式
 * - TARO_APP_MOCK=true 时强制启用 mock（无后端的纯前端预览）
 * - 否则默认走真实后端（H5 经 ipv4-proxy 转发 /api 到 8001）
 */
// Mock 仅允许 H5 本地预览，防止小程序/原生生产包因误配置绕过真实认证。
export const MOCK_ENABLED = TARO_ENV === 'h5' && TARO_APP_MOCK === 'true';

/** 旧版本曾使用的明文存储键，仅用于迁移清理。 */
const ACCESS_TOKEN_KEY = 'mr_access_token';
const REFRESH_TOKEN_KEY = 'mr_refresh_token';
const USER_KEY = 'mr_user';

let memoryAccessToken: string | null = null;
let memoryRefreshToken: string | null = null;
let memoryUser: unknown = null;
let legacyStoragePurged = false;

function purgeLegacyCredentialStorage() {
  if (legacyStoragePurged) return;
  try {
    Taro.removeStorageSync(ACCESS_TOKEN_KEY);
    Taro.removeStorageSync(REFRESH_TOKEN_KEY);
    Taro.removeStorageSync(USER_KEY);
    legacyStoragePurged = true;
  } catch {
    // 下一次访问时继续尝试，绝不回读旧版明文凭证。
  }
}

/**
 * 敏感会话仅保存在当前 JS 内存，不写入 localStorage/小程序 Storage。
 * 应用进程结束后会话失效，换取凭证不落盘的安全边界。
 */
export const tokenStorage = {
  getAccessToken(): string | null {
    purgeLegacyCredentialStorage();
    return memoryAccessToken;
  },
  getRefreshToken(): string | null {
    purgeLegacyCredentialStorage();
    return memoryRefreshToken;
  },
  setTokens(access: string, refresh: string) {
    purgeLegacyCredentialStorage();
    memoryAccessToken = access;
    memoryRefreshToken = refresh;
  },
  clearTokens() {
    memoryAccessToken = null;
    memoryRefreshToken = null;
    purgeLegacyCredentialStorage();
  },
  getUser<T>(): T | null {
    purgeLegacyCredentialStorage();
    return (memoryUser as T | null) || null;
  },
  setUser(user: unknown) {
    purgeLegacyCredentialStorage();
    memoryUser = user;
  },
  clearUser() {
    memoryUser = null;
    purgeLegacyCredentialStorage();
  },
};

/** 请求方法类型 */
type HttpMethod = 'GET' | 'POST' | 'PUT' | 'DELETE' | 'PATCH';

/** 请求配置 */
export interface RequestOptions {
  url: string;
  method?: HttpMethod;
  data?: Record<string, unknown> | unknown[];
  params?: Record<string, string | number | boolean | undefined>;
  headers?: Record<string, string>;
  /** 是否需要认证（默认 true） */
  auth?: boolean;
  /** 是否跳过响应包装解析（默认 false，会自动解包 data 字段） */
  rawResponse?: boolean;
  /** 自定义超时（毫秒） */
  timeout?: number;
}

/** 拼接查询字符串 */
function buildQueryString(params: Record<string, unknown>): string {
  const entries = Object.entries(params).filter(([, v]) => v !== undefined && v !== null);
  if (entries.length === 0) return '';
  const search = entries
    .map(([k, v]) => `${encodeURIComponent(k)}=${encodeURIComponent(String(v))}`)
    .join('&');
  return `?${search}`;
}

/** 解析后端响应（兼容 success/code 两种格式） */
function unwrapResponse<T>(data: unknown): T {
  if (data && typeof data === 'object') {
    const obj = data as ApiResponse<T>;
    // { success: true, data: ... }
    if ('success' in obj && obj.success === true && 'data' in obj) {
      return obj.data as T;
    }
    // { code: 200, data: ... }
    if ('code' in obj && (obj.code === 200 || obj.code === 0) && 'data' in obj) {
      return obj.data as T;
    }
    // { error: "..." } 或 { message: "..." }
    if ('error' in obj || 'message' in obj) {
      const msg =
        (typeof obj.error === 'string' ? obj.error : obj.error?.message) ||
        obj.message ||
        '请求失败';
      throw new AppError(msg, undefined, 'BUSINESS_ERROR');
    }
  }
  return data as T;
}

/** 提取错误消息 */
function extractErrorMessage(status: number, data: unknown): string {
  if (data && typeof data === 'object') {
    const obj = data as ApiResponse;
    return (
      (typeof obj.error === 'string' ? obj.error : obj.error?.message) ||
      obj.message ||
      obj.detail?.[0]?.msg ||
      `请求失败（${status}）`
    );
  }
  return `请求失败（${status}）`;
}

/** 防止并发刷新 Token */
let refreshPromise: Promise<string> | null = null;

/** 刷新 Token（带互斥锁，防止并发触发多次刷新） */
async function refreshAccessToken(): Promise<string> {
  if (refreshPromise) return refreshPromise;

  const refreshToken = tokenStorage.getRefreshToken();
  if (!refreshToken) {
    throw new AppError('未登录或登录已过期', 401, 'NOT_AUTHENTICATED');
  }

  refreshPromise = (async () => {
    try {
      const refreshUrl = `${API_BASE_URL}/auth/refresh`;
      assertSecureTransport(refreshUrl);
      const res = await Taro.request({
        url: refreshUrl,
        method: 'POST',
        data: { refresh_token: refreshToken },
        header: {
          'Content-Type': 'application/json',
          'Cache-Control': 'no-store',
          Pragma: 'no-cache',
        },
        timeout: 15000,
      });
      if (res.statusCode >= 200 && res.statusCode < 300) {
        const auth = unwrapResponse<{
          access_token: string;
          refresh_token: string;
        }>(res.data);
        tokenStorage.setTokens(auth.access_token, auth.refresh_token);
        return auth.access_token;
      }
      // 刷新失败：清理并跳转登录
      tokenStorage.clearTokens();
      tokenStorage.clearUser();
      redirectToLogin();
      throw new AppError('登录已过期，请重新登录', 401, 'TOKEN_EXPIRED');
    } finally {
      refreshPromise = null;
    }
  })();

  return refreshPromise;
}

/** 跳转登录页 */
function redirectToLogin() {
  const pages = Taro.getCurrentPages();
  const current = pages[pages.length - 1];
  const currentPath = current ? `/${current.route}` : '';
  if (currentPath !== '/pages/login/index') {
    Taro.reLaunch({ url: '/pages/login/index' });
  }
}

/**
 * 统一请求方法
 * 自动处理：
 * - JWT Bearer Token 注入
 * - X-Client-Platform / X-Client-Version 多端协商头
 * - 401 自动刷新 Token 并重试
 * - 业务响应解包（success/code → data）
 */
export async function request<T = unknown>(options: RequestOptions): Promise<T> {
  const {
    url,
    method = 'GET',
    data,
    params,
    headers = {},
    auth = true,
    rawResponse = false,
    timeout = 30000,
  } = options;

  // 拼接 URL
  assertNoSensitiveQuery(params);
  const queryString = params ? buildQueryString(params) : '';
  const fullUrl = `${API_BASE_URL}${url}${queryString}`;
  if (!MOCK_ENABLED) assertSecureTransport(fullUrl);

  // 注入鉴权头
  const finalHeaders: Record<string, string> = {
    'Content-Type': 'application/json',
    'Cache-Control': 'no-store',
    Pragma: 'no-cache',
    // 报告生成、正文读取等内容型接口必须以当前界面语言响应。
    'Accept-Language': getActiveLocale(),
    // 多端版本协商头（按深度研究报告建议）
    'X-Client-Platform': getClientPlatform(),
    'X-Client-Version': CLIENT_VERSION,
    ...headers,
  };
  if (auth) {
    const token = tokenStorage.getAccessToken();
    if (token) {
      finalHeaders.Authorization = `Bearer ${token}`;
    }
  }

  // 发起请求
  const res = await Taro.request({
    url: fullUrl,
    method,
    data,
    header: finalHeaders,
    timeout,
  });

  // 401 自动刷新并重试一次
  if (res.statusCode === 401 && auth) {
    try {
      await refreshAccessToken();
      // 递归重试一次（携带新 Token）
      return request<T>({ ...options, auth: true });
    } catch (err) {
      throw err;
    }
  }

  // 非 2xx 视为错误
  if (res.statusCode < 200 || res.statusCode >= 300) {
    const message = extractErrorMessage(res.statusCode, res.data);
    throw new AppError(message, res.statusCode, 'HTTP_ERROR');
  }

  // 解包业务响应
  if (rawResponse) {
    return res.data as T;
  }
  return unwrapResponse<T>(res.data);
}

/** 便捷方法 */
export const http = {
  get<T = unknown>(url: string, params?: Record<string, unknown>, options?: Partial<RequestOptions>) {
    return request<T>({ url, method: 'GET', params: params as RequestOptions['params'], ...options });
  },
  post<T = unknown>(url: string, data?: unknown, options?: Partial<RequestOptions>) {
    return request<T>({ url, method: 'POST', data: data as RequestOptions['data'], ...options });
  },
  put<T = unknown>(url: string, data?: unknown, options?: Partial<RequestOptions>) {
    return request<T>({ url, method: 'PUT', data: data as RequestOptions['data'], ...options });
  },
  delete<T = unknown>(url: string, options?: Partial<RequestOptions>) {
    return request<T>({ url, method: 'DELETE', ...options });
  },
};

export default request;

/**
 * 通用类型定义
 * 与后端 Rust API 响应结构保持一致
 */
import { TARO_ENV } from '@/utils/env';

/**
 * 统一响应包装（与现有前端 axios 拦截器约定一致）
 * 后端返回格式：{ success: true, data: ... } 或 { code: 200, data: ... }
 */
export interface ApiResponse<T = unknown> {
  success?: boolean;
  code?: number;
  data?: T;
  message?: string;
  error?: { message?: string } | string;
  detail?: Array<{ msg: string }>;
}

/**
 * 分页响应
 */
export interface PaginatedResponse<T> {
  items: T[];
  total: number;
  page: number;
  page_size: number;
  has_more: boolean;
}

/**
 * 通用错误类型
 */
export class AppError extends Error {
  public readonly statusCode?: number;
  public readonly code?: string;
  public readonly detail?: unknown;

  constructor(message: string, statusCode?: number, code?: string, detail?: unknown) {
    super(message);
    this.name = 'AppError';
    this.statusCode = statusCode;
    this.code = code;
    this.detail = detail;
  }
}

/**
 * 列表查询通用参数
 */
export interface ListQuery {
  page?: number;
  page_size?: number;
  limit?: number;
}

/**
 * 客户端平台标识（用于 X-Client-Platform 请求头）
 * 让后端 BFF 知道当前端类型以做版本降级
 */
export type ClientPlatform = 'weapp' | 'alipay' | 'tt' | 'h5' | 'rn' | 'harmony' | 'qq' | 'jd';

/**
 * 获取当前客户端平台标识
 * 通过 Taro 编译时常量 TARO_ENV 获取（经 utils/env 安全包装，兼容无 process 的 H5 环境）
 */
export function getClientPlatform(): ClientPlatform {
  const env = TARO_ENV as ClientPlatform;
  return env || 'h5';
}

/**
 * 客户端版本号（用于 X-Client-Version 请求头）
 */
export const CLIENT_VERSION = '1.0.0';

/**
 * 运行时安全获取环境变量
 *
 * 背景：H5 浏览器运行时没有 Node 的 process 对象。
 * 正常 Taro 构建会通过 webpack DefinePlugin 把 process.env.TARO_ENV 替换为静态字符串，
 * 但部分云预览环境（如 Trae 预览）未注入 process polyfill，直接访问 process.env 会抛
 * ReferenceError: process is not defined。
 *
 * 本模块用 typeof 守卫安全访问，所有需要读取 process.env 的地方统一从此导入。
 */

/** 读取单个环境变量（process 未定义时返回 undefined） */
export function getEnvVar(key: string): string | undefined {
  try {
    if (typeof process !== 'undefined' && process.env) {
      return process.env[key];
    }
  } catch {
    // process 访问异常时忽略，返回 undefined
  }
  return undefined;
}

/** 当前构建平台（process.env.TARO_ENV 的安全版本，默认 h5） */
export const TARO_ENV: string = getEnvVar('TARO_ENV') || 'h5';

/** 后端 API 地址环境变量（来自 TARO_APP_API_URL） */
export const TARO_APP_API_URL: string | undefined = getEnvVar('TARO_APP_API_URL');

/**
 * 是否强制启用 mock 模式（来自 TARO_APP_MOCK）
 * - 显式设置 TARO_APP_MOCK=true 时强制走 mock（用于无后端的纯前端预览）
 * - 默认 false：H5 通过 ipv4-proxy 转发 /api 到后端 8001，走真实接口
 */
export const TARO_APP_MOCK: string | undefined = getEnvVar('TARO_APP_MOCK');

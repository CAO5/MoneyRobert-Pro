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

declare const __TARO_APP_API_URL__: string;
declare const __TARO_APP_MOCK__: string;

/**
 * 以下变量必须使用静态属性访问，Taro/webpack 才能在编译期替换。
 * try/catch 负责兼容未经过 Taro 构建的浏览器预览。
 */
export const TARO_ENV: string = (() => {
  try {
    return process.env.TARO_ENV || 'h5';
  } catch {
    return 'h5';
  }
})();

/** 后端 API 地址环境变量（生产小程序必须是 HTTPS 绝对地址） */
export const TARO_APP_API_URL: string | undefined = (() => {
  try {
    return __TARO_APP_API_URL__ || undefined;
  } catch {
    return undefined;
  }
})();

/**
 * 是否强制启用 mock 模式（来自 TARO_APP_MOCK）
 * - 显式设置 TARO_APP_MOCK=true 时强制走 mock（用于无后端的纯前端预览）
 * - 默认 false：H5 通过 ipv4-proxy 转发 /api 到后端 8001，走真实接口
 */
export const TARO_APP_MOCK: string | undefined = (() => {
  try {
    return __TARO_APP_MOCK__;
  } catch {
    return undefined;
  }
})();

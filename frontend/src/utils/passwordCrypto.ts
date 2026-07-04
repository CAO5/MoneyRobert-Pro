import CryptoJS from 'crypto-js'

/**
 * 密码加密工具（PC 端）
 *
 * 安全策略：客户端绝不发送明文密码
 * - 登录/注册前必须先做 SHA-256，发送 hex 字符串（64 字符）
 * - 后端只接受 hex，存储为 bcrypt(sha256_hex)
 *
 * 注意：本函数无脑走 sha256，不再支持明文模式
 * 使用主包导入避免子路径解析问题（容器内 crypto-js/sha256 子路径可能缺失）
 */

/**
 * 加密密码：返回 sha256(password) 的 hex 字符串
 * @param password 用户输入的明文密码
 * @returns 64 字符的 hex 字符串
 */
export function encryptPassword(password: string): string {
  return CryptoJS.SHA256(password).toString(CryptoJS.enc.Hex)
}

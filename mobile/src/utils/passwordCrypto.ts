import sha256 from 'crypto-js/sha256'
import Hex from 'crypto-js/enc-hex'

/**
 * 密码加密工具（移动端）
 *
 * 安全策略：客户端绝不发送明文密码
 * - 登录/注册前必须先做 SHA-256，发送 hex 字符串（64 字符）
 * - 后端只接受 hex，存储为 bcrypt(sha256_hex)
 *
 * 注意：本函数无脑走 sha256，不再支持明文模式
 *       Mock 模式下也走 sha256（mock 数据已同步为 sha256 hex）
 */

/**
 * 加密密码：返回 sha256(password) 的 hex 字符串
 * @param password 用户输入的明文密码
 * @returns 64 字符的 hex 字符串
 */
export function encryptPassword(password: string): string {
  return sha256(password).toString(Hex)
}

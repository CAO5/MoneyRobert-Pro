-- Migration 033: 密码加密方式升级（sha256 + bcrypt）
--
-- 背景：旧密码存储为 bcrypt(明文)，新策略强制 bcrypt(sha256_hex)
-- 客户端不再发送明文密码，只发送 sha256(password) 的 hex 字符串
--
-- 本迁移清空所有现有用户的 hashed_password，强制走密码重置流程
-- 用户必须通过 /auth/password-reset/request + /auth/password-reset/confirm 重新设置密码
--
-- 安全考虑：
-- 1. 不可逆：bcrypt 是单向哈希，无法从旧 hash 还原明文再 sha256
-- 2. 强制重置：清空后用户无法登录，必须通过邮箱验证身份后设置新密码
-- 3. 审计：记录受影响用户数

-- 前置检查：仅在 hashed_password 非空时执行
DO $$
DECLARE
    affected_count INTEGER;
BEGIN
    SELECT COUNT(*) INTO affected_count
    FROM users
    WHERE hashed_password IS NOT NULL AND hashed_password != '';

    -- 清空所有旧密码（迁移后无法用旧密码登录）
    UPDATE users
    SET hashed_password = '',
        updated_at = NOW()
    WHERE hashed_password IS NOT NULL AND hashed_password != '';

    RAISE NOTICE 'Password migration: % users affected. All users must reset their passwords.', affected_count;
END $$;

-- 添加密码重置相关的索引（如果不存在）
CREATE INDEX IF NOT EXISTS idx_users_email_lower ON users(LOWER(email));

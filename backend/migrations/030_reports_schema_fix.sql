-- P1-3: Reports API 与 Schema 修复
-- 1. content 从 TEXT 改为 JSONB（支持结构化查询）
-- 2. 新增 report_type 列（与 format 语义分离）
-- 3. 新增 updated_at 列
--
-- 安全处理：旧 content 可能包含非 JSON 字符串（如纯文本报告），
-- 直接 content::jsonb 会导致迁移失败。
-- 使用 PL/pgSQL 函数 try_jsonb 安全转换：合法 JSON 直接解析，
-- 非法内容包装为 {"raw": "..."} 后再转 JSONB。

-- 临时函数：安全转换 TEXT → JSONB
CREATE OR REPLACE FUNCTION try_jsonb(input text) RETURNS jsonb AS $$
BEGIN
  IF input IS NULL OR input = '' THEN
    RETURN '{}'::jsonb;
  END IF;
  RETURN input::jsonb;
EXCEPTION WHEN OTHERS THEN
  -- 非合法 JSON → 包装为 {"raw": "..."} 避免迁移失败
  RETURN jsonb_build_object('raw', input);
END;
$$ LANGUAGE plpgsql IMMUTABLE;

-- 1. content 改为 JSONB（安全转换：兼容历史非 JSON 内容）
-- 注意：content 可能已是 JSONB（迁移重应用场景），显式转 text 再调函数，
-- 避免 "函数 try_jsonb(jsonb) 不存在" 错误
ALTER TABLE reports
  ALTER COLUMN content TYPE JSONB
  USING try_jsonb(content::text);

-- 清理临时函数
DROP FUNCTION IF EXISTS try_jsonb(text);

-- 2. 新增 report_type 列（默认 'general'，从 format 迁移数据）
ALTER TABLE reports
  ADD COLUMN IF NOT EXISTS report_type VARCHAR(50) NOT NULL DEFAULT 'general';

-- 将现有 format 值复制到 report_type（保持向后兼容）
UPDATE reports SET report_type = format WHERE report_type = 'general';

-- 3. 新增 updated_at 列
ALTER TABLE reports
  ADD COLUMN IF NOT EXISTS updated_at TIMESTAMP WITH TIME ZONE NOT NULL DEFAULT NOW();

-- 索引：按 report_type 查询
CREATE INDEX IF NOT EXISTS idx_reports_report_type ON reports(report_type);
CREATE INDEX IF NOT EXISTS idx_reports_updated_at ON reports(updated_at DESC);

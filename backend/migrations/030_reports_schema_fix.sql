-- P1-3: Reports API 与 Schema 修复
-- 1. content 从 TEXT 改为 JSONB（支持结构化查询）
-- 2. 新增 report_type 列（与 format 语义分离）
-- 3. 新增 updated_at 列

-- 1. content 改为 JSONB（如果当前是 TEXT）
ALTER TABLE reports
  ALTER COLUMN content TYPE JSONB
  USING content::jsonb;

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

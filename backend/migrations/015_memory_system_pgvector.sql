-- 015 pgvector extension and memory system tables
-- Implements: AGENT_SYSTEM_DESIGN.md Chapter 12 - Multi-tier Memory Management System

-- Enable pgvector extension for embedding similarity search
-- 注意：如果 pgvector 扩展不可用，跳过向量相关表的创建
DO $$
BEGIN
    -- 尝试创建 pgvector 扩展
    BEGIN
        CREATE EXTENSION IF NOT EXISTS vector;
    EXCEPTION WHEN OTHERS THEN
        -- 如果扩展不可用，记录警告并继续
        RAISE WARNING 'pgvector extension not available, skipping vector tables';
        RETURN;
    END;

    -- 只有在扩展成功创建后才创建向量表
    IF EXISTS (SELECT 1 FROM pg_extension WHERE extname = 'vector') THEN
        -- L2 Episodic Memory (中期情景记忆)
        CREATE TABLE IF NOT EXISTS episodic_memory (
            id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
            agent_id VARCHAR(100) NOT NULL,
            session_id UUID,
            symbol VARCHAR(50) NOT NULL,
            event_type VARCHAR(50) NOT NULL,
            content TEXT NOT NULL,
            context JSONB DEFAULT '{}'::jsonb,
            outcome JSONB,
            embedding vector(1536),
            importance_score FLOAT DEFAULT 0.5,
            access_count INT DEFAULT 0,
            created_at TIMESTAMPTZ DEFAULT NOW(),
            last_accessed_at TIMESTAMPTZ DEFAULT NOW(),
            expires_at TIMESTAMPTZ
        );

        CREATE INDEX IF NOT EXISTS idx_episodic_memory_agent_id ON episodic_memory(agent_id);
        CREATE INDEX IF NOT EXISTS idx_episodic_memory_symbol ON episodic_memory(symbol);
        CREATE INDEX IF NOT EXISTS idx_episodic_memory_event_type ON episodic_memory(event_type);
        CREATE INDEX IF NOT EXISTS idx_episodic_memory_created_at ON episodic_memory(created_at DESC);
        CREATE INDEX IF NOT EXISTS idx_episodic_memory_importance ON episodic_memory(importance_score DESC);
        CREATE INDEX IF NOT EXISTS idx_episodic_memory_embedding ON episodic_memory USING ivfflat (embedding vector_cosine_ops) WITH (lists = 100);

        -- L3 Knowledge Memory (长期知识记忆)
        CREATE TABLE IF NOT EXISTS knowledge_memory (
            id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
            agent_id VARCHAR(100),
            category VARCHAR(50) NOT NULL,
            title VARCHAR(255) NOT NULL,
            content TEXT NOT NULL,
            source_type VARCHAR(50),
            source_id UUID,
            embedding vector(1536),
            confidence_score FLOAT DEFAULT 0.5,
            verification_count INT DEFAULT 0,
            is_validated BOOLEAN DEFAULT FALSE,
            importance_score FLOAT DEFAULT 0.5,
            access_count INT DEFAULT 0,
            created_at TIMESTAMPTZ DEFAULT NOW(),
            last_accessed_at TIMESTAMPTZ DEFAULT NOW(),
            updated_at TIMESTAMPTZ DEFAULT NOW()
        );

        CREATE INDEX IF NOT EXISTS idx_knowledge_memory_agent_id ON knowledge_memory(agent_id);
        CREATE INDEX IF NOT EXISTS idx_knowledge_memory_category ON knowledge_memory(category);
        CREATE INDEX IF NOT EXISTS idx_knowledge_memory_symbol ON knowledge_memory(category);
        CREATE INDEX IF NOT EXISTS idx_knowledge_memory_validated ON knowledge_memory(is_validated);
        CREATE INDEX IF NOT EXISTS idx_knowledge_memory_confidence ON knowledge_memory(confidence_score DESC);
        CREATE INDEX IF NOT EXISTS idx_knowledge_memory_embedding ON knowledge_memory USING ivfflat (embedding vector_cosine_ops) WITH (lists = 100);

        -- Agent 校准表
        CREATE TABLE IF NOT EXISTS agent_calibration (
            id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
            agent_id VARCHAR(100) NOT NULL,
            calibration_date DATE NOT NULL DEFAULT CURRENT_DATE,
            calibration_factor FLOAT NOT NULL DEFAULT 1.0,
            accuracy_score FLOAT NOT NULL DEFAULT 0.5,
            bias_score FLOAT DEFAULT 0.0,
            confidence_correlation FLOAT DEFAULT 0.0,
            sample_size INT DEFAULT 0,
            notes TEXT,
            created_at TIMESTAMPTZ DEFAULT NOW(),
            UNIQUE(agent_id, calibration_date)
        );

        CREATE INDEX IF NOT EXISTS idx_agent_calibration_agent_id ON agent_calibration(agent_id);
        CREATE INDEX IF NOT EXISTS idx_agent_calibration_date ON agent_calibration(calibration_date DESC);

        -- 记忆反思日志
        CREATE TABLE IF NOT EXISTS memory_reflection_log (
            id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
            reflection_type VARCHAR(50) NOT NULL,
            agent_id VARCHAR(100),
            trigger_event VARCHAR(100),
            insights JSONB DEFAULT '[]'::jsonb,
            patterns_detected JSONB DEFAULT '[]'::jsonb,
            knowledge_validated INT DEFAULT 0,
            knowledge_invalidated INT DEFAULT 0,
            agents_calibrated INT DEFAULT 0,
            memory_items_cleaned INT DEFAULT 0,
            duration_ms BIGINT,
            created_at TIMESTAMPTZ DEFAULT NOW()
        );

        CREATE INDEX IF NOT EXISTS idx_memory_reflection_type ON memory_reflection_log(reflection_type);
        CREATE INDEX IF NOT EXISTS idx_memory_reflection_agent ON memory_reflection_log(agent_id);
        CREATE INDEX IF NOT EXISTS idx_memory_reflection_created ON memory_reflection_log(created_at DESC);

        -- 为 knowledge_nodes 添加向量列
        ALTER TABLE knowledge_nodes ADD COLUMN IF NOT EXISTS embedding vector(1536);
        ALTER TABLE knowledge_nodes ADD COLUMN IF NOT EXISTS importance_score FLOAT DEFAULT 0.5;
        ALTER TABLE knowledge_nodes ADD COLUMN IF NOT EXISTS access_count INT DEFAULT 0;
        ALTER TABLE knowledge_nodes ADD COLUMN IF NOT EXISTS last_accessed_at TIMESTAMPTZ DEFAULT NOW();

        CREATE INDEX IF NOT EXISTS idx_knowledge_nodes_embedding ON knowledge_nodes USING ivfflat (embedding vector_cosine_ops) WITH (lists = 100);
    END IF;
END $$;

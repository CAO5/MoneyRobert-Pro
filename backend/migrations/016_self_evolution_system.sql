-- 016 Self-evolution system tables
-- Implements: AGENT_SYSTEM_DESIGN.md Chapter 13 - Self-evolving Fund Manager Agent

-- ============================================================================
-- Prompt Versions (Prompt 版本管理)
-- ============================================================================
CREATE TABLE IF NOT EXISTS prompt_versions (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    agent_id VARCHAR(100) NOT NULL,
    version_number INT NOT NULL,
    prompt_text TEXT NOT NULL,
    description TEXT,
    change_reason TEXT,
    performance_score FLOAT,
    status VARCHAR(20) DEFAULT 'draft',
    parent_version_id UUID,
    created_by VARCHAR(50) DEFAULT 'system',
    approved_by VARCHAR(50),
    approved_at TIMESTAMPTZ,
    activated_at TIMESTAMPTZ,
    created_at TIMESTAMPTZ DEFAULT NOW(),
    UNIQUE(agent_id, version_number),
    FOREIGN KEY (parent_version_id) REFERENCES prompt_versions(id) ON DELETE SET NULL
);

CREATE INDEX IF NOT EXISTS idx_prompt_versions_agent ON prompt_versions(agent_id);
CREATE INDEX IF NOT EXISTS idx_prompt_versions_status ON prompt_versions(status);
CREATE INDEX IF NOT EXISTS idx_prompt_versions_version ON prompt_versions(agent_id, version_number DESC);

-- ============================================================================
-- Strategy Versions (策略版本管理)
-- ============================================================================
CREATE TABLE IF NOT EXISTS strategy_versions (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    name VARCHAR(100) NOT NULL,
    version_number INT NOT NULL,
    strategy_type VARCHAR(50) NOT NULL,
    parameters JSONB NOT NULL DEFAULT '{}'::jsonb,
    risk_params JSONB DEFAULT '{}'::jsonb,
    description TEXT,
    change_reason TEXT,
    backtest_score FLOAT,
    live_score FLOAT,
    status VARCHAR(20) DEFAULT 'draft',
    parent_version_id UUID,
    created_by VARCHAR(50) DEFAULT 'system',
    approved_by VARCHAR(50),
    approved_at TIMESTAMPTZ,
    activated_at TIMESTAMPTZ,
    created_at TIMESTAMPTZ DEFAULT NOW(),
    UNIQUE(name, version_number),
    FOREIGN KEY (parent_version_id) REFERENCES strategy_versions(id) ON DELETE SET NULL
);

CREATE INDEX IF NOT EXISTS idx_strategy_versions_name ON strategy_versions(name);
CREATE INDEX IF NOT EXISTS idx_strategy_versions_status ON strategy_versions(status);
CREATE INDEX IF NOT EXISTS idx_strategy_versions_type ON strategy_versions(strategy_type);

-- ============================================================================
-- Reflection Logs (反思日志 - 自提升专用)
-- ============================================================================
CREATE TABLE IF NOT EXISTS reflection_logs (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    reflection_type VARCHAR(50) NOT NULL,
    trigger VARCHAR(100),
    scope VARCHAR(50) DEFAULT 'fund_manager',
    observations JSONB DEFAULT '[]'::jsonb,
    lessons_learned JSONB DEFAULT '[]'::jsonb,
    proposed_changes JSONB DEFAULT '[]'::jsonb,
    applied_changes JSONB DEFAULT '[]'::jsonb,
    effectiveness_score FLOAT,
    status VARCHAR(20) DEFAULT 'pending',
    reviewed_by VARCHAR(50),
    reviewed_at TIMESTAMPTZ,
    created_at TIMESTAMPTZ DEFAULT NOW(),
    updated_at TIMESTAMPTZ DEFAULT NOW()
);

CREATE INDEX IF NOT EXISTS idx_reflection_logs_type ON reflection_logs(reflection_type);
CREATE INDEX IF NOT EXISTS idx_reflection_logs_status ON reflection_logs(status);
CREATE INDEX IF NOT EXISTS idx_reflection_logs_scope ON reflection_logs(scope);
CREATE INDEX IF NOT EXISTS idx_reflection_logs_created ON reflection_logs(created_at DESC);

-- ============================================================================
-- Evolution Logs (进化日志)
-- ============================================================================
CREATE TABLE IF NOT EXISTS evolution_logs (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    evolution_type VARCHAR(50) NOT NULL,
    target_agent VARCHAR(100),
    from_version INT,
    to_version INT,
    change_description TEXT NOT NULL,
    change_data JSONB DEFAULT '{}'::jsonb,
    rationale TEXT,
    expected_improvement TEXT,
    actual_improvement FLOAT,
    rollback_available BOOLEAN DEFAULT TRUE,
    rolled_back BOOLEAN DEFAULT FALSE,
    rolled_back_at TIMESTAMPTZ,
    rolled_back_reason TEXT,
    status VARCHAR(20) DEFAULT 'applied',
    created_at TIMESTAMPTZ DEFAULT NOW()
);

CREATE INDEX IF NOT EXISTS idx_evolution_logs_type ON evolution_logs(evolution_type);
CREATE INDEX IF NOT EXISTS idx_evolution_logs_agent ON evolution_logs(target_agent);
CREATE INDEX IF NOT EXISTS idx_evolution_logs_status ON evolution_logs(status);
CREATE INDEX IF NOT EXISTS idx_evolution_logs_created ON evolution_logs(created_at DESC);

-- ============================================================================
-- Agent Definitions (Agent 定义表 - 第 5.2 节缺失表)
-- ============================================================================
CREATE TABLE IF NOT EXISTS agent_definitions (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    agent_id VARCHAR(100) UNIQUE NOT NULL,
    name VARCHAR(100) NOT NULL,
    department VARCHAR(50) NOT NULL,
    role VARCHAR(100) NOT NULL,
    reference_institution VARCHAR(200),
    personality_traits JSONB DEFAULT '[]'::jsonb,
    base_credibility_score FLOAT DEFAULT 0.5,
    current_credibility_score FLOAT DEFAULT 0.5,
    calibration_factor FLOAT DEFAULT 1.0,
    prompt_template TEXT,
    active_prompt_version_id UUID,
    is_active BOOLEAN DEFAULT TRUE,
    created_at TIMESTAMPTZ DEFAULT NOW(),
    updated_at TIMESTAMPTZ DEFAULT NOW(),
    FOREIGN KEY (active_prompt_version_id) REFERENCES prompt_versions(id) ON DELETE SET NULL
);

CREATE INDEX IF NOT EXISTS idx_agent_definitions_agent_id ON agent_definitions(agent_id);
CREATE INDEX IF NOT EXISTS idx_agent_definitions_department ON agent_definitions(department);
CREATE INDEX IF NOT EXISTS idx_agent_definitions_active ON agent_definitions(is_active);

-- ============================================================================
-- Department Reports (部门共识报告 - 独立表，第 5.2 节)
-- ============================================================================
CREATE TABLE IF NOT EXISTS department_reports (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    session_id UUID NOT NULL,
    department VARCHAR(50) NOT NULL,
    consensus_sentiment VARCHAR(20),
    consensus_confidence FLOAT,
    bull_count INT DEFAULT 0,
    bear_count INT DEFAULT 0,
    neutral_count INT DEFAULT 0,
    key_factors JSONB DEFAULT '[]'::jsonb,
    summary TEXT,
    agent_contributions JSONB DEFAULT '[]'::jsonb,
    created_at TIMESTAMPTZ DEFAULT NOW()
);

CREATE INDEX IF NOT EXISTS idx_department_reports_session ON department_reports(session_id);
CREATE INDEX IF NOT EXISTS idx_department_reports_dept ON department_reports(department);

-- ============================================================================
-- Fund Manager Decisions (独立表 - 第 5.2 节，原为 JSONB 列)
-- 注意：007 已创建 fund_manager_decisions 表，此处补充索引
-- ============================================================================
CREATE INDEX IF NOT EXISTS idx_fund_manager_decisions_session ON fund_manager_decisions(session_id);
CREATE INDEX IF NOT EXISTS idx_fund_manager_decisions_symbol ON fund_manager_decisions(symbol);
CREATE INDEX IF NOT EXISTS idx_fund_manager_decisions_action ON fund_manager_decisions(action);
CREATE INDEX IF NOT EXISTS idx_fund_manager_decisions_created ON fund_manager_decisions(timestamp DESC);

-- ============================================================================
-- Agent Sessions (分析会话表 - 第 5.2 节)
-- 注：现有 debate_sessions 已覆盖此功能，添加补充索引
-- ============================================================================
CREATE INDEX IF NOT EXISTS idx_debate_sessions_user ON debate_sessions(user_id);
CREATE INDEX IF NOT EXISTS idx_debate_sessions_symbol ON debate_sessions(symbol);
CREATE INDEX IF NOT EXISTS idx_debate_sessions_status ON debate_sessions(status);
CREATE INDEX IF NOT EXISTS idx_debate_sessions_created ON debate_sessions(created_at DESC);

# MoneyRobert Pro - Agent System 测试报告

**项目**: MoneyRobert Pro  
**测试日期**: 2026-05-31  
**测试范围**: Agent 辩论系统、基金经理决策系统、模拟交易引擎、晋级系统、Level 3 自主交易引擎

---

## 📋 目录

1. [测试环境说明](#测试环境说明)
2. [后端编译测试](#后端编译测试)
3. [单元测试清单](#单元测试清单)
4. [集成测试清单](#集成测试清单)
5. [API 端点测试](#api-端点测试)
6. [前端编译测试](#前端编译测试)
7. [数据库迁移验证](#数据库迁移验证)
8. [手动测试用例](#手动测试用例)
9. [性能测试计划](#性能测试计划)
10. [安全测试计划](#安全测试计划)
11. [部署前检查清单](#部署前检查清单)

---

## 1. 测试环境说明

### 1.1 系统要求

#### 硬件要求
- **CPU**: 4 核以上
- **内存**: 8GB 以上
- **磁盘**: 20GB 以上可用空间

#### 软件要求
- **Rust**: 1.75.0 或更高
- **Node.js**: 20.x 或更高
- **PostgreSQL**: 15.x 或更高
- **Redis**: 7.x 或更高
- **Docker**: 24.x 或更高（可选）

### 1.2 环境变量配置

创建 `backend/.env` 文件：

```env
# 数据库配置
DATABASE_URL=postgres://moneyrobert:password@localhost:5432/moneyrobert

# Redis 配置
REDIS_URL=redis://localhost:6379

# JWT 密钥
JWT_SECRET=your-super-secret-jwt-key-change-in-production

# OKX API 配置（可选）
OKX_API_KEY=your-okx-api-key
OKX_API_SECRET=your-okx-api-secret
OKX_API_PASSPHRASE=your-okx-passphrase
OKX_USE_SANDBOX=true

# LLM 配置
OPENAI_API_KEY=your-openai-api-key
LLM_PROVIDER=openai
LLM_MODEL=gpt-4-turbo

# 应用配置
RUST_LOG=info
APP_ENV=development
```

---

## 2. 后端编译测试

### 2.1 编译命令

```bash
cd backend

# 检查代码（不编译）
cargo check

# 编译（开发模式）
cargo build

# 编译（发布模式）
cargo build --release

# 格式化检查
cargo fmt --check

# 静态分析
cargo clippy -- -D warnings
```

### 2.2 预期编译结果

#### ✅ 应成功编译的文件

| 模块 | 文件 | 状态 |
|------|------|------|
| Agent 核心模型 | `src/agents/models.rs` | ✅ 应编译成功 |
| 错误处理 | `src/agents/errors.rs` | ✅ 应编译成功 |
| 配置管理 | `src/agents/config.rs` | ✅ 应编译成功 |
| 辩论引擎 | `src/agents/debate.rs` | ✅ 应编译成功 |
| 基金经理 | `src/agents/agents.rs` | ✅ 应编译成功 |
| 模拟交易 | `src/agents/simulation.rs` | ✅ 应编译成功 |
| 晋级系统 | `src/agents/promotion.rs` | ✅ 应编译成功 |
| 自主引擎 | `src/agents/autonomous.rs` | ✅ 应编译成功 |
| 市场数据 | `src/agents/market.rs` | ✅ 应编译成功 |
| 风险检查 | `src/agents/risk.rs` | ✅ 应编译成功 |
| 通知服务 | `src/agents/notification.rs` | ✅ 应编译成功 |
| API 路由 | `src/routes/agent_simulation.rs` | ✅ 应编译成功 |

---

## 3. 单元测试清单

### 3.1 辩论引擎测试 (`backend/tests/agents/debate_tests.rs`)

#### 测试用例

| ID | 测试名称 | 测试内容 | 预期结果 |
|----|---------|---------|---------|
| DEBATE-001 | `test_k_line_agent_creation` | 创建 K 线形态分析师 | ✅ 成功创建，属性正确 |
| DEBATE-002 | `test_technical_indicator_agent_creation` | 创建技术指标分析师 | ✅ 成功创建，属性正确 |
| DEBATE-003 | `test_funding_rate_agent_creation` | 创建资金费率分析师 | ✅ 成功创建，属性正确 |
| DEBATE-004 | `test_sentiment_agent_creation` | 创建舆情情绪分析师 | ✅ 成功创建，属性正确 |
| DEBATE-005 | `test_agent_sentiment_variants` | 测试情感枚举变体 | ✅ 所有变体可用 |
| DEBATE-006 | `test_department_variants` | 测试部门枚举变体 | ✅ 所有变体可用 |
| DEBATE-007 | `test_debate_status_variants` | 测试辩论状态枚举 | ✅ 所有变体可用 |

#### 运行命令

```bash
cd backend
cargo test debate_tests -- --nocapture
```

#### 预期输出

```
running 7 tests
test debate_tests::test_k_line_agent_creation ... ok
test debate_tests::test_technical_indicator_agent_creation ... ok
test debate_tests::test_funding_rate_agent_creation ... ok
test debate_tests::test_sentiment_agent_creation ... ok
test debate_tests::test_agent_sentiment_variants ... ok
test debate_tests::test_department_variants ... ok
test debate_tests::test_debate_status_variants ... ok

test result: ok. 7 passed; 0 failed
```

### 3.2 基金经理决策测试 (`backend/tests/agents/decision_tests.rs`)

#### 测试用例

| ID | 测试名称 | 测试内容 | 预期结果 |
|----|---------|---------|---------|
| DEC-001 | `test_fund_manager_creation` | 创建基金经理实例 | ✅ 成功创建 |
| DEC-002 | `test_decision_action_variants` | 测试决策动作枚举 | ✅ 所有变体可用 |
| DEC-003 | `test_credibility_weight_calculation` | 测试可信度权重计算 | ✅ 计算正确 |
| DEC-004 | `test_position_size_calculation` | 测试仓位大小计算 | ✅ 计算正确 |
| DEC-005 | `test_stop_loss_calculation` | 测试止损价格计算 | ✅ 计算正确 |
| DEC-006 | `test_risk_assessment_generation` | 测试风险评估生成 | ✅ 评估正确 |

#### 运行命令

```bash
cargo test decision_tests -- --nocapture
```

### 3.3 模拟交易测试 (`backend/tests/agents/simulation_tests.rs`)

#### 测试用例

| ID | 测试名称 | 测试内容 | 预期结果 |
|----|---------|---------|---------|
| SIM-001 | `test_simulation_config_creation` | 创建模拟配置 | ✅ 配置正确 |
| SIM-002 | `test_simulation_trade_creation` | 创建模拟交易记录 | ✅ 记录正确 |
| SIM-003 | `test_pnl_calculation_profit` | 测试盈利 PnL 计算 | ✅ 计算正确 |
| SIM-004 | `test_pnl_calculation_loss` | 测试亏损 PnL 计算 | ✅ 计算正确 |
| SIM-005 | `test_stop_loss_trigger_long` | 测试多头止损触发 | ✅ 正确判断 |
| SIM-006 | `test_take_profit_trigger_long` | 测试多头止盈触发 | ✅ 正确判断 |
| SIM-007 | `test_execution_mode_variants` | 测试执行模式枚举 | ✅ 所有变体可用 |

#### 运行命令

```bash
cargo test simulation_tests -- --nocapture
```

### 3.4 晋级系统测试 (`backend/tests/agents/promotion_tests.rs`)

#### 测试用例

| ID | 测试名称 | 测试内容 | 预期结果 |
|----|---------|---------|---------|
| PROM-001 | `test_level_zero_requirements` | 测试 L0 晋级条件 | ✅ 条件判断正确 |
| PROM-002 | `test_level_one_requirements` | 测试 L1 晋级条件 | ✅ 条件判断正确 |
| PROM-003 | `test_level_two_requirements` | 测试 L2 晋级条件 | ✅ 条件判断正确 |
| PROM-004 | `test_level_three_requirements` | 测试 L3 晋级条件 | ✅ 条件判断正确 |
| PROM-005 | `test_promotion_eligibility_calculation` | 测试晋级资格计算 | ✅ 计算正确 |
| PROM-006 | `test_demotion_trigger_detection` | 测试降级触发检测 | ✅ 检测正确 |
| PROM-007 | `test_rolling_stats_calculation` | 测试滚动统计计算 | ✅ 计算正确 |

#### 运行命令

```bash
cargo test promotion_tests -- --nocapture
```

### 3.5 风险检查测试 (`backend/tests/agents/risk_tests.rs`)

#### 测试用例

| ID | 测试名称 | 测试内容 | 预期结果 |
|----|---------|---------|---------|
| RISK-001 | `test_position_size_check` | 测试仓位大小检查 | ✅ 检查正确 |
| RISK-002 | `test_leverage_check` | 测试杠杆倍数检查 | ✅ 检查正确 |
| RISK-003 | `test_daily_loss_limit_check` | 测试每日亏损限制 | ✅ 检查正确 |
| RISK-004 | `test_weekly_loss_limit_check` | 测试每周亏损限制 | ✅ 检查正确 |
| RISK-005 | `test_trade_frequency_check` | 测试交易频率限制 | ✅ 检查正确 |
| RISK-006 | `test_risk_level_variants` | 测试风险级别枚举 | ✅ 所有变体可用 |
| RISK-007 | `test_circuit_breaker_config` | 测试熔断配置 | ✅ 配置正确 |

#### 运行命令

```bash
cargo test risk_tests -- --nocapture
```

### 3.6 自主交易测试 (`backend/tests/agents/autonomous_tests.rs`)

#### 测试用例

| ID | 测试名称 | 测试内容 | 预期结果 |
|----|---------|---------|---------|
| AUTO-001 | `test_autonomous_config_defaults` | 测试自主配置默认值 | ✅ 默认值正确 |
| AUTO-002 | `test_autonomous_status_defaults` | 测试自主状态默认值 | ✅ 默认值正确 |
| AUTO-003 | `test_engine_state_variants` | 测试引擎状态枚举 | ✅ 所有变体可用 |
| AUTO-004 | `test_circuit_breaker_state_variants` | 测试熔断状态枚举 | ✅ 所有变体可用 |
| AUTO-005 | `test_notification_level_variants` | 测试通知级别枚举 | ✅ 所有变体可用 |
| AUTO-006 | `test_severity_variants` | 测试严重程度枚举 | ✅ 所有变体可用 |
| AUTO-007 | `test_portfolio_context_creation` | 测试投资组合上下文创建 | ✅ 创建成功 |

#### 运行命令

```bash
cargo test autonomous_tests -- --nocapture
```

### 3.7 完整测试运行

```bash
cd backend

# 运行所有 Agent 相关测试
cargo test agents -- --nocapture

# 运行所有测试（包括现有测试）
cargo test -- --nocapture

# 运行测试并生成覆盖率报告（需要 tarpaulin）
cargo tarpaulin --out Html --report-dir coverage/
```

---

## 4. 集成测试清单

### 4.1 数据库集成测试

#### 测试用例

| ID | 测试名称 | 测试内容 | 预期结果 |
|----|---------|---------|---------|
| DB-INT-001 | `test_ai_simulation_config_crud` | 测试配置 CRUD 操作 | ✅ 创建/读取/更新/删除成功 |
| DB-INT-002 | `test_ai_simulation_trade_crud` | 测试交易记录 CRUD | ✅ 创建/读取/更新/删除成功 |
| DB-INT-003 | `test_debate_session_crud` | 测试辩论会话 CRUD | ✅ 创建/读取/更新/删除成功 |
| DB-INT-004 | `test_promotion_audit_crud` | 测试晋级审核 CRUD | ✅ 创建/读取/更新/删除成功 |
| DB-INT-005 | `test_autonomous_decision_log_crud` | 测试决策日志 CRUD | ✅ 创建/读取/更新/删除成功 |

### 4.2 辩论系统集成测试

#### 测试用例

| ID | 测试名称 | 测试内容 | 预期结果 |
|----|---------|---------|---------|
| DEBATE-INT-001 | `test_full_debate_flow` | 完整辩论流程 | ✅ 12 个 Agent 参与并产生决策 |
| DEBATE-INT-002 | `test_cross_department_debate` | 跨部门辩论 | ✅ 部门间观点交换 |
| DEBATE-INT-003 | `test_debate_message_ordering` | 消息顺序验证 | ✅ 消息按时间顺序排列 |
| DEBATE-INT-004 | `test_debate_session_persistence` | 会话持久化 | ✅ 会话正确保存到数据库 |

### 4.3 模拟交易集成测试

#### 测试用例

| ID | 测试名称 | 测试内容 | 预期结果 |
|----|---------|---------|---------|
| SIM-INT-001 | `test_paper_trade_execution` | Paper 模式交易执行 | ✅ 虚拟账户正确扣款 |
| SIM-INT-002 | `test_demo_trade_execution` | Demo 模式交易执行 | ✅ OKX 模拟盘正确执行 |
| SIM-INT-003 | `test_live_trade_execution` | Live 模式交易执行 | ✅ OKX 实盘正确执行 |
| SIM-INT-004 | `test_stop_loss_auto_trigger` | 止损自动触发 | ✅ 价格触及止损时自动平仓 |
| SIM-INT-005 | `test_take_profit_auto_trigger` | 止盈自动触发 | ✅ 价格触及止盈时自动平仓 |

### 4.4 晋级系统集成测试

#### 测试用例

| ID | 测试名称 | 测试内容 | 预期结果 |
|----|---------|---------|---------|
| PROM-INT-001 | `test_level_0_to_1_promotion` | L0 → L1 晋级 | ✅ 满足条件后自动晋级 |
| PROM-INT-002 | `test_level_1_to_2_promotion` | L1 → L2 晋级 | ✅ 满足条件后进入审核队列 |
| PROM-INT-003 | `test_level_2_to_3_promotion` | L2 → L3 晋级 | ✅ 需要二级审核和风险确认书 |
| PROM-INT-004 | `test_auto_demotion_on_loss` | 亏损自动降级 | ✅ 胜率低于阈值时降级 |
| PROM-INT-005 | `test_observation_period` | 观察期管理 | ✅ 观察期内限制更严格 |

---

## 5. API 端点测试

### 5.1 模拟交易 API

#### 测试端点

| 方法 | 端点 | 测试用例 | 预期结果 |
|------|------|---------|---------|
| POST | `/api/v1/agent/simulation/start` | 启动模拟交易 | ✅ 返回配置和状态 |
| POST | `/api/v1/agent/simulation/stop` | 停止模拟交易 | ✅ 状态更新为 stopped |
| GET | `/api/v1/agent/simulation/status` | 获取模拟状态 | ✅ 返回当前状态 |
| GET | `/api/v1/agent/simulation/trades` | 获取交易记录 | ✅ 返回交易列表 |
| GET | `/api/v1/agent/simulation/stats` | 获取统计数据 | ✅ 返回统计数据 |
| GET | `/api/v1/agent/simulation/level` | 获取等级进度 | ✅ 返回等级和晋级条件 |
| POST | `/api/v1/agent/simulation/reset` | 重置模拟账户 | ✅ 账户重置为初始状态 |

### 5.2 辩论系统 API

#### 测试端点

| 方法 | 端点 | 测试用例 | 预期结果 |
|------|------|---------|---------|
| POST | `/api/v1/agent/debate/start` | 启动辩论会话 | ✅ 返回会话 ID 和状态 |
| GET | `/api/v1/agent/debate/:id` | 获取辩论详情 | ✅ 返回辩论内容和决策 |
| GET | `/api/v1/agent/debate/:id/messages` | 获取辩论消息 | ✅ 返回消息列表 |

### 5.3 晋级系统 API

#### 测试端点

| 方法 | 端点 | 测试用例 | 预期结果 |
|------|------|---------|---------|
| GET | `/api/v1/agent/promotion/status` | 获取晋级状态 | ✅ 返回晋级进度 |
| POST | `/api/v1/agent/promotion/approve` | 审批晋级 | ✅ 更新状态和等级 |
| POST | `/api/v1/agent/promotion/reject` | 拒绝晋级 | ✅ 更新状态并记录原因 |
| GET | `/api/v1/agent/promotion/audit-report` | 获取审核报告 | ✅ 返回完整审核报告 |

### 5.4 自主交易 API

#### 测试端点

| 方法 | 端点 | 测试用例 | 预期结果 |
|------|------|---------|---------|
| POST | `/api/v1/agent/autonomous/start` | 启动自主交易 | ✅ 引擎开始运行 |
| POST | `/api/v1/agent/autonomous/stop` | 停止自主交易 | ✅ 引擎停止 |
| GET | `/api/v1/agent/autonomous/status` | 获取自主状态 | ✅ 返回运行状态 |
| GET | `/api/v1/agent/autonomous/decision-log` | 获取决策日志 | ✅ 返回决策历史 |
| POST | `/api/v1/agent/emergency/stop` | 紧急停止 | ✅ 所有交易立即停止 |

### 5.5 API 测试脚本

创建 `backend/tests/api_tests.rs`：

```rust
#[cfg(test)]
mod api_tests {
    use super::*;
    use axum::{body::Body, http::Request};
    use tower::ServiceExt;

    #[tokio::test]
    async fn test_simulation_start_endpoint() {
        let app = router().await;
        
        let response = app
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/api/v1/agent/simulation/start")
                    .header("Authorization", "Bearer <valid_token>")
                    .header("Content-Type", "application/json")
                    .body(Body::from(r#"{"symbol": "DOGE-USDT-SWAP"}"#))
                    .unwrap(),
            )
            .await
            .unwrap();
        
        assert_eq!(response.status(), StatusCode::OK);
    }
    
    // ... 更多测试
}
```

---

## 6. 前端编译测试

### 6.1 依赖安装

```bash
cd frontend
npm install
```

### 6.2 编译检查

```bash
# 开发模式编译
npm run dev

# 生产模式编译
npm run build

# 类型检查
npm run type-check

# ESLint 检查
npm run lint
```

### 6.3 预期结果

| 命令 | 预期结果 | 状态 |
|------|---------|------|
| `npm install` | 所有依赖安装成功 | ⏳ 待测试 |
| `npm run dev` | 开发服务器启动 | ⏳ 待测试 |
| `npm run build` | 生产构建成功 | ⏳ 待测试 |
| `npm run type-check` | 无类型错误 | ⏳ 待测试 |
| `npm run lint` | 无 ESLint 错误 | ⏳ 待测试 |

### 6.4 前端页面测试清单

| 页面 | 路由 | 测试内容 | 预期结果 |
|------|------|---------|---------|
| Agent 仪表盘 | `/agent` | 等级显示、操作按钮 | ✅ 界面正常 |
| 辩论查看器 | `/agent/debate` | 辩论流程展示 | ✅ 实时更新 |
| 交易历史 | `/agent/history` | 交易列表和图表 | ✅ 数据正确 |

---

## 7. 数据库迁移验证

### 7.1 迁移脚本检查

```bash
cd backend

# 运行迁移
sqlx migrate run

# 检查迁移状态
sqlx migrate info

# 回滚最后一次迁移
sqlx migrate revert
```

### 7.2 新增表清单

| 表名 | 说明 | 索引数 |
|------|------|--------|
| `ai_simulation_configs` | AI 模拟操盘配置 | 4 |
| `ai_simulation_trades` | AI 模拟交易记录 | 4 |
| `agent_debate_sessions` | Agent 辩论会话 | 3 |
| `agent_debate_messages` | Agent 辩论消息 | 3 |
| `promotion_audits` | 晋级审核记录 | 2 |
| `demotion_records` | 降级记录 | 1 |
| `daily_simulation_stats` | 每日统计快照 | 2 |
| `agent_performance` | Agent 性能记录 | 0 |
| `knowledge_nodes` | 知识节点 | 2 |
| `knowledge_links` | 知识关联 | 2 |
| `autonomous_decision_logs` | 自主决策日志 | 2 |
| `circuit_break_records` | 熔断记录 | 2 |
| `risk_confirmations` | 风险确认书 | 2 |
| `emergency_stop_records` | 紧急停止记录 | 2 |
| `backtest_results` | 回测结果 | 2 |

### 7.3 表结构验证查询

```sql
-- 验证表是否存在
SELECT table_name 
FROM information_schema.tables 
WHERE table_schema = 'public' 
AND table_name LIKE 'ai_%' 
   OR table_name LIKE 'agent_%' 
   OR table_name LIKE 'promotion_%' 
   OR table_name LIKE 'demotion_%';

-- 验证关键字段
\d ai_simulation_configs
\d ai_simulation_trades
\d agent_debate_sessions
```

---

## 8. 手动测试用例

### 8.1 辩论系统手动测试

#### 测试场景 1: 启动完整辩论

**步骤**:
1. 登录系统
2. 进入 Agent 仪表盘
3. 点击"启动辩论"按钮
4. 观察辩论过程
5. 查看最终决策

**预期结果**:
- ✅ 辩论启动成功
- ✅ 12 个 Agent 依次发言
- ✅ 显示各 Agent 的分析和置信度
- ✅ 显示最终的基金经理决策
- ✅ 决策包含做多/做空/观望和理由

#### 测试场景 2: 跨部门辩论验证

**步骤**:
1. 启动辩论
2. 观察技术部门和资金部门的观点差异
3. 观察新闻部门的信息整合

**预期结果**:
- ✅ 技术部门关注 K 线形态和技术指标
- ✅ 资金部门关注资金费率、持仓结构
- ✅ 新闻部门关注舆情和宏观事件
- ✅ 跨部门辩论时观点碰撞明显

### 8.2 晋级系统手动测试

#### 测试场景 1: Level 0 → Level 1 晋级

**步骤**:
1. 启动 Paper 模式模拟交易
2. 积累至少 30 笔交易
3. 保持胜率 ≥ 80%
4. 运行 14 天以上
5. 观察晋级提示

**预期结果**:
- ✅ 系统显示晋级条件进度
- ✅ 满足条件后自动晋级
- ✅ 晋级无需人工审核

#### 测试场景 2: Level 1 → Level 2 晋级

**步骤**:
1. 在 Level 1 模式运行
2. 积累至少 30 笔交易
3. 保持胜率 ≥ 90%
4. 运行 7 天以上
5. 观察晋级审核队列

**预期结果**:
- ✅ 满足条件后进入审核队列
- ✅ 管理员可查看审核报告
- ✅ 管理员可批准或拒绝

#### 测试场景 3: Level 2 → Level 3 晋级

**步骤**:
1. 在 Level 2 模式运行
2. 积累至少 100 笔交易
3. 保持胜率 ≥ 80%
4. 运行 90 天以上
5. 签署风险确认书
6. 通过二级审核

**预期结果**:
- ✅ 需要签署风险确认书
- ✅ 需要一级和二级管理员审核
- ✅ 通过后进入 14 天观察期
- ✅ 观察期通过后正式启用 Level 3

### 8.3 Level 3 自主交易手动测试

#### 测试场景 1: 自主交易启动

**步骤**:
1. 达到 Level 3 资格
2. 进入自主交易页面
3. 点击"启动自主交易"
4. 观察 AI 自主决策过程

**预期结果**:
- ✅ AI 自主分析市场
- ✅ AI 自主决定买入/卖出
- ✅ 无需人工确认
- ✅ 实时推送决策通知

#### 测试场景 2: 紧急停止

**步骤**:
1. 自主交易运行中
2. 点击"紧急停止"按钮
3. 观察系统响应

**预期结果**:
- ✅ 所有交易立即停止
- ✅ 发送紧急通知
- ✅ 需要人工介入才能恢复

#### 测试场景 3: 熔断触发

**步骤**:
1. 自主交易运行中
2. 触发单日亏损 > 3%
3. 观察熔断响应

**预期结果**:
- ✅ 自动暂停自主交易
- ✅ 降级到 Level 2
- ✅ 发送告警通知

---

## 9. 性能测试计划

### 9.1 负载测试

#### 测试场景: 并发辩论会话

```bash
# 使用 Apache Bench
ab -n 1000 -c 100 -T 'application/json' \
   -H 'Authorization: Bearer <token>' \
   -p debate_request.json \
   http://localhost:3000/api/v1/agent/debate/start
```

#### 性能指标

| 指标 | 目标值 | 可接受值 |
|------|--------|---------|
| 响应时间 (p50) | < 500ms | < 1s |
| 响应时间 (p95) | < 2s | < 5s |
| 响应时间 (p99) | < 5s | < 10s |
| 错误率 | < 0.1% | < 1% |
| 并发用户数 | 100 | 50 |

### 9.2 压力测试

#### 测试场景: 模拟交易高频执行

```bash
# 使用 k6
k6 run scripts/simulation_stress_test.js
```

### 9.3 内存泄漏检测

```bash
# 使用 Valgrind (Linux/macOS)
valgrind --leak-check=full cargo run

# 使用 AddressSanitizer
RUSTFLAGS="-Z sanitizer=address" cargo test
```

---

## 10. 安全测试计划

### 10.1 认证和授权测试

| 测试项 | 测试内容 | 预期结果 |
|--------|---------|---------|
| 认证 | 未登录访问 API | ❌ 返回 401 |
| 认证 | Token 过期 | ❌ 返回 401 |
| 授权 | 普通用户访问管理 API | ❌ 返回 403 |
| 授权 | Level 0 用户尝试 Level 3 操作 | ❌ 返回 403 |

### 10.2 输入验证测试

| 测试项 | 测试内容 | 预期结果 |
|--------|---------|---------|
| SQL 注入 | 在 symbol 参数中注入 SQL | ❌ 被阻止 |
| XSS | 在内容字段中注入脚本 | ❌ 被转义 |
| 参数篡改 | 修改 JWT 中的 level | ❌ 被拒绝 |

### 10.3 敏感数据测试

| 测试项 | 测试内容 | 预期结果 |
|--------|---------|---------|
| 密码暴露 | API 返回密码 | ❌ 不返回 |
| API 密钥暴露 | API 返回密钥 | ❌ 不返回 |
| 审计日志 | 记录所有敏感操作 | ✅ 记录 |

---

## 11. 部署前检查清单

### 11.1 开发环境检查

- [ ] Rust 编译器安装
- [ ] Node.js 安装
- [ ] PostgreSQL 安装和配置
- [ ] Redis 安装和配置
- [ ] 环境变量配置完成

### 11.2 代码检查

- [ ] `cargo check` 通过
- [ ] `cargo clippy` 无警告
- [ ] `cargo fmt` 格式化正确
- [ ] 所有单元测试通过
- [ ] 所有集成测试通过

### 11.3 前端检查

- [ ] `npm install` 成功
- [ ] `npm run build` 成功
- [ ] 无 TypeScript 类型错误
- [ ] 无 ESLint 错误

### 11.4 数据库检查

- [ ] 迁移脚本语法正确
- [ ] 所有表创建成功
- [ ] 索引创建成功
- [ ] 初始数据插入成功

### 11.5 安全检查

- [ ] JWT 密钥已更改
- [ ] API 密钥已配置
- [ ] CORS 配置正确
- [ ] 速率限制配置正确

### 11.6 部署配置

- [ ] Docker 镜像构建成功
- [ ] docker-compose 配置正确
- [ ] Nginx 配置正确
- [ ] HTTPS 证书配置（生产环境）

---

## 📊 测试执行总结

### 测试覆盖统计

| 模块 | 单元测试 | 集成测试 | API 测试 |
|------|---------|---------|---------|
| 辩论引擎 | 7 | 4 | 3 |
| 基金经理 | 6 | 3 | 2 |
| 模拟交易 | 7 | 5 | 7 |
| 晋级系统 | 7 | 5 | 4 |
| 风险检查 | 7 | 2 | 2 |
| 自主交易 | 7 | 3 | 5 |
| **总计** | **41** | **22** | **23** |

### 手动测试用例

| 模块 | 测试用例数 |
|------|-----------|
| 辩论系统 | 2 |
| 晋级系统 | 3 |
| 自主交易 | 3 |
| **总计** | **8** |

### 测试执行命令

```bash
# 1. 后端编译检查
cd backend
cargo check

# 2. 后端单元测试
cargo test --lib

# 3. 后端集成测试
cargo test --test '*_tests'

# 4. 前端依赖安装
cd frontend
npm install

# 5. 前端类型检查
npm run type-check

# 6. 前端构建
npm run build

# 7. 数据库迁移
cd backend
sqlx migrate run

# 8. 启动服务（开发）
cargo run

# 9. 启动服务（前端）
cd frontend
npm run dev
```

---

## 📝 测试报告模板

### 测试执行记录

```
测试执行日期: _______________
测试执行人: _______________
环境: _______________

后端编译: [ ] 通过 [ ] 失败
单元测试: [ ] 通过 [ ] 失败 (___/41 通过)
集成测试: [ ] 通过 [ ] 失败 (___/22 通过)
API 测试: [ ] 通过 [ ] 失败 (___/23 通过)
前端编译: [ ] 通过 [ ] 失败
数据库迁移: [ ] 通过 [ ] 失败

问题记录:
1. _________________________________
2. _________________________________
3. _________________________________

总体评估: [ ] 通过 [ ] 有条件通过 [ ] 不通过
签字: _______________
```

---

## 🔧 故障排查指南

### 编译错误

| 错误 | 解决方案 |
|------|---------|
| `cannot find crate` | 运行 `cargo build` |
| `unused import` | 删除未使用的导入 |
| `type mismatch` | 检查类型定义 |

### 测试失败

| 错误 | 解决方案 |
|------|---------|
| `assertion failed` | 检查测试断言逻辑 |
| `timeout` | 增加测试超时时间 |
| `connection refused` | 检查数据库连接 |

### 运行错误

| 错误 | 解决方案 |
|------|---------|
| `database not found` | 运行数据库迁移 |
| `port already in use` | 停止占用端口的进程 |
| `permission denied` | 检查文件权限 |

---

**文档版本**: 1.0  
**最后更新**: 2026-05-31  
**维护人**: MoneyRobert Team

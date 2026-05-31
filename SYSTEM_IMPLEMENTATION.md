# MoneyRobert Pro - Agent System 实现与验证文档

**项目**: MoneyRobert Pro  
**版本**: 1.0  
**日期**: 2026-05-31  
**状态**: ✅ 实现完成，待测试验证

---

## 📋 目录

1. [项目概述](#项目概述)
2. [已完成的功能模块](#已完成的功能模块)
3. [文件清单](#文件清单)
4. [快速开始指南](#快速开始指南)
5. [测试执行指南](#测试执行指南)
6. [API 端点文档](#api-端点文档)
7. [数据库架构](#数据库架构)
8. [前端页面说明](#前端页面说明)
9. [已知问题和限制](#已知问题和限制)
10. [技术支持](#技术支持)

---

## 1. 项目概述

### 1.1 项目目标

MoneyRobert Pro Agent 系统是一个基于多智能体辩论的投资决策系统，通过模拟专业投资机构的分工协作模式，实现：

- 🤖 **多 Agent 协作分析**: 12 个专业 Agent 分属三大部门，协同分析市场
- 📊 **可信度加权决策**: 基于历史表现的动态权重调整
- 🎯 **4 级渐进式权限**: 从纯模拟到完全自主交易
- 🛡️ **多层风控体系**: 仓位、杠杆、亏损、熔断等多重保护
- 📈 **持续自我进化**: 从交易经验中学习和优化策略

### 1.2 技术架构

```
┌─────────────────────────────────────────────────────────────┐
│                      前端 (Vue 3 + TypeScript)               │
│   AgentDashboardPage │ AgentDebateViewer │ TradingHistory   │
└─────────────────────────────────────────────────────────────┘
                              │
                              ▼ HTTP/WebSocket
┌─────────────────────────────────────────────────────────────┐
│                  API 网关 (Axum + Tower)                   │
│         Agent Routes │ Auth │ Trading │ Dashboard          │
└─────────────────────────────────────────────────────────────┘
                              │
        ┌──────────────────────┼──────────────────────┐
        ▼                      ▼                      ▼
┌───────────────┐    ┌─────────────────┐    ┌─────────────────┐
│ Agent Engine  │    │ Simulation      │    │ OKX Exchange    │
│               │    │ Engine          │    │ Client          │
│ - Debate      │    │                 │    │                 │
│ - Decision    │    │ - Paper Mode    │    │ - Real Trading  │
│ - Memory      │    │ - Demo Mode     │    │ - Simulated     │
│ - Evolution   │    │ - Live Mode     │    │                 │
└───────────────┘    └─────────────────┘    └─────────────────┘
                              │
                              ▼
┌─────────────────────────────────────────────────────────────┐
│                   PostgreSQL + Redis                        │
│   Agent Tables │ Market Data │ User Data │ Cache          │
└─────────────────────────────────────────────────────────────┘
```

### 1.3 Agent 部门架构

```
┌─────────────────────────────────────────────────────────────┐
│                  基金经理 (Fund Manager)                    │
│                  参考: Bridgewater / RenTech                │
│                                                             │
│  ┌─────────────────┬─────────────────┬─────────────────┐   │
│  │  技术分析部门     │  资金分析部门     │  新闻分析部门     │   │
│  │  (35% 权重)     │  (35% 权重)     │  (30% 权重)     │   │
│  │                 │                 │                 │   │
│  │ ├ K线形态分析师  │ ├ 资金费率分析师  │ ├ 舆情分析师     │   │
│  │ ├ 技术指标分析师 │ ├ 持仓结构分析师  │ ├ 宏观政策分析师  │   │
│  │ ├ 链上数据分析员 │ ├ 多空博弈分析师  │ ├ KOL/鲸鱼监控  │   │
│  │ └ 量化模型专家   │ └ 流动性分析师   │ └ 事件驱动分析师  │   │
│  └─────────────────┴─────────────────┴─────────────────┘   │
│                                                             │
│  参考机构:                                                   │
│  - Two Sigma (量化)                                        │
│  - Citadel (做市)                                           │
│  - Bloomberg (新闻)                                         │
│  -幻方量化 (技术分析)                                       │
└─────────────────────────────────────────────────────────────┘
```

---

## 2. 已完成的功能模块

### 2.1 后端模块

| 模块 | 文件 | 功能描述 | 状态 |
|------|------|---------|------|
| 核心模型 | `src/agents/models.rs` | 数据结构、枚举、类型定义 | ✅ |
| 错误处理 | `src/agents/errors.rs` | 统一的错误类型和转换 | ✅ |
| 配置管理 | `src/agents/config.rs` | Agent 系统配置加载 | ✅ |
| 辩论引擎 | `src/agents/debate.rs` | 多 Agent 辩论流程 | ✅ |
| 基金经理 | `src/agents/agents.rs` | 决策生成和权重计算 | ✅ |
| 模拟交易 | `src/agents/simulation.rs` | Paper/Demo/Live 执行 | ✅ |
| 晋级系统 | `src/agents/promotion.rs` | 晋级/降级逻辑 | ✅ |
| 自主引擎 | `src/agents/autonomous.rs` | Level 3 自主决策 | ✅ |
| 市场数据 | `src/agents/market.rs` | 数据获取和指标计算 | ✅ |
| 风险检查 | `src/agents/risk.rs` | 风控规则和检查 | ✅ |
| 通知服务 | `src/agents/notification.rs` | WebSocket 通知 | ✅ |
| API 路由 | `src/routes/agent_simulation.rs` | HTTP API 端点 | ✅ |

### 2.2 数据库迁移

| 文件 | 表数量 | 说明 | 状态 |
|------|--------|------|------|
| `005_agent_system_tables.sql` | 15 | Agent 系统完整表结构 | ✅ |

**包含的表**:
- AI 模拟配置和交易记录
- Agent 辩论会话和消息
- 晋级审核和降级记录
- 每日统计快照
- Agent 性能记录
- 知识节点和关联
- 自主决策日志
- 熔断记录
- 风险确认书
- 回测结果

### 2.3 前端模块

| 页面 | 文件 | 功能描述 | 状态 |
|------|------|---------|------|
| Agent 仪表盘 | `AgentDashboardPage.vue` | 等级、统计、快速操作 | ✅ |
| 辩论查看器 | `AgentDebateViewer.vue` | 辩论过程展示 | ✅ |
| 交易历史 | `AgentTradingHistory.vue` | 交易记录和图表 | ✅ |
| 状态管理 | `stores/agent.ts` | Pinia 状态管理 | ✅ |
| API 集成 | `api/index.ts` | Agent API 客户端 | ✅ |

### 2.4 测试套件

| 测试文件 | 测试数量 | 覆盖范围 | 状态 |
|---------|---------|---------|------|
| `debate_tests.rs` | 7 | 辩论引擎核心功能 | ✅ |
| `decision_tests.rs` | 6 | 基金经理决策 | ✅ |
| `simulation_tests.rs` | 7 | 模拟交易逻辑 | ✅ |
| `promotion_tests.rs` | 7 | 晋级/降级机制 | ✅ |
| `risk_tests.rs` | 7 | 风控检查 | ✅ |
| `autonomous_tests.rs` | 7 | 自主交易引擎 | ✅ |
| **总计** | **41** | 核心功能覆盖 | ✅ |

---

## 3. 文件清单

### 3.1 后端文件

```
backend/
├── Cargo.toml                           # 项目依赖配置
├── src/
│   ├── lib.rs                           # 库入口，导出 agents 模块
│   ├── bin/main.rs                      # 主程序入口
│   ├── agents/                          # Agent 系统核心
│   │   ├── mod.rs                       # 模块导出
│   │   ├── models.rs                     # 数据模型 (346 行)
│   │   ├── errors.rs                     # 错误处理 (73 行)
│   │   ├── config.rs                     # 配置管理
│   │   ├── debate.rs                     # 辩论引擎
│   │   ├── agents.rs                     # 基金经理
│   │   ├── simulation.rs                 # 模拟交易
│   │   ├── promotion.rs                  # 晋级系统
│   │   ├── autonomous.rs                 # 自主交易
│   │   ├── market.rs                     # 市场数据
│   │   ├── risk.rs                       # 风险检查
│   │   └── notification.rs               # 通知服务
│   ├── routes/
│   │   ├── mod.rs                        # 路由模块
│   │   └── agent_simulation.rs           # Agent API 路由
│   ├── exchanges/
│   │   ├── mod.rs
│   │   └── okx.rs                        # OKX 交易所客户端
│   └── [其他现有文件...]
├── migrations/
│   ├── 005_agent_system_tables.sql       # Agent 系统迁移 (373 行)
│   └── [其他迁移文件...]
└── tests/agents/                         # Agent 测试套件
    ├── debate_tests.rs                    # 辩论引擎测试
    ├── decision_tests.rs                 # 决策系统测试
    ├── simulation_tests.rs                # 模拟交易测试
    ├── promotion_tests.rs                # 晋级系统测试
    ├── risk_tests.rs                     # 风险检查测试
    └── autonomous_tests.rs               # 自主交易测试
```

### 3.2 前端文件

```
frontend/
├── src/
│   ├── api/
│   │   └── index.ts                      # API 客户端 (Agent 相关)
│   ├── pages/
│   │   ├── AgentDashboardPage.vue         # Agent 仪表盘
│   │   ├── AgentDebateViewer.vue          # 辩论查看器
│   │   └── AgentTradingHistory.vue       # 交易历史
│   ├── stores/
│   │   ├── agent.ts                       # Agent 状态管理
│   │   └── [其他 stores...]
│   ├── router/
│   │   └── index.ts                       # 路由配置 (已更新)
│   └── layouts/
│       └── DashboardLayout.vue            # 布局组件 (已更新)
└── [其他配置文件...]
```

### 3.3 文档文件

```
docs/
└── AGENT_SYSTEM_DESIGN.md                # Agent 系统设计文档

根目录/
├── TEST_REPORT.md                        # 测试报告
├── test_runner.sh                         # Linux/macOS 测试脚本
└── test_runner.ps1                       # Windows PowerShell 测试脚本
```

---

## 4. 快速开始指南

### 4.1 环境要求

- **Rust**: 1.75.0 或更高
- **Node.js**: 20.x 或更高
- **PostgreSQL**: 15.x 或更高
- **Redis**: 7.x 或更高

### 4.2 安装步骤

#### 1. 克隆项目

```bash
git clone https://github.com/your-org/MoneyRobert-Pro.git
cd MoneyRobert-Pro
```

#### 2. 配置环境变量

```bash
# 后端环境变量
cp backend/.env.example backend/.env
# 编辑 backend/.env，填写必要的配置

# 前端环境变量
cp frontend/.env.example frontend/.env
```

#### 3. 数据库设置

```bash
# 创建数据库
createdb moneyrobert

# 运行迁移
cd backend
sqlx migrate run
```

#### 4. 安装前端依赖

```bash
cd frontend
npm install
```

### 4.3 运行测试

#### 方式 1: 使用测试脚本（推荐）

**Windows PowerShell**:
```powershell
.\test_runner.ps1
```

**Linux/macOS**:
```bash
bash test_runner.sh
```

#### 方式 2: 手动测试

**后端测试**:
```bash
cd backend

# 编译检查
cargo check

# 运行所有测试
cargo test -- --nocapture

# 只运行 Agent 测试
cargo test agents -- --nocapture
```

**前端测试**:
```bash
cd frontend

# 类型检查
npm run type-check

# 构建
npm run build
```

### 4.4 启动服务

#### 开发模式

**后端**:
```bash
cd backend
cargo run
```

**前端**:
```bash
cd frontend
npm run dev
```

#### Docker 部署

```bash
# 构建并启动所有服务
docker-compose up -d

# 查看日志
docker-compose logs -f backend
```

---

## 5. 测试执行指南

### 5.1 测试脚本使用说明

#### Windows PowerShell 脚本

```powershell
# 运行完整测试
.\test_runner.ps1

# 跳过后端测试
.\test_runner.ps1 -SkipBackend

# 跳过前端测试
.\test_runner.ps1 -SkipFrontend

# 详细输出
.\test_runner.ps1 -Verbose
```

#### Linux/macOS Bash 脚本

```bash
# 运行完整测试
bash test_runner.sh

# 仅运行后端测试
cd backend && cargo test
```

### 5.2 测试分类

#### 单元测试

单元测试验证各个模块的独立功能，不需要外部依赖。

```bash
# 运行所有单元测试
cargo test --lib

# 运行特定模块测试
cargo test agents --lib
```

#### 集成测试

集成测试验证模块之间的交互，需要完整的开发环境。

```bash
# 运行所有集成测试
cargo test --test '*_tests'

# 运行特定集成测试
cargo test debate_tests
```

#### API 测试

API 测试通过 HTTP 请求验证端点功能。

```bash
# 启动服务器后，运行 API 测试
# (需要单独的 API 测试工具，如 curl 或 Postman)
```

### 5.3 查看测试日志

测试脚本会将日志保存到临时目录：

- Windows: `%TEMP%\cargo_*.log`
- Linux/macOS: `/tmp/cargo_*.log`

### 5.4 常见测试问题

#### 问题 1: `cargo: command not found`

**解决方案**: 安装 Rust
```bash
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
```

#### 问题 2: 数据库连接失败

**解决方案**: 确保 PostgreSQL 正在运行
```bash
# Windows
net start postgresql

# Linux/macOS
sudo systemctl start postgresql
```

#### 问题 3: 前端依赖安装失败

**解决方案**: 清除缓存并重试
```bash
cd frontend
rm -rf node_modules package-lock.json
npm install
```

---

## 6. API 端点文档

### 6.1 模拟交易 API

#### 启动模拟交易

```
POST /api/v1/agent/simulation/start
```

**请求体**:
```json
{
  "symbol": "DOGE-USDT-SWAP",
  "initial_balance": 100000
}
```

**响应**:
```json
{
  "id": "uuid",
  "symbol": "DOGE-USDT-SWAP",
  "mode": "paper",
  "level": 0,
  "status": "running",
  "initial_balance": 100000,
  "current_balance": 100000
}
```

#### 获取模拟状态

```
GET /api/v1/agent/simulation/status
```

**响应**:
```json
{
  "id": "uuid",
  "symbol": "DOGE-USDT-SWAP",
  "mode": "paper",
  "level": 0,
  "status": "running",
  "current_balance": 105000,
  "total_trades": 10,
  "win_rate": 0.7,
  "promotion_eligible": true
}
```

#### 获取交易记录

```
GET /api/v1/agent/simulation/trades
```

**查询参数**:
- `limit`: 每页数量 (默认 20)
- `offset`: 偏移量 (默认 0)
- `status`: 交易状态 (open/closed)

**响应**:
```json
{
  "items": [
    {
      "id": "uuid",
      "direction": "long",
      "entry_price": 0.15,
      "exit_price": 0.16,
      "quantity": 1000,
      "pnl": 10.0,
      "pnl_percent": 6.67,
      "status": "closed",
      "opened_at": "2026-05-31T00:00:00Z",
      "closed_at": "2026-05-31T12:00:00Z"
    }
  ],
  "total": 100,
  "limit": 20,
  "offset": 0
}
```

### 6.2 辩论系统 API

#### 启动辩论

```
POST /api/v1/agent/debate/start
```

**请求体**:
```json
{
  "symbol": "DOGE-USDT-SWAP"
}
```

**响应**:
```json
{
  "id": "uuid",
  "symbol": "DOGE-USDT-SWAP",
  "status": "in_progress",
  "message": "辩论已开始"
}
```

#### 获取辩论详情

```
GET /api/v1/agent/debate/{session_id}
```

**响应**:
```json
{
  "id": "uuid",
  "symbol": "DOGE-USDT-SWAP",
  "status": "completed",
  "final_decision": {
    "action": "long",
    "confidence": 0.85,
    "position_size_percent": 5,
    "leverage": 2,
    "reasoning": "技术面看涨，资金面支持，新闻面利好"
  },
  "created_at": "2026-05-31T00:00:00Z",
  "updated_at": "2026-05-31T00:05:00Z"
}
```

### 6.3 晋级系统 API

#### 获取等级信息

```
GET /api/v1/agent/simulation/level
```

**响应**:
```json
{
  "current_level": 1,
  "current_mode": "demo",
  "promotion_eligible": true,
  "next_level": 2,
  "requirements": {
    "total_trades": 80,
    "required_trades": 30,
    "win_rate": 0.85,
    "required_win_rate": 0.90,
    "running_days": 10,
    "required_days": 7,
    "profit_loss_ratio": 2.1,
    "required_profit_loss_ratio": 2.0
  },
  "progress_percent": 85
}
```

#### 签署风险确认书

```
POST /api/v1/agent/risk/confirmation/sign
```

**请求体**:
```json
{
  "max_acceptable_loss": 1000,
  "accepted": true
}
```

### 6.4 自主交易 API

#### 启动自主交易

```
POST /api/v1/agent/autonomous/start
```

**响应**:
```json
{
  "status": "started",
  "message": "Level 3 自主交易已启动",
  "config": {
    "max_position_size_percent": 10,
    "max_leverage": 5,
    "max_daily_trades": 20,
    "high_confidence_threshold": 0.8
  }
}
```

#### 紧急停止

```
POST /api/v1/agent/emergency/stop
```

**响应**:
```json
{
  "status": "stopped",
  "message": "紧急停止已触发",
  "stopped_at": "2026-05-31T12:00:00Z",
  "reason": "用户手动触发"
}
```

---

## 7. 数据库架构

### 7.1 核心表

#### ai_simulation_configs

AI 模拟操盘配置表，存储每个用户的 Agent 交易配置。

| 字段 | 类型 | 说明 |
|------|------|------|
| id | UUID | 主键 |
| user_id | BIGINT | 用户 ID |
| symbol | VARCHAR(50) | 交易品种 |
| mode | VARCHAR(20) | 执行模式 (paper/demo/live) |
| level | INTEGER | 当前等级 (0-3) |
| status | VARCHAR(32) | 状态 |
| initial_balance | DOUBLE | 初始资金 |
| current_balance | DOUBLE | 当前资金 |
| win_rate | DOUBLE | 胜率 |
| ... | | ... |

#### ai_simulation_trades

AI 交易记录表，存储每笔模拟交易。

| 字段 | 类型 | 说明 |
|------|------|------|
| id | UUID | 主键 |
| config_id | UUID | 配置 ID |
| symbol | VARCHAR(50) | 交易品种 |
| direction | VARCHAR(10) | 方向 (long/short/hold) |
| entry_price | DOUBLE | 入场价格 |
| exit_price | DOUBLE | 出场价格 |
| quantity | DOUBLE | 数量 |
| pnl | DOUBLE | 盈亏金额 |
| pnl_percent | DOUBLE | 盈亏百分比 |
| ai_confidence | DOUBLE | AI 置信度 |
| status | VARCHAR(20) | 状态 (open/closed) |
| ... | | ... |

### 7.2 Agent 表

#### agent_debate_sessions

辩论会话表，存储每个辩论会话的元数据。

#### agent_debate_messages

辩论消息表，存储每个 Agent 的发言。

### 7.3 晋级表

#### promotion_audits

晋级审核记录表，存储晋级审核流程。

#### demotion_records

降级记录表，存储降级事件。

### 7.4 知识表

#### knowledge_nodes

知识节点表，实现卢曼卡片笔记法。

#### knowledge_links

知识关联表，存储知识节点之间的关系。

---

## 8. 前端页面说明

### 8.1 Agent 仪表盘 (`/agent`)

**功能**:
- 显示当前等级和经验值
- 显示模拟交易统计
- 显示晋级/降级状态
- 提供快速操作按钮

**主要组件**:
- 等级进度条
- 统计数据卡片
- 操作按钮组
- 最近交易列表

### 8.2 辩论查看器 (`/agent/debate`)

**功能**:
- 实时显示辩论过程
- 展示各 Agent 的分析意见
- 显示置信度和部门权重
- 展示最终决策和理由

**主要组件**:
- 辩论进度指示器
- Agent 发言卡片
- 部门综合报告
- 决策结果展示

### 8.3 交易历史 (`/agent/history`)

**功能**:
- 显示完整的交易记录
- 支持多维度过滤
- 展示 PnL 曲线
- 显示统计汇总

**主要组件**:
- 过滤表单
- 交易列表
- PnL 图表
- 统计卡片

---

## 9. 已知问题和限制

### 9.1 已知问题

1. **测试环境依赖**: 当前环境未安装 Rust，无法实际执行编译和测试
2. **OKX API 集成**: 需要真实的 API 密钥才能测试实盘交易
3. **WebSocket 推送**: 需要 Redis 支持

### 9.2 限制

1. **仅支持 DOGE**: 当前版本仅支持 DOGE-USDT-SWAP 交易对
2. **单一数据库**: 仅支持 PostgreSQL
3. **无集群支持**: 单实例部署

### 9.3 未来改进

- [ ] 支持更多交易品种
- [ ] 多数据库支持
- [ ] 集群部署支持
- [ ] 机器学习模型集成
- [ ] 社交交易功能

---

## 10. 技术支持

### 10.1 文档

- [设计文档](docs/AGENT_SYSTEM_DESIGN.md)
- [测试报告](TEST_REPORT.md)
- [代码文档](CODE_WIKI.md)

### 10.2 常见问题

#### Q: 如何配置 OKX API?

A: 在 `backend/.env` 中配置：
```env
OKX_API_KEY=your-api-key
OKX_API_SECRET=your-secret
OKX_USE_SANDBOX=true
```

#### Q: 如何查看实时日志?

A: 运行以下命令：
```bash
cd backend
RUST_LOG=debug cargo run
```

#### Q: 如何重置数据库?

A: 运行迁移回滚：
```bash
cd backend
sqlx migrate revert
sqlx migrate run
```

### 10.3 联系方式

- **GitHub Issues**: https://github.com/your-org/MoneyRobert-Pro/issues
- **邮箱**: support@moneyrobert.com

---

**文档版本**: 1.0  
**最后更新**: 2026-05-31  
**维护团队**: MoneyRobert Team

# MoneyRobert Pro — 多 Agent 投资分析系统设计方案

> 目标资产：DOGE-USDT-SWAP  
> 设计理念：模拟顶级投资机构的多部门协作决策流程  
> 核心参考：TradingAgents (TauricResearch)、MetaGPT、AutoGen Multi-Agent Debate

---

## 1. 系统总体架构

### 1.1 架构概览

系统采用 **分层多智能体辩论架构 (Hierarchical Multi-Agent Debate Architecture)**，灵感来源于现实中顶级对冲基金（如 Citadel、Two Sigma、Bridgewater）的投研组织结构。核心创新在于：各部门内部先进行 **内部辩论收敛**，再由部门代表进行 **跨部门辩论**，最终由基金经理 Agent 综合裁决。

```
┌─────────────────────────────────────────────────────────────────┐
│                    基金经理 Agent (Fund Manager)                 │
│              最终决策 · 风险否决 · 仓位分配 · 执行指令             │
└──────────────────────────┬──────────────────────────────────────┘
                           │ 综合报告 + 辩论纪要
          ┌────────────────┼────────────────┐
          ▼                ▼                ▼
┌─────────────────┐ ┌─────────────────┐ ┌─────────────────┐
│   技术分析部门    │ │   资金分析部门    │ │   新闻分析部门    │
│  Tech Division   │ │ Capital Division │ │  News Division  │
│                  │ │                  │ │                  │
│ ┌─────────────┐ │ │ ┌─────────────┐ │ │ ┌─────────────┐ │
│ │K线形态分析师 │ │ │ │资金费率分析师│ │ │ │舆情情绪分析师│ │
│ └──────┬──────┘ │ │ └──────┬──────┘ │ │ └──────┬──────┘ │
│ ┌──────┴──────┐ │ │ ┌──────┴──────┐ │ │ ┌──────┴──────┐ │
│ │技术指标分析师│ │ │ │持仓结构分析师│ │ │ │宏观政策分析师│ │
│ └──────┬──────┘ │ │ └──────┬──────┘ │ │ └──────┬──────┘ │
│ ┌──────┴──────┐ │ │ ┌──────┴──────┐ │ │ ┌──────┴──────┐ │
│ │链上数据分析师│ │ │ │多空博弈分析师│ │ │ │KOL/鲸鱼监控师│ │
│ └──────┬──────┘ │ │ └──────┬──────┘ │ │ └──────┬──────┘ │
│ ┌──────┴──────┐ │ │ ┌──────┴──────┐ │ │ ┌──────┴──────┐ │
│ │量化模型分析师│ │ │ │流动性分析师 │ │ │ │事件驱动分析师│ │
│ └──────┬──────┘ │ │ └──────┬──────┘ │ │ └──────┬──────┘ │
│        │        │ │        │        │ │        │        │
│  内部辩论→共识   │ │  内部辩论→共识   │ │  内部辩论→共识   │
│        │        │ │        │        │ │        │        │
│ ┌──────┴──────┐ │ │ ┌──────┴──────┐ │ │ ┌──────┴──────┐ │
│ │技术部门代表  │ │ │ │资金部门代表  │ │ │ │新闻部门代表  │ │
│ │(多头视角)    │ │ │ │(多头视角)    │ │ │ │(多头视角)    │ │
│ │技术部门代表  │ │ │ │资金部门代表  │ │ │ │新闻部门代表  │ │
│ │(空头视角)    │ │ │ │(空头视角)    │ │ │ │(空头视角)    │ │
│ └─────────────┘ │ │ └─────────────┘ │ │ └─────────────┘ │
└─────────────────┘ └─────────────────┘ └─────────────────┘
          │                  │                  │
          └──────────────────┼──────────────────┘
                             ▼
                    跨部门辩论 (Inter-Debate)
                             │
                             ▼
                    基金经理最终裁决
```

### 1.2 执行流程

```
阶段1: 数据采集          阶段2: 部门内部分析         阶段3: 跨部门辩论         阶段4: 最终决策
┌──────────┐      ┌──────────────────────┐    ┌──────────────────┐    ┌──────────────┐
│ 市场数据   │─────►│ 各 Agent 独立分析     │───►│ 多头 vs 空头辩论  │───►│ 基金经理综合  │
│ 资金数据   │      │ 部门内部辩论收敛      │    │ 部门代表对抗      │    │ 风险评估      │
│ 新闻数据   │      │ 生成部门共识报告      │    │ 生成辩论纪要      │    │ 最终决策      │
│ 链上数据   │      │ 输出多/空双视角       │    │ 暴露分歧与风险    │    │ 仓位/止损/止盈│
└──────────┘      └──────────────────────┘    └──────────────────┘    └──────────────┘
     5min                  10min                      8min                   2min
```

### 1.3 设计模式参考

本系统融合了当前最先进的 5 种 AI Agent 设计模式：

| 设计模式 | 来源 | 在本系统中的应用 |
|---------|------|---------------|
| **ReAct (Reason+Act)** | Yao et al. 2023 | 每个 Agent 先推理再调用工具获取数据 |
| **Multi-Agent Debate** | Liang et al. 2023 / TradingAgents | 部门内多头/空头对抗辩论，跨部门辩论 |
| **Plan-and-Execute** | LangGraph | 基金经理制定分析计划，分配给各部门执行 |
| **Hierarchical Delegation** | MetaGPT | 基金经理→部门代表→专业分析师的层级委派 |
| **Structured Output + Reflection** | TradingAgents v0.2.4 | 强制结构化输出，历史决策反思学习 |

---

## 2. 部门分工明细与 Agent 角色定义

### 2.1 技术分析部门 (Tech Division)

> 参考机构：Two Sigma (量化技术)、Jump Crypto (高频技术分析)、国内：幻方量化

| Agent | 角色定位 | 分析维度 | 工具/数据源 | 输出 |
|-------|---------|---------|-----------|------|
| **K线形态分析师** | 识别经典与衍生K线形态 | 1H/4H/1D K线形态、支撑阻力位、关键价格区间 | OKX K线 API、`klines` 表 | 形态类型、关键价位、突破概率 |
| **技术指标分析师** | 计算与解读技术指标 | RSI、MACD、布林带、KDJ、EMA交叉、成交量异动 | `market_data` 表、实时 Ticker | 指标信号列表(买入/卖出/中性)、超买超卖状态 |
| **链上数据分析师** | 分析 DOGE 链上活动 | 活跃地址数、大额转账、交易所净流入流出、持币分布 | 链上 API (Blockchair/Etherscan) | 鲸鱼动向、筹码集中度、抛压评估 |
| **量化模型分析师** | 运行量化策略模型 | 均值回归信号、动量因子、波动率锥、相关性矩阵 | 历史数据回测引擎 | 因子得分、策略信号、历史胜率 |

**部门内辩论机制：**

```
┌──────────────────────────────────────────────────────┐
│              技术部门内部辩论流程                        │
│                                                      │
│  Round 1: 各分析师独立输出分析报告                      │
│           ↓                                          │
│  Round 2: 多头视角代表汇总看多证据                     │
│           空头视角代表汇总看空证据                      │
│           ↓                                          │
│  Round 3: 双方交叉质询 (max 2 rounds)                 │
│           ↓                                          │
│  输出: 技术部门共识报告                                │
│        - 多头论据列表 (置信度评分)                      │
│        - 空头论据列表 (置信度评分)                      │
│        - 部门综合倾向 (看多/看空/中性 + 置信度)          │
│        - 关键分歧点                                    │
└──────────────────────────────────────────────────────┘
```

### 2.2 资金分析部门 (Capital Division)

> 参考机构：Citadel (做市资金流)、Grayscale (机构持仓)、国内：OKX Research

| Agent | 角色定位 | 分析维度 | 工具/数据源 | 输出 |
|-------|---------|---------|-----------|------|
| **资金费率分析师** | 解读永续合约资金费率 | 当前费率、费率趋势、费率极值、费率与价格背离 | OKX Funding Rate API、`funding_rate_history` 表 | 费率方向信号、多空成本评估、费率异常预警 |
| **持仓结构分析师** | 分析 Open Interest 变化 | OI 增减趋势、OI 与价格关系、大户持仓变化 | OKX OI API、`open_interests` 表 | OI 趋势判断、主力资金方向、增仓/减仓信号 |
| **多空博弈分析师** | 分析多空力量对比 | 多空比、大户多空比、爆仓数据、强平价格分布 | OKX Long-Short Ratio API、`long_short_ratio_history` 表 | 多空力量对比、散户/大户分歧、爆仓压力位 |
| **流动性分析师** | 评估市场流动性深度 | 买卖盘口深度、滑点估算、24h成交量、换手率 | OKX Orderbook API、Ticker 数据 | 流动性评级、大单冲击成本、流动性风险预警 |

**部门内辩论机制：** 同技术部门，多头/空头双视角对抗。

### 2.3 新闻分析部门 (News Division)

> 参考机构：Bloomberg (新闻终端)、Messari (加密研究)、国内：ChainNews、PANews

| Agent | 角色定位 | 分析维度 | 工具/数据源 | 输出 |
|-------|---------|---------|-----------|------|
| **舆情情绪分析师** | 量化社交媒体情绪 | Twitter/X 情绪、Reddit 讨论、Telegram 信号、微博/币圈社区 | `sentiment_data` 表、社交 API | 情绪得分(0-1)、情绪趋势、极端情绪预警 |
| **宏观政策分析师** | 评估宏观政策影响 | 美联储政策、SEC/CFTC 监管动态、各国加密政策、DOGE 相关立法 | 新闻 API、`news_items` 表 | 政策风险评级、利好/利空事件列表 |
| **KOL/鲸鱼监控师** | 追踪关键人物动态 | Elon Musk 动态、DOGE 基金会、鲸鱼钱包异动、交易所公告 | Twitter API、链上监控 | KOL 影响力评估、鲸鱼行为预警、关联事件 |
| **事件驱动分析师** | 识别与评估特定事件 | DOGE 升级/分叉、交易所上/下架、支付集成、社区投票 | 项目官方渠道、GitHub | 事件影响力评级、时间线、价格冲击预估 |

**部门内辩论机制：** 同技术部门，多头/空头双视角对抗。

### 2.4 基金经理 Agent (Fund Manager)

> 参考机构：Bridgewater (Ray Dalio 决策框架)、Renaissance Technologies

| 属性 | 说明 |
|------|------|
| **角色定位** | 最终决策者，综合三个部门的辩论结果，做出投资决策 |
| **核心能力** | 风险评估、仓位管理、止损止盈设置、决策一致性校验 |
| **决策框架** | Bridgewater 可信度加权决策 + 风险预算约束 |
| **历史反思** | 引入决策记忆，回顾历史同类决策的实际收益，校准置信度 |

---

## 3. Agent 通信协议与消息格式

### 3.1 Agent 消息格式

所有 Agent 间通信采用统一的 JSON 消息格式：

```json
{
  "message_id": "uuid",
  "timestamp": "2026-05-31T10:00:00Z",
  "round": 1,
  "phase": "intra_debate|inter_debate|decision",
  "sender": {
    "agent_id": "tech_kline_analyst",
    "department": "tech",
    "role": "analyst"
  },
  "content": {
    "viewpoint": "bullish|bearish|neutral",
    "confidence": 0.75,
    "analysis": "详细分析文本...",
    "evidence": [
      {
        "type": "data_point",
        "source": "klines_1H",
        "description": "DOGE 在 0.22 形成双底形态",
        "significance": "high"
      }
    ],
    "key_levels": {
      "support": [0.20, 0.18],
      "resistance": [0.25, 0.28]
    }
  },
  "metadata": {
    "model": "gpt-5.4",
    "tokens_used": 1200,
    "latency_ms": 3500
  }
}
```

### 3.2 部门共识报告格式

```json
{
  "department": "tech",
  "consensus": {
    "bullish_evidence": [
      {"point": "双底形态确认", "confidence": 0.8, "source_agent": "tech_kline_analyst"},
      {"point": "RSI 超卖回升", "confidence": 0.7, "source_agent": "tech_indicator_analyst"}
    ],
    "bearish_evidence": [
      {"point": "成交量萎缩", "confidence": 0.6, "source_agent": "tech_indicator_analyst"},
      {"point": "链上大额转出交易所", "confidence": 0.75, "source_agent": "tech_onchain_analyst"}
    ],
    "overall_bias": "slightly_bullish",
    "confidence": 0.62,
    "key_disagreements": [
      "K线形态看多但链上数据偏空，需关注 0.22 支撑是否有效"
    ]
  },
  "bull_representative_summary": "...",
  "bear_representative_summary": "..."
}
```

### 3.3 基金经理决策格式

```json
{
  "decision_id": "uuid",
  "timestamp": "2026-05-31T10:30:00Z",
  "symbol": "DOGE-USDT-SWAP",
  "decision": {
    "action": "long|short|hold|close",
    "confidence": 0.68,
    "position_size_percent": 5.0,
    "entry_price_range": {"low": 0.215, "high": 0.225},
    "stop_loss": 0.195,
    "take_profit": [0.26, 0.30],
    "leverage": 3,
    "holding_period": "4h-24h",
    "risk_reward_ratio": 2.5
  },
  "reasoning": {
    "primary_thesis": "技术面双底+资金面费率偏低+Musk 近期利好，综合偏多",
    "key_risks": ["链上鲸鱼转出可能预示抛压", "宏观不确定性"],
    "department_weights": {
      "tech": 0.35,
      "capital": 0.35,
      "news": 0.30
    },
    "overridden_signals": [
      {"department": "capital", "signal": "bearish", "reason": "OI 下降但费率极低，空头拥挤风险更大"}
    ]
  },
  "debate_summary": "跨部门辩论摘要...",
  "historical_reference": "最近3次类似形态决策胜率 66.7%"
}
```

---

## 4. 辩论机制设计

### 4.1 三阶段辩论流程

```
┌─────────────────────────────────────────────────────────────────────┐
│                        完整辩论流程                                   │
│                                                                     │
│  Phase 1: 部门内部分析与辩论 (Intra-Department Debate)                │
│  ─────────────────────────────────────────────────                   │
│  1.1 各 Agent 独立获取数据并分析 (ReAct 模式)                        │
│  1.2 生成个人分析报告 (结构化输出)                                    │
│  1.3 部门内多头/空头分组                                              │
│  1.4 内部辩论 (max 2 rounds)                                         │
│  1.5 生成部门共识报告 + 多/空双视角摘要                               │
│                                                                     │
│  Phase 2: 跨部门辩论 (Inter-Department Debate)                       │
│  ─────────────────────────────────────────                           │
│  2.1 三个部门各派多头代表 + 空头代表 (共6名)                          │
│  2.2 多头阵营陈述看多论据 (跨部门整合)                                │
│  2.3 空头阵营陈述看空论据 (跨部门整合)                                │
│  2.4 交叉质询 (max 2 rounds)                                         │
│  2.5 生成辩论纪要 + 分歧点标注                                       │
│                                                                     │
│  Phase 3: 基金经理裁决 (Fund Manager Decision)                       │
│  ─────────────────────────────────────────                           │
│  3.1 接收三份部门报告 + 辩论纪要                                      │
│  3.2 可信度加权评估 (参考历史准确率)                                   │
│  3.3 风险预算检查 (最大回撤/仓位限制/相关性)                          │
│  3.4 生成最终决策 (含否决理由，如适用)                                 │
│  3.5 写入决策记忆 (用于未来反思)                                      │
└─────────────────────────────────────────────────────────────────────┘
```

### 4.2 辩论规则

| 规则 | 说明 |
|------|------|
| **证据强制** | 每个 Agent 必须引用具体数据点，禁止无数据断言 |
| **置信度校准** | 置信度 0-1 浮点数，必须与历史准确率一致 |
| **反方质询** | 辩论中必须回应对方最强论据，不得回避 |
| **分歧标注** | 未达成共识的点必须标注，传递给基金经理 |
| **否决权** | 基金经理可否决任何高置信度但风险超限的信号 |
| **反思注入** | 基金经理决策时注入历史同类决策的实际结果 |

### 4.3 可信度加权机制

参考 Bridgewater 的 Idea Meritocracy，每个 Agent 的意见权重由其历史准确率决定：

```
Agent 可信度权重 = 历史准确率 × 信号强度 × 部门权重

其中:
- 历史准确率 = 该 Agent 过去 N 次同方向预测的胜率
- 信号强度 = 该 Agent 本次输出的置信度
- 部门权重 = 基金经理根据市场环境动态调整 (默认: 技术0.35, 资金0.35, 新闻0.30)
```

---

## 5. 数据流与系统集成

### 5.1 与现有 MoneyRobert Pro 系统的集成

```
┌──────────────────────────────────────────────────────────────────┐
│                    MoneyRobert Pro 现有系统                        │
│                                                                  │
│  ┌──────────┐  ┌──────────┐  ┌──────────┐  ┌──────────────┐    │
│  │MarketData │  │FundingRate│  │Sentiment │  │  News Items  │    │
│  │  klines   │  │  History  │  │  Data    │  │              │    │
│  │  tickers  │  │  OI Data  │  │  Score   │  │  Articles    │    │
│  │  candles  │  │  L/S Ratio│  │  Source  │  │  Sentiment   │    │
│  └─────┬─────┘  └─────┬────┘  └────┬─────┘  └──────┬───────┘    │
│        │              │             │                │            │
│        └──────────────┴──────┬──────┴────────────────┘            │
│                              │                                     │
│                    ┌─────────▼──────────┐                          │
│                    │   Agent Data Bus    │                          │
│                    │  (数据统一分发层)     │                          │
│                    └─────────┬──────────┘                          │
│                              │                                     │
│         ┌────────────────────┼────────────────────┐               │
│         ▼                    ▼                    ▼               │
│  ┌─────────────┐    ┌─────────────┐    ┌─────────────┐          │
│  │ Tech Agents  │    │Capital Agents│    │ News Agents │          │
│  └──────┬──────┘    └──────┬──────┘    └──────┬──────┘          │
│         │                  │                  │                   │
│         └──────────────────┼──────────────────┘                   │
│                            ▼                                      │
│                   ┌────────────────┐                              │
│                   │  Debate Engine  │                              │
│                   │  (辩论引擎)      │                              │
│                   └────────┬───────┘                              │
│                            ▼                                      │
│                   ┌────────────────┐                              │
│                   │  Fund Manager   │                              │
│                   │  Agent          │                              │
│                   └────────┬───────┘                              │
│                            │                                     │
│                            ▼                                     │
│  ┌──────────────────────────────────────────────┐                │
│  │            新增 API 路由                       │                │
│  │  POST /api/v1/agent/analyze/{symbol}          │                │
│  │  GET  /api/v1/agent/sessions                  │                │
│  │  GET  /api/v1/agent/sessions/{id}             │                │
│  │  GET  /api/v1/agent/sessions/{id}/debate      │                │
│  │  GET  /api/v1/agent/decisions                 │                │
│  │  GET  /api/v1/agent/decisions/{id}            │                │
│  │  GET  /api/v1/agent/agents                    │                │
│  │  GET  /api/v1/agent/performance               │                │
│  └──────────────────────────────────────────────┘                │
└──────────────────────────────────────────────────────────────────┘
```

### 5.2 新增数据库表

```sql
-- Agent 定义表
CREATE TABLE agent_definitions (
    id SERIAL PRIMARY KEY,
    agent_id VARCHAR(64) UNIQUE NOT NULL,
    name VARCHAR(128) NOT NULL,
    department VARCHAR(32) NOT NULL,
    role VARCHAR(64) NOT NULL,
    system_prompt TEXT NOT NULL,
    model_name VARCHAR(64),
    tools JSONB DEFAULT '[]',
    is_active BOOLEAN DEFAULT true,
    created_at TIMESTAMPTZ DEFAULT NOW()
);

-- 分析会话表
CREATE TABLE agent_sessions (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    user_id BIGINT REFERENCES users(id),
    symbol VARCHAR(32) NOT NULL,
    status VARCHAR(32) DEFAULT 'running',
    config JSONB DEFAULT '{}',
    started_at TIMESTAMPTZ DEFAULT NOW(),
    completed_at TIMESTAMPTZ,
    created_at TIMESTAMPTZ DEFAULT NOW()
);

-- Agent 消息表 (辩论过程)
CREATE TABLE agent_messages (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    session_id UUID REFERENCES agent_sessions(id),
    round INTEGER NOT NULL,
    phase VARCHAR(32) NOT NULL,
    sender_agent_id VARCHAR(64) REFERENCES agent_definitions(agent_id),
    viewpoint VARCHAR(16),
    confidence FLOAT,
    content JSONB NOT NULL,
    tokens_used INTEGER,
    latency_ms INTEGER,
    created_at TIMESTAMPTZ DEFAULT NOW()
);

-- 部门共识报告表
CREATE TABLE department_reports (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    session_id UUID REFERENCES agent_sessions(id),
    department VARCHAR(32) NOT NULL,
    consensus JSONB NOT NULL,
    bull_summary TEXT,
    bear_summary TEXT,
    overall_bias VARCHAR(32),
    confidence FLOAT,
    created_at TIMESTAMPTZ DEFAULT NOW()
);

-- 基金经理决策表
CREATE TABLE fund_manager_decisions (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    session_id UUID REFERENCES agent_sessions(id),
    symbol VARCHAR(32) NOT NULL,
    action VARCHAR(16) NOT NULL,
    confidence FLOAT,
    position_size_percent FLOAT,
    entry_price_range JSONB,
    stop_loss FLOAT,
    take_profit JSONB,
    leverage INTEGER,
    holding_period VARCHAR(32),
    risk_reward_ratio FLOAT,
    reasoning JSONB NOT NULL,
    debate_summary TEXT,
    created_at TIMESTAMPTZ DEFAULT NOW()
);

-- Agent 历史表现表 (用于可信度加权)
CREATE TABLE agent_performance (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    agent_id VARCHAR(64) REFERENCES agent_definitions(agent_id),
    symbol VARCHAR(32),
    prediction_direction VARCHAR(16),
    confidence FLOAT,
    actual_outcome VARCHAR(16),
    is_correct BOOLEAN,
    pnl_percent FLOAT,
    created_at TIMESTAMPTZ DEFAULT NOW()
);

-- 决策记忆表 (基金经理反思)
CREATE TABLE decision_memory (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    symbol VARCHAR(32) NOT NULL,
    decision_summary TEXT NOT NULL,
    market_context JSONB,
    actual_result JSONB,
    reflection TEXT,
    created_at TIMESTAMPTZ DEFAULT NOW()
);
```

---

## 6. Agent Prompt 设计

### 6.1 技术部门 — K线形态分析师

```
你是 MoneyRobert Pro 投资系统中的 K线形态分析师，专注于 DOGE-USDT-SWAP 合约。

## 你的职责
分析 DOGE 的 K线图表形态，识别经典与衍生价格形态，判断当前价格所处位置。

## 分析框架
1. 识别当前 K线形态（双底/双顶/头肩/三角/旗形/楔形等）
2. 标注关键支撑位和阻力位
3. 判断形态确认程度（初步/确认/失效）
4. 评估形态目标价
5. 计算突破/跌破概率

## 输出要求
你必须以 JSON 格式输出，包含：
- viewpoint: "bullish" | "bearish" | "neutral"
- confidence: 0.0-1.0
- analysis: 详细分析文本
- evidence: [{type, source, description, significance}]
- key_levels: {support: [], resistance: []}

## 约束
- 必须引用具体价格数据，禁止模糊描述
- 置信度必须反映形态确认程度
- 如果形态不清晰，置信度不得高于 0.5
- 考虑 DOGE 特有的高波动性特征
```

### 6.2 资金部门 — 资金费率分析师

```
你是 MoneyRobert Pro 投资系统中的资金费率分析师，专注于 DOGE-USDT-SWAP 永续合约。

## 你的职责
分析 DOGE 永续合约的资金费率，解读多空成本结构，识别费率异常信号。

## 分析框架
1. 当前资金费率水平与历史分位数对比
2. 费率趋势（连续正/负费率的持续时间）
3. 费率与价格的背离信号
4. 费率极值预警（>0.1% 或 <-0.1%）
5. 空头/多头拥挤度评估

## 关键信号
- 费率极低 + 价格下跌 → 空头拥挤，可能逼空
- 费率极高 + 价格上涨 → 多头拥挤，可能回调
- 费率与价格背离 → 趋势反转信号

## 输出要求
同标准 Agent 输出格式。
```

### 6.3 新闻部门 — KOL/鲸鱼监控师

```
你是 MoneyRobert Pro 投资系统中的 KOL/鲸鱼监控师，专注于 DOGE 生态。

## 你的职责
追踪与 DOGE 相关的关键人物和鲸鱼动态，评估其对价格的影响。

## 重点监控对象
1. Elon Musk — Twitter/X 动态、Tesla 相关 DOGE 提及
2. DOGE 基金会 — 官方公告、开发进展
3. 鲸鱼钱包 — 大额 DOGE 转账（>1亿 DOGE）
4. 交易所 — 大额充提币、上/下架公告

## 影响力评级
- critical: 直接影响价格 >10%
- high: 可能影响价格 5-10%
- medium: 可能影响价格 2-5%
- low: 情绪层面影响

## 输出要求
同标准 Agent 输出格式，evidence 中必须包含事件来源和时间。
```

### 6.4 基金经理 Agent

```
你是 MoneyRobert Pro 投资系统的基金经理，负责 DOGE-USDT-SWAP 的最终投资决策。

## 你的角色
你就像一个对冲基金的基金经理，综合技术、资金、新闻三个部门的辩论结果，做出最终决策。

## 决策框架 (Bridgewater 可信度加权)
1. 审阅三份部门共识报告
2. 审阅跨部门辩论纪要
3. 对每个部门的可信度进行加权：
   - 历史准确率高的部门/Agent 权重更大
   - 当前市场环境下更相关的部门权重更大
4. 评估关键分歧点，判断哪方论据更强
5. 进行风险预算检查：
   - 单笔最大亏损不超过总资金 2%
   - 杠杆不超过 5x
   - 止损必须设置
6. 做出最终决策

## 风险否决规则
- 如果三个部门中两个以上看空且置信度 >0.7 → 不得做多
- 如果止损位距离入场价 >5% → 降低仓位至 2%
- 如果流动性评级为"差" → 不得开仓
- 如果有 critical 级别利空事件 → 不得做多

## 历史反思
你会收到历史同类决策的实际结果，请参考：
- 过去类似形态/信号的实际胜率
- 哪些部门/Agent 历史上更准确
- 上次决策的反思教训

## 输出要求
按基金经理决策格式输出，必须包含：
- action, confidence, position_size_percent
- entry_price_range, stop_loss, take_profit, leverage
- reasoning (含部门权重、否决理由、风险评估)
```

---

## 7. 后端实现架构

### 7.1 新增模块结构

```
backend/src/
├── agents/                          # 新增: Agent 系统
│   ├── mod.rs                       # 模块声明
│   ├── engine.rs                    # 辩论引擎核心
│   ├── orchestrator.rs              # 会话编排器
│   ├── llm_client.rs                # LLM API 客户端 (多模型)
│   ├── memory.rs                    # 决策记忆管理
│   ├── credibility.rs               # 可信度加权计算
│   ├── departments/
│   │   ├── mod.rs
│   │   ├── tech.rs                  # 技术部门 Agent 定义
│   │   ├── capital.rs               # 资金部门 Agent 定义
│   │   └── news.rs                  # 新闻部门 Agent 定义
│   ├── agents/
│   │   ├── tech_kline.rs            # K线形态分析师
│   │   ├── tech_indicator.rs        # 技术指标分析师
│   │   ├── tech_onchain.rs          # 链上数据分析师
│   │   ├── tech_quant.rs            # 量化模型分析师
│   │   ├── capital_funding.rs       # 资金费率分析师
│   │   ├── capital_position.rs      # 持仓结构分析师
│   │   ├── capital_ls.rs            # 多空博弈分析师
│   │   ├── capital_liquidity.rs     # 流动性分析师
│   │   ├── news_sentiment.rs        # 舆情情绪分析师
│   │   ├── news_macro.rs            # 宏观政策分析师
│   │   ├── news_kol.rs              # KOL/鲸鱼监控师
│   │   ├── news_event.rs            # 事件驱动分析师
│   │   └── fund_manager.rs          # 基金经理 Agent
│   └── routes/
│       └── mod.rs                   # Agent API 路由
└── routes/
    └── agent.rs                     # 新增路由入口
```

### 7.2 核心 Trait 定义

```rust
#[async_trait]
pub trait Agent: Send + Sync {
    fn agent_id(&self) -> &str;
    fn name(&self) -> &str;
    fn department(&self) -> &str;
    fn role(&self) -> &str;
    
    async fn analyze(&self, context: &AnalysisContext) -> Result<AgentOutput>;
    async fn debate(
        &self,
        context: &AnalysisContext,
        opponent_view: &AgentOutput,
    ) -> Result<AgentOutput>;
}

pub struct AnalysisContext {
    pub symbol: String,
    pub market_data: MarketDataSnapshot,
    pub previous_rounds: Vec<AgentOutput>,
    pub historical_performance: AgentPerformanceSummary,
}

pub struct AgentOutput {
    pub agent_id: String,
    pub viewpoint: Viewpoint,
    pub confidence: f64,
    pub analysis: String,
    pub evidence: Vec<Evidence>,
    pub key_levels: Option<KeyLevels>,
}

pub enum Viewpoint {
    Bullish,
    Bearish,
    Neutral,
}
```

### 7.3 辩论引擎核心逻辑

```rust
pub struct DebateEngine {
    llm_client: Arc<LlmClient>,
    memory: Arc<DecisionMemory>,
    credibility: Arc<CredibilityCalculator>,
}

impl DebateEngine {
    pub async fn run_session(&self, symbol: &str, config: &SessionConfig) -> Result<SessionResult> {
        // Phase 1: 部门内部分析与辩论
        let tech_report = self.intra_department_debate("tech", symbol, &config).await?;
        let capital_report = self.intra_department_debate("capital", symbol, &config).await?;
        let news_report = self.intra_department_debate("news", symbol, &config).await?;
        
        // Phase 2: 跨部门辩论
        let debate_summary = self.inter_department_debate(
            &[&tech_report, &capital_report, &news_report],
            &config,
        ).await?;
        
        // Phase 3: 基金经理裁决
        let decision = self.fund_manager_decide(
            &[&tech_report, &capital_report, &news_report],
            &debate_summary,
            symbol,
            &config,
        ).await?;
        
        // 写入决策记忆
        self.memory.record_decision(symbol, &decision).await?;
        
        Ok(SessionResult {
            tech_report,
            capital_report,
            news_report,
            debate_summary,
            decision,
        })
    }
    
    async fn intra_department_debate(
        &self,
        department: &str,
        symbol: &str,
        config: &SessionConfig,
    ) -> Result<DepartmentReport> {
        let agents = self.get_department_agents(department);
        
        // Round 1: 独立分析
        let mut outputs = Vec::new();
        for agent in &agents {
            let context = self.build_context(symbol).await?;
            let output = agent.analyze(&context).await?;
            outputs.push(output);
        }
        
        // Round 2-3: 内部辩论
        let (bull_outputs, bear_outputs): (Vec<_>, Vec<_>) = outputs
            .iter()
            .partition(|o| matches!(o.viewpoint, Viewpoint::Bullish));
        
        let debated_outputs = self.run_debate_rounds(
            &bull_outputs,
            &bear_outputs,
            config.max_intra_debate_rounds,
        ).await?;
        
        // 生成部门共识报告
        self.generate_department_report(department, &debated_outputs).await
    }
}
```

---

## 8. 前端展示设计

### 8.1 新增页面：Agent 分析页

| 区域 | 内容 |
|------|------|
| **顶部** | 目标资产 (DOGE)、分析时间、当前状态 |
| **左侧面板** | 三个部门 Tab 切换，每个部门下显示各 Agent 的分析卡片 |
| **中央区域** | 辩论实况 — 多头/空头论据对比、辩论轮次、实时更新 |
| **右侧面板** | 基金经理决策卡片 — 最终决策、仓位建议、风险评估 |
| **底部** | 历史决策记录、Agent 准确率排行榜 |

### 8.2 实时更新

通过 WebSocket 推送 Agent 分析进度：

```json
{
  "type": "agent_update",
  "data": {
    "session_id": "uuid",
    "phase": "intra_debate",
    "department": "tech",
    "agent_id": "tech_kline_analyst",
    "status": "analyzing|debating|completed",
    "viewpoint": "bullish",
    "confidence": 0.75,
    "summary": "DOGE 在 0.22 形成双底形态..."
  }
}
```

---

## 9. LLM 模型配置策略

### 9.1 分层模型策略

| 用途 | 推荐模型 | 原因 |
|------|---------|------|
| 深度分析 (各分析师) | GPT-5.4 / Claude 4.6 / DeepSeek R1 | 需要强推理能力 |
| 快速判断 (辩论回应) | GPT-5.4-mini / Qwen3-8B | 速度优先，降低成本 |
| 基金经理决策 | GPT-5.5 / Claude 4.6 Opus | 最高推理质量 |
| 结构化输出 | 强制 JSON mode | 确保输出可解析 |

### 9.2 成本控制

| 策略 | 说明 |
|------|------|
| **模型分层** | 分析用强模型，辩论用轻模型 |
| **缓存** | 相同数据 5 分钟内不重复调用 LLM |
| **并行** | 同部门 Agent 并行分析，缩短总时间 |
| **增量** | 仅在数据变化时重新分析 |
| **预算** | 单次分析最大 token 消耗限制 |

---

## 10. 性能与可靠性

### 10.1 性能目标

| 指标 | 目标 |
|------|------|
| 单次完整分析耗时 | < 25 分钟 |
| 部门内部分析耗时 | < 10 分钟 |
| 跨部门辩论耗时 | < 8 分钟 |
| 基金经理决策耗时 | < 2 分钟 |
| 单次分析 LLM 成本 | < $0.50 |

### 10.2 可靠性保障

| 机制 | 说明 |
|------|------|
| **Checkpoint** | 每个阶段完成后保存进度，崩溃可恢复 |
| **超时控制** | 单个 Agent 超时 60 秒自动降级为中性 |
| **降级策略** | LLM 调用失败时使用规则引擎兜底 |
| **一致性校验** | 检查输出格式、置信度范围、逻辑一致性 |
| **人工介入** | 高风险决策 (>5% 仓位) 可配置为需人工确认 |

---

## 11. 实施路线图

| 阶段 | 周期 | 交付内容 |
|------|------|---------|
| **P0: 基础框架** | 2 周 | Agent trait 体系、LLM 客户端、数据库表、API 骨架 |
| **P1: 核心分析师** | 2 周 | 技术部门 4 个 Agent + 资金部门 4 个 Agent (对接现有数据) |
| **P2: 新闻 + 辩论** | 2 周 | 新闻部门 4 个 Agent + 部门内辩论引擎 |
| **P3: 基金经理** | 1 周 | 基金经理 Agent + 跨部门辩论 + 可信度加权 |
| **P4: 前端 + 记忆** | 2 周 | Agent 分析页 + WebSocket 实时推送 + 决策记忆 |
| **P5: 优化上线** | 1 周 | 性能优化、成本控制、监控告警、文档 |

---

## 12. 多层级记忆管理系统

### 12.1 设计理念

人类投资专家的决策能力来源于三个层次的记忆：
- **短期工作记忆**：当前分析过程中的数据和推理
- **中期情景记忆**：过去类似市场环境下的经历和结果
- **长期知识记忆**：经过反复验证的市场规律和决策原则

本系统模拟这一认知架构，构建三层记忆体系，使 Agent 能够"从经验中学习"。

```
┌─────────────────────────────────────────────────────────────────────┐
│                     三层记忆架构                                      │
│                                                                     │
│  ┌─────────────────────────────────────────────────────────────┐    │
│  │  L1: 短期工作记忆 (Working Memory)                           │    │
│  │  生命周期: 单次分析会话 (25分钟)                               │    │
│  │  存储: Redis                                                  │    │
│  │  内容: 当前会话的中间推理、辩论记录、数据快照                    │    │
│  │  特点: 读写极快，会话结束自动清理                               │    │
│  └──────────────────────────┬──────────────────────────────────┘    │
│                             │ 会话结束后提炼                         │
│  ┌──────────────────────────▼──────────────────────────────────┐    │
│  │  L2: 中期情景记忆 (Episodic Memory)                          │    │
│  │  生命周期: 30天 (带衰减)                                      │    │
│  │  存储: PostgreSQL + pgvector                                 │    │
│  │  内容: 历史分析会话摘要、决策结果、市场环境特征                  │    │
│  │  特点: 向量化存储，支持相似场景检索 (RAG)                      │    │
│  └──────────────────────────┬──────────────────────────────────┘    │
│                             │ 定期反思巩固                           │
│  ┌──────────────────────────▼──────────────────────────────────┐    │
│  │  L3: 长期知识记忆 (Semantic Memory)                          │    │
│  │  生命周期: 永久 (带版本管理)                                   │    │
│  │  存储: PostgreSQL + pgvector                                 │    │
│  │  内容: 验证过的市场规律、Agent 经验规则、部门协作模式            │    │
│  │  特点: 高置信度知识，需多轮验证才能写入                         │    │
│  └─────────────────────────────────────────────────────────────┘    │
└─────────────────────────────────────────────────────────────────────┘
```

### 12.2 L1: 短期工作记忆

**用途：** 在单次分析会话内，缓存各 Agent 的中间推理结果，避免重复计算和数据请求。

```
┌──────────────────────────────────────────────────────┐
│              L1 工作记忆结构 (Redis)                    │
│                                                      │
│  Key: agent:session:{session_id}                     │
│  ├── market_snapshot    → 当前市场数据快照             │
│  ├── tech:indicators    → 技术指标计算结果缓存         │
│  ├── tech:patterns      → K线形态识别结果             │
│  ├── capital:funding    → 资金费率分析结果             │
│  ├── capital:oi         → 持仓结构分析结果             │
│  ├── news:sentiment     → 情绪分析结果                │
│  ├── news:events        → 事件分析结果                │
│  ├── debate:intra:tech  → 技术部门辩论记录             │
│  ├── debate:intra:cap   → 资金部门辩论记录             │
│  ├── debate:intra:news  → 新闻部门辩论记录             │
│  ├── debate:inter       → 跨部门辩论记录              │
│  └── decision           → 基金经理决策结果             │
│                                                      │
│  TTL: 1小时 (会话结束后自动过期)                       │
└──────────────────────────────────────────────────────┘
```

**关键特性：**
- 同一数据在会话内只获取一次，后续 Agent 直接从工作记忆读取
- 辩论过程中的每一轮输出都缓存，避免 Agent 重复生成
- 会话结束后触发 L1→L2 的记忆提炼流程

### 12.3 L2: 中期情景记忆

**用途：** 存储历史分析会话的结构化摘要，支持基于市场环境相似度的 RAG 检索。

**情景记忆条目结构：**

```json
{
  "episode_id": "uuid",
  "timestamp": "2026-05-31T10:30:00Z",
  "symbol": "DOGE-USDT-SWAP",
  "market_context": {
    "price": 0.22,
    "24h_change": -3.5,
    "volatility_rank": "high",
    "funding_rate": -0.0001,
    "oi_change_24h": -5.2,
    "sentiment_score": 0.35,
    "dominant_pattern": "double_bottom",
    "macro_environment": "risk_off",
    "key_event": "Elon Musk tweet about DOGE"
  },
  "agent_consensus": {
    "tech_bias": "bullish",
    "tech_confidence": 0.62,
    "capital_bias": "neutral",
    "capital_confidence": 0.55,
    "news_bias": "bullish",
    "news_confidence": 0.70
  },
  "decision": {
    "action": "long",
    "entry_price": 0.22,
    "stop_loss": 0.195,
    "take_profit": 0.26
  },
  "actual_outcome": {
    "price_after_4h": 0.235,
    "price_after_24h": 0.25,
    "max_drawdown": -4.5,
    "pnl_percent": 13.6,
    "result": "win"
  },
  "embedding": [0.012, -0.034, 0.078, ...],
  "importance_score": 0.82,
  "access_count": 5,
  "last_accessed": "2026-06-01T08:00:00Z"
}
```

**RAG 检索流程：**

```
当前市场环境
     │
     ▼
┌──────────────────┐
│ 环境特征提取       │  提取当前: 价格位置/波动率/费率/情绪/事件
└────────┬─────────┘
         │
         ▼
┌──────────────────┐
│ 向量化编码        │  将市场环境特征编码为 embedding 向量
└────────┬─────────┘
         │
         ▼
┌──────────────────┐
│ pgvector 相似检索 │  SELECT * FROM episodic_memory
│                  │  ORDER BY embedding <=> $current_embedding
│                  │  LIMIT 10
│                  │  WHERE importance_score > 0.5
│                  │  AND created_at > NOW() - INTERVAL '30 days'
└────────┬─────────┘
         │
         ▼
┌──────────────────┐
│ 相关度过滤        │  过滤: 相似度 > 0.7 且市场环境真正可比
│                  │  排除: 过时情景 (衰减权重 < 0.3)
└────────┬─────────┘
         │
         ▼
┌──────────────────┐
│ 注入 Agent 上下文 │  将 Top-K 相关情景摘要注入各 Agent 的 prompt
└──────────────────┘
```

**情景记忆注入 Prompt 示例：**

```
## 历史相似情景 (供参考)

以下是过去30天内与当前市场环境最相似的3个情景：

### 情景1 (相似度: 0.89, 2026-05-15)
- 环境: DOGE 在 0.20 形成双底，资金费率 -0.0002，情绪偏空
- 决策: 做多，入场 0.20，止损 0.185
- 实际结果: ✅ 盈利 15%，4h 后价格涨至 0.23
- 教训: 双底+极低费率是有效的逼空信号

### 情景2 (相似度: 0.82, 2026-05-08)
- 环境: DOGE 跌破支撑位，OI 大幅下降，Musk 无动态
- 决策: 做空，入场 0.24，止损 0.255
- 实际结果: ❌ 亏损 3%，价格反弹至 0.25
- 教训: OI 下降+低费率时做空风险大，可能逼空

### 情景3 (相似度: 0.76, 2026-04-28)
- 环境: DOGE 横盘整理，资金费率中性，情绪中性
- 决策: 持有观望
- 实际结果: ➖ 持平，24h 内波动 <2%
- 教训: 信号不明确时观望是正确选择

请参考以上历史情景，但不要机械照搬——市场环境可能存在关键差异。
```

### 12.4 L3: 长期知识记忆

**用途：** 存储经过多轮验证的市场规律和决策原则，是系统"最可靠"的知识层。

**知识条目结构：**

```json
{
  "knowledge_id": "uuid",
  "category": "market_rule|agent_heuristic|collaboration_pattern|risk_principle",
  "title": "低费率+双底形态的逼空信号",
  "content": "当 DOGE 永续合约资金费率低于 -0.01% 且价格形成双底形态时，逼空概率约 65%，建议轻仓做多",
  "evidence": {
    "occurrence_count": 8,
    "win_count": 5,
    "avg_pnl_percent": 12.3,
    "last_validated": "2026-05-28",
    "source_episodes": ["ep_uuid_1", "ep_uuid_2", ...]
  },
  "confidence": 0.72,
  "applicable_conditions": {
    "symbol": "DOGE-USDT-SWAP",
    "market_regime": "range_bound_or_bottoming",
    "min_funding_rate": -0.01,
    "pattern": "double_bottom"
  },
  "embedding": [0.045, -0.012, 0.089, ...],
  "version": 3,
  "created_at": "2026-04-15",
  "updated_at": "2026-05-28"
}
```

**知识分类：**

| 类别 | 说明 | 示例 |
|------|------|------|
| `market_rule` | 市场规律 | "DOGE 在 Musk 发推后 1h 内平均波动 8%" |
| `agent_heuristic` | Agent 经验 | "技术指标分析师在低波动环境下准确率下降" |
| `collaboration_pattern` | 协作模式 | "技术+资金同时看多时，胜率 72%" |
| `risk_principle` | 风险原则 | "OI 单日下降 >10% 时禁止开仓" |

**知识写入门槛（防止错误知识污染）：**

```
写入 L3 知识记忆的条件:
┌──────────────────────────────────────────────────┐
│ 1. 同类情景在 L2 中出现 >= 5 次                    │
│ 2. 一致性验证: >= 70% 的情景结果方向一致             │
│ 3. 置信度 >= 0.65                                  │
│ 4. 至少跨越 7 天的时间范围 (排除短期偏差)            │
│ 5. 基金经理 Agent 审核通过                          │
└──────────────────────────────────────────────────┘
```

### 12.5 记忆共享层级

```
┌─────────────────────────────────────────────────────────────────┐
│                     记忆共享层级                                  │
│                                                                 │
│  ┌─────────────────────────────────────────────────────────┐    │
│  │  全局知识库 (Global Knowledge Base)                       │    │
│  │  所有 Agent 共享，存储 L3 长期知识记忆                     │    │
│  │  - 市场规律 (market_rule)                                 │    │
│  │  - 风险原则 (risk_principle)                              │    │
│  │  - 协作模式 (collaboration_pattern)                       │    │
│  └──────────────────────────┬──────────────────────────────┘    │
│                             │                                    │
│        ┌────────────────────┼────────────────────┐              │
│        ▼                    ▼                    ▼              │
│  ┌───────────┐       ┌───────────┐       ┌───────────┐        │
│  │技术部门记忆 │       │资金部门记忆 │       │新闻部门记忆 │        │
│  │(L2 情景)   │       │(L2 情景)   │       │(L2 情景)   │        │
│  │           │       │           │       │           │        │
│  │- 双底形态  │       │- 费率极值  │       │- Musk推文  │        │
│  │  历史胜率  │       │  后走势    │       │  影响模式  │        │
│  │- RSI超卖  │       │- OI异动   │       │- 监管事件  │        │
│  │  反弹概率  │       │  后走势    │       │  影响模式  │        │
│  └─────┬─────┘       └─────┬─────┘       └─────┬─────┘        │
│        │                    │                    │              │
│  ┌─────┴─────┐       ┌─────┴─────┐       ┌─────┴─────┐        │
│  │Agent个体记忆│       │Agent个体记忆│       │Agent个体记忆│        │
│  │           │       │           │       │           │        │
│  │K线分析师:  │       │费率分析师:  │       │情绪分析师:  │        │
│  │ 个人准确率  │       │ 个人准确率  │       │ 个人准确率  │        │
│  │ 偏好模式   │       │ 偏好模式   │       │ 偏好模式   │        │
│  │ 校准偏差   │       │ 校准偏差   │       │ 校准偏差   │        │
│  └───────────┘       └───────────┘       └───────────┘        │
└─────────────────────────────────────────────────────────────────┘
```

**共享规则：**

| 层级 | 可读 | 可写 | 说明 |
|------|------|------|------|
| Agent 个体记忆 | 仅自己 | 仅自己 | 每个 Agent 的个人准确率和偏差校准 |
| 部门共享记忆 | 同部门 Agent | 部门代表 | 部门内的历史情景和经验 |
| 全局知识库 | 所有 Agent | 基金经理 (审核) | 经过验证的通用知识 |

### 12.6 记忆衰减机制

参考 Ebbinghaus 遗忘曲线，对 L2 情景记忆实施时间衰减：

```
记忆权重 = importance_score × decay_factor × access_boost

其中:
  decay_factor = e^(-λ × days_since_creation)
    λ = 0.05 (半衰期约 14 天)
    30 天后权重降至约 0.22

  access_boost = 1 + 0.1 × min(access_count, 10)
    被频繁检索的记忆衰减更慢 (最多 2x 加成)

  importance_score 初始值:
    - 盈利 >10% 的决策: 0.9
    - 亏损 >5% 的决策: 0.85 (失败教训更重要)
    - 小幅盈亏: 0.6
    - 观望决策: 0.4
```

**衰减执行：** 每日定时任务扫描 L2 记忆，将权重低于 0.1 的条目归档或删除。

```
┌──────────────────────────────────────────────────────────┐
│                  记忆衰减曲线                              │
│                                                          │
│  权重 1.0 ┤                                               │
│         ┤╲                                               │
│         ┤  ╲                                             │
│    0.75 ┤    ╲___                                        │
│         ┤        ╲___                                    │
│    0.50 ┤            ╲___                                │
│         ┤                ╲___                            │
│    0.25 ┤                    ╲___                        │
│         ┤                        ╲___  ╲___              │
│    0.10 ┤                            ╲___    ╲___ (归档)  │
│         ┤                                                 │
│         └──┬──┬──┬──┬──┬──┬──┬──┬──┬──┬──→ 天             │
│            3  6  9  12 15 18 21 24 27 30                  │
│                                                          │
│  ─── 普通记忆 (importance=0.6)                            │
│  ─── 重要记忆 (importance=0.9, access_boost=1.5)          │
└──────────────────────────────────────────────────────────┘
```

### 12.7 记忆反思循环

系统定期执行反思循环，将 L2 情景记忆提炼为 L3 知识记忆：

```
┌──────────────────────────────────────────────────────────────────┐
│                     记忆反思循环 (每日执行)                        │
│                                                                  │
│  Step 1: 结果回填                                                 │
│  ────────────────                                                │
│  对过去 24h 内到期的决策，获取实际价格数据                          │
│  更新 L2 情景记忆的 actual_outcome 字段                           │
│  更新 agent_performance 表的 is_correct 和 pnl_percent            │
│                                                                  │
│  Step 2: 模式发现                                                 │
│  ────────────────                                                │
│  扫描 L2 中同一 Agent 的近期情景，寻找重复模式:                    │
│  - "K线分析师最近 5 次双底判断，4 次正确" → 潜在知识               │
│  - "资金部门在费率 < -0.01% 时判断准确率 80%" → 潜在知识           │
│  - "技术+资金同时看多时胜率 72%" → 潜在协作模式                    │
│                                                                  │
│  Step 3: 知识验证                                                 │
│  ────────────────                                                │
│  对发现的潜在知识进行验证:                                         │
│  - 检查是否满足写入门槛 (>=5次, >=70%一致, >=0.65置信度)           │
│  - 检查是否与已有 L3 知识冲突                                      │
│  - 冲突时: 保留置信度更高的，或标记为"待验证"                       │
│                                                                  │
│  Step 4: 知识写入                                                 │
│  ────────────────                                                │
│  通过验证的知识写入 L3:                                            │
│  - 新知识: 直接写入                                               │
│  - 更新知识: 合并 evidence，更新置信度                              │
│  - 废弃知识: 置信度降至 0.4 以下的知识标记为 deprecated             │
│                                                                  │
│  Step 5: Agent 校准                                               │
│  ────────────────                                                │
│  根据近期表现校准 Agent 个体记忆:                                  │
│  - 更新每个 Agent 的历史准确率                                     │
│  - 计算置信度校准偏差 (如 Agent 习惯性给出 0.8 置信度但胜率仅 50%)  │
│  - 生成校准因子，在下次分析时调整 Agent 的置信度输出                │
│                                                                  │
│  Step 6: 记忆清理                                                 │
│  ────────────────                                                │
│  - L2: 归档权重 < 0.1 的情景记忆                                  │
│  - L3: 标记 90 天未使用且置信度 < 0.5 的知识为 deprecated          │
│  - L1: 清理所有过期会话缓存                                       │
└──────────────────────────────────────────────────────────────────┘
```

**Agent 置信度校准示例：**

```
K线形态分析师校准报告:
┌─────────────────────────────────────────────────────────┐
│  历史预测: 20 次                                         │
│  平均输出置信度: 0.72                                     │
│  实际胜率: 0.55                                          │
│  校准因子: 0.55 / 0.72 = 0.76                            │
│                                                         │
│  → 下次该 Agent 输出置信度 0.80 时，系统自动校准为:       │
│    0.80 × 0.76 = 0.61                                   │
│                                                         │
│  分项校准:                                                │
│  - 双底形态判断: 胜率 75% → 校准因子 1.04 (可信)          │
│  - 头肩形态判断: 胜率 40% → 校准因子 0.56 (过度自信)      │
│  - 三角整理判断: 胜率 50% → 校准因子 0.71 (轻微过度自信)   │
└─────────────────────────────────────────────────────────┘
```

### 12.8 记忆增强的完整分析流程

```
用户发起 DOGE 分析请求
         │
         ▼
┌─────────────────────┐
│ Step 1: 记忆检索     │  从 L3 检索 DOGE 相关的市场规律
│ (L3 知识记忆)        │  从 L2 检索相似市场环境的历史情景
└─────────┬───────────┘
          │
          ▼
┌─────────────────────┐
│ Step 2: 上下文构建   │  将检索到的记忆注入各 Agent 的 prompt
│                     │  - L3 知识 → 所有 Agent 共享
│                     │  - L2 部门记忆 → 对应部门 Agent
│                     │  - Agent 个体校准因子 → 各 Agent
└─────────┬───────────┘
          │
          ▼
┌─────────────────────┐
│ Step 3: 分析与辩论   │  各 Agent 使用增强上下文进行分析
│ (L1 工作记忆)        │  中间结果缓存到 L1
└─────────┬───────────┘
          │
          ▼
┌─────────────────────┐
│ Step 4: 基金经理决策 │  基金经理综合辩论结果 + 历史记忆
│                     │  使用可信度加权 (含校准因子)
└─────────┬───────────┘
          │
          ▼
┌─────────────────────┐
│ Step 5: 记忆写入     │  L1 → 提炼为 L2 情景记忆
│                     │  决策结果写入 agent_performance
│                     │  清理 L1 工作记忆
└─────────┬───────────┘
          │
          ▼
┌─────────────────────┐
│ Step 6: 异步反思     │  (每日定时) 结果回填 → 模式发现
│ (反思循环)           │  → 知识验证 → L3 写入 → Agent 校准
└─────────────────────┘
```

### 12.9 新增数据库表

```sql
-- L2: 情景记忆表
CREATE TABLE episodic_memory (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    session_id UUID REFERENCES agent_sessions(id),
    symbol VARCHAR(32) NOT NULL,
    market_context JSONB NOT NULL,
    agent_consensus JSONB NOT NULL,
    decision JSONB NOT NULL,
    actual_outcome JSONB,
    embedding vector(1536),
    importance_score FLOAT DEFAULT 0.6,
    access_count INTEGER DEFAULT 0,
    last_accessed TIMESTAMPTZ,
    decay_factor FLOAT DEFAULT 1.0,
    effective_weight FLOAT DEFAULT 0.6,
    created_at TIMESTAMPTZ DEFAULT NOW(),
    archived_at TIMESTAMPTZ
);

CREATE INDEX idx_episodic_symbol ON episodic_memory(symbol);
CREATE INDEX idx_episodic_weight ON episodic_memory(effective_weight);
CREATE INDEX idx_episodic_embedding ON episodic_memory 
    USING ivfflat (embedding vector_cosine_ops) WITH (lists = 100);

-- L3: 知识记忆表
CREATE TABLE knowledge_memory (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    category VARCHAR(32) NOT NULL,
    title VARCHAR(256) NOT NULL,
    content TEXT NOT NULL,
    evidence JSONB NOT NULL,
    confidence FLOAT NOT NULL,
    applicable_conditions JSONB NOT NULL,
    embedding vector(1536),
    version INTEGER DEFAULT 1,
    status VARCHAR(32) DEFAULT 'active',
    source_episodes UUID[],
    created_at TIMESTAMPTZ DEFAULT NOW(),
    updated_at TIMESTAMPTZ DEFAULT NOW(),
    deprecated_at TIMESTAMPTZ
);

CREATE INDEX idx_knowledge_category ON knowledge_memory(category);
CREATE INDEX idx_knowledge_confidence ON knowledge_memory(confidence);
CREATE INDEX idx_knowledge_embedding ON knowledge_memory 
    USING ivfflat (embedding vector_cosine_ops) WITH (lists = 100);

-- Agent 个体校准表
CREATE TABLE agent_calibration (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    agent_id VARCHAR(64) REFERENCES agent_definitions(agent_id),
    total_predictions INTEGER DEFAULT 0,
    correct_predictions INTEGER DEFAULT 0,
    avg_output_confidence FLOAT DEFAULT 0.5,
    actual_win_rate FLOAT DEFAULT 0.5,
    calibration_factor FLOAT DEFAULT 1.0,
    category_calibration JSONB DEFAULT '{}',
    last_calibrated TIMESTAMPTZ DEFAULT NOW(),
    updated_at TIMESTAMPTZ DEFAULT NOW()
);

-- 记忆反思日志表
CREATE TABLE memory_reflection_log (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    reflection_type VARCHAR(32) NOT NULL,
    findings JSONB NOT NULL,
    knowledge_created UUID[],
    knowledge_updated UUID[],
    knowledge_deprecated UUID[],
    agents_calibrated VARCHAR(64)[],
    run_at TIMESTAMPTZ DEFAULT NOW()
);
```

### 12.10 新增后端模块

```
backend/src/agents/
├── memory/
│   ├── mod.rs              # 记忆模块声明
│   ├── working.rs          # L1 工作记忆 (Redis)
│   ├── episodic.rs         # L2 情景记忆 (PostgreSQL + pgvector)
│   ├── knowledge.rs        # L3 知识记忆 (PostgreSQL + pgvector)
│   ├── retrieval.rs        # RAG 检索引擎
│   ├── decay.rs            # 衰减计算器
│   ├── consolidation.rs    # L2→L3 巩固逻辑
│   ├── calibration.rs      # Agent 置信度校准
│   └── reflection.rs       # 反思循环调度器
```

**核心 Trait：**

```rust
#[async_trait]
pub trait MemoryStore: Send + Sync {
    type Entry;
    
    async fn store(&self, entry: &Self::Entry) -> Result<()>;
    async fn retrieve(&self, query: &MemoryQuery) -> Result<Vec<Self::Entry>>;
    async fn update(&self, id: &Uuid, entry: &Self::Entry) -> Result<()>;
    async fn delete(&self, id: &Uuid) -> Result<()>;
}

pub struct MemoryQuery {
    pub symbol: String,
    pub market_context: Option<MarketContext>,
    pub category: Option<String>,
    pub top_k: usize,
    pub min_similarity: f64,
    pub min_weight: f64,
}

pub struct MemoryManager {
    working: Arc<WorkingMemory>,
    episodic: Arc<EpisodicMemory>,
    knowledge: Arc<KnowledgeMemory>,
    retrieval: Arc<RetrievalEngine>,
    reflection: Arc<ReflectionScheduler>,
}

impl MemoryManager {
    pub async fn enrich_context(&self, symbol: &str, market_context: &MarketContext) -> MemoryContext {
        let relevant_knowledge = self.knowledge.retrieve(&MemoryQuery {
            symbol: symbol.to_string(),
            market_context: Some(market_context.clone()),
            min_similarity: 0.7,
            top_k: 5,
            ..Default::default()
        }).await.unwrap_or_default();
        
        let similar_episodes = self.episodic.retrieve(&MemoryQuery {
            symbol: symbol.to_string(),
            market_context: Some(market_context.clone()),
            min_similarity: 0.7,
            min_weight: 0.3,
            top_k: 5,
        }).await.unwrap_or_default();
        
        MemoryContext {
            knowledge: relevant_knowledge,
            episodes: similar_episodes,
        }
    }
    
    pub async fn consolidate_session(&self, session: &SessionResult) -> Result<()> {
        let episode = self.extract_episode(session);
        self.episodic.store(&episode).await?;
        self.working.cleanup(&session.session_id).await?;
        Ok(())
    }
    
    pub async fn run_reflection_cycle(&self) -> Result<ReflectionReport> {
        self.reflection.run_full_cycle().await
    }
}
```

### 12.11 记忆相关 API

| 方法 | 路径 | 说明 |
|------|------|------|
| GET | `/api/v1/agent/memory/episodes` | 查询情景记忆 |
| GET | `/api/v1/agent/memory/episodes/{id}` | 获取情景详情 |
| GET | `/api/v1/agent/memory/knowledge` | 查询知识记忆 |
| GET | `/api/v1/agent/memory/knowledge/{id}` | 获取知识详情 |
| POST | `/api/v1/agent/memory/search` | 向量相似度搜索 |
| GET | `/api/v1/agent/memory/calibration` | 获取 Agent 校准数据 |
| POST | `/api/v1/agent/memory/reflect` | 手动触发反思循环 |
| GET | `/api/v1/agent/memory/stats` | 获取记忆统计信息 |

### 12.12 基础设施需求

| 组件 | 用途 | 说明 |
|------|------|------|
| **pgvector 扩展** | 向量存储与相似度检索 | PostgreSQL 扩展，需在现有数据库中启用 |
| **Embedding 模型** | 将市场环境编码为向量 | 推荐 OpenAI text-embedding-3-small 或本地 BGE-M3 |
| **Redis** (已有) | L1 工作记忆 | 复用现有 Redis 实例 |
| **定时任务** | 反思循环调度 | 复用现有 `tokio-cron-scheduler` |

**pgvector 启用：**

```sql
CREATE EXTENSION IF NOT EXISTS vector;
```

---

## 13. 自提升基金经理 Agent 设计

### 13.1 现有设计的问题诊断

当前基金经理 Agent 本质上是一个**静态决策器**——每次分析都从零开始，仅靠注入历史情景做参考，但自身不会"变聪明"。具体问题：

| 问题 | 说明 |
|------|------|
| **Prompt 静态** | 系统提示词写死，不会根据经验自动优化 |
| **规则硬编码** | 风险否决规则是固定的，无法根据市场环境动态调整 |
| **权重固定** | 部门权重 (0.35/0.35/0.30) 是人工设定的，不会根据各部门实际表现自动调整 |
| **无策略进化** | 不会从成功/失败中提炼新的决策策略 |
| **无自我评估** | 不知道自己哪里做得好、哪里做得差 |
| **反思被动** | 仅在每日反思循环中被动接收结果，不会主动发起反思 |

### 13.2 自提升设计理论基础

本设计融合 5 个前沿自提升框架的核心思想：

| 框架 | 核心思想 | 在基金经理中的应用 |
|------|---------|------------------|
| **Reflexion** (Shinn et al.) | Actor → Evaluator → Self-Reflection 三角色闭环 | 决策(Actor) → 结果评估(Evaluator) → 自我反思(Self-Reflection) |
| **Hermes Agent** (NousResearch) | 内置学习闭环 + Skills 沉淀 + 周期性微调 | 决策经验沉淀为可复用 Skills，Prompt 自动进化 |
| **Self-Evolving Agent 综述** (普林斯顿/清华/CMU) | What/When/How 四维进化 | 进化 Prompt、进化记忆、进化工具、进化架构 |
| **A-MEM** (卢曼卡片笔记法) | 原子化笔记 + 动态链接 + 记忆进化 | 决策经验原子化，语义链接自动关联 |
| **Mem0** | ADD/UPDATE/DELETE/NOOP 记忆操作 | 经验知识库的动态增删改 |

### 13.3 自提升基金经理 Agent 架构

```
┌─────────────────────────────────────────────────────────────────────┐
│              自提升基金经理 Agent (Self-Evolving Fund Manager)        │
│                                                                     │
│  ┌─────────────────────────────────────────────────────────────┐    │
│  │                    决策核心 (Actor)                          │    │
│  │  ┌──────────┐  ┌──────────┐  ┌──────────┐  ┌──────────┐   │    │
│  │  │ 动态Prompt│  │ 可信度引擎│  │ 风险引擎  │  │ 策略选择器│   │    │
│  │  │ (自进化)  │  │ (自校准)  │  │ (自调整)  │  │ (自进化)  │   │    │
│  │  └──────────┘  └──────────┘  └──────────┘  └──────────┘   │    │
│  └──────────────────────────┬──────────────────────────────────┘    │
│                             │ 决策输出                               │
│  ┌──────────────────────────▼──────────────────────────────────┐    │
│  │                  结果评估器 (Evaluator)                       │    │
│  │  ┌──────────┐  ┌──────────┐  ┌──────────┐  ┌──────────┐   │    │
│  │  │ 盈亏评估  │  │ 决策质量  │  │ 风险控制  │  │ 一致性    │   │    │
│  │  │ (PnL)    │  │ (评分)    │  │ (评分)    │  │ (评分)    │   │    │
│  │  └──────────┘  └──────────┘  └──────────┘  └──────────┘   │    │
│  └──────────────────────────┬──────────────────────────────────┘    │
│                             │ 评估结果                               │
│  ┌──────────────────────────▼──────────────────────────────────┐    │
│  │                  自我反思器 (Self-Reflector)                  │    │
│  │  ┌──────────┐  ┌──────────┐  ┌──────────┐  ┌──────────┐   │    │
│  │  │ Prompt进化│  │ 策略进化  │  │ 规则进化  │  │ 经验沉淀  │   │    │
│  │  │ (优化提示)│  │ (提炼策略)│  │ (调整规则)│  │ (写入知识)│   │    │
│  │  └──────────┘  └──────────┘  └──────────┘  └──────────┘   │    │
│  └──────────────────────────┬──────────────────────────────────┘    │
│                             │ 进化指令                               │
│  ┌──────────────────────────▼──────────────────────────────────┐    │
│  │                  进化执行器 (Evolution Engine)                │    │
│  │  ┌──────────┐  ┌──────────┐  ┌──────────┐  ┌──────────┐   │    │
│  │  │ Prompt修改│  │ 策略版本  │  │ 规则版本  │  │ 知识图谱  │   │    │
│  │  │ (写入)   │  │ (管理)    │  │ (管理)    │  │ (更新)    │   │    │
│  │  └──────────┘  └──────────┘  └──────────┘  └──────────┘   │    │
│  └─────────────────────────────────────────────────────────────┘    │
│                                                                     │
│  ┌─────────────────────────────────────────────────────────────┐    │
│  │              周期性自省 (Periodic Self-Review)                │    │
│  │  每日: 结果回填 → 评估 → 反思 → 微调                          │    │
│  │  每周: 策略复盘 → 知识巩固 → Prompt 重写                      │    │
│  │  每月: 架构审视 → 能力边界评估 → 进化方向决策                   │    │
│  └─────────────────────────────────────────────────────────────┘    │
└─────────────────────────────────────────────────────────────────────┘
```

### 13.4 四维自进化机制 (What Evolves)

参考 Self-Evolving Agent 综述，基金经理 Agent 在四个维度上持续进化：

```
┌─────────────────────────────────────────────────────────────────────┐
│                     四维自进化机制                                    │
│                                                                     │
│  ┌───────────────────────┐  ┌───────────────────────┐              │
│  │  1. Prompt 进化         │  │  2. 策略进化           │              │
│  │  (Context Evolution)   │  │  (Strategy Evolution) │              │
│  │                        │  │                        │              │
│  │  - 系统提示词自动优化    │  │  - 从经验中提炼新策略   │              │
│  │  - 决策规则动态调整      │  │  - 策略版本管理        │              │
│  │  - 部门权重自适应        │  │  - 策略组合优化        │              │
│  │  - 风险阈值自适应        │  │  - 策略淘汰机制        │              │
│  └───────────────────────┘  └───────────────────────┘              │
│                                                                     │
│  ┌───────────────────────┐  ┌───────────────────────┐              │
│  │  3. 记忆进化            │  │  4. 架构进化           │              │
│  │  (Memory Evolution)    │  │  (Architecture Evol.) │              │
│  │                        │  │                        │              │
│  │  - 经验知识图谱自构建    │  │  - 决策流程优化        │              │
│  │  - 记忆动态增删改       │  │  - 子能力自动发现      │              │
│  │  - 知识冲突自动解决      │  │  - 能力边界自评估      │              │
│  │  - 记忆质量持续提升      │  │  - 协作模式进化        │              │
│  └───────────────────────┘  └───────────────────────┘              │
└─────────────────────────────────────────────────────────────────────┘
```

### 13.5 维度一：Prompt 进化

基金经理 Agent 的系统提示词不是静态的，而是根据经验持续优化。

**Prompt 版本管理：**

```
┌──────────────────────────────────────────────────────────────────┐
│  Prompt 版本演进示例                                               │
│                                                                  │
│  v1.0 (初始版本)                                                  │
│  ├── 固定部门权重: tech=0.35, capital=0.35, news=0.30             │
│  ├── 固定风险规则: 止损>5%降仓, 两部门看空禁做多                   │
│  └── 固定杠杆上限: 5x                                             │
│                                                                  │
│  v1.1 (运行7天后自动进化)                                          │
│  ├── 动态部门权重: tech=0.40, capital=0.35, news=0.25             │
│  │   (原因: 技术部门近7天胜率78%，新闻部门仅52%)                    │
│  ├── 新增规则: DOGE 在0.18-0.20区间时技术信号权重额外+0.1          │
│  │   (原因: 历史数据表明此区间技术面预测更准)                       │
│  └── 调整杠杆: 高波动期(ATR>5%)上限降为3x                          │
│      (原因: 高波动期5x杠杆导致2次止损被扫)                          │
│                                                                  │
│  v1.2 (运行14天后自动进化)                                         │
│  ├── 动态部门权重: tech=0.38, capital=0.37, news=0.25             │
│  ├── 新增策略: "费率极低+双底"组合信号 → 可突破正常仓位限制至8%     │
│  │   (原因: 此组合信号历史胜率83%，值得加大仓位)                     │
│  ├── 新增规则: Musk发推后30min内不做反向操作                        │
│  │   (原因: 2次在Musk推文后做空被套)                                │
│  └── 调整止损: 从固定百分比改为基于ATR动态止损                      │
│      (原因: 固定5%止损在高波动期过窄，低波动期过宽)                  │
└──────────────────────────────────────────────────────────────────┘
```

**Prompt 进化流程：**

```
每日反思循环
     │
     ▼
┌──────────────────┐
│ 分析近期决策质量   │  最近 N 次决策的胜率、盈亏比、最大回撤
└────────┬─────────┘
         │
         ▼
┌──────────────────┐
│ 识别 Prompt 弱点  │  哪些规则导致了错误决策？哪些规则缺失？
│ (Self-Reflector) │  哪些权重分配不合理？
└────────┬─────────┘
         │
         ▼
┌──────────────────┐
│ 生成 Prompt 修改案│  提出 1-3 条具体修改建议
│ (LLM 生成)       │  每条修改附带: 原因、预期效果、回测依据
└────────┬─────────┘
         │
         ▼
┌──────────────────┐
│ A/B 测试验证      │  新旧 Prompt 在历史数据上对比
│ (回测引擎)        │  仅在胜率/盈亏比提升时才采纳
└────────┬─────────┘
         │
         ▼
┌──────────────────┐
│ 写入新版本 Prompt │  版本号+1，记录变更日志
│ (Evolution Engine)│  保留旧版本用于回滚
└──────────────────┘
```

**Prompt 动态组装：**

```rust
pub struct EvolvingPrompt {
    base_prompt: String,
    version: u32,
    dynamic_rules: Vec<DynamicRule>,
    department_weights: HashMap<String, f64>,
    risk_parameters: RiskParameters,
    active_strategies: Vec<StrategyId>,
    recent_lessons: Vec<String>,
}

impl EvolvingPrompt {
    pub fn build(&self, context: &AnalysisContext) -> String {
        let mut prompt = self.base_prompt.clone();
        
        prompt.push_str(&format!("\n## 当前部门权重 (v{})", self.version));
        for (dept, weight) in &self.department_weights {
            prompt.push_str(&format!("\n- {}: {:.2}", dept, weight));
        }
        
        prompt.push_str("\n## 动态决策规则");
        for rule in &self.dynamic_rules {
            prompt.push_str(&format!("\n- [{}] {} (来源: {})", 
                rule.priority, rule.content, rule.source));
        }
        
        prompt.push_str("\n## 近期教训");
        for lesson in &self.recent_lessons.iter().take(5) {
            prompt.push_str(&format!("\n- {}", lesson));
        }
        
        prompt
    }
}
```

### 13.6 维度二：策略进化

基金经理 Agent 从经验中自动提炼、验证、淘汰决策策略。

**策略生命周期：**

```
┌──────────────────────────────────────────────────────────────────┐
│                     策略生命周期                                   │
│                                                                  │
│  ┌──────────┐    ┌──────────┐    ┌──────────┐    ┌──────────┐  │
│  │  萌芽     │───►│  候选     │───►│  活跃     │───►│  淘汰     │  │
│  │ (Sprout) │    │(Candidate)│    │(Active)  │    │(Retired) │  │
│  └──────────┘    └──────────┘    └──────────┘    └──────────┘  │
│       │               │               │               │          │
│  从1-2次成功经验  出现3-5次且       验证通过          胜率连续     │
│  中识别的模式     胜率>60%         胜率>65%          2周低于50%   │
│                                                                  │
│  示例:                                                           │
│  萌芽: "费率极低时做多似乎有效" (1次)                              │
│  候选: "费率<-0.01%+双底 → 做多" (3次, 胜率66%)                   │
│  活跃: "费率<-0.01%+双底+OI上升 → 做多, 仓位8%" (8次, 胜率75%)    │
│  淘汰: "Musk推文后追涨" (5次, 胜率40%, 已退化为噪音)               │
└──────────────────────────────────────────────────────────────────┘
```

**策略结构：**

```json
{
  "strategy_id": "str_001",
  "name": "低费率逼空策略",
  "version": 3,
  "status": "active",
  "conditions": {
    "funding_rate_max": -0.01,
    "pattern": "double_bottom",
    "oi_trend": "rising_or_stable",
    "sentiment_max": 0.4
  },
  "action": {
    "direction": "long",
    "position_size_override": 8.0,
    "leverage_override": 3,
    "stop_loss_method": "atr_based",
    "stop_loss_atr_multiplier": 1.5
  },
  "evidence": {
    "total_occurrences": 8,
    "win_count": 6,
    "avg_pnl": 11.2,
    "max_loss": -3.5,
    "first_seen": "2026-05-10",
    "last_triggered": "2026-05-28"
  },
  "confidence": 0.75,
  "evolution_log": [
    {"version": 1, "change": "初始发现", "date": "2026-05-10"},
    {"version": 2, "change": "新增OI上升条件", "date": "2026-05-20"},
    {"version": 3, "change": "仓位从5%提升至8%", "date": "2026-05-28"}
  ]
}
```

**策略进化算法：**

```
策略进化触发条件:
┌──────────────────────────────────────────────────────┐
│ 1. 每日反思时: 扫描近期情景，发现新的模式               │
│ 2. 策略触发时: 记录触发结果，更新策略统计               │
│ 3. 策略失效时: 连续3次亏损，降级为候选                  │
│                                                      │
│ 策略提炼过程:                                         │
│   输入: N个成功/失败的决策情景                         │
│   步骤1: LLM 分析共同特征 → 提取条件模式               │
│   步骤2: 在历史数据上回测验证                          │
│   步骤3: 通过验证 → 升级策略版本                       │
│   步骤4: 未通过 → 降级或淘汰                          │
│                                                      │
│ 策略组合优化:                                         │
│   - 同时活跃策略不超过 10 个                           │
│   - 冲突策略 (同条件不同方向) 保留胜率高的               │
│   - 冗余策略 (条件高度重叠) 合并为更精确的版本           │
└──────────────────────────────────────────────────────┘
```

### 13.7 维度三：记忆进化

基金经理 Agent 的记忆不是简单的"堆积"，而是像人类记忆一样会**巩固、更新、遗忘、重构**。

**经验知识图谱 (参考 A-MEM + Mem0)：**

```
┌──────────────────────────────────────────────────────────────────┐
│               基金经理经验知识图谱                                  │
│                                                                  │
│  ┌─────────┐    触发     ┌─────────────┐    导致    ┌─────────┐ │
│  │ Musk推文 │────────────►│ DOGE 短期波动 │──────────►│ 逼空行情 │ │
│  └────┬────┘             └──────┬──────┘           └────┬────┘ │
│       │                         │                       │       │
│  类型 │                    持续时间 │                  概率   │       │
│  ↓                           ↓                       ↓       │
│  ┌─────────┐             ┌─────────────┐          ┌─────────┐ │
│  │利好推文 │             │ 1-4小时      │          │ 65%     │ │
│  │中性提及 │             │ 波动幅度     │          │ 30%     │ │
│  │负面提及 │             │ 平均8%       │          │ 10%     │ │
│  └─────────┘             └─────────────┘          └─────────┘ │
│                                                                  │
│  ┌─────────┐    前提     ┌─────────────┐    信号    ┌─────────┐ │
│  │费率极低 │────────────►│ 空头拥挤     │──────────►│ 逼空行情 │ │
│  └────┬────┘             └──────┬──────┘           └────┬────┘ │
│       │                         │                       │       │
│  阈值 │                    确认条件 │                  概率   │       │
│  ↓                           ↓                       ↓       │
│  ┌─────────┐             ┌─────────────┐          ┌─────────┐ │
│  │<-0.01%  │             │OI不降+价格止跌│          │ 65%     │ │
│  │<-0.005% │             │OI上升       │          │ 45%     │ │
│  │<-0.02%  │             │OI不降+双底  │          │ 83%     │ │
│  └─────────┘             └─────────────┘          └─────────┘ │
│                                                                  │
│  知识图谱操作 (参考 Mem0):                                        │
│  ADD:    新发现的知识节点/关系                                     │
│  UPDATE: 更新已有知识的概率/阈值/条件                              │
│  DELETE: 删除被证伪的知识                                         │
│  MERGE:  合并冗余的知识节点                                       │
│  NOOP:   知识无需修改                                             │
└──────────────────────────────────────────────────────────────────┘
```

**记忆进化操作 (参考 A-MEM 动态链接)：**

```rust
pub enum MemoryOperation {
    Add {
        node: KnowledgeNode,
        links: Vec<KnowledgeLink>,
    },
    Update {
        node_id: Uuid,
        new_content: String,
        new_confidence: f64,
        reason: String,
    },
    Delete {
        node_id: Uuid,
        reason: String,
    },
    Merge {
        source_ids: Vec<Uuid>,
        merged_node: KnowledgeNode,
        reason: String,
    },
    Noop,
}

pub struct KnowledgeNode {
    id: Uuid,
    node_type: String,
    content: String,
    confidence: f64,
    evidence_count: u32,
    last_validated: DateTime<Utc>,
    embedding: Vec<f64>,
    links: Vec<KnowledgeLink>,
    tags: Vec<String>,
    version: u32,
}

pub struct KnowledgeLink {
    source_id: Uuid,
    target_id: Uuid,
    relation: String,
    strength: f64,
}
```

### 13.8 维度四：架构进化

基金经理 Agent 能发现自身能力的不足，并主动扩展。

**能力边界自评估：**

```
┌──────────────────────────────────────────────────────────────────┐
│                  能力边界自评估 (每月执行)                          │
│                                                                  │
│  Step 1: 错误模式分析                                             │
│  ─────────────────                                               │
│  统计近30天决策失误的分类:                                         │
│  - 类型A: "信息不足" — 缺少某个维度的数据 (如缺少链上数据)          │
│  - 类型B: "判断偏差" — 有数据但解读错误 (如误判形态)               │
│  - 类型C: "规则缺失" — 遇到新情况没有对应规则                      │
│  - 类型D: "执行问题" — 决策正确但执行参数不当 (如止损过窄)         │
│                                                                  │
│  Step 2: 能力差距识别                                             │
│  ─────────────────                                               │
│  类型A占比>30% → 需要新增数据源或工具                              │
│  类型B占比>30% → 需要优化分析模型或增加校验环节                     │
│  类型C占比>30% → 需要扩展规则库或策略库                            │
│  类型D占比>30% → 需要优化执行参数或增加模拟验证                     │
│                                                                  │
│  Step 3: 架构调整决策                                             │
│  ─────────────────                                               │
│  示例输出:                                                        │
│  "近30天42%的错误属于类型A(信息不足)，主要缺失:                     │
│   1. 链上大额转账实时监控 → 建议新增链上数据Agent                   │
│   2. 期权隐含波动率 → 建议新增期权数据源                           │
│   3. 交易所钱包余额变化 → 建议新增鲸鱼追踪工具"                    │
└──────────────────────────────────────────────────────────────────┘
```

**架构进化示例：**

```
v1.0 架构: 3部门12Agent + 基金经理
     │
     │ 运行30天, 发现链上数据缺失导致5次错误决策
     ▼
v1.1 架构: 3部门12Agent + 链上数据Agent + 基金经理
     │
     │ 运行60天, 发现期权到期日对DOGE价格影响显著
     ▼
v1.2 架构: 3部门12Agent + 链上Agent + 期权Agent + 基金经理
     │
     │ 运行90天, 发现单基金经理容易受认知偏差影响
     ▼
v2.0 架构: 3部门12Agent + 专项Agent + 双基金经理(多空对抗) + 仲裁者
```

### 13.9 Reflexion 闭环：决策→评估→反思→进化

这是基金经理 Agent 自提升的核心引擎，参考 Reflexion 框架的 Actor-Evaluator-SelfReflection 三角色设计：

```
┌─────────────────────────────────────────────────────────────────────┐
│                     Reflexion 闭环                                   │
│                                                                     │
│  ┌─────────────────┐                                                │
│  │  Actor (决策者)   │                                                │
│  │                  │                                                │
│  │  接收部门报告     │                                                │
│  │  使用当前 Prompt  │                                                │
│  │  应用活跃策略     │                                                │
│  │  输出投资决策     │──────┐                                         │
│  └─────────────────┘      │                                         │
│                           ▼                                         │
│  ┌─────────────────┐                                                │
│  │  Evaluator (评估) │                                                │
│  │                  │                                                │
│  │  等待结果揭晓     │  (4h/24h后获取实际价格)                         │
│  │  计算盈亏        │                                                │
│  │  评估决策质量     │  (0-100分, 多维度)                              │
│  │  标注错误类型     │  (信息不足/判断偏差/规则缺失/执行问题)            │
│  │  输出评估报告     │──────┐                                         │
│  └─────────────────┘      │                                         │
│                           ▼                                         │
│  ┌─────────────────┐                                                │
│  │ Self-Reflector   │                                                │
│  │  (自我反思)       │                                                │
│  │                  │                                                │
│  │  分析错误根因     │                                                │
│  │  提炼经验教训     │                                                │
│  │  生成改进方案     │                                                │
│  │  输出进化指令     │──────┐                                         │
│  └─────────────────┘      │                                         │
│                           ▼                                         │
│  ┌─────────────────┐                                                │
│  │ Evolution Engine │                                                │
│  │  (进化执行)       │                                                │
│  │                  │                                                │
│  │  执行 Prompt 修改 │                                                │
│  │  更新策略版本     │                                                │
│  │  更新知识图谱     │                                                │
│  │  调整风险参数     │                                                │
│  │  写入进化日志     │──────┐                                         │
│  └─────────────────┘      │                                         │
│                           │                                         │
│                           ▼                                         │
│                    下一次决策使用进化后的配置                          │
│                    (Prompt v1.2 + 策略v3 + 知识图谱v5)               │
└─────────────────────────────────────────────────────────────────────┘
```

**评估维度与评分：**

| 维度 | 权重 | 评分标准 |
|------|------|---------|
| **盈亏结果** | 30% | PnL% 相对预期，止损是否被合理触发 |
| **方向判断** | 25% | 多空方向是否正确，置信度是否匹配 |
| **时机把握** | 20% | 入场时机是否最优，是否过早/过晚 |
| **风险控制** | 15% | 止损设置是否合理，仓位是否适当 |
| **一致性** | 10% | 决策逻辑是否与声称的策略一致 |

**反思输出格式：**

```json
{
  "reflection_id": "uuid",
  "decision_id": "uuid",
  "overall_score": 72,
  "dimension_scores": {
    "pnl": 85,
    "direction": 90,
    "timing": 60,
    "risk_control": 65,
    "consistency": 80
  },
  "error_type": "execution",
  "root_cause": "止损设置为固定3%，但当时ATR为5.3%，止损过窄导致被扫",
  "lessons": [
    "高波动期应使用ATR倍数止损而非固定百分比",
    "DOGE在Musk推文后波动率会急剧上升，应提前调整止损参数"
  ],
  "evolution_actions": [
    {
      "type": "prompt_update",
      "description": "将止损规则从固定百分比改为ATR倍数",
      "target": "risk_parameters.stop_loss_method",
      "old_value": "fixed_percent: 3%",
      "new_value": "atr_multiplier: 1.5"
    },
    {
      "type": "strategy_update",
      "strategy_id": "str_001",
      "description": "新增条件: Musk推文后2h内ATR止损倍数从1.5提升至2.0"
    },
    {
      "type": "knowledge_add",
      "description": "Musk推文后DOGE的ATR平均从3%升至7%，持续约2小时"
    }
  ]
}
```

### 13.10 周期性自省机制 (参考 Hermes Periodic Nudge)

Hermes Agent 的关键创新之一是 **Periodic Nudge**——即使没有用户输入，系统也会定期自动触发自省。基金经理 Agent 采用类似机制：

```
┌──────────────────────────────────────────────────────────────────┐
│                  周期性自省时间表                                   │
│                                                                  │
│  ┌──────────────────────────────────────────────────────────┐    │
│  │  每日 08:00 — 晨间自省 (Morning Review)                   │    │
│  │  ├── 回填昨日决策的实际结果                                │    │
│  │  ├── 评估每笔决策的质量                                    │    │
│  │  ├── 更新 Agent 准确率统计                                 │    │
│  │  ├── 微调部门权重 (±0.02)                                  │    │
│  │  └── 更新近期教训列表 (保留最近10条)                       │    │
│  └──────────────────────────────────────────────────────────┘    │
│                                                                  │
│  ┌──────────────────────────────────────────────────────────┐    │
│  │  每周日 20:00 — 周度复盘 (Weekly Retrospective)            │    │
│  │  ├── 统计本周整体胜率、盈亏比、最大回撤                     │    │
│  │  ├── 识别本周最佳/最差决策                                 │    │
│  │  ├── 策略状态审查: 萌芽→候选, 候选→活跃, 活跃→淘汰          │    │
│  │  ├── Prompt 版本评估: 是否需要升级                         │    │
│  │  ├── 知识图谱维护: 合并冗余, 删除过时, 添加新知             │    │
│  │  └── 生成周度进化报告                                      │    │
│  └──────────────────────────────────────────────────────────┘    │
│                                                                  │
│  ┌──────────────────────────────────────────────────────────┐    │
│  │  每月1日 20:00 — 月度架构审视 (Monthly Architecture Rev.)  │    │
│  │  ├── 能力边界评估: 错误模式分类统计                        │    │
│  │  ├── 架构调整决策: 是否需要新增Agent/工具/数据源            │    │
│  │  ├── Prompt 大版本重写: 基于月度经验全面优化                │    │
│  │  ├── 策略库大扫除: 淘汰低效策略, 合并相似策略              │    │
│  │  ├── 知识图谱重构: 消除矛盾, 强化核心知识                  │    │
│  │  └── 生成月度进化报告 + 人工审核建议                       │    │
│  └──────────────────────────────────────────────────────────┘    │
│                                                                  │
│  ┌──────────────────────────────────────────────────────────┐    │
│  │  触发式自省 (Event-Driven Reflection)                      │    │
│  │  ├── 连续3次决策失误 → 立即触发深度反思                     │    │
│  │  ├── 单日亏损超过5% → 立即触发风控审查                     │    │
│  │  ├── 市场出现黑天鹅事件 → 立即触发策略暂停+重新评估         │    │
│  │  └── 新策略验证失败 → 立即触发策略回滚                     │    │
└──────────────────────────────────────────────────────────────────┘
```

### 13.11 安全护栏：防止自进化失控

自进化的最大风险是"越进化越差"——错误的经验被固化，导致恶性循环。

```
┌──────────────────────────────────────────────────────────────────┐
│                     自进化安全护栏                                  │
│                                                                  │
│  1. 版本回滚机制                                                  │
│  ────────────────                                                │
│  - 保留所有历史 Prompt 版本和策略版本                              │
│  - 如果新版本连续3次表现差于旧版本 → 自动回滚                     │
│  - 任何版本变更都有 A/B 回测验证                                  │
│                                                                  │
│  2. 人工审核关卡                                                  │
│  ────────────────                                                │
│  - Prompt 大版本升级 (v1.x → v2.x) 需人工审核                    │
│  - 架构进化决策需人工确认                                         │
│  - 知识图谱中置信度 <0.5 的新知识需人工验证                       │
│  - 月度进化报告推送给管理员审核                                    │
│                                                                  │
│  3. 进化速度限制                                                  │
│  ────────────────                                                │
│  - 每日最多修改 3 条 Prompt 规则                                  │
│  - 部门权重单日调整不超过 ±0.05                                   │
│  - 新策略至少观察 3 天才能从萌芽升级为候选                         │
│  - 候选策略至少观察 7 天才能升级为活跃                             │
│                                                                  │
│  4. 一致性校验                                                    │
│  ────────────────                                                │
│  - 新规则不能与硬性风控规则冲突                                   │
│  - 策略进化不能突破最大杠杆/最大仓位限制                          │
│  - 知识图谱更新不能删除"核心风险原则"类知识                       │
│  - Prompt 修改后必须通过格式校验和逻辑一致性检查                   │
│                                                                  │
│  5. 不可变核心                                                    │
│  ────────────────                                                │
│  以下规则永远不可被自进化修改:                                     │
│  - 单笔最大亏损不超过总资金 2%                                    │
│  - 最大杠杆不超过 5x                                              │
│  - 必须设置止损                                                   │
│  - 两部门看空且置信度>0.7时禁止做多                                │
│  - 流动性差时禁止开仓                                             │
└──────────────────────────────────────────────────────────────────┘
```

### 13.12 新增数据库表

```sql
-- Prompt 版本管理表
CREATE TABLE prompt_versions (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    version INTEGER NOT NULL,
    content TEXT NOT NULL,
    dynamic_rules JSONB DEFAULT '[]',
    department_weights JSONB DEFAULT '{}',
    risk_parameters JSONB DEFAULT '{}',
    change_log TEXT,
    performance_score FLOAT,
    is_active BOOLEAN DEFAULT false,
    created_at TIMESTAMPTZ DEFAULT NOW(),
    activated_at TIMESTAMPTZ
);

-- 策略版本管理表
CREATE TABLE strategy_versions (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    strategy_id VARCHAR(64) NOT NULL,
    version INTEGER NOT NULL,
    name VARCHAR(256) NOT NULL,
    status VARCHAR(32) DEFAULT 'sprout',
    conditions JSONB NOT NULL,
    action JSONB NOT NULL,
    evidence JSONB DEFAULT '{}',
    confidence FLOAT DEFAULT 0.5,
    evolution_log JSONB DEFAULT '[]',
    created_at TIMESTAMPTZ DEFAULT NOW(),
    updated_at TIMESTAMPTZ DEFAULT NOW(),
    retired_at TIMESTAMPTZ
);

-- 经验知识图谱节点表
CREATE TABLE knowledge_nodes (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    node_type VARCHAR(32) NOT NULL,
    content TEXT NOT NULL,
    confidence FLOAT NOT NULL,
    evidence_count INTEGER DEFAULT 1,
    last_validated TIMESTAMPTZ DEFAULT NOW(),
    embedding vector(1536),
    tags TEXT[] DEFAULT '{}',
    version INTEGER DEFAULT 1,
    status VARCHAR(32) DEFAULT 'active',
    created_at TIMESTAMPTZ DEFAULT NOW(),
    updated_at TIMESTAMPTZ DEFAULT NOW()
);

CREATE INDEX idx_knowledge_nodes_embedding ON knowledge_nodes 
    USING ivfflat (embedding vector_cosine_ops) WITH (lists = 100);

-- 经验知识图谱关系表
CREATE TABLE knowledge_links (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    source_id UUID REFERENCES knowledge_nodes(id),
    target_id UUID REFERENCES knowledge_nodes(id),
    relation VARCHAR(64) NOT NULL,
    strength FLOAT DEFAULT 1.0,
    created_at TIMESTAMPTZ DEFAULT NOW()
);

CREATE INDEX idx_knowledge_links_source ON knowledge_links(source_id);
CREATE INDEX idx_knowledge_links_target ON knowledge_links(target_id);

-- 反思日志表
CREATE TABLE reflection_logs (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    decision_id UUID REFERENCES fund_manager_decisions(id),
    overall_score FLOAT NOT NULL,
    dimension_scores JSONB NOT NULL,
    error_type VARCHAR(32),
    root_cause TEXT,
    lessons TEXT[],
    evolution_actions JSONB DEFAULT '[]',
    created_at TIMESTAMPTZ DEFAULT NOW()
);

-- 进化日志表
CREATE TABLE evolution_logs (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    evolution_type VARCHAR(32) NOT NULL,
    description TEXT NOT NULL,
    before_state JSONB,
    after_state JSONB,
    reason TEXT,
    backtest_result JSONB,
    approved_by VARCHAR(64),
    created_at TIMESTAMPTZ DEFAULT NOW()
);
```

### 13.13 自提升基金经理 Agent 完整 Prompt (v1.0 基础版)

```
你是 MoneyRobert Pro 投资系统的自提升基金经理，负责 DOGE-USDT-SWAP 的最终投资决策。

## 核心身份
你不是一个静态的决策器，而是一个能从每次决策中学习、反思、进化的智能体。
你的目标不仅是做出正确的决策，更是持续提升自己的决策能力。

## 决策框架 (Bridgewater 可信度加权 + 动态调整)
1. 审阅三份部门共识报告
2. 审阅跨部门辩论纪要
3. 检索经验知识图谱中的相关知识
4. 匹配当前活跃策略
5. 使用动态部门权重进行可信度加权评估
6. 使用动态风险参数进行风险预算检查
7. 做出最终决策

## 当前动态部门权重 (v{version})
{department_weights}

## 当前活跃策略
{active_strategies}

## 近期教训 (最近5条)
{recent_lessons}

## 动态决策规则 (v{version})
{dynamic_rules}

## 动态风险参数 (v{version})
{risk_parameters}

## 不可变核心规则 (不可修改)
- 单笔最大亏损不超过总资金 2%
- 最大杠杆不超过 5x
- 必须设置止损
- 两部门看空且置信度>0.7时禁止做多
- 流动性差时禁止开仓

## 自我评估要求
在做出决策的同时，你必须评估：
1. 本次决策的信心水平 (1-10)
2. 本次决策最可能出错的原因
3. 如果决策错误，最可能的错误类型 (信息不足/判断偏差/规则缺失/执行问题)
4. 哪些额外信息可以提升决策质量

## 输出要求
按基金经理决策格式输出，额外包含 self_assessment 字段。
```

### 13.14 自提升效果度量

| 指标 | 度量方式 | 目标 |
|------|---------|------|
| **决策胜率** | 盈利决策数 / 总决策数 | 持续提升，月环比 >2% |
| **盈亏比** | 平均盈利 / 平均亏损 | > 2.0 |
| **Prompt 版本胜率** | 新版本 vs 旧版本回测胜率 | 每次升级 >0% |
| **策略库质量** | 活跃策略平均胜率 | > 65% |
| **知识图谱准确率** | 知识节点预测准确率 | > 70% |
| **进化频率** | 每月 Prompt/策略更新次数 | 2-4 次 (不过度) |
| **反思深度** | 反思日志中 root_cause 覆盖率 | > 80% |
| **回滚率** | 进化后回滚次数 / 总进化次数 | < 10% |

---

## 14. AI 模拟操盘与实盘晋级系统

### 14.1 现有代码基础

| 模块 | 文件 | 已有能力 | 缺失能力 |
|------|------|---------|---------|
| 模拟交易路由 | `routes/paper_trading.rs` | 开仓/平仓/PnL计算 | 自动止损止盈触发、实时价格驱动、AI自动下单 |
| 自动交易配置 | `routes/auto_trading.rs` | mode字段(默认paper)、ai_confidence_threshold | 实际AI决策执行循环、胜率统计、晋级逻辑 |
| OKX客户端 | `exchanges/okx.rs` | is_demo标志、x-simulated-trading请求头 | 模拟盘与实盘的统一执行接口 |
| 数据库 | `paper_trading_accounts` 表 | 基础账户字段 | 胜率统计、晋级记录、风控参数 |

### 14.2 系统总体设计

```
┌─────────────────────────────────────────────────────────────────────┐
│                AI 模拟操盘与实盘晋级系统                               │
│                                                                     │
│  ┌───────────────────────────────────────────────────────────┐      │
│  │                  AI 决策引擎                                │      │
│  │  (多Agent辩论 → 基金经理决策)                               │      │
│  └──────────────────────────┬────────────────────────────────┘      │
│                             │ 投资决策                               │
│                             ▼                                       │
│  ┌───────────────────────────────────────────────────────────┐      │
│  │              交易执行路由 (Execution Router)                 │      │
│  │                                                             │      │
│  │  ┌─────────────┐    ┌──────────────┐    ┌──────────────┐  │      │
│  │  │ 模拟执行器   │    │ OKX模拟盘     │    │ OKX实盘      │  │      │
│  │  │ (PaperExec) │    │ (DemoExec)   │    │ (LiveExec)   │  │      │
│  │  │ 内部数据库   │    │ x-simulated  │    │ 真实API      │  │      │
│  │  └─────────────┘    └──────────────┘    └──────────────┘  │      │
│  └──────────────────────────┬────────────────────────────────┘      │
│                             │ 交易结果                               │
│                             ▼                                       │
│  ┌───────────────────────────────────────────────────────────┐      │
│  │              胜率统计与晋级引擎 (Promotion Engine)           │      │
│  │                                                             │      │
│  │  模拟胜率 < 90% ──► 继续模拟 (不允许实盘)                    │      │
│  │  模拟胜率 ≥ 90% ──► 晋级审核 (人工确认) ──► 开放实盘权限     │      │
│  │  实盘胜率 < 60% ──► 降级回模拟 (自动触发)                    │      │
│  └───────────────────────────────────────────────────────────┘      │
└─────────────────────────────────────────────────────────────────────┘
```

### 14.3 三层执行模式

```
┌──────────────────────────────────────────────────────────────────┐
│                    三层执行模式                                     │
│                                                                  │
│  Layer 1: 纯模拟 (Paper Mode)                                    │
│  ─────────────────────────                                       │
│  执行方式: 内部数据库模拟，不连接任何外部API                         │
│  价格来源: MarketCollector 采集的实时Ticker数据                     │
│  订单匹配: 以当前Ticker价格即时成交 (模拟无限流动性)                 │
│  止损止盈: 定时任务检查价格，触发时自动平仓                          │
│  资金管理: 虚拟初始资金 100,000 USDT                               │
│  适用阶段: AI Agent 初期训练，积累决策经验                          │
│                                                                  │
│  Layer 2: OKX模拟盘 (Demo Mode)                                  │
│  ─────────────────────────                                       │
│  执行方式: OKX模拟交易API (x-simulated-trading: 1)                 │
│  价格来源: OKX真实行情 (与实盘一致)                                │
│  订单匹配: OKX模拟撮合引擎 (接近真实滑点和深度)                     │
│  止损止盈: OKX系统止损止盈 (与实盘机制一致)                        │
│  资金管理: OKX模拟账户余额                                        │
│  适用阶段: 模拟胜率达到80%后，进入OKX模拟盘验证                     │
│                                                                  │
│  Layer 3: 实盘 (Live Mode)                                       │
│  ─────────────────────────                                       │
│  执行方式: OKX真实交易API                                         │
│  价格来源: OKX真实行情                                            │
│  订单匹配: 真实撮合 (有滑点、有深度限制)                            │
│  止损止盈: OKX系统止损止盈                                        │
│  资金管理: 用户真实资金 (受仓位限制保护)                            │
│  适用阶段: OKX模拟盘胜率≥90% + 人工审核通过                        │
└──────────────────────────────────────────────────────────────────┘
```

### 14.4 AI 模拟操盘自动执行循环

```
┌─────────────────────────────────────────────────────────────────────┐
│              AI 模拟操盘自动执行循环 (每5分钟)                         │
│                                                                     │
│  ┌──────────────┐                                                   │
│  │ 1. 市场扫描   │  MarketCollector 获取最新 DOGE Ticker/K线数据      │
│  └──────┬───────┘                                                   │
│         │                                                           │
│         ▼                                                           │
│  ┌──────────────┐                                                   │
│  │ 2. 持仓检查   │  检查当前模拟持仓:                                │
│  │              │  - 止损是否触发? → 自动平仓                        │
│  │              │  - 止盈是否触发? → 自动平仓                        │
│  │              │  - 持仓时间是否超限? → 自动平仓                    │
│  └──────┬───────┘                                                   │
│         │                                                           │
│         ▼                                                           │
│  ┌──────────────┐                                                   │
│  │ 3. AI 分析    │  触发多Agent辩论 (仅在无持仓或满足分析间隔时)      │
│  │   (可选)      │  基金经理输出决策: long/short/hold                │
│  └──────┬───────┘                                                   │
│         │                                                           │
│         ▼                                                           │
│  ┌──────────────┐                                                   │
│  │ 4. 风控检查   │  检查决策是否通过风控规则:                        │
│  │              │  - 单日最大交易次数                                │
│  │              │  - 单日最大亏损                                    │
│  │              │  - 最大持仓数量                                    │
│  │              │  - AI置信度阈值                                    │
│  └──────┬───────┘                                                   │
│         │                                                           │
│         ▼                                                           │
│  ┌──────────────┐                                                   │
│  │ 5. 订单执行   │  根据当前模式执行:                                │
│  │              │  Paper → 写入 positions 表                        │
│  │              │  Demo  → OKX模拟盘API                             │
│  │              │  Live  → OKX实盘API                               │
│  └──────┬───────┘                                                   │
│         │                                                           │
│         ▼                                                           │
│  ┌──────────────┐                                                   │
│  │ 6. 结果记录   │  写入交易记录、更新胜率统计、触发反思循环          │
│  └──────────────┘                                                   │
└─────────────────────────────────────────────────────────────────────┘
```

### 14.5 模拟操盘核心引擎

```rust
pub struct SimulationEngine {
    db_pool: PgPool,
    okx_client: Arc<OkxClient>,
    agent_engine: Arc<DebateEngine>,
    config: SimulationConfig,
    stats: Arc<TradingStats>,
}

pub struct SimulationConfig {
    mode: ExecutionMode,
    symbol: String,
    initial_balance: f64,
    max_position_size_percent: f64,
    max_leverage: i32,
    max_daily_trades: i32,
    max_daily_loss_percent: f64,
    ai_confidence_threshold: f64,
    analysis_interval_minutes: u32,
    max_holding_period_hours: u32,
    stop_loss_check_interval_seconds: u32,
}

pub enum ExecutionMode {
    Paper,
    Demo,
    Live,
}

impl SimulationEngine {
    pub async fn run_cycle(&self) -> Result<CycleResult> {
        let market_data = self.fetch_market_data().await?;
        
        self.check_stop_loss_take_profit(&market_data).await?;
        
        let should_analyze = self.should_trigger_analysis().await?;
        if should_analyze {
            let decision = self.agent_engine
                .run_session(&self.config.symbol, &Default::default())
                .await?;
            
            if self.pass_risk_check(&decision) {
                self.execute_decision(&decision, &market_data).await?;
            }
        }
        
        self.update_stats().await?;
        
        Ok(CycleResult::default())
    }
    
    async fn check_stop_loss_take_profit(&self, market: &MarketSnapshot) -> Result<()> {
        let positions = self.get_open_positions().await?;
        let current_price = market.current_price;
        
        for pos in &positions {
            let should_close = match pos.side.as_str() {
                "BUY" => {
                    current_price <= pos.stop_loss.unwrap_or(0.0) ||
                    current_price >= pos.take_profit.unwrap_or(f64::MAX)
                }
                "SELL" => {
                    current_price >= pos.stop_loss.unwrap_or(f64::MAX) ||
                    current_price <= pos.take_profit.unwrap_or(0.0)
                }
                _ => false,
            };
            
            let holding_too_long = pos.opened_at
                .map(|t| t < Utc::now() - Duration::hours(self.config.max_holding_period_hours as i64))
                .unwrap_or(false);
            
            if should_close || holding_too_long {
                self.close_position(pos, current_price, if should_close { "sl_tp_trigger" } else { "timeout" }).await?;
            }
        }
        Ok(())
    }
    
    fn pass_risk_check(&self, decision: &FundManagerDecision) -> bool {
        if decision.confidence < self.config.ai_confidence_threshold / 100.0 {
            return false;
        }
        if decision.leverage > self.config.max_leverage {
            return false;
        }
        if decision.position_size_percent > self.config.max_position_size_percent {
            return false;
        }
        true
    }
    
    async fn execute_decision(&self, decision: &FundManagerDecision, market: &MarketSnapshot) -> Result<()> {
        match self.config.mode {
            ExecutionMode::Paper => self.execute_paper(decision, market).await,
            ExecutionMode::Demo => self.execute_okx_demo(decision).await,
            ExecutionMode::Live => self.execute_okx_live(decision).await,
        }
    }
}
```

### 14.6 胜率统计与晋级机制

```
┌─────────────────────────────────────────────────────────────────────┐
│                    晋级阶梯系统                                       │
│                                                                     │
│  ┌─────────────────┐                                               │
│  │  Level 0: 纯模拟 │  初始状态                                     │
│  │  (Paper Mode)    │  虚拟资金 100,000 USDT                        │
│  │                  │  无外部API调用                                 │
│  │  晋级条件:        │  最近50笔交易胜率 ≥ 80%                       │
│  │                  │  且运行天数 ≥ 14天                              │
│  │                  │  且最大单日回撤 < 5%                            │
│  └────────┬─────────┘                                               │
│           │ 自动晋级                                                 │
│           ▼                                                         │
│  ┌─────────────────┐                                               │
│  │  Level 1: OKX   │  OKX模拟盘验证                                 │
│  │  模拟盘          │  使用OKX模拟撮合引擎                           │
│  │  (Demo Mode)    │  仓位限制: 最大2%总资金                         │
│  │                  │  杠杆限制: 最大3x                               │
│  │  晋级条件:        │  最近50笔交易胜率 ≥ 90%                       │
│  │                  │  且运行天数 ≥ 7天                               │
│  │                  │  且最大单日回撤 < 3%                            │
│  │                  │  且盈亏比 > 2.0                                │
│  └────────┬─────────┘                                               │
│           │ 需人工审核                                               │
│           ▼                                                         │
│  ┌─────────────────┐                                               │
│  │  Level 2: 实盘   │  真实交易 (受限)                               │
│  │  (Live Mode)    │  仓位限制: 最大2%总资金                         │
│  │  - 初期          │  杠杆限制: 最大2x                               │
│  │                  │  每日最大交易: 3笔                               │
│  │                  │  每笔交易需人工确认                              │
│  │                  │                                               │
│  │  扩展条件:        │  实盘运行30天 + 胜率≥70%                       │
│  │                  │  → 仓位提升至5%, 杠杆提升至3x                   │
│  │                  │  → 每日最大交易: 5笔                            │
│  │                  │                                               │
│  │  扩展条件:        │  实盘运行60天 + 胜率≥75%                       │
│  │                  │  → 仓位提升至8%, 杠杆提升至5x                   │
│  │                  │  → 每日最大交易: 10笔                           │
│  │                  │  → 取消逐笔人工确认                             │
│  └────────┬─────────┘                                               │
│           │ 需人工审核 + 签署风险确认书                                │
│           ▼                                                         │
│  ┌─────────────────────────────────────────────────────────────┐    │
│  │  Level 3: AI 自主决策账户 (Autonomous Mode)                   │    │
│  │                                                               │    │
│  │  核心特征: AI 完全自主执行买入卖出，无需人工确认                  │    │
│  │                                                               │    │
│  │  仓位限制: 最大10%总资金                                       │    │
│  │  杠杆限制: 最大5x                                              │    │
│  │  每日最大交易: 20笔                                            │    │
│  │  单笔最大亏损: 总资金1%                                        │    │
│  │  单日最大亏损: 总资金3%                                        │    │
│  │  单周最大亏损: 总资金5%                                        │    │
│  │                                                               │    │
│  │  晋级条件:                                                     │    │
│  │  - Level 2 实盘运行 ≥ 90天                                    │    │
│  │  - 最近100笔交易胜率 ≥ 80%                                    │    │
│  │  - 盈亏比 ≥ 2.5                                               │    │
│  │  - 最大回撤 < 8%                                              │    │
│  │  - 夏普比率 ≥ 1.5                                             │    │
│  │  - 连续30天无风控触发                                          │    │
│  │  - 用户签署《AI自主交易风险确认书》                              │    │
│  │  - 管理员二级审核通过                                          │    │
│  │                                                               │    │
│  │  自主决策范围:                                                 │    │
│  │  ✅ 自主选择做多/做空/观望                                      │    │
│  │  ✅ 自主设置止损/止盈价格                                       │    │
│  │  ✅ 自主调整仓位大小 (在限制内)                                 │    │
│  │  ✅ 自主选择杠杆倍数 (在限制内)                                 │    │
│  │  ✅ 自主决定持仓时长                                           │    │
│  │  ✅ 自主触发多Agent辩论分析                                     │    │
│  │  ✅ 自主调整分析频率                                           │    │
│  │  ❌ 不可突破硬性风控上限                                        │    │
│  │  ❌ 不可修改账户级参数 (出金/划转)                               │    │
│  │  ❌ 不可交易非白名单品种                                        │    │
│  └─────────────────────────────────────────────────────────────┘    │
│                                                                     │
│  ┌─────────────────────────────────────────────────────────────┐    │
│  │  降级机制:                                                    │    │
│  │                                                               │    │
│  │  L3→L2: 自主账户胜率 < 70% (最近50笔) → 降级至实盘受限        │    │
│  │         单日亏损 > 3% → 立即暂停 + 降级                       │    │
│  │         单周亏损 > 5% → 立即暂停 + 降级                       │    │
│  │         连续3次止损触发 → 暂停24h + 重新评估                   │    │
│  │                                                               │    │
│  │  L2→L1: 实盘胜率 < 60% (最近30笔) → 降级至OKX模拟盘           │    │
│  │         单日亏损 > 5% → 立即暂停 + 降级                       │    │
│  │                                                               │    │
│  │  L1→L0: OKX模拟盘胜率 < 50% → 降级至纯模拟                   │    │
│  │                                                               │    │
│  │  紧急熔断: 任何级别单日亏损 > 8% → 全系统暂停 + 人工介入       │    │
│  └─────────────────────────────────────────────────────────────┘    │
└─────────────────────────────────────────────────────────────────────┘
```

**胜率计算规则：**

```
胜率 = 盈利交易数 / 总已平仓交易数

统计窗口:
┌──────────────────────────────────────────────────────────────┐
│  最小样本: 30笔已平仓交易才开始计算胜率                         │
│  统计范围: 最近 50 笔已平仓交易 (滚动窗口)                      │
│  盈利定义: PnL > 0 (扣除手续费 0.05% 后)                       │
│  微利排除: PnL < 0.1% 的交易不计入盈利 (避免噪音)               │
│                                                              │
│  附加指标:                                                   │
│  - 盈亏比: 平均盈利% / 平均亏损% (需 > 2.0)                   │
│  - 最大回撤: 从峰值到谷值的最大跌幅 (需 < 5%)                  │
│  - 夏普比率: (平均收益 - 无风险利率) / 收益标准差               │
│  - 稳定性: 最近10笔交易的胜率方差 (需 < 0.05)                  │
└──────────────────────────────────────────────────────────────┘
```

### 14.7 晋级审核流程

```
AI 触发晋级条件 (自动)
         │
         ▼
┌──────────────────┐
│ 系统自动审核       │  验证: 胜率、盈亏比、回撤、样本量、运行天数
│ (Pre-check)      │  全部通过 → 进入人工审核队列
└────────┬─────────┘
         │
         ▼
┌──────────────────┐
│ 生成审核报告       │  包含:
│ (Audit Report)   │  - 完整交易记录
│                  │  - 胜率趋势图数据
│                  │  - 最大回撤时点
│                  │  - 各市场环境下的表现
│                  │  - 风险事件列表
└────────┬─────────┘
         │
         ▼
┌──────────────────┐
│ 人工审核          │  管理员在后台审核:
│ (Human Review)   │  - 查看审核报告
│                  │  - 确认或拒绝晋级
│                  │  - 可设置额外限制条件
└────────┬─────────┘
         │
         ▼
┌──────────────────┐
│ 晋级执行          │  - 更新执行模式
│ (Promotion)      │  - 设置新的仓位/杠杆限制
│                  │  - 通知用户
│                  │  - 进入7天观察期 (观察期内限制更严)
└──────────────────┘
```

### 14.8 新增数据库表

```sql
-- AI 模拟操盘配置表
CREATE TABLE ai_simulation_configs (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    user_id BIGINT REFERENCES users(id),
    symbol VARCHAR(32) NOT NULL DEFAULT 'DOGE-USDT-SWAP',
    mode VARCHAR(16) NOT NULL DEFAULT 'paper',
    level INTEGER DEFAULT 0,
    status VARCHAR(32) DEFAULT 'stopped',
    
    initial_balance FLOAT DEFAULT 100000.0,
    current_balance FLOAT DEFAULT 100000.0,
    max_position_size_percent FLOAT DEFAULT 2.0,
    max_leverage INTEGER DEFAULT 2,
    max_daily_trades INTEGER DEFAULT 3,
    max_daily_loss_percent FLOAT DEFAULT 2.0,
    max_weekly_loss_percent FLOAT DEFAULT 5.0,
    max_single_trade_loss_percent FLOAT DEFAULT 1.0,
    ai_confidence_threshold FLOAT DEFAULT 0.7,
    analysis_interval_minutes INTEGER DEFAULT 30,
    max_holding_period_hours INTEGER DEFAULT 24,
    allowed_symbols TEXT[] DEFAULT '{"DOGE-USDT-SWAP"}',
    requires_manual_confirm BOOLEAN DEFAULT true,
    autonomous_mode_enabled BOOLEAN DEFAULT false,
    
    total_trades INTEGER DEFAULT 0,
    winning_trades INTEGER DEFAULT 0,
    losing_trades INTEGER DEFAULT 0,
    win_rate FLOAT DEFAULT 0.0,
    avg_pnl_percent FLOAT DEFAULT 0.0,
    profit_loss_ratio FLOAT DEFAULT 0.0,
    max_drawdown_percent FLOAT DEFAULT 0.0,
    sharpe_ratio FLOAT DEFAULT 0.0,
    
    weekly_pnl FLOAT DEFAULT 0.0,
    weekly_loss_percent FLOAT DEFAULT 0.0,
    daily_pnl FLOAT DEFAULT 0.0,
    daily_loss_percent FLOAT DEFAULT 0.0,
    consecutive_stop_losses INTEGER DEFAULT 0,
    
    running_days INTEGER DEFAULT 0,
    last_trade_at TIMESTAMPTZ,
    promotion_eligible BOOLEAN DEFAULT false,
    risk_confirmation_signed BOOLEAN DEFAULT false,
    risk_confirmation_signed_at TIMESTAMPTZ,
    
    created_at TIMESTAMPTZ DEFAULT NOW(),
    updated_at TIMESTAMPTZ DEFAULT NOW()
);

-- AI 模拟交易记录表 (区分 paper/demo/live)
CREATE TABLE ai_simulation_trades (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    config_id UUID REFERENCES ai_simulation_configs(id),
    symbol VARCHAR(32) NOT NULL,
    mode VARCHAR(16) NOT NULL,
    
    direction VARCHAR(8) NOT NULL,
    entry_price FLOAT NOT NULL,
    exit_price FLOAT,
    quantity FLOAT NOT NULL,
    leverage INTEGER DEFAULT 1,
    stop_loss FLOAT,
    take_profit FLOAT,
    
    ai_confidence FLOAT,
    ai_reasoning JSONB,
    agent_session_id UUID,
    
    pnl FLOAT,
    pnl_percent FLOAT,
    fee_percent FLOAT DEFAULT 0.05,
    net_pnl_percent FLOAT,
    
    status VARCHAR(16) DEFAULT 'open',
    close_reason VARCHAR(32),
    holding_duration_minutes INTEGER,
    
    opened_at TIMESTAMPTZ DEFAULT NOW(),
    closed_at TIMESTAMPTZ
);

CREATE INDEX idx_sim_trades_config ON ai_simulation_trades(config_id);
CREATE INDEX idx_sim_trades_status ON ai_simulation_trades(status);
CREATE INDEX idx_sim_trades_mode ON ai_simulation_trades(mode);

-- 晋级审核记录表
CREATE TABLE promotion_audits (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    config_id UUID REFERENCES ai_simulation_configs(id),
    from_level INTEGER NOT NULL,
    to_level INTEGER NOT NULL,
    from_mode VARCHAR(16) NOT NULL,
    to_mode VARCHAR(16) NOT NULL,
    
    stats_snapshot JSONB NOT NULL,
    audit_report JSONB,
    
    status VARCHAR(32) DEFAULT 'pending',
    reviewed_by VARCHAR(64),
    review_comment TEXT,
    reviewed_at TIMESTAMPTZ,
    
    observation_period_days INTEGER DEFAULT 7,
    observation_started_at TIMESTAMPTZ,
    observation_completed_at TIMESTAMPTZ,
    
    created_at TIMESTAMPTZ DEFAULT NOW()
);

-- 降级记录表
CREATE TABLE demotion_records (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    config_id UUID REFERENCES ai_simulation_configs(id),
    from_level INTEGER NOT NULL,
    to_level INTEGER NOT NULL,
    from_mode VARCHAR(16) NOT NULL,
    to_mode VARCHAR(16) NOT NULL,
    
    trigger_reason TEXT NOT NULL,
    stats_snapshot JSONB NOT NULL,
    
    cooldown_until TIMESTAMPTZ,
    created_at TIMESTAMPTZ DEFAULT NOW()
);

-- 每日统计快照表
CREATE TABLE daily_simulation_stats (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    config_id UUID REFERENCES ai_simulation_configs(id),
    date DATE NOT NULL,
    mode VARCHAR(16) NOT NULL,
    
    trades_count INTEGER DEFAULT 0,
    wins INTEGER DEFAULT 0,
    losses INTEGER DEFAULT 0,
    daily_pnl FLOAT DEFAULT 0.0,
    daily_pnl_percent FLOAT DEFAULT 0.0,
    max_drawdown_percent FLOAT DEFAULT 0.0,
    balance_at_close FLOAT,
    
    rolling_win_rate_50 FLOAT,
    rolling_profit_loss_ratio FLOAT,
    
    created_at TIMESTAMPTZ DEFAULT NOW(),
    
    UNIQUE(config_id, date, mode)
);
```

### 14.9 API 设计

| 方法 | 路径 | 说明 |
|------|------|------|
| POST | `/api/v1/agent/simulation/start` | 启动AI模拟操盘 |
| POST | `/api/v1/agent/simulation/stop` | 停止模拟操盘 |
| GET | `/api/v1/agent/simulation/status` | 获取模拟状态 |
| GET | `/api/v1/agent/simulation/trades` | 获取模拟交易记录 |
| GET | `/api/v1/agent/simulation/stats` | 获取胜率统计 |
| GET | `/api/v1/agent/simulation/stats/daily` | 获取每日统计 |
| GET | `/api/v1/agent/simulation/level` | 获取当前等级与晋级进度 |
| GET | `/api/v1/agent/simulation/promotion-status` | 获取晋级审核状态 |
| POST | `/api/v1/agent/simulation/reset` | 重置模拟账户 (仅Paper模式) |
| GET | `/api/v1/agent/simulation/audit-report` | 生成审核报告 |
| POST | `/api/v1/agent/simulation/promotion/approve` | 管理员批准晋级 |
| POST | `/api/v1/agent/simulation/promotion/reject` | 管理员拒绝晋级 |
| POST | `/api/v1/agent/simulation/risk-confirmation/sign` | 签署AI自主交易风险确认书 (L3必需) |
| GET | `/api/v1/agent/simulation/autonomous/status` | 获取AI自主决策状态 (L3) |
| POST | `/api/v1/agent/simulation/autonomous/pause` | 暂停AI自主交易 |
| POST | `/api/v1/agent/simulation/autonomous/resume` | 恢复AI自主交易 |
| GET | `/api/v1/agent/simulation/autonomous/decision-log` | 获取AI自主决策日志 |
| POST | `/api/v1/agent/simulation/emergency-stop` | 紧急停止所有交易 (任何级别) |

### 14.10 自动止损止盈检查器

现有 `paper_trading.rs` 的最大缺陷是没有自动止损止盈触发。新增定时检查器：

```rust
pub struct StopLossTakeProfitChecker {
    db_pool: PgPool,
    market_collector: Arc<MarketCollector>,
    check_interval: Duration,
}

impl StopLossTakeProfitChecker {
    pub async fn start(self: Arc<Self>) {
        let mut interval = tokio::time::interval(self.check_interval);
        loop {
            interval.tick().await;
            if let Err(e) = self.check_all_positions().await {
                tracing::error!("SL/TP check failed: {}", e);
            }
        }
    }
    
    async fn check_all_positions(&self) -> Result<()> {
        let open_positions = sqlx::query_as::<_, OpenPosition>(
            "SELECT * FROM ai_simulation_trades WHERE status = 'open'"
        )
        .fetch_all(&self.db_pool)
        .await?;
        
        for pos in &open_positions {
            let current_price = self.market_collector
                .get_latest_ticker(&pos.symbol)
                .await
                .map(|t| t.last)
                .unwrap_or(0.0);
            
            if current_price <= 0.0 { continue; }
            
            let triggered = self.check_trigger(pos, current_price);
            if let Some(reason) = triggered {
                self.close_position(pos, current_price, &reason).await?;
            }
        }
        Ok(())
    }
    
    fn check_trigger(&self, pos: &OpenPosition, price: f64) -> Option<String> {
        match pos.direction.as_str() {
            "long" => {
                if let Some(sl) = pos.stop_loss {
                    if price <= sl { return Some("stop_loss".to_string()); }
                }
                if let Some(tp) = pos.take_profit {
                    if price >= tp { return Some("take_profit".to_string()); }
                }
            }
            "short" => {
                if let Some(sl) = pos.stop_loss {
                    if price >= sl { return Some("stop_loss".to_string()); }
                }
                if let Some(tp) = pos.take_profit {
                    if price <= tp { return Some("take_profit".to_string()); }
                }
            }
            _ => {}
        }
        
        if let Some(opened) = pos.opened_at {
            let max_hours = 24;
            if Utc::now() - opened > Duration::hours(max_hours) {
                return Some("timeout".to_string());
            }
        }
        
        None
    }
}
```

### 14.11 胜率统计与晋级检查器

```rust
pub struct PromotionChecker {
    db_pool: PgPool,
}

impl PromotionChecker {
    pub async fn check_promotion_eligibility(&self, config_id: &Uuid) -> Result<PromotionEligibility> {
        let config = self.get_config(config_id).await?;
        let stats = self.calculate_rolling_stats(config_id).await?;
        
        let eligible = match config.level {
            0 => {
                stats.total_trades >= 30 &&
                stats.win_rate >= 0.80 &&
                stats.running_days >= 14 &&
                stats.max_drawdown_percent < 5.0
            }
            1 => {
                stats.total_trades >= 30 &&
                stats.win_rate >= 0.90 &&
                stats.running_days >= 7 &&
                stats.max_drawdown_percent < 3.0 &&
                stats.profit_loss_ratio > 2.0
            }
            2 => {
                stats.total_trades >= 100 &&
                stats.win_rate >= 0.80 &&
                stats.running_days >= 90 &&
                stats.max_drawdown_percent < 8.0 &&
                stats.profit_loss_ratio > 2.5 &&
                stats.sharpe_ratio >= 1.5 &&
                stats.consecutive_days_without_risk_trigger >= 30 &&
                config.risk_confirmation_signed
            }
            _ => false,
        };
        
        let next_level = match config.level {
            0 => Some(1),
            1 => Some(2),
            2 => Some(3),
            _ => None,
        };
        
        Ok(PromotionEligibility {
            eligible,
            current_level: config.level,
            next_level,
            stats,
            requirements_met: eligible,
            missing_requirements: if !eligible { self.get_missing_requirements(&config, &stats) } else { vec![] },
        })
    }
    
    pub async fn check_demotion(&self, config_id: &Uuid) -> Result<Option<DemotionTrigger>> {
        let config = self.get_config(config_id).await?;
        let stats = self.calculate_rolling_stats(config_id).await?;
        
        // Level 3 → Level 2: AI自主账户降级
        if config.level == 3 {
            if stats.win_rate < 0.70 {
                return Ok(Some(DemotionTrigger {
                    from_level: 3,
                    to_level: 2,
                    reason: format!("AI自主账户胜率降至 {:.1}%，低于70%阈值", stats.win_rate * 100.0),
                }));
            }
            if stats.daily_loss_percent > 3.0 {
                return Ok(Some(DemotionTrigger {
                    from_level: 3,
                    to_level: 2,
                    reason: format!("单日亏损 {:.1}%，超过3%阈值", stats.daily_loss_percent),
                }));
            }
            if stats.weekly_loss_percent > 5.0 {
                return Ok(Some(DemotionTrigger {
                    from_level: 3,
                    to_level: 2,
                    reason: format!("单周亏损 {:.1}%，超过5%阈值", stats.weekly_loss_percent),
                }));
            }
            if stats.consecutive_stop_losses >= 3 {
                return Ok(Some(DemotionTrigger {
                    from_level: 3,
                    to_level: 2,
                    reason: format!("连续{}次止损触发", stats.consecutive_stop_losses),
                }));
            }
        }
        
        // Level 2 → Level 1: 实盘降级
        if config.level >= 2 && stats.win_rate < 0.60 {
            return Ok(Some(DemotionTrigger {
                from_level: config.level,
                to_level: 1,
                reason: format!("实盘胜率降至 {:.1}%，低于60%阈值", stats.win_rate * 100.0),
            }));
        }
        
        // 全局: 单日亏损 > 5% 降级
        if config.level >= 1 && stats.daily_loss_percent > 5.0 {
            return Ok(Some(DemotionTrigger {
                from_level: config.level,
                to_level: config.level.saturating_sub(1),
                reason: format!("单日亏损 {:.1}%，超过5%阈值", stats.daily_loss_percent),
            }));
        }
        
        // 紧急熔断: 任何级别单日亏损 > 8%
        if stats.daily_loss_percent > 8.0 {
            return Ok(Some(DemotionTrigger {
                from_level: config.level,
                to_level: 0,
                reason: format!("⚠️ 紧急熔断: 单日亏损 {:.1}%，超过8%紧急阈值，全系统暂停", stats.daily_loss_percent),
            }));
        }
        
        Ok(None)
    }
}

pub struct RollingStats {
    pub total_trades: i32,
    pub winning_trades: i32,
    pub losing_trades: i32,
    pub win_rate: f64,
    pub avg_pnl_percent: f64,
    pub profit_loss_ratio: f64,
    pub max_drawdown_percent: f64,
    pub running_days: i32,
    pub daily_loss_percent: f64,
}
```

### 14.12 前端展示

**模拟操盘仪表盘：**

| 区域 | 内容 |
|------|------|
| **顶部状态栏** | 当前模式 (Paper/Demo/Live)、等级 (0/1/2)、运行状态 |
| **晋级进度条** | 当前胜率 vs 目标胜率、已完成交易数 vs 最低要求、运行天数 |
| **账户概览** | 初始资金、当前余额、总PnL、胜率、盈亏比 |
| **持仓列表** | 当前持仓、方向、入场价、止损/止盈、未实现PnL、实时价格 |
| **交易历史** | 已平仓交易列表、PnL分布图、胜率趋势图 |
| **AI决策日志** | 每笔交易的AI决策理由、置信度、辩论摘要 |
| **风控面板** | 每日交易次数/亏损、最大回撤、降级预警 |
| **自主决策面板** (L3) | AI自主决策实时状态、决策日志流、风控阈值监控、紧急停止按钮 |

### 14.14 Level 3 AI 自主决策引擎

Level 3 是系统的最高权限级别，AI 拥有完全自主的买卖决策权，无需人工逐笔确认。

```
┌─────────────────────────────────────────────────────────────────────┐
│           Level 3 AI 自主决策引擎 (Autonomous Engine)                │
│                                                                     │
│  ┌─────────────────────────────────────────────────────────────┐    │
│  │                   决策自主层                                  │    │
│  │                                                               │    │
│  │  ┌───────────────┐  ┌───────────────┐  ┌───────────────┐   │    │
│  │  │ 市场监控器     │  │ 机会扫描器     │  │ 风险监控器     │   │    │
│  │  │ (MarketWatch) │  │(OpportunityScan)│ │(RiskWatchdog) │   │    │
│  │  │               │  │               │  │               │   │    │
│  │  │ 实时价格监控   │  │ 信号强度评估   │  │ 持仓风险计算   │   │    │
│  │  │ 异常波动检测   │  │ 入场时机判断   │  │ 亏损限额检查   │   │    │
│  │  │ 趋势变化识别   │  │ 多信号聚合     │  │ 相关性风险     │   │    │
│  │  └───────┬───────┘  └───────┬───────┘  └───────┬───────┘   │    │
│  │          │                  │                  │             │    │
│  │          └──────────────────┼──────────────────┘             │    │
│  │                             │                                 │    │
│  │                             ▼                                 │    │
│  │  ┌───────────────────────────────────────────────────────┐   │    │
│  │  │              AI 自主决策核心                            │   │    │
│  │  │                                                        │   │    │
│  │  │  触发条件 (满足任一):                                    │   │    │
│  │  │  - 机会扫描器发现高置信度信号 (≥0.8)                     │   │    │
│  │  │  - 风险监控器发出减仓/平仓警告                           │   │    │
│  │  │  - 定时分析周期到达 (每30分钟)                           │   │    │
│  │  │  - 市场出现异常波动 (>5% 瞬时变动)                      │   │    │
│  │  │                                                        │   │    │
│  │  │  决策流程:                                              │   │    │
│  │  │  1. 自动触发多Agent辩论 (无需人工)                      │   │    │
│  │  │  2. 基金经理输出决策                                    │   │    │
│  │  │  3. 风控引擎二次校验                                    │   │    │
│  │  │  4. 直接执行 (无需确认)                                 │   │    │
│  │  │  5. 写入决策日志 + 通知用户                             │   │    │
│  │  └───────────────────────────────────────────────────────┘   │    │
│  └─────────────────────────────────────────────────────────────┘    │
│                                                                     │
│  ┌─────────────────────────────────────────────────────────────┐    │
│  │                   风控护栏层 (不可逾越)                        │    │
│  │                                                               │    │
│  │  ┌─────────────┐  ┌─────────────┐  ┌─────────────┐         │    │
│  │  │ 仓位护栏     │  │ 亏损护栏     │  │ 频率护栏     │         │    │
│  │  │             │  │             │  │             │         │    │
│  │  │ 单笔≤10%   │  │ 单笔≤1%    │  │ 每日≤20笔  │         │    │
│  │  │ 总持仓≤30% │  │ 每日≤3%    │  │ 每小时≤5笔 │         │    │
│  │  │ 杠杆≤5x    │  │ 每周≤5%    │  │ 最少间隔5min│         │    │
│  │  └─────────────┘  └─────────────┘  └─────────────┘         │    │
│  │                                                               │    │
│  │  ┌─────────────┐  ┌─────────────┐  ┌─────────────┐         │    │
│  │  │ 品种护栏     │  │ 时间护栏     │  │ 熔断护栏     │         │    │
│  │  │             │  │             │  │             │         │    │
│  │  │ 白名单品种  │  │ 非交易时段  │  │ 8%日亏熔断  │         │    │
│  │  │ 仅DOGE     │  │  禁止开仓   │  │ 3次SL暂停  │         │    │
│  │  │ 可扩展     │  │ 最长持仓24h │  │ 异常暂停    │         │    │
│  │  └─────────────┘  └─────────────┘  └─────────────┘         │    │
│  └─────────────────────────────────────────────────────────────┘    │
│                                                                     │
│  ┌─────────────────────────────────────────────────────────────┐    │
│  │                   通知与审计层                                 │    │
│  │                                                               │    │
│  │  - 每笔交易实时推送通知 (WebSocket + 可选短信/邮件)           │    │
│  │  - 每日交易汇总报告                                          │    │
│  │  - 风控触发即时告警                                          │    │
│  │  - 降级/熔断紧急通知                                         │    │
│  │  - 全量决策日志可审计                                        │    │
│  └─────────────────────────────────────────────────────────────┘    │
└─────────────────────────────────────────────────────────────────────┘
```

**Level 3 自主决策引擎核心代码：**

```rust
pub struct AutonomousEngine {
    simulation_engine: Arc<SimulationEngine>,
    market_watch: Arc<MarketWatcher>,
    risk_watchdog: Arc<RiskWatchdog>,
    opportunity_scanner: Arc<OpportunityScanner>,
    notification: Arc<NotificationService>,
    config: Arc<RwLock<AutonomousConfig>>,
    status: Arc<RwLock<AutonomousStatus>>,
}

pub struct AutonomousConfig {
    enabled: bool,
    max_single_trade_loss_percent: f64,
    max_daily_loss_percent: f64,
    max_weekly_loss_percent: f64,
    max_position_size_percent: f64,
    max_total_position_percent: f64,
    max_leverage: i32,
    max_daily_trades: i32,
    max_hourly_trades: i32,
    min_trade_interval_seconds: u64,
    max_holding_period_hours: u32,
    allowed_symbols: Vec<String>,
    analysis_interval_minutes: u32,
    high_confidence_threshold: f64,
    emergency_stop: bool,
}

pub struct AutonomousStatus {
    running: bool,
    paused: bool,
    last_trade_at: Option<DateTime<Utc>>,
    daily_trade_count: i32,
    hourly_trade_count: i32,
    daily_pnl: f64,
    weekly_pnl: f64,
    consecutive_stop_losses: i32,
    last_decision_summary: Option<String>,
}

impl AutonomousEngine {
    pub async fn run_loop(self: Arc<Self>) {
        let mut analysis_interval = tokio::time::interval(
            Duration::minutes(self.config.read().await.analysis_interval_minutes as i64)
        );
        let mut risk_check_interval = tokio::time::interval(Duration::seconds(10));
        let mut market_watch_interval = tokio::time::interval(Duration::seconds(30));
        
        loop {
            tokio::select! {
                _ = analysis_interval.tick() => {
                    if self.should_analyze().await {
                        if let Err(e) = self.autonomous_analysis_cycle().await {
                            tracing::error!("Autonomous analysis failed: {}", e);
                        }
                    }
                }
                _ = risk_check_interval.tick() => {
                    if let Err(e) = self.risk_check_cycle().await {
                        tracing::error!("Risk check failed: {}", e);
                    }
                }
                _ = market_watch_interval.tick() => {
                    if let Some(alert) = self.market_watch.check_anomaly().await {
                        tracing::warn!("Market anomaly detected: {:?}", alert);
                        if alert.severity >= Severity::High {
                            self.autonomous_analysis_cycle().await.ok();
                        }
                    }
                }
            }
        }
    }
    
    async fn autonomous_analysis_cycle(&self) -> Result<()> {
        let config = self.config.read().await;
        let status = self.status.read().await;
        
        if !config.enabled || !status.running || status.paused || config.emergency_stop {
            return Ok(());
        }
        
        if !self.pass_frequency_check(&status, &config) {
            return Ok(());
        }
        
        let decision = self.simulation_engine
            .agent_engine()
            .run_session("DOGE-USDT-SWAP", &Default::default())
            .await?;
        
        if !self.pass_autonomous_risk_check(&decision, &config, &status) {
            self.notification.send(
                NotificationLevel::Info,
                &format!("AI决策被风控拦截: {} (置信度:{:.0}%)", 
                    decision.action, decision.confidence * 100.0)
            ).await;
            return Ok(());
        }
        
        self.simulation_engine.execute_decision(&decision, &Default::default()).await?;
        
        self.record_and_notify(&decision).await?;
        
        Ok(())
    }
    
    fn pass_autonomous_risk_check(
        &self,
        decision: &FundManagerDecision,
        config: &AutonomousConfig,
        status: &AutonomousStatus,
    ) -> bool {
        if decision.confidence < config.high_confidence_threshold {
            return false;
        }
        if decision.position_size_percent > config.max_position_size_percent {
            return false;
        }
        if decision.leverage > config.max_leverage {
            return false;
        }
        if status.daily_trade_count >= config.max_daily_trades {
            return false;
        }
        if status.hourly_trade_count >= config.max_hourly_trades {
            return false;
        }
        if status.daily_pnl < -(config.max_daily_loss_percent / 100.0 * self.get_total_balance()) {
            return false;
        }
        if status.weekly_pnl < -(config.max_weekly_loss_percent / 100.0 * self.get_total_balance()) {
            return false;
        }
        if !config.allowed_symbols.contains(&decision.symbol) {
            return false;
        }
        if let Some(last) = status.last_trade_at {
            if Utc::now() - last < Duration::seconds(config.min_trade_interval_seconds as i64) {
                return false;
            }
        }
        true
    }
    
    async fn risk_check_cycle(&self) -> Result<()> {
        let mut status = self.status.write().await;
        let config = self.config.read().await;
        
        if status.daily_pnl < -(config.max_daily_loss_percent / 100.0 * self.get_total_balance()) {
            tracing::error!("Daily loss limit breached: {:.2}%", config.max_daily_loss_percent);
            status.paused = true;
            drop(status);
            self.notification.send(
                NotificationLevel::Emergency,
                &format!("⚠️ AI自主交易已暂停: 单日亏损超过{}%阈值", config.max_daily_loss_percent)
            ).await;
        }
        
        if status.weekly_pnl < -(config.max_weekly_loss_percent / 100.0 * self.get_total_balance()) {
            tracing::error!("Weekly loss limit breached: {:.2}%", config.max_weekly_loss_percent);
            status.paused = true;
            drop(status);
            self.notification.send(
                NotificationLevel::Emergency,
                &format!("⚠️ AI自主交易已暂停: 单周亏损超过{}%阈值", config.max_weekly_loss_percent)
            ).await;
        }
        
        Ok(())
    }
    
    pub async fn emergency_stop(&self) {
        let mut config = self.config.write().await;
        config.emergency_stop = true;
        config.enabled = false;
        drop(config);
        
        let mut status = self.status.write().await;
        status.running = false;
        status.paused = true;
        
        self.notification.send(
            NotificationLevel::Emergency,
            "🚨 紧急停止已触发: 所有AI交易已立即停止，需人工介入恢复"
        ).await;
    }
}
```

### 14.15 Level 3 晋级审核流程 (二级审核)

Level 3 的晋级审核比 Level 2 更严格，需要**二级审核**：

```
Level 2 实盘运行 ≥ 90天 + 胜率 ≥ 80%
         │
         ▼
┌──────────────────┐
│ Step 1: 系统预审  │  自动验证所有硬性指标:
│ (Auto Pre-check) │  - 胜率、盈亏比、回撤、夏普比率
│                  │  - 运行天数、风控触发次数
│                  │  - 全部通过 → 进入 Step 2
└────────┬─────────┘
         │
         ▼
┌──────────────────┐
│ Step 2: 风险确认  │  用户必须签署:
│ (Risk Confirm)   │  《AI自主交易风险确认书》
│                  │  - 确认理解AI可能造成真实资金损失
│                  │  - 确认理解AI决策不可撤销
│                  │  - 确认理解降级/熔断机制
│                  │  - 设定最大可承受亏损金额
└────────┬─────────┘
         │
         ▼
┌──────────────────┐
│ Step 3: 一级审核  │  普通管理员审核:
│ (Admin Review)   │  - 审查交易记录和统计
│                  │  - 审查风险确认书
│                  │  - 通过 → 进入 Step 4
└────────┬─────────┘
         │
         ▼
┌──────────────────┐
│ Step 4: 二级审核  │  高级管理员/风控负责人审核:
│ (Senior Review)  │  - 最终风险评估
│                  │  - 可设置额外限制条件
│                  │  - 通过 → 进入 Step 5
└────────┬─────────┘
         │
         ▼
┌──────────────────┐
│ Step 5: 观察期    │  14天严格观察期:
│ (Observation)    │  - 仓位限制减半 (5%→2.5%)
│                  │  - 每日最大交易减半 (20→10笔)
│                  │  - 每日人工审查交易记录
│                  │  - 观察期通过 → 正式启用 Level 3
│                  │  - 观察期未通过 → 退回 Level 2
└──────────────────┘
```

### 14.16 Level 3 各等级参数对照表

| 参数 | Level 0 (Paper) | Level 1 (Demo) | Level 2 (Live) | Level 3 (Autonomous) |
|------|-----------------|----------------|----------------|---------------------|
| 执行方式 | 内部数据库 | OKX模拟盘 | OKX实盘 | OKX实盘 |
| 人工确认 | 不需要 | 不需要 | 初期需要，后期取消 | 不需要 |
| 最大仓位 | 10% | 2% | 2%→5%→8% | 10% |
| 最大杠杆 | 5x | 3x | 2x→3x→5x | 5x |
| 每日最大交易 | 无限 | 5笔 | 3→5→10笔 | 20笔 |
| 单笔最大亏损 | 无限 | 2% | 2% | 1% |
| 单日最大亏损 | 无限 | 3% | 5% | 3% |
| 单周最大亏损 | 无限 | 5% | 无限制 | 5% |
| 品种限制 | 无限 | 白名单 | 白名单 | 白名单 |
| 晋级胜率要求 | - | 80% | 90% | 80% (100笔) |
| 晋级运行天数 | - | 14天 | 7天 | 90天 |
| 降级胜率阈值 | - | <50% | <60% | <70% |
| 审核方式 | 自动 | 自动 | 人工审核 | 二级审核+风险确认书 |

### 14.13 与现有代码的集成点

| 现有代码 | 集成方式 |
|---------|---------|
| `routes/paper_trading.rs` | 保留现有接口，新增 `ai_simulation_trades` 表作为AI专用交易记录，与手动模拟交易分离 |
| `routes/auto_trading.rs` | 复用 `auto_trading_configs` 表的 mode 字段，扩展支持 paper/demo/live 三级 |
| `exchanges/okx.rs` | 复用 `is_demo` 标志，Demo模式设置 `is_demo=true`，Live模式设置 `is_demo=false` |
| `collector.rs` | 复用 MarketCollector 的实时 Ticker 数据作为模拟价格源 |
| `websocket.rs` | 复用 WebSocket 推送模拟交易状态更新 |
| Agent 辩论引擎 | 基金经理决策输出直接对接 SimulationEngine |

---

## 15. 与现有系统的兼容性

本方案完全兼容现有 MoneyRobert Pro 架构：

| 维度 | 兼容方式 |
|------|---------|
| **数据库** | 新增表，不修改现有表结构 |
| **API** | 新增 `/api/v1/agent/*` 路由组，不影响现有路由 |
| **数据源** | 复用现有 `klines`、`ticker_history`、`funding_rate_history`、`sentiment_data`、`news_items` 等表 |
| **WebSocket** | 复用现有 `WebSocketManager`，新增 `agent_update` 消息类型 |
| **认证** | 复用现有 JWT 认证体系 |
| **前端** | 新增 Agent 分析页，在 DashboardLayout 中添加导航 |
| **部署** | 无需新增基础设施，LLM 调用通过 HTTP 出站 |

# MoneyRobert-Pro 系统 2.0 待补全问题清单与更新方案

> 目标：完整覆盖前期从经济学家、基金经理、数学分析师、系统架构师、产品设计经理角度形成的审查结果，并把剩余缺口拆解成 AI 可执行的开发任务，降低后续实现时的理解偏差和代码幻觉。

## 1. 当前结论摘要

当前项目已经补入了较多系统 2.0 骨架，包括：

- 市场状态识别 Regime；
- 特征工程 Feature Store；
- 信号概率校准 Calibration；
- EV / CVaR / VaR / Alpha / Beta 等量化指标；
- Kelly、波动率目标、风险预算等仓位模型；
- 微观结构数据、跨交易所数据、模型卡、反事实解释；
- 回测绩效指标增强；
- Agents 集成测试入口已接入。

但系统还没有完全达到“可信交易决策系统”的要求。最关键的缺口集中在：

1. 交易账本和权益口径仍不统一；
2. 回测结尾强平没有走统一撮合链路；
3. 交易归因中的 realized_pnl 仍可能失真；
4. EV / CVaR / Kelly / Portfolio Risk 等模型尚未完全接入主交易流程；
5. Reports 接口与数据库 Schema 仍有潜在类型不一致；
6. 产品层缺少“为什么可信、何时不可信、建议如何使用”的解释闭环。

建议优先级：

| 优先级 | 模块 | 处理目标 |
|---|---|---|
| P0 | backtest/matching/account/runner | 统一资金权益口径，修复账本可信度 |
| P0 | trade attribution | 确保每笔交易 PnL、费用、滑点、归因准确 |
| P1 | signal → sizing → risk → order 主链路 | 把数学期望和基金经理资金模型接入交易决策 |
| P1 | reports API/schema | 修复运行时类型风险和用户隔离完整性 |
| P1 | system orchestration | 建立系统 2.0 服务编排层 |
| P2 | frontend/product trust UX | 建立可信解释、风险提示、模型卡、归因展示 |
| P2 | observability/testing | 建立端到端、资金守恒、回归测试体系 |

---

## 2. P0：交易账本与资金模型必须统一

### 2.1 当前问题

当前 `AccountState::recompute_total_equity` 已经调整为：

```text
total_equity = cash + unrealized_pnl
```

这代表系统倾向于使用“合约/永续账户口径”：cash 已包含已实现盈亏，realized_pnl 只用于归因，不应重复计入权益。

但撮合引擎仍存在旧口径：

- 开仓时从 cash 扣完整 notional + fee；
- 平仓时返还 margin_return + pnl - fee；
- 反手时继续对剩余数量扣完整 new_notional；
- 强平时仍可能加回 price * quantity + trade_pnl。

这会导致：

- 合约账户权益被低估；
- 现货账户又没有持仓市值进入 total_equity；
- 回测收益率、最大回撤、Sharpe、胜率等指标不可信；
- 后续 AI 根据错误收益数据训练/校准，会产生错误决策。

### 2.2 更新目标

系统必须明确账户模式：

#### 推荐方案：优先实现合约/永续保证金模型

因为项目当前交易所方向偏 OKX、SWAP、杠杆、资金费率、爆仓价、保证金，建议先统一成：

```text
total_equity = cash + unrealized_pnl
available_cash = cash - margin_used
cash 只反映余额、手续费、已实现盈亏、资金费率
margin_used 独立记录保证金占用
notional 不直接从 cash 扣除
```

### 2.3 详细实现方案

#### 2.3.1 修改 AccountState 语义

保持：

```rust
total_equity = cash + unrealized_pnl
```

但补充字段语义约束：

| 字段 | 语义 |
|---|---|
| cash | 账户余额，包含已实现盈亏，扣除手续费和资金费 |
| margin_used | 当前持仓保证金占用 |
| unrealized_pnl | 当前全部未平仓浮动盈亏 |
| realized_pnl | 统计归因字段，不直接进入权益 |
| total_equity | cash + unrealized_pnl |
| total_notional | 当前未平仓名义价值 |
| leverage | total_notional / total_equity |

#### 2.3.2 修改开仓逻辑

当前错误模式：

```text
cash -= notional + fee
```

应改为：

```text
cash -= fee
realized_pnl -= fee
margin_used += notional / leverage
total_notional += notional
```

新仓位字段：

```text
quantity = fill_qty
avg_entry_price = fill_price
notional = fill_qty * fill_price
margin = notional / leverage
unrealized_pnl = 0
realized_pnl = -fee
```

#### 2.3.3 修改加仓逻辑

同方向加仓：

```text
new_quantity = old_quantity + fill_quantity
new_avg_entry_price = weighted_average(old_avg, old_qty, fill_price, fill_qty)
cash -= fee
realized_pnl -= fee
position.realized_pnl -= fee
margin_used += added_notional / leverage
```

不能扣完整 notional。

#### 2.3.4 修改减仓/平仓逻辑

平多：

```text
gross_pnl = (exit_price - avg_entry_price) * close_qty
net_pnl = gross_pnl - close_fee
cash += net_pnl
realized_pnl += net_pnl
position.realized_pnl += net_pnl
margin_used -= released_margin
```

平空：

```text
gross_pnl = (avg_entry_price - exit_price) * close_qty
net_pnl = gross_pnl - close_fee
cash += net_pnl
realized_pnl += net_pnl
position.realized_pnl += net_pnl
margin_used -= released_margin
```

#### 2.3.5 修改反手逻辑

例如当前持有 long 1 BTC，收到 sell 3 BTC：

```text
close_qty = min(existing_qty, fill_qty) = 1
remaining_qty = fill_qty - close_qty = 2
```

流程必须是：

1. 先平 long 1；
2. 计算 close_fee；
3. 写入原 position.realized_pnl；
4. 原 position.quantity 归零，closed_at 写入；
5. 剩余 2 BTC 按 short 新开仓；
6. open_fee 计入新仓；
7. 不允许出现负 quantity。

手续费应按数量拆分：

```text
close_fee = total_fee * close_qty / fill_qty
open_fee = total_fee - close_fee
```

#### 2.3.6 修改强平/回测结束平仓

禁止 runner 直接调用简单 `force_close_all` 改 cash。

应统一走：

```text
open_position
→ matching.close_position_at_price
→ matching.apply_fill
→ runner.apply_fill_to_account
→ closed_trades
→ performance_report
```

这样才能保证：

- 结尾平仓产生 fill；
- 结尾平仓产生 closed_trades；
- 手续费、滑点、PnL 归因一致；
- 回测交易数量、胜率、Profit Factor 正确。

### 2.4 验收标准

必须新增/修改测试：

1. 开多后：

```text
cash = initial_cash - fee
total_equity = initial_cash - fee
margin_used = notional / leverage
```

2. 平多盈利后：

```text
cash = initial_cash + gross_pnl - open_fee - close_fee
position.closed_at != None
closed_trades.len() == 1
trade.pnl == gross_pnl - close_fee 或按完整生命周期净值计算
```

3. 反手后：

```text
原 position quantity = 0
原 position closed_at != None
新 position side = short
新 position quantity = remaining_qty
不存在负 quantity
```

4. 回测结束自动平仓：

```text
closed_trades 包含结束平仓交易
performance.total_trades 正确
total_fee 包含结束平仓费用
total_slippage_cost 包含结束平仓滑点
```

---

## 3. P0：交易归因 Trade Attribution 必须可信

### 3.1 当前问题

runner 在生成 closed_trades 时读取 position.realized_pnl。

但撮合引擎平仓时可能只更新 account.realized_pnl，没有完整更新 position.realized_pnl。

结果：

- closed_trades.pnl 可能为 0；
- by_agent、by_asset、by_regime 归因失真；
- 胜率、Profit Factor、平均盈利、平均亏损失真；
- 模型校准、晋级/降级依据不可靠。

### 3.2 更新方案

#### 3.2.1 Position 级别归因字段

每次平仓都应更新：

```text
position.realized_pnl += net_pnl_for_closed_part
position.closed_at = Some(fill_time) when quantity == 0
```

#### 3.2.2 TradeAttribution 生命周期口径

建议 TradeAttribution 记录完整生命周期净值：

```text
pnl = gross_pnl - entry_fee_allocated - exit_fee_allocated - funding_fee_allocated - slippage_cost_allocated
```

如果短期内无法完整分摊，至少保证：

```text
pnl = position.realized_pnl
fee_total = entry_fee + exit_fee
slippage_cost_total = entry_slippage + exit_slippage
```

### 3.3 验收标准

新增测试：

- 平仓后 `position.realized_pnl != 0`；
- closed_trades.pnl 与 position.realized_pnl 一致；
- 亏损交易 result = loss；
- 盈利交易 result = win；
- by_regime 按实际交易 PnL 聚合。

---

## 4. P1：数学期望模型接入主交易链路

### 4.1 当前问题

项目已有：

- signal probability；
- expected value；
- CVaR；
- calibration；
- model card；
- suggested action。

但 runner 主流程仍主要依赖：

```text
confidence threshold
signal_strength threshold
position_pct = 0.05 * strength
```

这还不是完整数学期望交易模型。

### 4.2 推荐主链路

目标链路：

```text
AlphaSignal
→ SignalPrediction
→ ProbabilityCalibration
→ ExpectedValueDecision
→ PositionSizing
→ PortfolioRiskCheck
→ TradeIntent
→ SimulatedOrder / LiveOrder
→ Fill
→ AccountState
→ TradeAttribution
→ CalibrationFeedback
```

### 4.3 参数设计

#### 4.3.1 输入参数

| 参数 | 来源 | 用途 |
|---|---|---|
| p_up | 模型/Agent | 上涨概率 |
| p_down | 模型/Agent | 下跌概率 |
| p_flat | 模型/Agent | 震荡概率 |
| expected_return_bps | 模型 | 预期收益 |
| expected_volatility | Feature | 预期波动 |
| fee_bps | 配置/交易所 | 成本 |
| slippage_bps | 撮合/微观结构 | 成本 |
| funding_rate | 市场数据 | 永续资金成本 |
| market_regime | RegimeClassifier | 市场状态 |
| liquidity_score | orderbook/trade ticks | 流动性约束 |
| correlation | portfolio risk | 组合相关性 |
| max_drawdown_state | account/risk | 风控状态 |

#### 4.3.2 EV 计算

建议基础公式：

```text
EV_long =
  p_up * expected_gain
- p_down * expected_loss
- p_flat * flat_cost
- fee_cost
- slippage_cost
- expected_funding_cost
```

```text
EV_short =
  p_down * expected_gain
- p_up * expected_loss
- p_flat * flat_cost
- fee_cost
- slippage_cost
- expected_funding_cost
```

#### 4.3.3 决策规则

```text
if EV_long > EV_min and EV_long > EV_short and CVaR_long acceptable:
    action = open_long
elif EV_short > EV_min and EV_short > EV_long and CVaR_short acceptable:
    action = open_short
else:
    action = hold
```

#### 4.3.4 Regime 修正

不同市场状态应调整阈值：

| Regime | 策略倾向 | EV 阈值 | 仓位 |
|---|---|---|---|
| trending_bull | 顺势做多 | 降低 long EV_min | long 仓位放大 |
| trending_bear | 顺势做空 | 降低 short EV_min | short 仓位放大 |
| ranging | 均值回归/少交易 | 提高 EV_min | 仓位降低 |
| high_volatility | 风险控制 | 提高 EV_min | 仓位显著降低 |
| crisis | 防守 | 只允许小仓/不开仓 | 仓位归零或极低 |

### 4.4 实现任务

1. 新增 `DecisionEngine` 或扩展 signals 模块：

```rust
pub struct DecisionInput {
    signal: AlphaSignal,
    calibrated_probabilities: CalibratedProbabilities,
    market_features: FeatureSet,
    regime: MarketRegime,
    cost_model: CostModel,
    account_state: AccountState,
    portfolio_state: PortfolioState,
}

pub struct DecisionOutput {
    action: SuggestedAction,
    expected_value: f64,
    cvar: f64,
    confidence: f64,
    reasons: Vec<String>,
    blockers: Vec<String>,
}
```

2. runner 中替换简单阈值逻辑。
3. 决策结果写入数据库，供前端展示。
4. 决策结果进入模型卡和交易归因。

### 4.5 验收标准

- EV 为负时不下单；
- CVaR 超限时不下单；
- 高波动 regime 下仓位降低；
- crisis regime 下默认不开新仓；
- 相同信号在不同 regime 下给出不同建议；
- 决策解释包含成本、概率、风险、市场状态。

---

## 5. P1：基金经理资金模型接入

### 5.1 当前问题

已有 Kelly、风险预算、波动率目标、组合风险等模块，但 runner 仓位仍偏简单。

### 5.2 更新方案

将仓位计算改为组合式：

```text
raw_size = Kelly(EV, win_rate, payoff)
vol_adjusted_size = volatility_target(raw_size, expected_volatility)
risk_budget_size = risk_budget_limit(vol_adjusted_size, account_risk_budget)
portfolio_adjusted_size = portfolio_risk_check(risk_budget_size, correlation, cvar, liquidity)
final_size = min(all_constraints)
```

### 5.3 必须支持的资金约束

| 约束 | 说明 |
|---|---|
| max_single_position_pct | 单资产最大仓位 |
| max_total_leverage | 总杠杆上限 |
| max_daily_loss_pct | 日亏损熔断 |
| max_portfolio_cvar | 组合 CVaR 上限 |
| max_correlation_exposure | 相关性集中度上限 |
| min_liquidity_score | 流动性不足拒单 |
| regime_position_multiplier | 不同市场状态仓位系数 |

### 5.4 验收标准

- Kelly 为负时不开仓；
- 高波动时仓位小于低波动；
- 单资产仓位超限时自动缩减；
- 组合 CVaR 超限时拒单；
- 高相关资产叠加时拒单或降仓；
- 资金模型输出应写入 TradeIntent。

---

## 6. P1：交易所与市场数据覆盖补全

### 6.1 当前问题

系统已有 OKX 主线和部分跨交易所数据结构，但需要明确交易决策所需市场数据是否完整。

### 6.2 必补数据

| 数据 | 用途 |
|---|---|
| Kline OHLCV | 趋势、波动率、技术指标 |
| Funding Rate | 永续资金成本 |
| Open Interest | 杠杆资金拥挤度 |
| Long/Short Ratio | 多空结构 |
| Orderbook Depth | 流动性、滑点估计 |
| Trade Ticks | 主动买卖、CVD |
| Liquidations | 爆仓冲击 |
| Basis | 期现结构 |
| Cross Exchange Price | 跨所价差与异常检测 |
| News/Sentiment | 事件风险 |

### 6.3 更新方案

1. 建立统一 `MarketSnapshot`：

```rust
pub struct MarketSnapshot {
    symbol: String,
    exchange: String,
    timestamp: DateTime<Utc>,
    kline: Option<Kline>,
    funding_rate: Option<f64>,
    open_interest: Option<f64>,
    long_short_ratio: Option<f64>,
    orderbook_imbalance: Option<f64>,
    cvd: Option<f64>,
    liquidation_pressure: Option<f64>,
    basis: Option<f64>,
    cross_exchange_spread: Option<f64>,
    sentiment_score: Option<f64>,
}
```

2. 所有 Agent、Signal、Decision、Risk 都使用统一快照。
3. 记录每次决策使用的数据版本和时间戳。

### 6.4 验收标准

- 缺失关键数据时不能伪造结论；
- 决策结果必须列出使用的数据；
- 数据延迟超过阈值时标记为 stale；
- 不同交易所价格偏离过大时降低信任等级或拒单。

---

## 7. P1：Reports API 与 Schema 修复

### 7.1 当前问题

`reports.content` 在 schema 中是 TEXT，但接口层绑定的是 JSON Value，存在运行时类型风险。

此外 report_type 与 format 语义混用。

### 7.2 推荐方案

#### 方案 A：content 改为 JSONB

迁移：

```sql
ALTER TABLE reports
  ALTER COLUMN content TYPE JSONB
  USING content::jsonb;

ALTER TABLE reports
  ADD COLUMN IF NOT EXISTS report_type VARCHAR(50) NOT NULL DEFAULT 'general';

ALTER TABLE reports
  ADD COLUMN IF NOT EXISTS updated_at TIMESTAMP WITH TIME ZONE NOT NULL DEFAULT NOW();
```

#### 方案 B：保持 TEXT

后端写入时：

```rust
.bind(req.content.to_string())
```

读取时按字符串返回，或尝试 parse JSON。

### 7.3 推荐选择

推荐方案 A，因为报告内容后续会包含：

- 回测摘要；
- 决策解释；
- 图表配置；
- 模型卡；
- 归因数据。

JSONB 更适合结构化查询。

### 7.4 验收标准

- create_report 成功写入 JSON 内容；
- update_report 成功更新 content；
- list/search/recent/get 全部按 user_id 隔离；
- 非本人 report_id 返回 404；
- report_type 与 format 分离；
- updated_at 在 update 时变化。

---

## 8. P1：系统架构补全

### 8.1 当前问题

系统已经有多个能力模块，但缺少统一编排层。

### 8.2 推荐架构

```text
Market Data Layer
→ Feature Layer
→ Signal Layer
→ Calibration Layer
→ Decision Layer
→ Position Sizing Layer
→ Risk Layer
→ Execution Layer
→ Account Ledger Layer
→ Attribution Layer
→ Trust / Model Card Layer
→ Product API Layer
```

### 8.3 必须补充的服务

#### 8.3.1 DecisionOrchestrator

职责：

- 接收 symbol / timeframe / user_id；
- 获取市场快照；
- 计算特征；
- 调用 Agent/Signal；
- 校准概率；
- 计算 EV/CVaR；
- 调用仓位模型；
- 调用风险模型；
- 输出最终建议。

#### 8.3.2 LedgerService

职责：

- 统一开仓/加仓/减仓/平仓/反手；
- 统一手续费、滑点、资金费；
- 统一权益、保证金、杠杆；
- 对接回测、模拟盘、实盘。

#### 8.3.3 TrustService

职责：

- 计算信号可信度；
- 判断模型是否可用于交易；
- 输出 blockers；
- 生成模型卡摘要。

#### 8.3.4 FeedbackService

职责：

- 交易结束后写入结果；
- 更新校准数据；
- 更新 Agent 表现；
- 触发模型降级/报警。

### 8.4 验收标准

- 回测、模拟盘、实盘使用同一套 Ledger 口径；
- 每次决策都有 trace_id；
- 每次交易能追溯到 signal、features、model_version、risk_check；
- 每次交易完成后进入反馈闭环。

---

## 9. P2：产品体验与用户信任补全

### 9.1 当前问题

系统已有分析与模型，但用户为什么要信任它，前端展示需要更明确。

### 9.2 推荐产品功能

#### 9.2.1 交易建议卡片

每个建议必须展示：

- 建议动作：做多 / 做空 / 观望；
- 置信度；
- EV；
- CVaR；
- 最大建议仓位；
- 止损/止盈；
- 主要理由；
- 主要风险；
- 不能交易的原因。

#### 9.2.2 信任等级

建议分级：

| 等级 | 含义 | UI 表现 |
|---|---|---|
| A | 可用于模拟/小额实盘 | 绿色 |
| B | 可观察/谨慎使用 | 蓝色 |
| C | 仅参考 | 黄色 |
| D | 不建议交易 | 红色 |

#### 9.2.3 反事实解释

展示：

- 如果提前止盈会怎样；
- 如果减半仓位会怎样；
- 如果反向交易会怎样；
- 哪些因素导致亏损。

#### 9.2.4 用户依赖增强

用户每次交易前，应看到：

```text
系统建议：
1. 为什么建议这样做；
2. 这个建议在历史类似场景中表现如何；
3. 当前最大风险是什么；
4. 如果错了，系统会如何止损；
5. 什么时候建议失效。
```

### 9.3 验收标准

- 用户不需要读日志也能理解建议；
- 每个建议都有“可交易/不可交易”解释；
- 每个亏损交易都有复盘；
- 前端展示模型卡、EV、CVaR、风险约束；
- 不允许只显示“AI 看多/看空”。

---

## 10. P2：测试体系补全

### 10.1 必补测试类型

| 类型 | 目标 |
|---|---|
| 单元测试 | 账本、EV、仓位、风险模型 |
| 集成测试 | signal → order → fill → account → report |
| 回归测试 | 防止旧资金口径回归 |
| Schema 测试 | API 与 DB 字段类型一致 |
| 安全测试 | user_id 隔离 |
| 产品接口测试 | 前端所需字段完整 |

### 10.2 P0 测试清单

1. 合约开仓不扣完整 notional；
2. 平仓只结算 PnL 和费用；
3. 反手不会产生负仓位；
4. 回测结束强平产生 closed_trades；
5. total_equity 不重复加 realized_pnl；
6. 强平和普通平仓的账本结果一致；
7. 滑点成本进入绩效；
8. 手续费进入 TradeAttribution；
9. Reports JSON 写入成功；
10. 非本人 reports 无法访问。

---

## 11. 推荐实施路线

### 阶段 1：修复可信账本

范围：

- `backend/src/backtest/models.rs`
- `backend/src/backtest/matching_engine.rs`
- `backend/src/backtest/account_engine.rs`
- `backend/src/backtest/runner.rs`
- `backend/src/backtest/performance_engine.rs`

目标：

- 统一合约权益口径；
- 删除或重构绕过 matching 的强平；
- 修复 position.realized_pnl；
- 完整补测试。

### 阶段 2：接入数学期望和资金模型

范围：

- `backend/src/signals/*`
- `backend/src/backtest/position_sizing.rs`
- `backend/src/backtest/portfolio_risk.rs`
- `backend/src/backtest/runner.rs`
- 新增 `decision_engine.rs` 或 `decision_orchestrator.rs`

目标：

- EV/CVaR 决策替换简单 confidence/strength；
- Kelly/波动率目标/风险预算接入仓位；
- portfolio risk 接入风控。

### 阶段 3：修复 Reports 与产品 API

范围：

- `backend/src/routes/reports.rs`
- `backend/migrations`
- 前端 reports 页面/API client

目标：

- content 类型统一；
- report_type 与 format 分离；
- user_id 隔离测试；
- 提供前端所需结构化报告。

### 阶段 4：产品信任闭环

范围：

- 模型卡 API；
- 决策解释 API；
- 前端交易建议卡；
- 回测/复盘页面。

目标：

- 用户能理解为什么交易；
- 用户能理解为什么不交易；
- 用户能复盘每次亏损；
- 系统显示可信度，不伪装确定性。

---

## 12. 给后续 AI 的实现约束

后续 AI 更新系统时必须遵守：

1. 不要只新增模型，不接入主链路；
2. 不要用单测去证明错误资金口径；
3. 不要让回测、模拟盘、实盘使用三套不同账本；
4. 不要把 realized_pnl 同时计入 cash 和 total_equity；
5. 不要在数据缺失时伪造强置信度；
6. 不要只返回 AI 结论，必须返回依据、成本、风险、失效条件；
7. 每个新接口必须说明 user_id 隔离；
8. 每个新交易决策必须能追踪 signal_id、model_version、features、risk_check、order_id、fill_id；
9. 每个 P0 修复必须有回归测试；
10. 每个产品展示字段必须有后端来源。

---

## 13. 最终完成标准

系统 2.0 被认为完成时，至少满足：

- 账本资金守恒；
- 回测结尾平仓归因完整；
- EV/CVaR 决策进入主链路；
- Kelly/风险预算/组合风险进入仓位计算；
- 市场状态影响决策和仓位；
- Reports schema 与 API 一致；
- 用户隔离完整；
- 前端能展示交易建议、信任等级、主要风险、反事实复盘；
- 后端 `cargo test --all-targets` 通过；
- 新增 P0/P1 关键路径集成测试通过。


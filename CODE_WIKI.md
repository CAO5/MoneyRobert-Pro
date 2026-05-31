# MoneyRobert Pro — Code Wiki

## 1. 项目概览

**MoneyRobert Pro** 是一个加密货币智能投资策略生成与执行系统，采用前后端分离架构。后端基于 Rust (Axum) 构建，前端基于 Vue 3 + TypeScript 构建，通过 Docker Compose 编排部署。

| 维度 | 说明 |
|------|------|
| 项目名称 | `moneyrobert-rs` |
| 版本 | 1.0.0 |
| 后端语言 | Rust 2021 Edition |
| 前端框架 | Vue 3 + TypeScript + Vite |
| 数据库 | PostgreSQL 15 |
| 缓存 | Redis 7 |
| 交易所对接 | OKX (REST API + WebSocket) |
| 容器化 | Docker + Docker Compose |
| 许可证 | MIT |

---

## 2. 项目架构

### 2.1 整体架构图

```
┌─────────────────────────────────────────────────────────┐
│                    Nginx (反向代理)                       │
│                   :80 / :443 (生产)                      │
│                   :3000 (开发)                           │
├──────────────────────┬──────────────────────────────────┤
│   Frontend (Vue 3)   │    Backend (Rust/Axum)           │
│   :5173 (dev)        │    :8001                         │
│                      │                                  │
│  ┌────────────────┐  │  ┌────────────────────────────┐  │
│  │  Pages (17)    │  │  │  Routes (20 modules)       │  │
│  │  Components    │  │  │  Middleware (Auth/Rate/Log) │  │
│  │  Stores (Pinia)│  │  │  WebSocket Manager         │  │
│  │  Composables   │  │  │  Market Collector          │  │
│  │  API Layer     │  │  │  OKX Exchange Client       │  │
│  └────────────────┘  │  └────────────────────────────┘  │
│                      │          │          │             │
│                      │    ┌─────┘    ┌─────┘             │
│                      │    ▼          ▼                   │
│                      │  PostgreSQL  Redis                │
│                      │  :5432       :6379                │
└──────────────────────┴──────────────────────────────────┘
```

### 2.2 目录结构

```
MoneyRobert-Pro/
├── backend/                    # Rust 后端
│   ├── src/
│   │   ├── bin/main.rs         # 应用入口
│   │   ├── lib.rs              # 库模块声明
│   │   ├── config.rs           # 配置管理
│   │   ├── state.rs            # 应用状态 (DB/Redis/WS)
│   │   ├── server.rs           # HTTP 服务器与路由组装
│   │   ├── auth.rs             # JWT 认证与密码哈希
│   │   ├── models.rs           # 数据库模型 (ORM 映射)
│   │   ├── schemas.rs          # 请求/响应 Schema
│   │   ├── error.rs            # 统一错误处理
│   │   ├── extractors.rs       # 自定义 Axum 提取器
│   │   ├── middleware.rs       # 中间件 (限流/日志)
│   │   ├── collector.rs        # 市场数据采集器
│   │   ├── websocket.rs        # WebSocket 管理器
│   │   ├── logging.rs          # 日志初始化
│   │   ├── exchanges/
│   │   │   ├── mod.rs
│   │   │   └── okx.rs          # OKX 交易所客户端
│   │   └── routes/             # API 路由模块 (20个)
│   │       ├── mod.rs           # 路由注册中心
│   │       ├── health.rs        # 健康检查
│   │       ├── auth.rs          # 认证
│   │       ├── market_data.rs   # 市场数据
│   │       ├── trading.rs       # 交易
│   │       ├── strategies.rs    # 策略
│   │       ├── auto_trading.rs  # 自动交易
│   │       ├── ai_analysis.rs   # AI 分析
│   │       ├── ai_chat.rs       # AI 聊天
│   │       ├── ai_predictions.rs# AI 预测
│   │       ├── paper_trading.rs # 模拟交易
│   │       ├── dashboard.rs     # 仪表盘
│   │       ├── news.rs          # 新闻
│   │       ├── notifications.rs # 通知
│   │       ├── reports.rs       # 报告
│   │       ├── sentiment_data.rs# 情绪数据
│   │       ├── api_keys.rs      # API 密钥管理
│   │       ├── billing.rs       # 计费
│   │       ├── admin.rs         # 管理员
│   │       ├── tasks.rs         # 定时任务
│   │       └── validation.rs    # 预测验证
│   ├── migrations/             # SQL 迁移文件
│   ├── .env.example            # 环境变量模板
│   ├── Cargo.toml              # Rust 依赖
│   └── Dockerfile              # 后端 Docker 构建
├── frontend/                   # Vue 3 前端
│   ├── src/
│   │   ├── main.ts             # 应用入口
│   │   ├── App.vue             # 根组件
│   │   ├── api/index.ts        # Axios API 层
│   │   ├── router/index.ts     # 路由配置
│   │   ├── stores/             # Pinia 状态管理
│   │   │   ├── auth.ts         # 认证状态
│   │   │   └── app.ts          # 应用状态
│   │   ├── composables/        # 组合式函数
│   │   │   ├── useWebSocket.ts # WebSocket 连接
│   │   │   └── useTheme.ts     # 主题切换
│   │   ├── layouts/            # 布局组件
│   │   │   └── DashboardLayout.vue
│   │   ├── pages/              # 页面组件 (17个)
│   │   ├── components/         # 通用组件
│   │   └── lib/utils.ts        # 工具函数
│   ├── package.json            # 前端依赖
│   ├── vite.config.ts          # Vite 配置
│   └── tailwind.config.js      # Tailwind CSS 配置
├── docker/                     # Docker 构建文件
│   ├── Dockerfile.backend
│   ├── Dockerfile.frontend
│   ├── Dockerfile.frontend.dev
│   └── nginx.conf              # Nginx 配置
├── docker-compose.yml          # 开发环境编排
├── docker-compose.prod.yml     # 生产环境编排
└── .env.example                # 根级环境变量模板
```

---

## 3. 后端模块详解

### 3.1 应用入口 (`bin/main.rs`)

应用启动流程：

1. 加载配置 (`AppConfig::load()`) — 从环境变量读取
2. 初始化日志 (`init_logging()`)
3. 创建应用状态 (`AppState::new()`) — 初始化 DB 连接池、Redis、WebSocket 管理器
4. 运行数据库迁移 (`initialize_database()`)
5. 启动市场数据采集器 (`MarketCollector::start()`)
6. 启动 HTTP 服务器 (`run_server()`)

### 3.2 配置管理 (`config.rs`)

配置通过环境变量加载，前缀为 `APP`，使用双下划线 `__` 作为层级分隔符。

| 配置结构体 | 环境变量前缀 | 关键字段 |
|-----------|-------------|---------|
| `AppConfig` | `APP` | 顶层配置聚合 |
| `ServerConfig` | `APP_SERVER__` | host, port, debug, environment |
| `DatabaseConfig` | `APP_DATABASE__` | url, pool_size |
| `RedisConfig` | `APP_REDIS__` | url |
| `SecurityConfig` | `APP_SECURITY__` | secret_key, algorithm, access_token_expire_minutes, refresh_token_expire_days |
| `CorsConfig` | `APP_CORS__` | origins, allow_credentials |
| `RateLimitConfig` | `APP_RATE_LIMIT__` | enabled, requests_per_minute, requests_per_hour |
| `WebSocketConfig` | `APP_WEBSOCKET__` | okx_public_url, okx_private_url, okx_business_url |

**关键方法：**
- `AppConfig::load()` — 从 `.env` 文件和环境变量加载配置
- `AppConfig::is_production()` / `is_development()` — 环境判断

### 3.3 应用状态 (`state.rs`)

`AppState` 是全局共享状态，通过 Axum 的 State 机制注入到所有路由处理器。

```rust
pub struct AppState {
    pub config: AppConfig,                    // 应用配置
    pub db_pool: PgPool,                      // PostgreSQL 连接池
    pub redis: Option<ConnectionManager>,     // Redis 连接 (可选)
    pub ws_manager: Arc<WebSocketManager>,     // WebSocket 管理器
    pub rate_limit_map: Arc<DashMap<String, RateLimitEntry>>, // 限流计数器
}
```

**关键函数：**
- `create_db_pool()` — 创建 PostgreSQL 连接池
- `create_redis_client()` — 创建 Redis 连接管理器 (失败时降级为 None)
- `initialize_database()` — 运行数据库迁移

### 3.4 认证系统 (`auth.rs`)

基于 JWT (HS256) 的认证系统。

**核心结构：**

| 结构/函数 | 说明 |
|----------|------|
| `Claims` | JWT 载荷，包含 sub, user_id, username, role, exp, iat, type |
| `Claims::new()` | 创建新的 Claims，指定过期时间 |
| `Claims::generate_token()` | 将 Claims 编码为 JWT 字符串 |
| `Claims::from_token()` | 解码并验证 JWT 字符串 |
| `hash_password()` | 使用 bcrypt 哈希密码 |
| `verify_password()` | 验证密码与哈希是否匹配 |

**角色层级：** `admin` > `trader` > `viewer` > `normal`

### 3.5 错误处理 (`error.rs`)

统一错误类型 `AppError`，实现 `IntoResponse` trait，自动转换为 HTTP 响应。

| 错误变体 | HTTP 状态码 | 错误码 | 严重度 |
|---------|-----------|--------|-------|
| `Authentication` | 401 | UNAUTHORIZED | medium |
| `Authorization` | 403 | FORBIDDEN | medium |
| `Validation` | 400 | VALIDATION_ERROR | low |
| `NotFound` | 404 | NOT_FOUND | low |
| `Conflict` | 409 | CONFLICT | low |
| `RateLimitExceeded` | 429 | RATE_LIMIT_EXCEEDED | low |
| `Database` | 500 | INTERNAL_ERROR | critical |
| `Redis` | 500 | INTERNAL_ERROR | critical |
| `ExternalApi` | 502 | EXTERNAL_API_ERROR | high |

错误响应格式：
```json
{
  "error": "ERROR_CODE",
  "message": "详细描述",
  "category": "分类",
  "severity": "严重度",
  "recoverable": true,
  "timestamp": "ISO8601时间"
}
```

### 3.6 中间件 (`middleware.rs`)

| 中间件 | 函数 | 说明 |
|-------|------|------|
| 限流 | `rate_limit()` | 基于 IP 的每分钟请求计数，使用 DashMap 存储 |
| 请求日志 | `request_logging()` | 开发模式记录所有请求，生产模式仅记录服务端错误 |

**限流机制：** 使用 `DashMap<String, RateLimitEntry>` 进行内存级限流计数，每 60 秒重置一次窗口。

### 3.7 认证提取器 (`extractors.rs`)

| 提取器/函数 | 说明 |
|------------|------|
| `CurrentUser` | 从请求扩展中提取当前用户信息 (user_id, username, role) |
| `auth_middleware` | Bearer Token 认证中间件，解析 JWT 并注入 Claims 到请求扩展 |
| `require_role()` | 角色权限检查函数 |

### 3.8 市场数据采集器 (`collector.rs`)

`MarketCollector` 负责从 OKX API 定时采集市场数据并广播。

**采集任务：**

| 任务 | 间隔 | 数据源 | 存储 | 广播 |
|------|------|--------|------|------|
| Ticker 采集 | 10秒 | `/api/v5/market/ticker` | `ticker_history` 表 | `type: "ticker"` |
| K线采集 | 60秒 | `/api/v5/market/candles` | `klines` 表 | `type: "kline_update"` |
| 资金费率采集 | 300秒 | `/api/v5/public/funding-rate` | `funding_rate_history` 表 | `type: "funding_rate"` |

**监控的 20 个交易对：**
BTC-USDT-SWAP, ETH-USDT-SWAP, SOL-USDT-SWAP, DOGE-USDT-SWAP, XRP-USDT-SWAP, ADA-USDT-SWAP, AVAX-USDT-SWAP, DOT-USDT-SWAP, LINK-USDT-SWAP, MATIC-USDT-SWAP, UNI-USDT-SWAP, ATOM-USDT-SWAP, LTC-USDT-SWAP, FIL-USDT-SWAP, APT-USDT-SWAP, ARB-USDT-SWAP, OP-USDT-SWAP, NEAR-USDT-SWAP, SUI-USDT-SWAP, PEPE-USDT-SWAP

**K线周期：** 5m, 15m, 30m, 1H, 4H, 1D

**数据清理：** ticker_history 保留 24 小时，funding_rate_history 保留 7 天。

**代理支持：** 自动检测 `ALL_PROXY` / `HTTPS_PROXY` / `HTTP_PROXY` 环境变量，支持 SOCKS5 代理。

### 3.9 WebSocket 管理器 (`websocket.rs`)

`WebSocketManager` 管理所有 WebSocket 连接，支持广播和定向推送。

| 方法 | 说明 |
|------|------|
| `handle_connection()` | 处理新的 WebSocket 连接，启动收发任务 |
| `broadcast_to_user()` | 向指定用户的所有连接广播消息 |
| `broadcast_to_all()` | 向所有连接广播消息 |
| `connection_count()` | 获取当前连接数 |

**消息格式：**
```json
{
  "type": "ticker|kline_update|funding_rate",
  "data": { ... },
  "timestamp": 1700000000
}
```

**订阅请求格式：**
```json
{
  "type": "subscribe",
  "symbols": ["BTC-USDT-SWAP"],
  "channels": ["ticker", "kline"]
}
```

### 3.10 OKX 交易所客户端 (`exchanges/okx.rs`)

`OkxClient` 封装了 OKX 交易所的 REST API 调用，支持 HMAC-SHA256 签名认证。

| 方法 | API 路径 | 说明 |
|------|---------|------|
| `get_account_balance()` | `/api/v5/account/balance` | 获取账户余额 |
| `get_positions()` | `/api/v5/account/positions` | 获取持仓信息 |
| `get_ticker()` | `/api/v5/market/ticker` | 获取行情数据 |
| `get_candles()` | `/api/v5/market/candles` | 获取K线数据 |
| `place_order()` | `/api/v5/trade/order` | 下单 |
| `cancel_order()` | `/api/v5/trade/cancel-order` | 撤单 |
| `set_leverage()` | `/api/v5/account/set-leverage` | 设置杠杆 |

**数据结构：**

| 结构体 | 说明 |
|--------|------|
| `OkxAccount` | 账户信息 (总权益、保证金率等) |
| `OkxPosition` | 持仓信息 (方向、杠杆、未实现盈亏等) |
| `OkxTicker` | 行情数据 (最新价、买卖价、24h涨跌等) |
| `OkxCandle` | K线数据 |
| `OkxOrderRequest` | 下单请求 (合约、方向、类型、数量、止盈止损) |
| `OkxOrderResponse` | 下单响应 (订单ID、状态码) |

**签名机制：** `HMAC-SHA256(timestamp + method + requestPath + body)` → Base64 编码

**模拟交易：** 通过 `is_demo` 标志和 `x-simulated-trading: 1` 请求头支持 OKX 模拟盘。

---

## 4. API 路由详解

### 4.1 路由总览

所有 API 路由挂载在 `/api/v1` 下，需认证的路由使用 `auth_middleware` 中间件保护。

| 模块 | 路径前缀 | 认证 | 说明 |
|------|---------|------|------|
| health | `/api/v1/health` | 否 | 健康检查 |
| auth (公开) | `/api/v1/auth` | 否 | 注册/登录/刷新令牌 |
| auth (认证) | `/api/v1/auth` | 是 | 获取当前用户 |
| market | `/api/v1/market` | 是 | 市场数据查询 |
| trading | `/api/v1/trading` | 是 | 交易操作 |
| strategies | `/api/v1/strategies` | 是 | 策略管理 |
| dashboard | `/api/v1/dashboard` | 是 | 仪表盘数据 |
| news | `/api/v1/news` | 是 | 新闻资讯 |
| sentiment | `/api/v1/sentiment` | 是 | 情绪数据 |
| notifications | `/api/v1/notifications` | 是 | 通知管理 |
| reports | `/api/v1/reports` | 是 | 报告管理 |
| admin | `/api/v1/admin` | 是 (admin) | 管理员操作 |
| billing | `/api/v1/billing` | 是 | 计费管理 |
| ai | `/api/v1/ai` | 是 | AI 分析 |
| chat | `/api/v1/chat` | 是 | AI 聊天 |
| ai/prediction | `/api/v1/ai/prediction` | 是 | AI 预测 |
| auto-trading | `/api/v1/auto-trading` | 是 | 自动交易 |
| api-keys | `/api/v1/api-keys` | 是 | API 密钥管理 |
| papers | `/api/v1/papers` | 是 | 模拟交易 |
| validation | `/api/v1/validation` | 是 | 预测验证 |
| tasks | `/api/v1/tasks` | 是 | 定时任务 |
| ws | `/api/v1/ws/stream` | 否 | WebSocket 连接 |

### 4.2 认证 API (`auth.rs`)

| 方法 | 路径 | 说明 |
|------|------|------|
| POST | `/register` | 用户注册 (username, email, password) |
| POST | `/login` | 用户登录，返回 access_token + refresh_token |
| POST | `/refresh` | 刷新令牌 |
| GET | `/me` | 获取当前用户信息 |

### 4.3 市场数据 API (`market_data.rs`)

| 方法 | 路径 | 说明 |
|------|------|------|
| GET | `/klines/{symbol}` | 按交易对查询K线 (支持 interval, limit, offset) |
| GET | `/klines` | 查询K线 (支持 symbol, interval, limit, offset) |
| GET | `/tickers` | 获取所有交易对最新行情 |
| GET | `/ticker/{symbol}` | 获取单个交易对行情 |
| GET | `/candles/{symbol}` | 获取K线数据 |
| GET | `/funding-rate/{symbol}` | 按交易对查询资金费率 |
| GET | `/funding-rates` | 查询资金费率列表 |
| GET | `/open-interest/{symbol}` | 按交易对查询未平仓合约 |
| GET | `/open-interests` | 查询未平仓合约列表 |
| GET | `/long-short-ratio/{symbol}` | 按交易对查询多空比 |
| GET | `/long-short-ratio` | 查询多空比列表 |
| GET | `/sentiment` | 获取市场情绪概览 |
| GET | `/popular-symbols` | 获取热门交易对 |
| GET | `/status` | 获取数据同步状态 |

### 4.4 交易 API (`trading.rs`)

| 方法 | 路径 | 说明 |
|------|------|------|
| GET | `/balance` | 获取账户余额 (通过 OKX API) |
| GET | `/positions` | 获取当前持仓 |
| POST | `/orders` | 创建订单 |
| GET | `/orders` | 查询订单列表 |
| POST | `/orders/{order_id}/cancel` | 取消订单 |
| POST | `/leverage` | 设置杠杆 |
| GET | `/trades` | 查询交易历史 |
| GET | `/ticker/{symbol}` | 获取行情 (通过 OKX API) |
| GET | `/candles/{symbol}` | 获取K线 (通过 OKX API) |

### 4.5 策略 API (`strategies.rs`)

| 方法 | 路径 | 说明 |
|------|------|------|
| POST | `/` | 创建策略 |
| GET | `/` | 查询策略列表 (支持 status, symbol, search 分页) |
| GET | `/symbols` | 获取策略关联的交易对 |
| GET | `/{strategy_id}` | 获取策略详情 |
| PUT | `/{strategy_id}` | 更新策略 |
| DELETE | `/{strategy_id}` | 删除策略 |
| POST | `/{strategy_id}/execute` | 执行策略 |
| POST | `/{strategy_id}/cancel` | 取消策略 |
| POST | `/{strategy_id}/pause` | 暂停策略 |
| POST | `/{strategy_id}/resume` | 恢复策略 |
| GET | `/{strategy_id}/risk-metrics` | 获取策略风险指标 |

### 4.6 AI 分析 API (`ai_analysis.rs`)

| 方法 | 路径 | 说明 |
|------|------|------|
| GET | `/symbols` | 获取可分析的交易对 |
| GET | `/timeframes` | 获取支持的时间框架 |
| POST | `/market-data` | 获取综合市场数据 (K线+资金费率+情绪) |
| POST | `/analyze/technical` | 技术分析 (SMA20/50, RSI, MACD) |
| POST | `/analyze/funding` | 资金费率分析 |
| POST | `/analyze/sentiment` | 情绪分析 |
| POST | `/analyze/comprehensive` | 综合分析 (含入场/出场/止损建议) |
| POST | `/technical` | 技术分析 (别名) |
| POST | `/funding` | 资金费率分析 (别名) |
| POST | `/sentiment` | 情绪分析 (别名) |
| POST | `/comprehensive` | 综合分析 (别名) |
| GET | `/usage` | 获取 AI 使用量 |
| POST | `/usage/reset` | 重置使用量计数 |
| POST | `/generate-report` | 生成分析报告 |

### 4.7 AI 聊天 API (`ai_chat.rs`)

| 方法 | 路径 | 说明 |
|------|------|------|
| POST | `/sessions` | 创建聊天会话 |
| GET | `/sessions` | 列出聊天会话 |
| GET | `/sessions/{session_id}` | 获取会话详情 |
| DELETE | `/sessions/{session_id}` | 删除会话 |
| GET | `/sessions/{session_id}/messages` | 获取会话消息 |
| POST | `/sessions/{session_id}/messages` | 发送消息 |

### 4.8 AI 预测 API (`ai_predictions.rs`)

| 方法 | 路径 | 说明 |
|------|------|------|
| POST | `/` | 创建预测 |
| GET | `/` | 查询预测列表 |
| GET | `/{prediction_id}` | 获取预测详情 |
| POST | `/{prediction_id}/cancel` | 取消预测 |
| GET | `/statistics` | 获取预测统计 (胜率) |
| GET | `/statistics/summary` | 获取7天/30天统计摘要 |

### 4.9 自动交易 API (`auto_trading.rs`)

| 方法 | 路径 | 说明 |
|------|------|------|
| POST | `/configs` | 创建自动交易配置 |
| GET | `/configs` | 列出配置 |
| GET | `/configs/{config_id}` | 获取配置详情 |
| PUT | `/configs/{config_id}` | 更新配置 |
| POST | `/configs/{config_id}/enable` | 启用配置 |
| POST | `/configs/{config_id}/disable` | 禁用配置 |
| POST | `/start` | 启动自动交易 |
| GET | `/sessions` | 列出交易会话 |
| GET | `/sessions/{session_id}` | 获取会话详情 |
| POST | `/sessions/{session_id}/close` | 关闭会话 |
| GET | `/monitor` | 监控持仓 |

### 4.10 模拟交易 API (`paper_trading.rs`)

| 方法 | 路径 | 说明 |
|------|------|------|
| GET | `/account` | 获取模拟账户信息 |
| POST | `/account/reset` | 重置模拟账户 |
| GET | `/positions` | 获取模拟持仓 |
| POST | `/orders` | 创建模拟订单 |
| POST | `/positions/close` | 平仓 (计算 PnL) |
| GET | `/trades` | 查询交易历史 |

### 4.11 仪表盘 API (`dashboard.rs`)

| 方法 | 路径 | 说明 |
|------|------|------|
| GET | `/metrics` | 获取核心指标 (权益/余额/盈亏/策略数/持仓数) |
| GET | `/asset-distribution` | 获取资产分布 |
| GET | `/profit-trend` | 获取盈亏趋势 (支持 day/week/month) |
| GET | `/strategy-summary` | 获取策略摘要 |
| GET | `/market-tickers` | 获取市场行情概览 |
| GET | `/positions` | 获取持仓摘要 |

### 4.12 新闻 API (`news.rs`)

| 方法 | 路径 | 说明 |
|------|------|------|
| GET | `/` | 查询新闻列表 (分页) |
| GET | `/{news_id}` | 获取新闻详情 |
| POST | `/fetch` | 触发新闻抓取 |
| GET | `/recent/{symbol}` | 获取交易对相关新闻 |
| POST | `/sentiment/analyze` | 分析文本情绪 |
| GET | `/sentiment/{symbol}` | 获取情绪摘要 |
| GET | `/sentiment/{symbol}/aggregated` | 获取聚合情绪 (24h) |
| GET | `/sentiment/{symbol}/history` | 获取情绪历史 |

### 4.13 通知 API (`notifications.rs`)

| 方法 | 路径 | 说明 |
|------|------|------|
| GET | `/` | 查询通知列表 |
| GET | `/stats` | 获取通知统计 |
| GET | `/{notification_id}` | 获取通知详情 |
| PUT | `/{notification_id}/read` | 标记已读 |
| PUT | `/read-all` | 全部标记已读 |
| DELETE | `/{notification_id}` | 删除通知 |
| DELETE | `/read` | 删除已读通知 |
| GET | `/settings` | 获取通知设置 |
| PUT | `/settings` | 更新通知设置 |
| POST | `/test` | 发送测试通知 |

### 4.14 报告 API (`reports.rs`)

| 方法 | 路径 | 说明 |
|------|------|------|
| GET | `/statistics` | 获取报告统计 |
| GET | `/` | 查询报告列表 |
| GET | `/search` | 搜索报告 |
| GET | `/{report_id}` | 获取报告详情 |
| POST | `/` | 创建报告 |
| PUT | `/{report_id}` | 更新报告 |
| DELETE | `/{report_id}` | 删除报告 |
| POST | `/{report_id}/export` | 导出报告 |
| POST | `/compare` | 对比报告 |
| GET | `/recent` | 获取最近报告 |

### 4.15 情绪数据 API (`sentiment_data.rs`)

| 方法 | 路径 | 说明 |
|------|------|------|
| GET | `/` | 查询情绪数据列表 |
| GET | `/stats` | 获取情绪统计 (按来源/交易对) |
| GET | `/{sentiment_id}` | 获取情绪数据详情 |
| POST | `/` | 创建情绪数据 |
| POST | `/batch` | 批量创建情绪数据 |
| PUT | `/{sentiment_id}` | 更新情绪数据 |
| DELETE | `/{sentiment_id}` | 删除情绪数据 |
| GET | `/symbol/{symbol}/for-ai` | 获取交易对的 AI 情绪数据 |

### 4.16 API 密钥 API (`api_keys.rs`)

| 方法 | 路径 | 说明 |
|------|------|------|
| GET | `/` | 列出 API 密钥 (脱敏显示) |
| POST | `/` | 创建 API 密钥 |
| DELETE | `/{key_id}` | 删除 API 密钥 |

### 4.17 计费 API (`billing.rs`)

| 方法 | 路径 | 说明 |
|------|------|------|
| GET | `/subscription` | 获取当前订阅 |
| POST | `/subscription` | 创建订阅 |
| POST | `/subscription/cancel` | 取消订阅 |
| GET | `/records` | 查询账单记录 |
| POST | `/payment` | 创建支付 |
| POST | `/pay-per-use` | 按量付费 |
| GET | `/usage-records` | 查询使用记录 |
| GET | `/pricing` | 获取价格方案 |
| GET | `/check-subscription` | 检查订阅状态 |

**定价方案：**
- Free: $0 — 基础分析、模拟交易
- Pro: $29.99 — AI 分析、自动交易、实时数据
- Enterprise: $99.99 — 全部功能、优先支持、自定义策略

### 4.18 管理员 API (`admin.rs`)

所有管理员接口均需 `admin` 角色权限。

| 方法 | 路径 | 说明 |
|------|------|------|
| GET | `/users` | 列出用户 |
| GET | `/users/{user_id}` | 获取用户详情 |
| POST | `/users` | 创建用户 |
| PUT | `/users/{user_id}` | 更新用户 |
| DELETE | `/users/{user_id}` | 删除用户 |
| POST | `/users/{user_id}/toggle-active` | 切换用户激活状态 |
| GET | `/logs` | 查询系统日志 |
| GET | `/logs/stats` | 获取日志统计 |
| GET | `/stats` | 获取管理员统计 (总用户/活跃用户/总交易) |

### 4.19 定时任务 API (`tasks.rs`)

| 方法 | 路径 | 说明 |
|------|------|------|
| GET | `/scheduled` | 获取计划任务列表 |
| GET | `/active` | 获取活跃任务 |
| GET | `/status/{task_id}` | 获取任务状态 |
| POST | `/trigger` | 触发任务 |
| POST | `/sync` | 同步数据 (异步) |
| POST | `/sync-direct` | 同步数据 (同步) |
| POST | `/cancel/{task_id}` | 取消任务 |
| GET | `/history` | 获取任务历史 |
| GET | `/custom` | 列出自定义任务 |
| POST | `/custom` | 创建自定义任务 |
| PUT | `/custom/{task_id}` | 更新自定义任务 |
| DELETE | `/custom/{task_id}` | 删除自定义任务 |
| POST | `/custom/{task_id}/toggle` | 切换任务激活状态 |

### 4.20 预测验证 API (`validation.rs`)

| 方法 | 路径 | 说明 |
|------|------|------|
| POST | `/validate` | 验证预测 (对比实际价格) |
| GET | `/results/{validation_id}` | 获取验证结果 |
| GET | `/results` | 列出验证结果 |
| GET | `/statistics` | 获取验证统计 (胜率) |
| GET | `/confidence-analysis` | 置信度分析 (按桶统计) |
| GET | `/pattern-analysis` | 模式分析 (按方向/风险) |
| GET | `/recent-performance` | 近7天表现 |
| GET | `/confidence-threshold` | 置信度阈值分析 |
| GET | `/direction-analysis` | 方向分析 (多/空胜率) |

### 4.21 健康检查 API (`health.rs`)

| 方法 | 路径 | 说明 |
|------|------|------|
| GET | `/` | 完整健康检查 (含 DB/Redis 状态) |
| GET | `/ready` | 就绪检查 (仅 DB) |
| GET | `/live` | 存活检查 |

---

## 5. 前端模块详解

### 5.1 技术栈

| 技术 | 版本 | 用途 |
|------|------|------|
| Vue | ^3.4.15 | UI 框架 |
| TypeScript | ~5.3.3 | 类型安全 |
| Vite | ^5.0.12 | 构建工具 |
| Pinia | ^3.0.4 | 状态管理 |
| Vue Router | ^4.2.5 | 路由管理 |
| Axios | ^1.16.1 | HTTP 客户端 |
| Tailwind CSS | ^3.4.1 | 样式框架 |
| Lightweight Charts | ^5.2.0 | K线图表 |
| Lucide Vue | ^0.511.0 | 图标库 |
| VueUse | ^14.3.0 | 组合式工具集 |

### 5.2 API 层 (`api/index.ts`)

基于 Axios 封装的 HTTP 客户端，核心特性：

- **请求拦截器：** 自动注入 `Authorization: Bearer {token}` 请求头
- **响应拦截器：**
  - 自动解包 `{ success: true, data: ... }` 格式的响应
  - 401 错误自动尝试使用 refresh_token 刷新令牌
  - 刷新失败则清除本地存储并跳转登录页
- **基础 URL：** 通过 `VITE_API_URL` 环境变量配置

### 5.3 路由配置 (`router/index.ts`)

| 路径 | 名称 | 组件 | 权限 |
|------|------|------|------|
| `/login` | login | LoginPage | guest |
| `/register` | register | RegisterPage | guest |
| `/dashboard` | dashboard | DashboardPage | auth |
| `/market` | market | MarketPage | auth |
| `/ai` | ai | AiAnalysisPage | auth |
| `/ai/chat` | ai-chat | AiChatPage | auth |
| `/ai/predictions` | ai-predictions | AiPredictionsPage | auth |
| `/trading` | trading | TradingPage | auth |
| `/strategies` | strategies | StrategiesPage | auth |
| `/auto-trading` | auto-trading | AutoTradingPage | auth |
| `/paper-trading` | paper-trading | PaperTradingPage | auth |
| `/news` | news | NewsPage | auth |
| `/reports` | reports | ReportsPage | auth |
| `/notifications` | notifications | NotificationsPage | auth |
| `/settings` | settings | SettingsPage | auth |
| `/admin` | admin | AdminPage | admin |

**路由守卫：**
- `meta.auth` — 需要登录，未登录重定向到 `/login`
- `meta.guest` — 仅游客可访问，已登录重定向到 `/dashboard`
- `meta.admin` — 需要 admin 角色，否则重定向到 `/dashboard`

### 5.4 状态管理

#### Auth Store (`stores/auth.ts`)

| 属性/方法 | 类型 | 说明 |
|----------|------|------|
| `token` | `ref<string>` | 访问令牌 |
| `refreshToken` | `ref<string>` | 刷新令牌 |
| `user` | `ref<object>` | 当前用户信息 |
| `isAuthenticated` | `computed` | 是否已认证 |
| `isAdmin` | `computed` | 是否管理员 |
| `login()` | `async` | 登录并获取用户信息 |
| `register()` | `async` | 注册 |
| `fetchUser()` | `async` | 获取当前用户信息 |
| `logout()` | `function` | 登出并清除状态 |

#### App Store (`stores/app.ts`)

管理全局应用状态，包括主题、语言和布局设置。

### 5.5 WebSocket 组合式函数 (`composables/useWebSocket.ts`)

| 属性/方法 | 说明 |
|----------|------|
| `connected` | WebSocket 连接状态 (readonly) |
| `lastMessage` | 最后接收的消息 (readonly) |
| `connect()` | 建立 WebSocket 连接 |
| `disconnect()` | 断开连接 |
| `on(type, handler)` | 注册消息类型监听器 |
| `off(type, handler)` | 移除消息类型监听器 |

**重连机制：** 指数退避重连，最大 20 次，基础延迟 1 秒，最大延迟 30 秒。

**连接地址：** `{ws|wss}://{host}/api/v1/ws/stream`

### 5.6 页面组件

| 页面 | 功能 |
|------|------|
| DashboardPage | 仪表盘 — 总权益、盈亏、策略、持仓概览 |
| MarketPage | 市场行情 — 实时 Ticker、K线图表、资金费率 |
| TradingPage | 交易 — 下单、持仓管理、交易历史 |
| AiAnalysisPage | AI 分析 — 技术/资金/情绪/综合分析 |
| AiChatPage | AI 聊天 — 对话式市场分析 |
| AiPredictionsPage | AI 预测 — 预测列表、胜率统计 |
| AutoTradingPage | 自动交易 — 配置管理、会话监控 |
| StrategiesPage | 策略管理 — 创建/编辑/执行策略 |
| PaperTradingPage | 模拟交易 — 虚拟资金交易 |
| NewsPage | 新闻 — 加密货币新闻与情绪分析 |
| NotificationsPage | 通知 — 系统通知管理 |
| ReportsPage | 报告 — 分析报告生成与导出 |
| SettingsPage | 设置 — API 密钥、通知偏好 |
| AdminPage | 管理后台 — 用户管理、系统日志、统计 |
| LoginPage | 登录 |
| RegisterPage | 注册 |

### 5.7 通用组件

| 组件 | 说明 |
|------|------|
| CandlestickChart | K线图表组件 (基于 Lightweight Charts) |
| IndicatorChart | 指标图表组件 |
| Empty | 空状态占位组件 |
| DashboardLayout | 仪表盘布局 (侧边栏 + 顶栏 + 内容区) |

---

## 6. 数据库设计

### 6.1 核心表

| 表名 | 说明 | 关键字段 |
|------|------|---------|
| `users` | 用户表 | id, username, email, hashed_password, role (enum), is_active, notification_settings |
| `api_keys` | API 密钥 | id, user_id, name, key, secret, passphrase, is_active |
| `market_data` | 市场数据 | id, symbol, interval, open_time, OHLCV |
| `klines` | K线数据 | id, symbol, interval, open_time, OHLCV, is_closed, updated_at |
| `ticker_history` | Ticker 历史 | id, symbol, last, open_24h, high_24h, low_24h, volume_24h, best_bid, best_ask, timestamp |
| `funding_rates` | 资金费率 | id, symbol, funding_rate, next_funding_time |
| `funding_rate_history` | 资金费率历史 | id, symbol, funding_rate, funding_time, realized_rate, avg_premium_index |
| `positions` | 持仓 | id, user_id, strategy_id, symbol, side (enum), quantity, entry_price, unrealized_pnl, leverage, stop_loss, take_profit, status (enum) |
| `trades` | 交易记录 | id, user_id, symbol, side (enum), order_type (enum), price, quantity, status (enum), order_id, pnl |
| `strategies` | 策略 | id, user_id, symbol, direction (enum), entry_price, stop_loss, take_profit, leverage, position_size, status (enum) |
| `ai_analysis` | AI 分析 | id, strategy_id, content (JSON), analysis_type |
| `reports` | 报告 | id, title, content, report_type (enum), status (enum) |
| `notifications` | 通知 | id, user_id, type (enum), title, content, channel (enum), is_read, sent_at |
| `sentiment_data` | 情绪数据 | id, user_id, symbol, platform, source_type, content, sentiment_type, sentiment_score, is_verified, is_kol, is_active |
| `news_items` | 新闻 | id, title, content, source, url, symbol, published_at, sentiment |

### 6.2 扩展表

| 表名 | 说明 |
|------|------|
| `open_interests` | 未平仓合约数据 |
| `long_short_ratio_history` | 多空比历史 |
| `fear_greed_index` | 恐惧贪婪指数 |
| `system_logs` | 系统日志 |
| `subscriptions` | 订阅信息 |
| `billing_records` | 账单记录 |
| `usage_records` | 使用记录 |
| `ai_chat_sessions` | AI 聊天会话 |
| `ai_chat_messages` | AI 聊天消息 |
| `ai_provider_keys` | AI 提供商密钥 |
| `ai_prediction_trades` | AI 预测交易 |
| `auto_trading_configs` | 自动交易配置 |
| `auto_trading_sessions` | 自动交易会话 |
| `validation_records` | 预测验证记录 |
| `scheduled_tasks` | 定时任务 |
| `equity_snapshots` | 权益快照 |

### 6.3 重要枚举类型

| 枚举 | 值 |
|------|---|
| `user_role_enum` | ADMIN, TRADER, VIEWER, NORMAL |
| `trade_side_enum` | BUY, SELL |
| `trade_status_enum` | PENDING, FILLED, CANCELLED, CLOSED |
| `order_type_enum` | MARKET, LIMIT |
| `position_status_enum` | OPEN, CLOSED |
| `strategy_direction_enum` | LONG, SHORT |
| `strategy_status_enum` | CREATED, ACTIVE, PAUSED, CANCELLED |
| `report_type_enum` | DAILY, WEEKLY, MONTHLY, CUSTOM |
| `notification_type_enum` | SYSTEM, TRADE, STRATEGY, RISK |
| `notification_channel_enum` | IN_APP, EMAIL, SMS, WECHAT |
| `ai_prediction_status_enum` | PENDING, TAKE_PROFIT_HIT, STOP_LOSS_HIT, CANCELLED |
| `ai_prediction_result_enum` | PENDING, WIN, LOSS |

### 6.4 种子数据

初始用户：
- 管理员: `admin` / `admin123` (ADMIN 角色)
- 演示用户: `demo` / `demo123` (NORMAL 角色)

---

## 7. 依赖关系

### 7.1 后端依赖 (Rust)

| 依赖 | 版本 | 用途 |
|------|------|------|
| `axum` | 0.8 | Web 框架 (含 WebSocket 和宏支持) |
| `tokio` | 1 | 异步运行时 |
| `serde` / `serde_json` | 1 | 序列化/反序列化 |
| `sqlx` | 0.8 | 异步 PostgreSQL 驱动 |
| `redis` | 0.28 | Redis 客户端 |
| `jsonwebtoken` | 9 | JWT 编解码 |
| `bcrypt` | 0.17 | 密码哈希 |
| `reqwest` | 0.12 | HTTP 客户端 (含 SOCKS5 代理) |
| `hmac` / `sha2` / `base64` | — | OKX API 签名 |
| `chrono` | 0.4 | 时间处理 |
| `uuid` | 1 | UUID 生成 |
| `validator` | 0.19 | 请求验证 |
| `tower` / `tower-http` | 0.5 / 0.6 | 中间件 (CORS, 日志, 限流) |
| `tracing` / `tracing-subscriber` | 0.1 / 0.3 | 日志框架 |
| `config` | 0.15 | 配置管理 |
| `dotenvy` | 0.15 | .env 文件加载 |
| `dashmap` | 6 | 并发 HashMap |
| `thiserror` / `anyhow` | 2 / 1 | 错误处理 |
| `aes-gcm` | 0.10 | AES 加密 |
| `utoipa` | 5 | OpenAPI 文档生成 |
| `tokio-cron-scheduler` | 0.13 | 定时任务调度 |
| `parking_lot` | 0.12 | 高效锁 |
| `futures` | 0.3 | 异步工具 |

### 7.2 前端依赖

| 依赖 | 版本 | 用途 |
|------|------|------|
| `vue` | ^3.4.15 | UI 框架 |
| `vue-router` | ^4.2.5 | 路由 |
| `pinia` | ^3.0.4 | 状态管理 |
| `axios` | ^1.16.1 | HTTP 客户端 |
| `lightweight-charts` | ^5.2.0 | TradingView K线图表 |
| `lucide-vue-next` | ^0.511.0 | 图标库 |
| `tailwind-merge` | ^3.3.0 | Tailwind 类名合并 |
| `clsx` | ^2.1.1 | 条件类名 |
| `@vueuse/core` | ^14.3.0 | 组合式工具 |

---

## 8. 项目运行方式

### 8.1 环境要求

- Rust 1.70+ (推荐最新稳定版)
- Node.js 18+
- PostgreSQL 15+
- Redis 7+
- Docker & Docker Compose (容器化部署)

### 8.2 Docker Compose 部署 (推荐)

#### 开发环境

```bash
# 克隆项目
git clone <repo-url>
cd MoneyRobert-Pro

# 复制环境变量
cp .env.example .env
cp backend/.env.example backend/.env

# 启动所有服务
docker-compose up -d

# 服务地址：
# - 前端: http://localhost:3000
# - 后端: http://localhost:8001
# - PostgreSQL: localhost:5432
# - Redis: localhost:6379
```

#### 生产环境

```bash
# 设置必要的环境变量
export SECRET_KEY="your-production-secret-key"
export POSTGRES_PASSWORD="your-strong-password"

# 启动生产服务
docker-compose -f docker-compose.prod.yml up -d

# 服务地址：
# - 前端: http://localhost:80 (Nginx)
# - 后端: :8001 (内部)
```

### 8.3 本地开发

#### 后端

```bash
cd backend

# 复制环境变量
cp .env.example .env
# 编辑 .env 配置数据库和 Redis 连接

# 运行
cargo run

# 或构建 release
cargo build --release
./target/release/moneyrobert
```

#### 前端

```bash
cd frontend

# 安装依赖
npm install

# 开发模式
npm run dev
# 访问 http://localhost:5173

# 构建
npm run build

# 预览构建结果
npm run preview
```

### 8.4 环境变量配置

#### 后端核心环境变量

| 变量 | 默认值 | 说明 |
|------|--------|------|
| `APP_SERVER__HOST` | 0.0.0.0 | 监听地址 |
| `APP_SERVER__PORT` | 8001 | 监听端口 |
| `APP_SERVER__DEBUG` | true | 调试模式 |
| `APP_SERVER__ENVIRONMENT` | development | 运行环境 |
| `APP_DATABASE__URL` | — | PostgreSQL 连接字符串 |
| `APP_DATABASE__POOL_SIZE` | 5 | 连接池大小 |
| `APP_REDIS__URL` | — | Redis 连接字符串 |
| `APP_SECURITY__SECRET_KEY` | — | JWT 签名密钥 |
| `APP_SECURITY__ALGORITHM` | HS256 | JWT 算法 |
| `APP_SECURITY__ACCESS_TOKEN_EXPIRE_MINUTES` | 30 | Access Token 过期时间 |
| `APP_SECURITY__REFRESH_TOKEN_EXPIRE_DAYS` | 7 | Refresh Token 过期时间 |
| `APP_CORS__ORIGINS` | — | CORS 允许的源 |
| `APP_RATE_LIMIT__ENABLED` | true | 限流开关 |
| `APP_RATE_LIMIT__REQUESTS_PER_MINUTE` | 100 | 每分钟请求限制 |
| `APP_WEBSOCKET__OKX_PUBLIC_URL` | wss://ws.okx.com:8443/ws/v5/public | OKX 公共 WebSocket |
| `APP_WEBSOCKET__OKX_PRIVATE_URL` | wss://ws.okx.com:8443/ws/v5/private | OKX 私有 WebSocket |
| `HTTP_PROXY` / `HTTPS_PROXY` / `ALL_PROXY` | — | 代理配置 (SOCKS5) |

#### 前端环境变量

| 变量 | 说明 |
|------|------|
| `VITE_API_URL` | 后端 API 地址 (开发: `/api/v1`, 生产: `/api/v1`) |

### 8.5 数据库迁移

数据库迁移通过 PostgreSQL 的 `docker-entrypoint-initdb.d` 机制自动执行。迁移文件按顺序编号：

1. `00000000000000_initial.sql` — 初始化
2. `001_initial_schema.sql` — 核心表和索引
3. `002_additional_tables.sql` — 扩展表
4. `003_seed_data.sql` — 种子数据
5. `004_market_data_tables.sql` — 市场数据表和种子数据

---

## 9. 关键数据流

### 9.1 市场数据流

```
OKX REST API
     │
     ▼
MarketCollector (定时采集)
     │
     ├──► PostgreSQL (持久化)
     │
     └──► WebSocketManager.broadcast_to_all()
              │
              ▼
         Frontend WebSocket Client
              │
              ▼
         页面实时更新 (MarketPage, TradingPage 等)
```

### 9.2 交易流程

```
用户下单请求
     │
     ▼
Trading API (POST /api/v1/trading/orders)
     │
     ├──► 写入 trades 表 (状态: pending)
     │
     └──► OkxClient.place_order() (如已配置 API Key)
              │
              ▼
         OKX 交易所
```

### 9.3 AI 分析流程

```
用户请求分析
     │
     ▼
AI Analysis API
     │
     ├──► 查询 market_data / funding_rates / sentiment_data
     │
     ├──► 计算技术指标 (SMA, RSI, MACD)
     │
     ├──► 生成分析结果 (方向/置信度/风险等级/入场出场建议)
     │
     └──► 写入 ai_analysis 表 / 返回结果
```

### 9.4 认证流程

```
用户登录
     │
     ▼
POST /api/v1/auth/login
     │
     ├──► 验证用户名密码 (bcrypt)
     │
     ├──► 生成 access_token (30分钟) + refresh_token (7天)
     │
     └──► 返回 JWT tokens
              │
              ▼
前端存储到 localStorage
     │
     ▼
Axios 拦截器自动注入 Authorization 头
     │
     ▼
auth_middleware 验证 JWT → 注入 Claims → CurrentUser
```

---

## 10. 部署架构

### 10.1 开发环境 (docker-compose.yml)

| 服务 | 镜像 | 端口 | 说明 |
|------|------|------|------|
| backend | 自建 | 8001:8001 | 后端服务 (debug 模式) |
| frontend | 自建 (dev) | 3000:3000 | 前端开发服务器 (热更新) |
| db | postgres:15-alpine | 5432:5432 | 数据库 |
| redis | redis:7-alpine | 6379:6379 | 缓存 |

### 10.2 生产环境 (docker-compose.prod.yml)

| 服务 | 镜像 | 端口 | 说明 |
|------|------|------|------|
| backend | 自建 | 8001 (内部) | 后端服务 (release 模式) |
| frontend | 自建 | 80:80, 443:443 | Nginx 静态服务 + 反向代理 |
| db | postgres:15-alpine | — | 数据库 (不暴露端口) |
| redis | redis:7-alpine | — | 缓存 (不暴露端口, AOF 持久化, 256MB 上限) |

**生产环境安全措施：**
- SECRET_KEY 和 POSTGRES_PASSWORD 必须通过环境变量设置
- CORS origins 默认为空
- 数据库和 Redis 不对外暴露端口
- 后端通过 Nginx 反向代理访问
- 日志使用 JSON 格式，限制大小 (10MB × 3)
- 后端配置健康检查

### 10.3 Nginx 配置

生产环境 Nginx 负责：
- 前端静态资源服务
- `/api/v1` 路径反向代理到后端
- WebSocket 升级 (`/api/v1/ws/`)
- Gzip 压缩
- 静态资源缓存

---

## 11. 开发规范

### 11.1 后端代码规范

- 路由处理器使用 Axum 的函数式风格，通过 `State`, `Path`, `Query`, `Json` 提取器获取参数
- 所有数据库查询使用 `sqlx` 原生 SQL，不使用 ORM
- 错误统一通过 `AppError` 返回，使用 `Result<T>` 类型别名
- 需认证的路由通过 `CurrentUser` 提取器获取当前用户
- 管理员接口需额外检查 `user.role == "admin"`
- 分页统一使用 `page` / `page_size` 参数，默认 page=1, page_size=20

### 11.2 前端代码规范

- 使用 Vue 3 Composition API (`<script setup>`)
- 状态管理使用 Pinia
- 样式使用 Tailwind CSS
- HTTP 请求通过 `@/api` 统一封装
- WebSocket 使用 `useWebSocket` 组合式函数
- 路由使用懒加载 (`() => import()`)

### 11.3 Git 规范

- `.env` 文件不纳入版本控制
- 前端 `dist/` 目录不纳入版本控制
- 使用 `.gitignore` 排除构建产物和敏感文件

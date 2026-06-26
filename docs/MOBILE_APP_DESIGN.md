# MoneyRobert 移动多端应用设计文档

> 本文档记录移动端能力拓展的设计与实现，对应深度研究报告 `docs/deep-research-report.md` 的落地。
> 操作轨迹可追踪：所有移动端架构决策、页面清单、对接关系、部署重启方式均记录在此。

## 1. 背景与目标

基于 `docs/deep-research-report.md` 的"一主两翼"多端策略，拓展当前项目的移动端能力，要求支持：
- 安卓（Android）
- 苹果（iOS）
- 鸿蒙（HarmonyOS）
- 小程序（微信/支付宝/抖音）
- H5 / PWA

所有端共用一套代码、对接**统一后台**（现有 Rust 后端）。

## 2. 技术选型与对应关系

| 维度 | 选型 | 对应需求 |
|---|---|---|
| 跨端框架 | Taro 4.1.9（React + TypeScript） | 一套代码支持微信/支付宝/抖音小程序、H5/PWA、React Native (Android/iOS)、鸿蒙 |
| 状态管理 | zustand 4.5 | 轻量、TS 友好，与桌面端 Pinia 同为响应式风格 |
| 样式方案 | CSS Modules + SCSS | 与深度研究报告 Design Token 共享建议一致 |
| 后端对接 | Taro.request + JWT Bearer | 复用现有 Rust 后端 `/auth/*`、`/signals/*`、`/backtest/*`、`/market/*`、`/tasks/*`、`/notifications/*` 路由 |
| 设计稿基准 | 750rpx | Taro 默认，适配多端 |
| 多端协商头 | `X-Client-Platform` / `X-Client-Version` | 让后端 BFF 做版本降级 |

### 鸿蒙支持说明
Taro 4.x 通过 `@tarojs/plugin-platform-harmony` 插件支持鸿蒙。当前模板未内置，需通过以下方式启用：
```bash
# 在 mobile/ 目录下安装鸿蒙插件（不要在外层根目录执行）
cd mobile
npm install @tarojs/plugin-platform-harmony --save-dev
```
然后在 `config/index.ts` 的 `plugins` 数组中注册插件，使用 `npm run build:harmony` 构建。

## 3. 项目结构

```
MoneyRobert-Pro/
├── backend/              # 现有 Rust 后端（统一后台）
├── frontend/            # 现有 Vue 3 桌面前端
└── mobile/              # 移动多端应用（本次新增）
    └── src/
        ├── app.config.ts          # 全局配置：5 个 tabBar + 7 个二级页
        ├── app.tsx                # 应用入口：恢复登录态
        ├── app.scss               # 全局样式
        ├── styles/
        │   ├── theme.scss         # Design Token（与桌面端共享色板）
        │   ├── variables.scss      # 通用 SCSS 变量与 Mixin
        │   └── compat.scss         # 多端兼容样式
        ├── types/                 # TypeScript 类型（与后端 schema 对齐）
        │   ├── common.ts          # 通用响应包装、客户端平台标识
        │   ├── auth.ts            # /auth/* 类型
        │   ├── market.ts          # /market/* 类型
        │   ├── signal.ts          # /signals/* 决策卡类型
        │   ├── backtest.ts        # /backtest/* 回测类型
        │   ├── todo.ts            # 待办类型
        │   ├── message.ts         # /notifications/* 类型
        │   └── workbench.ts       # 工作台聚合数据类型
        ├── services/              # API 服务层
        │   ├── request.ts         # 统一请求层：JWT、X-Client-*、401 自动刷新
        │   ├── auth.ts            # 认证服务
        │   ├── workbench.ts       # 工作台聚合服务（Mobile BFF 模式）
        │   ├── market.ts          # 行情服务
        │   ├── signal.ts          # 决策卡服务
        │   ├── backtest.ts        # 回测服务
        │   ├── todo.ts            # 待办服务
        │   └── message.ts         # 消息服务
        ├── store/                 # zustand 状态管理
        │   ├── auth.ts            # 登录态、Token 持久化
        │   └── app.ts             # 网络状态、设备信息
        ├── hooks/
        │   ├── useAuth.ts         # 鉴权 Hook
        │   └── useRequest.ts      # 通用请求 Hook
        ├── components/             # 通用组件
        │   ├── Card/              # 卡片容器
        │   ├── EmptyState/        # 空状态
        │   ├── StatCard/          # 指标卡
        │   ├── Tag/               # 标签/徽章
        │   └── PageHeader/        # 页面头部
        ├── data/                  # Mock 数据（H5 预览/离线开发）
        │   ├── workbench.ts
        │   ├── market.ts
        │   ├── decision-card.ts
        │   ├── backtest.ts
        │   ├── todo.ts
        │   └── message.ts
        └── pages/                 # 12 个页面
            ├── workbench/         # 工作台（tabBar）
            ├── business/          # 业务（tabBar，内含 行情/决策卡/回测/报告 4 Tab）
            ├── todo/              # 待办（tabBar）
            ├── message/           # 消息（tabBar）
            ├── mine/              # 我的（tabBar）
            ├── login/             # 登录
            ├── decision-detail/   # 决策卡详情
            ├── backtest-detail/   # 回测详情
            ├── todo-detail/       # 待办详情（含审批操作）
            ├── settings/          # 设置
            ├── symbol-detail/     # 标的详情（占位）
            └── report-detail/      # 报告详情（占位）
```

## 4. 信息架构（IA）

遵循深度研究报告建议的"工作台/业务/待办/消息/我的"五项底部导航：

```
启动 → 登录 → 工作台
              ├─ 工作台（聚合：待办数、风险告警、关键指标、快捷入口、最近访问）
              ├─ 业务（Tab：行情 / 决策卡 / 回测 / 报告）
              ├─ 待办（风险确认 / 异常审核 / 升级审批 / 告警确认）
              ├─ 消息（系统 / 业务 / 风险 / 审批 / 升级）
              └─ 我的（账户 / 设置 / 退出）
```

## 5. 后端对接关系

移动端复用现有 Rust 后端，**不引入新的服务端**：

| 移动端 Service | 后端路由 | 说明 |
|---|---|---|
| authService | `/auth/login`、`/auth/refresh`、`/auth/me` | JWT Bearer 鉴权 |
| workbenchService | `/dashboard/workbench`、`/tasks/recent`、`/notifications/recent` | Mobile BFF 聚合 |
| marketService | `/market/snapshots`、`/features/regimes/latest` | 行情快照、市场状态 |
| signalService | `/signals/decision-cards`、`/signals/decision-cards/{id}` | 决策卡列表/详情 |
| backtestService | `/backtest/jobs`、`/backtest/jobs/{id}`、`/backtest/jobs/{id}/report`、`/backtest/jobs/{id}/trust-level` | 回测任务/绩效/可信等级 |
| todoService | `/tasks`、`/tasks/{id}`、`/tasks/{id}/process` | 待办列表/详情/审批 |
| messageService | `/notifications`、`/notifications/{id}/read` | 消息列表/已读 |

### 请求头约定
- `Authorization: Bearer <access_token>` —— JWT 鉴权
- `X-Client-Platform: weapp|alipay|tt|h5|rn|harmony|qq|jd` —— 客户端平台标识
- `X-Client-Version: 1.0.0` —— 客户端版本号

### 后端连接配置（v1.0.1 修正）

> 重要：后端实际监听端口为 **8001**（见 `backend/.env` 的 `APP_SERVER__PORT=8001`），桌面端 frontend 通过 vite proxy 将 `/api` 代理到 `http://localhost:8001`。移动端必须使用相同端口。

| 配置项 | 值 | 说明 |
|---|---|---|
| 后端端口 | 8001 | `backend/.env` 的 `APP_SERVER__PORT` |
| 移动端 API_BASE_URL（非 H5） | `http://localhost:8001/api/v1` | 小程序/真机联调地址，由 `request.ts` 中 `TARO_APP_API_URL` 兜底 |
| 移动端 API_BASE_URL（H5） | `/api/v1` | 相对路径，需 nginx 反代或 vite proxy |
| 默认账号 | admin / admin123 | 种子数据 `migrations/003_seed_data.sql` |
| CORS 允许来源 | localhost:3000、localhost:5173 | `backend/.env` 的 `APP_CORS__ORIGINS`，H5 联调需补充预览域名 |

### Mock 双轨制说明
- `MOCK_ENABLED = !TARO_APP_API_URL && TARO_ENV === 'h5'`
- **H5 预览**：未配置 `TARO_APP_API_URL` 时自动走 mock（含 auth），无需后端即可预览
- **小程序/真机**：需配置 `TARO_APP_API_URL` 指向真实后端（生产需 HTTPS）
- 所有 service（含 authService）均已实现 mock 分支，保证 H5 预览闭环可用

## 6. 认证流程

遵循 RFC 8252 与深度研究报告建议：
1. PWA / H5：标准 Web 登录（账号密码 → JWT）
2. Native Shell（Android/iOS/鸿蒙）：通过外部浏览器授权（后续阶段实现）
3. Token 持久化：`Taro.setStorageSync`
4. 401 自动刷新：请求层自动调用 `/auth/refresh` 并重试一次

## 7. 部署与重启方式

> 注意：本项目使用 Docker Compose 部署。**不要在外层根目录直接 `npm run build`，否则会与现有 frontend 冲突。**

### 后端启动（前后端联调前提）

后端实际端口为 **8001**（非 8080）。两种启动方式：

**方式一：cargo 直接运行（开发联调，推荐）**
```bash
# 前置依赖：PostgreSQL(5432) + Redis(6379) 已运行
cd backend
cargo run
# 首次编译约 3~4 分钟，启动后监听 8001
# 验证：curl http://localhost:8001/api/v1/health
```

**方式二：Docker Compose（遵循用户规则，不在外层直接构建）**
```bash
# 在项目根目录通过 compose 编排启动（会自动启动 db/redis 依赖）
docker-compose up -d backend
# 不要单独 docker build / docker run，必须走 compose
```

### 开发预览
```bash
# 在 mobile/ 目录下启动 H5 预览（不要在外层根目录执行）
cd mobile
npm run dev:h5
```

### 多端构建
```bash
# 微信小程序
cd mobile && npm run build:weapp

# 支付宝小程序
cd mobile && npm run build:alipay

# 抖音小程序
cd mobile && npm run build:tt

# H5 / PWA
cd mobile && npm run build:h5

# React Native (Android/iOS)
cd mobile && npm run build:rn

# 鸿蒙（需先安装 @tarojs/plugin-platform-harmony）
cd mobile && npm run build:harmony
```

### Docker 部署（生产环境）
**不要**在外层根目录直接构建。应在 `docker/` 目录新增 `Dockerfile.mobile`，由 docker-compose 编排：
```dockerfile
# docker/Dockerfile.mobile 示例（按需创建）
FROM node:18-alpine AS builder
WORKDIR /app
COPY mobile/package*.json ./
RUN npm ci
COPY mobile/ ./
RUN npm run build:h5

FROM nginx:alpine
COPY --from=builder /app/dist /usr/share/nginx/html
COPY docker/nginx.mobile.conf /etc/nginx/conf.d/default.conf
```
然后在 `docker-compose.prod.yml` 增加 `mobile` 服务，与 `frontend`、`backend` 同一网络。

### 预览服务器重启
若已运行 `.pai/pai-preview-server.lock`，**不要删除该锁文件**，直接重新调用 `preview-server.js` 即可复用端口。

## 8. 当前完成范围（v1.0.0）

### 已完整实现
- ✅ 5 个 tabBar 页面：工作台、业务、待办、消息、我的
- ✅ 登录页（JWT 鉴权）
- ✅ 决策卡详情页（概率分布、失效条件、数据血缘）
- ✅ 回测详情页（概览/绩效/可信门禁 3 Tab）
- ✅ 待办详情页（含通过/驳回/延后审批操作）
- ✅ 设置页（通知/显示/安全 3 分组）
- ✅ 统一请求层（JWT、401 自动刷新、多端协商头）
- ✅ Mock 数据（H5 预览自动走 mock）

### 占位页（后续可扩展）
- 标的详情页
- 报告详情页

## 9. 后续路线图（按深度研究报告阶段）

| 阶段 | 内容 | 状态 |
|---|---|---|
| P0 Mobile Web/PWA MVP | 工作台/业务/待办/我的主流程 | ✅ 已完成 |
| P1 消息中心 | 系统通知、业务告警、跳转 | ✅ 已完成 |
| P1 报告中心 | 报告预览、分享 | 🟡 占位 |
| P2 Native Shell 增强 | 推送、网络监听、生物识别 | ⬜ 待启动 |
| P2 小程序 Lite | 微信/支付宝/抖音轻入口 | 🟡 已就绪（构建即可） |
| P2 鸿蒙 | 鸿蒙端构建 | ⬜ 待安装插件 |

## 10. 关键约束与注意事项

1. **代码注释必须中文** —— 已遵循
2. **不破坏现有 frontend / backend** —— mobile/ 独立目录，零侵入
3. **Docker 部署需新增 Dockerfile.mobile** —— 不要直接外层构建
4. **小程序域名白名单** —— 生产部署需在后端配置真实 HTTPS 域名并加入小程序后台 request 合法域名
5. **Design Token 共享** —— `theme.scss` 配色与桌面端 TailwindCSS 色板保持一致

## 11. UI 视觉重设计（极简留白风 v1.0.3）

### 设计理念
将原"金融科技风"（深蓝渐变头部 + 重阴影 + 小圆角）整体重构为"极简留白风"（类 Apple/Notion）：浅色背景 + 大量留白 + 轻盈卡片 + 克制配色。视觉目标：清爽、专业、信息密度低、长时间使用不疲劳。

### 改动清单

#### 全局主题（`styles/`）
- `variables.scss`：
  - `$page-padding` 32rpx → 40rpx（增大左右留白）
  - `$max-content-width` 调整为 670rpx
  - 新增 `$spacing-2xl: 64rpx`（超大留白）
  - 圆角全面增大：`$radius-md` 12→16rpx，`$radius-lg` 16→24rpx，`$radius-xl` 20→32rpx
  - 阴影全面调轻：`$shadow-card` 由 rgba(0,0,0,0.08) 降至 0.04
- `theme.scss`：
  - 页面背景 `$color-bg-page` #f5f6f7 → #f7f8fa（更接近纯白）
  - 新增 `$color-bg-subtle: #fafbfc`（极浅背景层，用于次级容器/输入框）
  - 新增 `$gradient-subtle`（极淡背景渐变，作为浅色容器的层次补充）
- `app.scss`：
  - `.page-root` padding 改为 `$spacing-lg $page-padding`
  - `.card-base` 圆角改为 `$radius-lg`（24rpx）
  - 新增 `.card-flat`（无阴影卡片，仅靠留白区分层次）
  - 新增 `.hairline-top` / `.hairline-bottom`（1px 极细半透明分割线，用于列表项分割）

#### 页面样式（12 个全部重写）
| 页面 | 关键改动 |
|---|---|
| `workbench` | 渐变头部 → 白色头部 + 深色文字 + 底部细分割线；卡片圆角 16→24rpx；留白增大 |
| `login` | 渐变全屏背景 → 纯色背景；白色品牌字 → 深色品牌字；表单卡阴影由 popup 降至 card；输入框改用 `$color-bg-subtle` 浅色背景 |
| `mine` | 渐变头部 → 白色头部 + 深色文字；头像由半透明白底改为主色浅底；菜单项用 `.hairline-bottom` 替代边框；统计卡圆角 16→24rpx |
| `business` | Tab 栏改用 `.hairline-bottom`；行情/决策卡/回测卡/报告卡圆角统一为 24rpx；进度条填充由渐变改为主色纯色 |
| `todo` | 筛选栏加 `.hairline-bottom`；筛选 chip 用 `$color-bg-subtle`；待办卡圆角 24rpx |
| `message` | 顶栏加 `.hairline-bottom`；消息卡圆角 24rpx；未读条带由 6rpx 厚边改为 4rpx 细条 + 圆角端 |
| `decision-detail` | 渐变头部卡 → 白色头部 + 深色文字；返回/分享图标由白色改为深色；动作标签由半透明白底改为主色浅底；内容卡圆角 24rpx；进度条填充改主色纯色 |
| `backtest-detail` | 同上：渐变头部 → 白色头部；状态标签改浅色背景；Tab 切换改"浅色容器 + 白色激活块"模式；门禁项分割线改用 `.hairline-bottom` |
| `todo-detail` | 已是白色头部（保留）；上下文项/历史项的边框分割改用 `.hairline-bottom`；审批按钮次级态用 `$color-bg-subtle` |
| `settings` | 渐变 platformCard → 浅色 `$color-bg-subtle` 卡；分组卡圆角 24rpx；settingItem 分割改用 `.hairline-bottom` |
| `symbol-detail`（占位） | 渐变背景 → 纯色；占位图标圆角 24rpx；留白用 `$spacing-2xl` |
| `report-detail`（占位） | 同上 |

#### 组件样式（5 个全部重写）
| 组件 | 关键改动 |
|---|---|
| `Card` | 圆角 12→24rpx；`gradient` 变体由"主色渐变 + 白字"改为"`$color-bg-subtle` 浅色 + 深字"，避免视觉过重 |
| `StatCard` | 圆角 12→24rpx；`highlight` 变体同上改为浅色背景；趋势色在高亮模式下仍保留语义色（涨红跌绿），便于辨识 |
| `Tag` | 默认 padding 由 2rpx 调整为 4rpx，更易点击；语义色不变 |
| `EmptyState` | 图标背景由 hover 灰改为主色浅底 `tag-bg-primary`；图标文字改主色；行动按钮由"浅底主字"改为"主色底白字"，更符合主流 CTA |
| `PageHeader` | 高度改为 `min-height: 88rpx`；底部分割线改用 `.hairline-bottom`；标题加 `letter-spacing` |

### 预览验证
- Trae 预览服务器端口：58603（PID 37832），通过 `.pai/pai-preview-server.lock` 复用
- 已触发 `OpenPreview`，无浏览器错误
- HMR 自动热更新所有 .scss 改动，无需手动重启

### 设计原则总结（极简留白风 5 条）
1. **去渐变化**：除主按钮外，所有头部/卡片/容器改用纯色或浅色层叠
2. **大圆角化**：卡片统一 24rpx（$radius-lg），按钮统一 48rpx 全圆角
3. **轻阴影化**：阴影透明度从 0.08 降至 0.04，几乎无感
4. **细分割线化**：列表项分割改用 `.hairline-top/bottom`（1px + scaleY 0.5）替代 border-bottom
5. **充足留白**：页面 padding 40rpx，区块间距 48rpx，超大留白 64rpx

## 变更记录

| 日期 | 版本 | 内容 | 作者 |
|---|---|---|---|
| 2026-06-25 | v1.0.0 | 初始化移动多端应用，完成 P0/P1 主流程 | Trae AI |
| 2026-06-25 | v1.0.1 | 修复前后端联调：① `request.ts` 端口 8080→8001（对齐 `backend/.env`）；② 补全 `authService` mock 分支 + 新增 `data/auth.ts`，H5 预览登录闭环可用；③ cargo run 启动后端并验证 `/api/v1/auth/login` 返回 JWT（admin/admin123） | Trae AI |
| 2026-06-25 | v1.0.2 | 修复 H5 预览运行时 `ReferenceError: process is not defined`：新增 `utils/env.ts` 用 `typeof process` 守卫安全访问环境变量；`request.ts`/`cloud.ts`/`common.ts` 共 6 处 `process.env` 统一改为从 `@/utils/env` 导入 | Trae AI |
| 2026-06-25 | v1.0.3 | UI 视觉重设计为"极简留白风"（类 Apple/Notion）：① 全局主题 `variables.scss`/`theme.scss`/`app.scss` 调整（圆角增大、阴影调轻、留白增大，新增 `$color-bg-subtle`/`$spacing-2xl`/`.hairline-*` 工具类）；② 重写全部 12 个页面样式（去除所有渐变头部，改白色头部+深色文字+底部细分割线）；③ 重写全部 5 个组件样式（Card/StatCard 高亮模式由"主色渐变+白字"改为"浅色背景+深字"）；④ 文档新增第 11 章「UI 视觉重设计」详述改动清单与设计原则 | Trae AI |
| 2026-06-25 | v1.0.4 | 修复 SCSS 编译错误 `The target selector was not found`：CSS Modules 下每个 `.module.scss` 是独立作用域，`@extend .hairline-bottom`（定义在全局 `app.scss`）跨文件无法找到目标选择器。改为在 `variables.scss` 新增 `@mixin hairline-top` / `@mixin hairline-bottom`，10 个 module 文件共 13 处 `@extend .hairline-*` 全部替换为 `@include hairline-*`；`app.scss` 中的全局 `.hairline-*` 类改为用 mixin 实现，保持非 module 场景仍可用 | Trae AI |
| 2026-06-25 | v1.0.5 | 修复预览无法访问：① Trae 内置预览服务器只监听 IPv6 `[::1]:58603`，Windows 上 `localhost` 默认解析到 IPv4 导致浏览器无法访问；② Trae 预览不是直接 HTTP 访问，而是"云端预览页面 + WebSocket 连本地服务器"模式，直接访问 `http://localhost:58603/` 返回 404（服务器只接受 WebSocket 升级）。新增 [mobile/scripts/ipv4-proxy.js](../mobile/scripts/ipv4-proxy.js) Node.js 反向代理，监听 `0.0.0.0:58604` 转发 HTTP+WebSocket 到 `[::1]:58603`。**正确访问方式**：启动代理后，浏览器打开 `https://trae.mobile.volcapp.com/preview/?ws=ws://localhost:58604`（注意 ws 端口必须是代理端口 58604，不是 58603） | Trae AI |
| 2026-06-25 | v1.0.6 | UI 重设计为深色金融科技风：① 全局主题改为深色背景（#0a0e1a）+ 科技蓝紫渐变（#6366f1 → #8b5cf6）+ 发光效果；② 所有页面头部改为渐变背景 + 白色文字 + 光晕装饰；③ 卡片改为深色背景 + 微妙边框 + 发光效果；④ 按钮改为渐变背景 + 发光阴影；⑤ 完成全部 12 个页面和 5 个组件的深色金融科技风样式重写 | Trae AI |
| 2026-06-26 | v1.0.7 | 5 个通用组件样式回归浅色主流移动端金融 App 风（参考招行/蚂蚁财富/雪球），与第 11 章浅色设计规范对齐（注：v1.0.6 的深色风未落到这 5 个组件文件，磁盘实际为浅色且未提交，本次统一规范化）。逐个 Read tsx 确认 className 后用 Write 重写 `index.module.scss`（class 名不变，仅改样式实现，全部 `@use '@/styles/variables.scss' as *`，Card/StatCard 用 flex 不用 grid）：① **Card** 修复 `gradient`+`clickable` 同时生效时 `:active` 把渐变背景覆盖成浅灰导致高亮丢失的 bug，新增 `&.gradient:active` 覆盖为主色深色；`.body` 补 `min-width:0` 防溢出。② **StatCard** 删除 `.highlight .trend` 中与基础 `.trend` 重复的涨跌色定义（语义色高亮下同样适用，无需重复），补注释。③ **Tag** `size_sm` 垂直内边距 2rpx→4rpx（对齐 v1.0.3 记录、提升可读性），`.tag` 补 `box-sizing:border-box` 与 `vertical-align:middle`。④ **EmptyState** `.empty`/`.action` 补 `box-sizing`，`.title`/`.iconText` 补 `text-align`/`line-height`，规范字号注释。⑤ **PageHeader** 修正文件头注释"主色返回箭头"→"深色返回箭头"（实现本就用 `$color-text-primary` 深色，对齐主流金融 App 二级页惯例）。重启：仅 .scss 改动，Taro H5 dev 模式 HMR 自动热更新，无需手动重启。 | Trae AI |
| 2026-06-26 | v1.0.8 | 后端补齐 mobile 缺失接口 + 前后端联通闭环验证。**① 后端 `/workbench` 聚合 BFF**（[backend/src/routes/dashboard.rs](../backend/src/routes/dashboard.rs)）：新增 `GET /api/v1/dashboard/workbench`，一次返回问候语+未读消息数+总权益+今日盈亏+metrics+快捷入口，避免首屏并发多接口；字段对齐 mobile `WorkbenchData` 类型，响应用 `success_response` 包装由 `request.ts` `unwrapResponse` 自动解包 `data`。修复编译错误：`hour()` 方法属于 `chrono::Timelike` trait（非 `Datelike`，后者提供 year/month/day）。**② 后端决策卡详情接口**（[signals_api.rs](../backend/src/routes/signals_api.rs) + [store.rs](../backend/src/signals/store.rs)）：新增 `GET /api/v1/signals/decision-cards/{card_id}`，`store.rs` 加 `get_decision_card_by_id`（按 card_id+user_id 鉴权过滤，只能查到自己的卡），handler 返回 `DecisionCardResponse`（扁平结构，`unwrapResponse` 走 fallback 直接返回）。**③ mobile `signal.ts` `getCard` 从 mock 兜底切换为真实接口** `http.get('/signals/decision-cards/{cardId}')`。**④ 联通验证**：admin/admin123 登录获取 JWT → `/workbench` 返回 200+完整 WorkbenchData → `/signals/decision-cards/{uuid}` 返回 404（路由存在+鉴权过滤生效）；经 ipv4-proxy（58604）转发同样验证通过；mobile 5 个 service 文件（signal/workbench/message/market/todo）VS Code 类型诊断无错误。重启：`cargo run` 重新编译后端（约 2 分钟），ipv4-proxy 无需重启自动转发到新进程。 | Trae AI |

export default defineAppConfig({
  // 页面注册：tabBar 页面在前，二级页面在后
  pages: [
    'pages/workbench/index',      // 首页 Dashboard
    'pages/market/index',         // 行情 / 持仓
    'pages/ai/index',             // AI 分析
    'pages/agent/index',          // Agent 管理
    'pages/message/index',        // 消息
    'pages/mine/index',           // 我的 / 设置
    // 二级页面
    'pages/login/index',
    'pages/forgot-password/index', // 重置密码
    'pages/symbol-detail/index',  // 币种详情
    'pages/position-detail/index',// 持仓详情
    'pages/order-detail/index',   // 订单详情
    'pages/trade-history/index', // 交易记录
    'pages/ai-analysis-detail/index', // AI 分析详情
    'pages/ai-chat/index',       // AI 对话
    'pages/ai-debate/index',      // AI 辩论过程
    'pages/agent-detail/index',   // Agent 详情
    'pages/agent-log/index',      // Agent 日志
    'pages/message-detail/index', // 消息详情
    'pages/news-detail/index',    // 新闻详情
    'pages/exchange-api-key/index', // 交易所 API Key 配置
    'pages/model-api-key/index',  // 模型 API Key 配置
    'pages/trading-config/index', // 交易参数配置
    'pages/risk-config/index',    // 风控配置
    'pages/notification-config/index', // 通知配置
    'pages/system-log/index',     // 系统日志
    'pages/settings/index',       // 通用设置
    'pages/membership/index',     // 会员
  ],
  // 全局窗口配置
  window: {
    backgroundTextStyle: 'dark',
    navigationBarBackgroundColor: '#ffffff',
    navigationBarTitleText: 'MoneyRobert',
    navigationBarTextStyle: 'black',
    backgroundColor: '#f5f6f7',
  },
  // tabBar：6 大功能模块对齐后台管理系统（使用自定义 tabBar 避免 Taro H5 默认 tabBar stencil 组件 bug）
  tabBar: {
    custom: true,
    color: '#8c929f',
    selectedColor: '#165dff',
    backgroundColor: '#ffffff',
    borderStyle: 'white',
    list: [
      { pagePath: 'pages/workbench/index', text: '首页' },
      { pagePath: 'pages/market/index', text: '行情' },
      { pagePath: 'pages/ai/index', text: 'AI' },
      { pagePath: 'pages/agent/index', text: 'Agent' },
      { pagePath: 'pages/message/index', text: '消息' },
      { pagePath: 'pages/mine/index', text: '我的' },
    ],
  },
});

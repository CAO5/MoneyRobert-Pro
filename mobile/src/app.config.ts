export default defineAppConfig({
  // 页面注册：tabBar 页面在前，二级页面在后
  pages: [
    'pages/workbench/index',
    'pages/business/index',
    'pages/todo/index',
    'pages/message/index',
    'pages/mine/index',
    'pages/login/index',
    'pages/decision-detail/index',
    'pages/backtest-detail/index',
    'pages/symbol-detail/index',
    'pages/todo-detail/index',
    'pages/report-detail/index',
    'pages/membership/index',
    'pages/settings/index',
  ],
  // 全局窗口配置
  window: {
    backgroundTextStyle: 'dark',
    navigationBarBackgroundColor: '#ffffff',
    navigationBarTitleText: 'MoneyRobert',
    navigationBarTextStyle: 'black',
    backgroundColor: '#f5f6f7',
  },
  // 面向个人交易者的任务式导航：从看盘到判断、执行与复盘。
  tabBar: {
    color: '#8c929f',
    selectedColor: '#111827',
    backgroundColor: '#ffffff',
    borderStyle: 'white',
    list: [
      {
        pagePath: 'pages/workbench/index',
        text: '首页',
      },
      {
        pagePath: 'pages/business/index',
        text: '行情',
      },
      {
        pagePath: 'pages/todo/index',
        text: '策略',
      },
      {
        pagePath: 'pages/message/index',
        text: '消息',
      },
      {
        pagePath: 'pages/mine/index',
        text: '我的',
      },
    ],
  },
});

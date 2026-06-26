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
  // 底部导航：遵循深度研究报告建议的"工作台/业务/待办/消息/我的"五项
  // 仅显示文字（按 Skill 规范，模型无法生成 icon 文件）
  tabBar: {
    color: '#86909c',
    selectedColor: '#165dff',
    backgroundColor: '#ffffff',
    borderStyle: 'white',
    list: [
      {
        pagePath: 'pages/workbench/index',
        text: '工作台',
      },
      {
        pagePath: 'pages/business/index',
        text: '业务',
      },
      {
        pagePath: 'pages/todo/index',
        text: '待办',
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

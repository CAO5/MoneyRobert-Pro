import { createRouter, createWebHistory } from 'vue-router'
import { useAuthStore } from '@/stores/auth'

const router = createRouter({
  history: createWebHistory(),
  routes: [
    { path: '/login', name: 'login', component: () => import('@/pages/LoginPage.vue'), meta: { guest: true } },
    { path: '/register', name: 'register', component: () => import('@/pages/RegisterPage.vue'), meta: { guest: true } },
    { path: '/', component: () => import('@/layouts/DashboardLayout.vue'), meta: { auth: true }, children: [
      { path: '', redirect: '/dashboard' },
      { path: 'dashboard', name: 'dashboard', component: () => import('@/pages/DashboardPage.vue') },
      { path: 'market', name: 'market', component: () => import('@/pages/MarketPage.vue') },
      { path: 'ai', name: 'ai', component: () => import('@/pages/AiAnalysisPage.vue') },
      { path: 'ai/chat', name: 'ai-chat', component: () => import('@/pages/AiChatPage.vue') },
      { path: 'ai/predictions', name: 'ai-predictions', component: () => import('@/pages/AiPredictionsPage.vue') },
      { path: 'signals/decision-card', name: 'decision-card', component: () => import('@/pages/DecisionCardPage.vue') },
      { path: 'signals/calibration', name: 'calibration', component: () => import('@/pages/CalibrationPage.vue') },
      { path: 'signals/trade-recommendation', name: 'trade-recommendation', component: () => import('@/pages/TradeRecommendationPage.vue') },
      { path: 'signals/model-card', name: 'model-card', component: () => import('@/pages/ModelCardPage.vue') },
      { path: 'backtest', name: 'backtest', component: () => import('@/pages/BacktestPage.vue') },
      { path: 'backtest/attribution', name: 'attribution', component: () => import('@/pages/AttributionPage.vue') },
      { path: 'backtest/counterfactual', name: 'counterfactual', component: () => import('@/pages/CounterfactualPage.vue') },
      { path: 'backtest/strategy-failure', name: 'strategy-failure', component: () => import('@/pages/StrategyFailurePage.vue') },
      { path: 'backtest/walk-forward', name: 'walk-forward', component: () => import('@/pages/WalkForwardPage.vue') },
      { path: 'backtest/portfolio-risk', name: 'portfolio-risk', component: () => import('@/pages/PortfolioRiskPage.vue') },
      { path: 'backtest/position-sizing', name: 'position-sizing', component: () => import('@/pages/PositionSizingPage.vue') },
      { path: 'features/lineage', name: 'feature-lineage', component: () => import('@/pages/FeatureLineagePage.vue') },
      { path: 'microstructure', name: 'microstructure', component: () => import('@/pages/MicrostructurePage.vue') },
      { path: 'data-quality', name: 'data-quality', component: () => import('@/pages/DataQualityPage.vue') },
      { path: 'trading', name: 'trading', component: () => import('@/pages/TradingPage.vue') },
      { path: 'strategies', name: 'strategies', component: () => import('@/pages/StrategiesPage.vue') },
      { path: 'auto-trading', name: 'auto-trading', component: () => import('@/pages/AutoTradingPage.vue') },
      { path: 'paper-trading', name: 'paper-trading', component: () => import('@/pages/PaperTradingPage.vue') },
      { path: 'news', name: 'news', component: () => import('@/pages/NewsPage.vue') },
      { path: 'reports', name: 'reports', component: () => import('@/pages/ReportsPage.vue') },
      { path: 'notifications', name: 'notifications', component: () => import('@/pages/NotificationsPage.vue') },
      { path: 'settings', name: 'settings', component: () => import('@/pages/SettingsPage.vue') },
      { path: 'admin', name: 'admin', component: () => import('@/pages/AdminPage.vue'), meta: { admin: true } },
      { path: 'agent', name: 'agent', component: () => import('@/pages/AgentDashboardPage.vue') },
      { path: 'agent/debate', name: 'agent-debate', component: () => import('@/pages/AgentDebateViewer.vue') },
      { path: 'agent/history', name: 'agent-history', component: () => import('@/pages/AgentTradingHistory.vue') },
    ]},
  ],
})

router.beforeEach(async (to, from, next) => {
  const auth = useAuthStore()
  if (to.meta.auth && !auth.isAuthenticated) {
    return next('/login')
  }
  if (to.meta.guest && auth.isAuthenticated) {
    return next('/dashboard')
  }
  if (to.meta.admin && !auth.isAdmin) {
    return next('/dashboard')
  }
  // Only fetch user once on first authenticated navigation
  if (auth.isAuthenticated && !auth.user) {
    try {
      await auth.fetchUser()
    } catch {
      // fetchUser failure already handles logout
      if (!auth.isAuthenticated) {
        return next('/login')
      }
    }
  }
  next()
})

export default router

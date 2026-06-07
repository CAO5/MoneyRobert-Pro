<script setup lang="ts">
import { ref, computed, onMounted, onUnmounted } from 'vue'
import { useRouter } from 'vue-router'
import {
  TrendingUp, TrendingDown, Play, Square, Bot, Target, Activity,
  CheckCircle2, Clock, Shield, Zap, BarChart3,
  ChevronRight, Swords, ShieldCheck, Sparkles, Award, Rocket, Lock,
  AlertTriangle, Power, RefreshCw
} from 'lucide-vue-next'
import api from '@/api'

const router = useRouter()

// ============ Types ============
interface LevelInfo {
  current_level: number
  current_level_name: string
  current_mode: string
  next_level: number | null
  next_level_name: string | null
  next_mode: string | null
  progress_percent: number
  experience_points: number
  next_level_points: number
}

interface LevelRequirements {
  next_level: number
  min_trades: number
  min_win_rate: number
  min_profit_loss_ratio: number
  min_running_days: number
  max_drawdown_percent: number
  max_consecutive_losses: number
  current_trades: number
  current_win_rate: number
  current_profit_loss_ratio: number
  current_running_days: number
  current_drawdown_percent: number
  current_consecutive_losses: number
}

interface SimulationConfig {
  id: string
  symbol: string
  mode: string
  level: number
  status: string
  initial_balance: number
  current_balance: number
  total_trades: number
  winning_trades: number
  losing_trades: number
  win_rate: number
  avg_pnl_percent: number
  profit_loss_ratio: number
  max_drawdown_percent: number
  sharpe_ratio: number
  weekly_pnl: number
  daily_pnl: number
  daily_loss_percent: number
  weekly_loss_percent: number
  consecutive_stop_losses: number
  running_days: number
  max_position_size_percent: number
  max_leverage: number
  max_daily_trades: number
  ai_confidence_threshold: number
  analysis_interval: string
  autonomous_mode_enabled: boolean
  requires_manual_confirm: boolean
  promotion_eligible: boolean
  risk_confirmation_signed: boolean
  last_trade_at: string | null
  created_at: string
}

interface Trade {
  id: string
  symbol: string
  direction: string
  entry_price: number
  exit_price: number | null
  leverage: number
  pnl: number | null
  pnl_percent: number | null
  ai_confidence: number | null
  ai_reasoning: Record<string, any> | null
  agent_session_id: string | null
  status: string
  close_reason: string | null
  opened_at: string
  closed_at: string | null
}

interface PromotionEligibility {
  eligible: boolean
  current_level: number
  next_level: number | null
  missing_requirements: string[]
}

interface DebateSession {
  id: string
  symbol: string
  status: string
  created_at: string
}

interface PositionWithPnl {
  trade: Trade
  current_price: number | null
  unrealized_pnl: number | null
  unrealized_pnl_percent: number | null
}

interface DashboardData {
  has_config: boolean
  config: SimulationConfig | null
  level_info: LevelInfo | null
  open_positions: PositionWithPnl[]
  closed_trades: Trade[]
  promotion_eligibility: PromotionEligibility | null
  level_requirements: LevelRequirements | null
  recent_debate_sessions: DebateSession[]
  risk_confirmation_signed: boolean
  reflections: ReflectionItem[]
  agent_credibility: Record<string, number>
  total_unrealized_pnl: number
  equity: number
}

// Market Context Interface
interface MarketContext {
  trend: string
  trend_strength: number
  volatility: string
  volume_profile: string
  key_levels: number[]
}

// Multi-Timeframe Data Interface
interface MultiTimeframeData {
  m5_trend: string
  m15_trend: string
  h1_trend: string
  h4_trend: string
  d1_trend: string
  alignment: number
  alignment_details: string
}

// Reflection Item Interface
interface ReflectionItem {
  type: string
  message: string
}

// Agent Performance Interface
interface AgentPerformance {
  agent_name: string
  agent_department: string
  accuracy: number
  credibility_score: number
  trend_accuracy: number
  volatility_accuracy: number
  volume_accuracy: number
  timing_accuracy: number
  weighted_accuracy: number
  total_analyses: number
}

// Extended Dashboard Data with market info
interface EnhancedDashboardData extends DashboardData {
  market_context: MarketContext | null
  multi_timeframe: MultiTimeframeData | null
  agent_performance: AgentPerformance[]
}

// ============ State ============
const dashboard = ref<DashboardData | null>(null)
const marketContext = ref<MarketContext | null>(null)
const multiTimeframe = ref<MultiTimeframeData | null>(null)
const agentPerformance = ref<AgentPerformance[]>([])
const loading = ref(true)
const actionLoading = ref<string | null>(null)
const showStartModal = ref(false)
const startSymbol = ref('BTC-USDT-SWAP')
const startBalance = ref(10000)
const startInterval = ref('1H')
const showRiskModal = ref(false)
const riskMaxLoss = ref(500)
const pollTimer = ref<ReturnType<typeof setInterval> | null>(null)

// Symbol & Interval selectors
const symbols = [
  { value: 'BTC-USDT-SWAP', label: 'BTC/USDT', icon: '₿' },
  { value: 'ETH-USDT-SWAP', label: 'ETH/USDT', icon: 'Ξ' },
  { value: 'SOL-USDT-SWAP', label: 'SOL/USDT', icon: '◎' },
  { value: 'DOGE-USDT-SWAP', label: 'DOGE/USDT', icon: 'Ð' },
  { value: 'XRP-USDT-SWAP', label: 'XRP/USDT', icon: '✕' },
  { value: 'ADA-USDT-SWAP', label: 'ADA/USDT', icon: '₳' },
  { value: 'AVAX-USDT-SWAP', label: 'AVAX/USDT', icon: 'A' },
  { value: 'LINK-USDT-SWAP', label: 'LINK/USDT', icon: '⬡' },
]

const intervals = [
  { value: '5m', label: '5分钟' },
  { value: '15m', label: '15分钟' },
  { value: '30m', label: '30分钟' },
  { value: '1H', label: '1小时' },
  { value: '4H', label: '4小时' },
  { value: '1D', label: '1天' },
]

const selectedSymbol = ref('BTC-USDT-SWAP')
const selectedInterval = ref('1H')

// ============ Computed ============
const levelTheme = computed(() => {
  const level = dashboard.value?.level_info?.current_level ?? 0
  const themes = [
    { color: '#64748B', bg: '#F1F5F9', label: 'Lv.0', icon: Sparkles },
    { color: '#2563EB', bg: '#EFF6FF', label: 'Lv.1', icon: Zap },
    { color: '#D97706', bg: '#FFFBEB', label: 'Lv.2', icon: Shield },
    { color: '#7C3AED', bg: '#F5F3FF', label: 'Lv.3', icon: Award },
  ]
  return themes[level] || themes[3]
})

const balanceChange = computed(() => {
  if (!dashboard.value?.config) return { percent: 0 }
  const cfg = dashboard.value.config
  const equity = dashboard.value.equity || cfg.current_balance
  return { percent: ((equity - cfg.initial_balance) / cfg.initial_balance) * 100 }
})

const requirementItems = computed(() => {
  const req = dashboard.value?.level_requirements
  if (!req) return []
  return [
    { label: '交易次数', current: req.current_trades, target: req.min_trades, met: req.current_trades >= req.min_trades, fmt: (v: number) => `${v}` },
    { label: '胜率', current: req.current_win_rate * 100, target: req.min_win_rate * 100, met: req.current_win_rate >= req.min_win_rate, fmt: (v: number) => `${v.toFixed(1)}%` },
    { label: '盈亏比', current: req.current_profit_loss_ratio, target: req.min_profit_loss_ratio, met: req.current_profit_loss_ratio >= req.min_profit_loss_ratio, fmt: (v: number) => v.toFixed(2) },
    { label: '运行天数', current: req.current_running_days, target: req.min_running_days, met: req.current_running_days >= req.min_running_days, fmt: (v: number) => `${v}` },
    { label: '最大回撤', current: req.current_drawdown_percent, target: req.max_drawdown_percent, met: req.current_drawdown_percent <= req.max_drawdown_percent, fmt: (v: number) => `${v.toFixed(1)}%`, inverse: true },
    { label: '连续亏损', current: req.current_consecutive_losses, target: req.max_consecutive_losses, met: req.current_consecutive_losses <= req.max_consecutive_losses, fmt: (v: number) => `${v}`, inverse: true },
  ]
})

const capabilities = computed(() => {
  const level = dashboard.value?.level_info?.current_level ?? 0
  return [
    { name: '模拟交易', desc: '虚拟资金验证', unlocked: level >= 0 },
    { name: 'AI辩论', desc: '多Agent决策', unlocked: level >= 0 },
    { name: 'Demo盘', desc: 'OKX Demo', unlocked: level >= 1 },
    { name: '自主交易', desc: '自动执行', unlocked: level >= 2 },
    { name: '实盘', desc: '真实资金', unlocked: level >= 3 },
  ]
})

const isRunning = computed(() => dashboard.value?.config?.status === 'running')
const isAutonomous = computed(() => dashboard.value?.config?.autonomous_mode_enabled === true)
const currentLevel = computed(() => dashboard.value?.level_info?.current_level ?? 0)

// ============ Methods ============
async function loadDashboard() {
  loading.value = true
  try {
    const res = await api.get('/agent/dashboard')
    dashboard.value = res.data
    // Sync selected values from config
    if (dashboard.value?.config) {
      selectedSymbol.value = dashboard.value.config.symbol
      selectedInterval.value = dashboard.value.config.analysis_interval || '1H'
    }
    // Load market context and agent performance
    await Promise.all([
      loadMarketContext(),
      loadAgentPerformance(),
    ])
  } catch (e: any) {
    if (e?.response?.status === 404) {
      dashboard.value = { has_config: false, config: null, level_info: null, open_positions: [], closed_trades: [], promotion_eligibility: null, level_requirements: null, recent_debate_sessions: [], risk_confirmation_signed: false, reflections: [], agent_credibility: {}, total_unrealized_pnl: 0, equity: 0 }
    } else {
      console.error('Failed to load dashboard:', e)
    }
  } finally {
    loading.value = false
  }
}

async function loadMarketContext() {
  if (!dashboard.value?.config) {
    marketContext.value = null
    multiTimeframe.value = null
    return
  }
  try {
    // Get current market context from OKX
    const symbol = dashboard.value.config.symbol
    const res = await api.get(`/market/candles/${symbol}?interval=${selectedInterval.value}&limit=50`)
    if (res.data?.candles) {
      // Analyze market context from candles
      const candles = res.data.candles
      if (candles.length >= 20) {
        const closes = candles.map((c: any) => parseFloat(c.close)).slice(-20)
        const ma5 = closes.slice(-5).reduce((a: number, b: number) => a + b, 0) / 5
        const ma20 = closes.reduce((a: number, b: number) => a + b, 0) / closes.length
        const trend = ma5 > ma20 * 1.02 ? 'bull' : ma5 < ma20 * 0.98 ? 'bear' : 'range'
        const trend_strength = Math.min(Math.abs(ma5 - ma20) / ma20 * 10, 1.0)

        // Calculate volatility
        const recentCandles = candles.slice(-14)
        const atr = recentCandles.reduce((sum: number, c: any) => {
          const h = parseFloat(c.high)
          const l = parseFloat(c.low)
          const o = parseFloat(c.open)
          const close = parseFloat(c.close)
          return sum + Math.max(h - l, Math.abs(close - o))
        }, 0) / 14
        const currentPrice = parseFloat(candles[candles.length - 1]?.close || closes[closes.length - 1])
        const volatilityRatio = atr / currentPrice
        const volatility = volatilityRatio < 0.01 ? 'low' : volatilityRatio < 0.03 ? 'medium' : 'high'

        // Volume profile
        const recentVols = candles.slice(-10).map((c: any) => parseFloat(c.volume))
        const avgVol = recentVols.reduce((a: number, b: number) => a + b, 0) / recentVols.length
        const volumeProfile = recentVols[0] > avgVol * 1.3 ? 'increasing' : recentVols[0] < avgVol * 0.7 ? 'decreasing' : 'stable'

        // Key levels
        const highs = candles.map((c: any) => parseFloat(c.high))
        const lows = candles.map((c: any) => parseFloat(c.low))
        const keyLevels = [Math.max(...lows.slice(-50)), (Math.max(...highs.slice(-50)) + Math.min(...lows.slice(-50))) / 2, Math.max(...highs.slice(-50))]

        marketContext.value = {
          trend,
          trend_strength,
          volatility,
          volume_profile: volumeProfile,
          key_levels: keyLevels,
        }

        // Simplified multi-timeframe (using same data for now, would need separate API calls)
        multiTimeframe.value = {
          m5_trend: trend,
          m15_trend: trend,
          h1_trend: trend,
          h4_trend: trend,
          d1_trend: trend,
          alignment: 0.8,
          alignment_details: '多周期一致',
        }
      }
    }
  } catch (e) {
    console.error('Failed to load market context:', e)
  }
}

async function loadAgentPerformance() {
  if (!dashboard.value?.config) {
    agentPerformance.value = []
    return
  }
  try {
    // Get agent performance from backend
    const res = await api.get('/agent/performance')
    if (res.data?.agents) {
      agentPerformance.value = res.data.agents
    }
  } catch (e) {
    console.error('Failed to load agent performance:', e)
    // Fallback to credibility from dashboard
    if (dashboard.value?.agent_credibility) {
      agentPerformance.value = Object.entries(dashboard.value.agent_credibility).map(([name, credibility]) => ({
        agent_name: name,
        agent_department: '',
        accuracy: credibility,
        credibility_score: credibility,
        trend_accuracy: credibility,
        volatility_accuracy: credibility,
        volume_accuracy: credibility,
        timing_accuracy: credibility,
        weighted_accuracy: credibility,
        total_analyses: 0,
      }))
    }
  }
}

async function changeSymbol(sym: string) {
  if (!dashboard.value?.config || sym === selectedSymbol.value) return
  selectedSymbol.value = sym
  actionLoading.value = 'symbol'
  try {
    await api.post('/agent/simulation/config', { symbol: sym })
    await loadDashboard()
  } catch (e) { console.error('Failed to change symbol:', e) }
  finally { actionLoading.value = null }
}

async function changeInterval(interval: string) {
  if (!dashboard.value?.config || interval === selectedInterval.value) return
  selectedInterval.value = interval
  actionLoading.value = 'interval'
  try {
    await api.post('/agent/simulation/config', { interval })
    await loadDashboard()
  } catch (e) { console.error('Failed to change interval:', e) }
  finally { actionLoading.value = null }
}

async function createAndStartSimulation() {
  actionLoading.value = 'start'
  try {
    await api.post('/agent/simulation/start', { symbol: startSymbol.value, initial_balance: startBalance.value, interval: startInterval.value })
    showStartModal.value = false
    await loadDashboard()
  } catch (e) { console.error('Failed to start simulation:', e) }
  finally { actionLoading.value = null }
}

async function toggleSimulation() {
  if (!dashboard.value?.config) return
  actionLoading.value = 'simulation'
  try {
    if (isRunning.value) {
      await api.post('/agent/simulation/stop', { config_id: dashboard.value.config.id })
    } else {
      await api.post('/agent/simulation/start', { symbol: dashboard.value.config.symbol, initial_balance: dashboard.value.config.initial_balance })
    }
    await loadDashboard()
  } catch (e) { console.error('Failed to toggle simulation:', e) }
  finally { actionLoading.value = null }
}

async function toggleAutonomous() {
  if (!dashboard.value?.config) return
  actionLoading.value = 'autonomous'
  try {
    if (isAutonomous.value) {
      await api.post('/agent/autonomous/stop', { config_id: dashboard.value.config.id })
    } else {
      if (!dashboard.value.config.risk_confirmation_signed) { showRiskModal.value = true; actionLoading.value = null; return }
      await api.post('/agent/autonomous/start', { config_id: dashboard.value.config.id })
    }
    await loadDashboard()
  } catch (e) { console.error('Failed to toggle autonomous:', e) }
  finally { actionLoading.value = null }
}

async function signRiskConfirmation() {
  if (!dashboard.value?.config) return
  actionLoading.value = 'risk'
  try {
    await api.post('/agent/risk/confirmation/sign', { config_id: dashboard.value.config.id, version: '1.0', max_acceptable_loss: riskMaxLoss.value })
    showRiskModal.value = false
    await loadDashboard()
  } catch (e) { console.error('Failed to sign risk confirmation:', e) }
  finally { actionLoading.value = null }
}

async function handleEmergencyStop() {
  if (!dashboard.value?.config) return
  actionLoading.value = 'emergency'
  try {
    await api.post('/agent/emergency/stop', { config_id: dashboard.value.config.id })
    await loadDashboard()
  } catch (e) { console.error('Failed to emergency stop:', e) }
  finally { actionLoading.value = null }
}

async function handleInitiatePromotion() {
  actionLoading.value = 'promotion'
  try {
    await api.post('/agent/promotion/initiate')
    await loadDashboard()
  } catch (e: any) {
    alert(e?.response?.data?.message || e?.message || '晋级申请失败')
  } finally { actionLoading.value = null }
}

function fmtTime(t: string) { return new Date(t).toLocaleString('zh-CN', { month: '2-digit', day: '2-digit', hour: '2-digit', minute: '2-digit' }) }
function fmtPnl(v: number | null) { if (v == null) return '--'; return `${v >= 0 ? '+' : ''}${v.toFixed(2)}%` }
function fmtPrice(v: number | null | undefined) { if (v == null) return '--'; return v.toFixed(6) }
function fmtPercent(v: number | null | undefined, decimals: number = 1) { if (v == null) return '--'; return `${(v * 100).toFixed(decimals)}%` }
function fmtTrend(trend: string) {
  const map: Record<string, { label: string, color: string }> = {
    'bull': { label: '多头', color: '#10B981' },
    'bear': { label: '空头', color: '#EF4444' },
    'range': { label: '震荡', color: '#F59E0B' },
    'unknown': { label: '未知', color: '#6B7280' },
  }
  return map[trend] || { label: trend, color: '#6B7280' }
}
function fmtVolatility(volatility: string) {
  const map: Record<string, { label: string, color: string }> = {
    'high': { label: '高波动', color: '#EF4444' },
    'medium': { label: '中等', color: '#F59E0B' },
    'low': { label: '低波动', color: '#10B981' },
  }
  return map[volatility] || { label: volatility, color: '#6B7280' }
}
function fmtVolumeProfile(profile: string) {
  const map: Record<string, { label: string, color: string }> = {
    'increasing': { label: '放量', color: '#10B981' },
    'decreasing': { label: '缩量', color: '#EF4444' },
    'stable': { label: '稳定', color: '#6B7280' },
  }
  return map[profile] || { label: profile, color: '#6B7280' }
}
function getAccuracyColor(accuracy: number) {
  if (accuracy >= 0.7) return '#10B981'
  if (accuracy >= 0.5) return '#F59E0B'
  return '#EF4444'
}
function getAlignmentColor(alignment: number) {
  if (alignment >= 0.8) return '#10B981'
  if (alignment >= 0.6) return '#F59E0B'
  return '#EF4444'
}

onMounted(() => { loadDashboard(); pollTimer.value = setInterval(() => loadDashboard(), 30000) })
onUnmounted(() => { if (pollTimer.value) clearInterval(pollTimer.value) })
</script>

<template>
  <div class="space-y-5">
    <!-- Header -->
    <div class="flex items-center justify-between">
      <div>
        <h1 class="text-xl font-bold" style="color: var(--text-primary)">Agent 仪表盘</h1>
        <p class="text-sm mt-0.5" style="color: var(--text-secondary)">AI 交易 Agent 管理</p>
      </div>
      <button v-if="isRunning" @click="handleEmergencyStop" :disabled="actionLoading === 'emergency'"
        class="px-3 py-1.5 rounded-lg text-sm font-medium flex items-center gap-1.5 border"
        style="border-color: var(--loss); color: var(--loss); background: var(--loss-light)">
        <Square class="w-3.5 h-3.5" /> 紧急停止
      </button>
    </div>

    <!-- Loading -->
    <div v-if="loading && !dashboard" class="grid grid-cols-4 gap-4">
      <div v-for="i in 4" :key="i" class="card animate-pulse h-24"></div>
    </div>

    <!-- No Config -->
    <div v-else-if="dashboard && !dashboard.has_config" class="card p-10 text-center max-w-lg mx-auto">
      <div class="w-16 h-16 rounded-2xl mx-auto mb-5 flex items-center justify-center" style="background: var(--primary-bg)">
        <Bot class="w-8 h-8" style="color: var(--primary)" />
      </div>
      <h2 class="text-lg font-bold mb-2" style="color: var(--text-primary)">启动 AI 交易 Agent</h2>
      <p class="text-sm mb-6" style="color: var(--text-secondary)">
        从模拟交易开始，逐步验证策略准确性，晋级解锁更强交易能力
      </p>
      <div class="flex items-center justify-center gap-2 mb-6">
        <div v-for="(lv, i) in ['实习→模拟', '初级→Demo', '资深→自主', '经理→实盘']" :key="i"
          class="px-2.5 py-1 rounded-full text-xs" :style="{ background: i === 0 ? 'var(--primary-bg)' : 'var(--surface-secondary)', color: i === 0 ? 'var(--primary)' : 'var(--text-muted)' }">
          {{ lv }}
        </div>
      </div>
      <button @click="showStartModal = true" class="btn-primary inline-flex items-center gap-2">
        <Rocket class="w-4 h-4" /> 开始模拟交易
      </button>
    </div>

    <!-- Main Dashboard -->
    <template v-else-if="dashboard?.has_config">
      <!-- Symbol & Interval Selector Bar -->
      <div class="card !p-3">
        <div class="flex items-center gap-3 flex-wrap">
          <!-- Symbol Selector -->
          <div class="flex items-center gap-1">
            <span class="text-xs font-medium shrink-0" style="color: var(--text-muted)">币种</span>
            <div class="flex items-center gap-1">
              <button v-for="sym in symbols" :key="sym.value"
                @click="changeSymbol(sym.value)"
                :disabled="actionLoading === 'symbol'"
                class="px-2 py-1 rounded-md text-xs font-medium transition-all"
                :style="{
                  background: selectedSymbol === sym.value ? 'var(--primary-bg)' : 'var(--surface-secondary)',
                  color: selectedSymbol === sym.value ? 'var(--primary)' : 'var(--text-secondary)',
                  boxShadow: selectedSymbol === sym.value ? '0 0 0 1px var(--primary)' : 'none'
                }">
                <span class="mr-0.5">{{ sym.icon }}</span> {{ sym.label.split('/')[0] }}
              </button>
            </div>
          </div>

          <div class="w-px h-5" style="background: var(--border)"></div>

          <!-- Interval Selector -->
          <div class="flex items-center gap-1">
            <span class="text-xs font-medium shrink-0" style="color: var(--text-muted)">周期</span>
            <div class="flex items-center gap-1">
              <button v-for="iv in intervals" :key="iv.value"
                @click="changeInterval(iv.value)"
                :disabled="actionLoading === 'interval'"
                class="px-2 py-1 rounded-md text-xs font-medium transition-all"
                :style="{
                  background: selectedInterval === iv.value ? 'var(--primary-bg)' : 'var(--surface-secondary)',
                  color: selectedInterval === iv.value ? 'var(--primary)' : 'var(--text-secondary)',
                  boxShadow: selectedInterval === iv.value ? '0 0 0 1px var(--primary)' : 'none'
                }">
                {{ iv.label }}
              </button>
            </div>
          </div>

          <div class="flex-1"></div>

          <!-- Refresh -->
          <button @click="loadDashboard()" :disabled="loading"
            class="p-1.5 rounded-md transition-colors"
            style="background: var(--surface-secondary); color: var(--text-muted)">
            <RefreshCw class="w-3.5 h-3.5" :class="{ 'animate-spin': loading }" />
          </button>
        </div>
      </div>

      <!-- Row 1: Stats -->
      <div class="grid grid-cols-2 lg:grid-cols-5 gap-4">
        <div class="card !p-4">
          <div class="text-xs mb-1" style="color: var(--text-secondary)">账户净值</div>
          <div class="text-lg font-bold" style="color: var(--text-primary)">
            ${{ dashboard.equity.toLocaleString(undefined, { maximumFractionDigits: 2 }) }}
          </div>
          <div class="text-xs" :style="{ color: balanceChange.percent >= 0 ? 'var(--profit)' : 'var(--loss)' }">
            {{ balanceChange.percent >= 0 ? '+' : '' }}{{ balanceChange.percent.toFixed(2) }}%
          </div>
        </div>
        <div class="card !p-4">
          <div class="text-xs mb-1" style="color: var(--text-secondary)">浮动盈亏</div>
          <div class="text-lg font-bold" :style="{ color: dashboard.total_unrealized_pnl >= 0 ? 'var(--profit)' : 'var(--loss)' }">
            {{ dashboard.total_unrealized_pnl >= 0 ? '+' : '' }}${{ dashboard.total_unrealized_pnl.toFixed(2) }}
          </div>
          <div class="text-xs" style="color: var(--text-muted)">
            余额 ${{ dashboard.config!.current_balance.toLocaleString(undefined, { maximumFractionDigits: 2 }) }}
          </div>
        </div>
        <div class="card !p-4">
          <div class="text-xs mb-1" style="color: var(--text-secondary)">胜率</div>
          <div class="text-lg font-bold" :style="{ color: dashboard.config!.win_rate >= 0.5 ? 'var(--profit)' : 'var(--loss)' }">
            {{ (dashboard.config!.win_rate * 100).toFixed(1) }}%
          </div>
          <div class="text-xs" style="color: var(--text-muted)">{{ dashboard.config!.winning_trades }}W / {{ dashboard.config!.losing_trades }}L</div>
        </div>
        <div class="card !p-4">
          <div class="text-xs mb-1" style="color: var(--text-secondary)">总交易</div>
          <div class="text-lg font-bold" style="color: var(--text-primary)">{{ dashboard.config!.total_trades }}</div>
          <div class="text-xs" style="color: var(--text-muted)">运行 {{ dashboard.config!.running_days }} 天</div>
        </div>
        <div class="card !p-4">
          <div class="text-xs mb-1" style="color: var(--text-secondary)">最大回撤</div>
          <div class="text-lg font-bold" :style="{ color: dashboard.config!.max_drawdown_percent > 10 ? 'var(--loss)' : 'var(--text-primary)' }">
            {{ dashboard.config!.max_drawdown_percent.toFixed(1) }}%
          </div>
          <div class="text-xs" style="color: var(--text-muted)">盈亏比 {{ dashboard.config!.profit_loss_ratio.toFixed(2) }}</div>
        </div>
      </div>

      <!-- Market Context & Agent Performance -->
      <div v-if="marketContext || agentPerformance.length > 0" class="grid grid-cols-1 lg:grid-cols-2 gap-4">
        <!-- Market Context Card -->
        <div v-if="marketContext" class="card !p-4">
          <div class="flex items-center justify-between mb-3">
            <div class="text-sm font-medium" style="color: var(--text-primary)">市场环境分析</div>
            <div class="text-xs" style="color: var(--text-muted)">实时分析</div>
          </div>
          <div class="space-y-3">
            <!-- Trend -->
            <div class="flex items-center justify-between">
              <div class="text-xs" style="color: var(--text-secondary)">趋势方向</div>
              <div class="flex items-center gap-2">
                <div class="text-sm font-bold" :style="{ color: fmtTrend(marketContext.trend).color }">
                  {{ fmtTrend(marketContext.trend).label }}
                </div>
                <div class="text-xs" style="color: var(--text-muted)">
                  强度 {{ (marketContext.trend_strength * 100).toFixed(0) }}%
                </div>
              </div>
            </div>
            <!-- Volatility -->
            <div class="flex items-center justify-between">
              <div class="text-xs" style="color: var(--text-secondary)">波动性</div>
              <div class="text-sm font-bold" :style="{ color: fmtVolatility(marketContext.volatility).color }">
                {{ fmtVolatility(marketContext.volatility).label }}
              </div>
            </div>
            <!-- Volume -->
            <div class="flex items-center justify-between">
              <div class="text-xs" style="color: var(--text-secondary)">成交量</div>
              <div class="text-sm font-bold" :style="{ color: fmtVolumeProfile(marketContext.volume_profile).color }">
                {{ fmtVolumeProfile(marketContext.volume_profile).label }}
              </div>
            </div>
            <!-- Multi-Timeframe Alignment -->
            <div v-if="multiTimeframe" class="pt-2 border-t" style="border-color: var(--border)">
              <div class="flex items-center justify-between mb-2">
                <div class="text-xs" style="color: var(--text-secondary)">多周期一致性</div>
                <div class="text-sm font-bold" :style="{ color: getAlignmentColor(multiTimeframe.alignment) }">
                  {{ (multiTimeframe.alignment * 100).toFixed(0) }}% {{ multiTimeframe.alignment_details }}
                </div>
              </div>
              <div class="flex gap-2 text-xs">
                <span class="px-1.5 py-0.5 rounded" style="background: var(--surface-secondary)">
                  5m: <span :style="{ color: fmtTrend(multiTimeframe.m5_trend).color }">{{ fmtTrend(multiTimeframe.m5_trend).label }}</span>
                </span>
                <span class="px-1.5 py-0.5 rounded" style="background: var(--surface-secondary)">
                  1H: <span :style="{ color: fmtTrend(multiTimeframe.h1_trend).color }">{{ fmtTrend(multiTimeframe.h1_trend).label }}</span>
                </span>
                <span class="px-1.5 py-0.5 rounded" style="background: var(--surface-secondary)">
                  4H: <span :style="{ color: fmtTrend(multiTimeframe.h4_trend).color }">{{ fmtTrend(multiTimeframe.h4_trend).label }}</span>
                </span>
              </div>
            </div>
          </div>
        </div>

        <!-- Agent Performance Card -->
        <div v-if="agentPerformance.length > 0" class="card !p-4">
          <div class="flex items-center justify-between mb-3">
            <div class="text-sm font-medium" style="color: var(--text-primary)">AI Agent 准确率</div>
            <div class="text-xs" style="color: var(--text-muted)">综合评分</div>
          </div>
          <div class="space-y-2 max-h-48 overflow-y-auto">
            <div v-for="agent in agentPerformance.slice(0, 6)" :key="agent.agent_name"
              class="flex items-center justify-between p-2 rounded-lg" style="background: var(--surface-secondary)">
              <div class="flex-1">
                <div class="text-xs font-medium" style="color: var(--text-primary)">{{ agent.agent_name }}</div>
                <div class="text-xs" style="color: var(--text-muted)">
                  {{ agent.total_analyses }} 次分析
                </div>
              </div>
              <div class="text-right">
                <div class="text-sm font-bold" :style="{ color: getAccuracyColor(agent.accuracy) }">
                  {{ (agent.accuracy * 100).toFixed(1) }}%
                </div>
                <div class="flex gap-1 text-xs mt-0.5">
                  <span class="px-1 py-0.5 rounded" :style="{ background: 'var(--primary-bg)', color: 'var(--primary)' }" title="可信度">
                    信 {{ (agent.credibility_score * 100).toFixed(0) }}%
                  </span>
                  <span v-if="agent.trend_accuracy" class="px-1 py-0.5 rounded" style="background: var(--surface); color: var(--text-muted)" title="趋势准确率">
                    趋 {{ (agent.trend_accuracy * 100).toFixed(0) }}%
                  </span>
                </div>
              </div>
            </div>
          </div>
        </div>
      </div>

      <!-- Row 2: Level + Controls -->
      <div class="grid grid-cols-1 lg:grid-cols-3 gap-4">
        <!-- Level & Progress -->
        <div class="lg:col-span-2 card !p-5">
          <div class="flex items-center gap-3 mb-4">
            <div class="w-10 h-10 rounded-xl flex items-center justify-center" :style="{ background: levelTheme.bg }">
              <component :is="levelTheme.icon" class="w-5 h-5" :style="{ color: levelTheme.color }" />
            </div>
            <div class="flex-1">
              <div class="flex items-center gap-2">
                <span class="text-xs font-bold px-1.5 py-0.5 rounded" :style="{ background: levelTheme.bg, color: levelTheme.color }">{{ levelTheme.label }}</span>
                <span class="font-semibold" style="color: var(--text-primary)">{{ dashboard.level_info?.current_level_name }}</span>
                <span class="text-xs px-1.5 py-0.5 rounded-full" style="background: var(--surface-secondary); color: var(--text-muted)">{{ dashboard.level_info?.current_mode }}</span>
              </div>
              <div class="text-xs mt-0.5" style="color: var(--text-secondary)">
                {{ dashboard.level_info?.next_level ? `下一级: ${dashboard.level_info.next_level_name} (${dashboard.level_info.next_mode})` : '已满级' }}
              </div>
            </div>
            <button v-if="dashboard.promotion_eligibility?.eligible" @click="handleInitiatePromotion" :disabled="actionLoading === 'promotion'" class="btn-primary text-xs !py-1.5 !px-3">
              申请晋级
            </button>
          </div>

          <!-- Progress -->
          <div class="mb-4">
            <div class="flex justify-between text-xs mb-1">
              <span style="color: var(--text-muted)">升级进度</span>
              <span style="color: var(--primary)">{{ dashboard.level_info?.progress_percent.toFixed(0) }}%</span>
            </div>
            <div class="h-2 rounded-full overflow-hidden" style="background: var(--border)">
              <div class="h-full rounded-full transition-all duration-700" :style="{ background: levelTheme.color, width: `${dashboard.level_info?.progress_percent || 0}%` }"></div>
            </div>
          </div>

          <!-- Requirements -->
          <div v-if="requirementItems.length > 0" class="grid grid-cols-3 gap-2">
            <div v-for="item in requirementItems" :key="item.label" class="p-2 rounded-lg" style="background: var(--surface-secondary)">
              <div class="flex items-center gap-1 mb-0.5">
                <CheckCircle2 v-if="item.met" class="w-3 h-3" style="color: var(--profit)" />
                <div v-else class="w-3 h-3 rounded-full border" style="border-color: var(--text-muted)"></div>
                <span class="text-xs" style="color: var(--text-secondary)">{{ item.label }}</span>
              </div>
              <div class="text-xs font-semibold" :style="{ color: item.met ? 'var(--profit)' : 'var(--text-primary)' }">
                {{ item.fmt(item.current) }} <span style="color: var(--text-muted); font-weight: 400">/ {{ item.fmt(item.target) }}</span>
              </div>
            </div>
          </div>

          <!-- Capabilities -->
          <div class="flex items-center gap-2 mt-4 pt-4" style="border-top: 1px solid var(--border)">
            <span class="text-xs shrink-0" style="color: var(--text-muted)">能力</span>
            <div class="flex items-center gap-1.5 flex-wrap">
              <span v-for="cap in capabilities" :key="cap.name"
                class="text-xs px-2 py-0.5 rounded-full"
                :style="{ background: cap.unlocked ? 'var(--primary-bg)' : 'var(--surface-secondary)', color: cap.unlocked ? 'var(--primary)' : 'var(--text-muted)', opacity: cap.unlocked ? 1 : 0.5 }">
                {{ cap.name }}
              </span>
            </div>
          </div>
        </div>

        <!-- Controls -->
        <div class="card !p-5 space-y-3">
          <h3 class="text-sm font-semibold" style="color: var(--text-primary)">控制</h3>

          <!-- Simulation -->
          <div class="flex items-center justify-between p-3 rounded-xl" style="background: var(--surface-secondary)">
            <div>
              <div class="text-sm font-medium" style="color: var(--text-primary)">模拟交易</div>
              <div class="flex items-center gap-1 text-xs mt-0.5" :style="{ color: isRunning ? 'var(--profit)' : 'var(--text-muted)' }">
                <div class="w-1.5 h-1.5 rounded-full" :style="{ background: isRunning ? 'var(--profit)' : 'var(--text-muted)', animation: isRunning ? 'pulse 2s infinite' : 'none' }"></div>
                {{ isRunning ? '运行中' : '已停止' }}
              </div>
            </div>
            <button @click="toggleSimulation" :disabled="actionLoading === 'simulation'"
              class="w-8 h-8 rounded-lg flex items-center justify-center"
              :style="{ background: isRunning ? 'var(--loss-light)' : 'var(--primary-bg)', color: isRunning ? 'var(--loss)' : 'var(--primary)' }">
              <component :is="isRunning ? Square : Play" class="w-4 h-4" />
            </button>
          </div>

          <!-- Autonomous -->
          <div class="flex items-center justify-between p-3 rounded-xl" style="background: var(--surface-secondary)">
            <div>
              <div class="flex items-center gap-1.5">
                <span class="text-sm font-medium" style="color: var(--text-primary)">自主交易</span>
                <Lock v-if="currentLevel < 2" class="w-3 h-3" style="color: var(--text-muted)" />
              </div>
              <div class="flex items-center gap-1 text-xs mt-0.5" :style="{ color: isAutonomous ? 'var(--profit)' : 'var(--text-muted)' }">
                <div class="w-1.5 h-1.5 rounded-full" :style="{ background: isAutonomous ? 'var(--profit)' : 'var(--text-muted)', animation: isAutonomous ? 'pulse 2s infinite' : 'none' }"></div>
                {{ isAutonomous ? '运行中' : (currentLevel < 2 ? 'Lv.2 解锁' : '已停止') }}
              </div>
            </div>
            <button v-if="currentLevel >= 2" @click="toggleAutonomous" :disabled="actionLoading === 'autonomous'"
              class="w-8 h-8 rounded-lg flex items-center justify-center"
              :style="{ background: isAutonomous ? 'var(--loss-light)' : 'var(--primary-bg)', color: isAutonomous ? 'var(--loss)' : 'var(--primary)' }">
              <component :is="isAutonomous ? Square : Play" class="w-4 h-4" />
            </button>
            <div v-else class="w-8 h-8 rounded-lg flex items-center justify-center" style="background: var(--border)">
              <Lock class="w-3.5 h-3.5" style="color: var(--text-muted)" />
            </div>
          </div>

          <!-- Quick Links -->
          <div class="grid grid-cols-2 gap-2 pt-1">
            <button @click="router.push('/agent/debate')" class="p-2.5 rounded-lg text-xs font-medium flex items-center justify-center gap-1.5" style="background: var(--surface-secondary); color: var(--text-primary)">
              <Swords class="w-3.5 h-3.5" style="color: var(--primary)" /> AI 辩论
            </button>
            <button @click="router.push('/agent/history')" class="p-2.5 rounded-lg text-xs font-medium flex items-center justify-center gap-1.5" style="background: var(--surface-secondary); color: var(--text-primary)">
              <BarChart3 class="w-3.5 h-3.5" style="color: var(--primary)" /> 交易历史
            </button>
          </div>

          <!-- Risk Info -->
          <div class="pt-2 space-y-1.5 text-xs" style="border-top: 1px solid var(--border)">
            <div class="flex justify-between"><span style="color: var(--text-muted)">交易对</span><span style="color: var(--text-primary)">{{ dashboard.config!.symbol }}</span></div>
            <div class="flex justify-between"><span style="color: var(--text-muted)">K线周期</span><span style="color: var(--text-primary)">{{ intervals.find(i => i.value === dashboard.config!.analysis_interval)?.label || dashboard.config!.analysis_interval }}</span></div>
            <div class="flex justify-between"><span style="color: var(--text-muted)">仓位上限</span><span style="color: var(--text-primary)">{{ dashboard.config!.max_position_size_percent }}%</span></div>
            <div class="flex justify-between"><span style="color: var(--text-muted)">最大杠杆</span><span style="color: var(--text-primary)">{{ dashboard.config!.max_leverage }}x</span></div>
            <div class="flex justify-between"><span style="color: var(--text-muted)">日亏损限制</span><span style="color: var(--text-primary)">{{ dashboard.config!.daily_loss_percent.toFixed(1) }}%</span></div>
            <div class="flex justify-between">
              <span style="color: var(--text-muted)">风控确认</span>
              <span :style="{ color: dashboard.risk_confirmation_signed ? 'var(--profit)' : 'var(--warning)' }">{{ dashboard.risk_confirmation_signed ? '已签署' : '未签署' }}</span>
            </div>
          </div>
        </div>
      </div>

      <!-- Row 3: Positions + Trades + Debates -->
      <div class="grid grid-cols-1 lg:grid-cols-3 gap-4">
        <!-- Positions & Trades -->
        <div class="lg:col-span-2 card !p-5">
          <div class="flex items-center justify-between mb-3">
            <h3 class="text-sm font-semibold" style="color: var(--text-primary)">持仓与交易</h3>
            <button @click="router.push('/paper-trading')" class="text-xs flex items-center gap-0.5" style="color: var(--primary)">详情 <ChevronRight class="w-3 h-3" /></button>
          </div>

          <!-- Open Positions -->
          <div v-if="dashboard.open_positions.length > 0" class="mb-3">
            <div class="text-xs font-medium mb-1.5" style="color: var(--text-secondary)">当前持仓</div>
            <div class="space-y-1.5">
              <div v-for="pos in dashboard.open_positions" :key="pos.trade.id"
                class="flex items-center gap-2.5 p-2 rounded-lg" style="background: var(--surface-secondary)">
                <div class="w-6 h-6 rounded-md flex items-center justify-center shrink-0"
                  :style="{ background: pos.trade.direction === 'long' ? 'var(--profit-light)' : 'var(--loss-light)' }">
                  <TrendingUp v-if="pos.trade.direction === 'long'" class="w-3 h-3" style="color: var(--profit)" />
                  <TrendingDown v-else class="w-3 h-3" style="color: var(--loss)" />
                </div>
                <div class="flex-1 min-w-0">
                  <div class="flex items-center gap-1">
                    <span class="text-xs font-medium" style="color: var(--text-primary)">{{ pos.trade.symbol }}</span>
                    <span class="text-xs" :style="{ color: pos.trade.direction === 'long' ? 'var(--profit)' : 'var(--loss)' }">{{ pos.trade.direction === 'long' ? '多' : '空' }}</span>
                    <span class="text-xs" style="color: var(--text-muted)">{{ pos.trade.leverage }}x</span>
                    <span v-if="pos.trade.agent_session_id" class="text-xs px-1 py-0.5 rounded" style="background: var(--primary-bg); color: var(--primary)">辩论</span>
                  </div>
                  <div v-if="pos.trade.ai_reasoning?.reasoning" class="text-xs mt-0.5 truncate" style="color: var(--text-muted); max-width: 200px">
                    {{ pos.trade.ai_reasoning.reasoning }}
                  </div>
                </div>
                <span class="text-xs font-medium" :style="{ color: pos.unrealized_pnl_percent && pos.unrealized_pnl_percent >= 0 ? 'var(--profit)' : 'var(--loss)' }">
                  {{ pos.unrealized_pnl_percent != null ? fmtPnl(pos.unrealized_pnl_percent) : '--' }}
                </span>
                <span class="text-xs flex items-center gap-1" style="color: var(--profit)">
                  <div class="w-1.5 h-1.5 rounded-full" style="background: var(--profit); animation: pulse 2s infinite"></div>
                  持仓
                </span>
              </div>
            </div>
          </div>

          <!-- Closed Trades -->
          <div>
            <div class="text-xs font-medium mb-1.5" style="color: var(--text-secondary)">已平仓</div>
            <div v-if="dashboard.closed_trades.length === 0" class="text-center py-4" style="color: var(--text-muted)">
              <Activity class="w-5 h-5 mx-auto mb-1 opacity-30" />
              <p class="text-xs">暂无交易记录</p>
            </div>
            <div v-else class="space-y-1.5">
              <div v-for="trade in dashboard.closed_trades.slice(0, 5)" :key="trade.id"
                class="flex items-center gap-2.5 p-2 rounded-lg" style="background: var(--surface-secondary)">
                <div class="w-6 h-6 rounded-md flex items-center justify-center shrink-0"
                  :style="{ background: trade.direction === 'long' ? 'var(--profit-light)' : 'var(--loss-light)' }">
                  <TrendingUp v-if="trade.direction === 'long'" class="w-3 h-3" style="color: var(--profit)" />
                  <TrendingDown v-else class="w-3 h-3" style="color: var(--loss)" />
                </div>
                <div class="flex-1 min-w-0">
                  <div class="flex items-center gap-1">
                    <span class="text-xs font-medium" style="color: var(--text-primary)">{{ trade.symbol }}</span>
                    <span class="text-xs" :style="{ color: trade.direction === 'long' ? 'var(--profit)' : 'var(--loss)' }">{{ trade.direction === 'long' ? '多' : '空' }}</span>
                    <span class="text-xs" style="color: var(--text-muted)">{{ trade.leverage }}x</span>
                    <span v-if="trade.agent_session_id" class="text-xs px-1 py-0.5 rounded" style="background: var(--primary-bg); color: var(--primary)">辩论</span>
                  </div>
                </div>
                <span class="text-xs font-medium" :style="{ color: trade.pnl_percent && trade.pnl_percent >= 0 ? 'var(--profit)' : 'var(--loss)' }">{{ fmtPnl(trade.pnl_percent) }}</span>
                <span v-if="trade.close_reason" class="text-xs" style="color: var(--text-muted)">{{ trade.close_reason }}</span>
                <span class="text-xs" style="color: var(--text-muted)">{{ fmtTime(trade.opened_at) }}</span>
              </div>
            </div>
          </div>
        </div>

        <!-- Recent Debates -->
        <div class="card !p-5">
          <div class="flex items-center justify-between mb-3">
            <h3 class="text-sm font-semibold" style="color: var(--text-primary)">近期辩论</h3>
            <button @click="router.push('/agent/debate')" class="text-xs flex items-center gap-0.5" style="color: var(--primary)">发起 <ChevronRight class="w-3 h-3" /></button>
          </div>
          <div v-if="dashboard.recent_debate_sessions.length === 0" class="text-center py-6" style="color: var(--text-muted)">
            <Swords class="w-6 h-6 mx-auto mb-1.5 opacity-30" />
            <p class="text-xs">暂无辩论记录</p>
          </div>
          <div v-else class="space-y-1.5">
            <div v-for="session in dashboard.recent_debate_sessions" :key="session.id"
              class="flex items-center gap-2 p-2 rounded-lg" style="background: var(--surface-secondary)">
              <Swords class="w-3.5 h-3.5 shrink-0" style="color: var(--primary)" />
              <span class="text-xs font-medium flex-1 truncate" style="color: var(--text-primary)">{{ session.symbol }}</span>
              <span class="text-xs px-1.5 py-0.5 rounded shrink-0"
                :style="{ background: session.status === 'completed' ? 'var(--profit-light)' : session.status === 'failed' ? 'var(--loss-light)' : 'var(--primary-bg)', color: session.status === 'completed' ? 'var(--profit)' : session.status === 'failed' ? 'var(--loss)' : 'var(--primary)' }">
                {{ session.status === 'completed' ? '完成' : session.status === 'failed' ? '失败' : '进行中' }}
              </span>
              <span class="text-xs shrink-0" style="color: var(--text-muted)">{{ fmtTime(session.created_at) }}</span>
            </div>
          </div>
        </div>
      </div>
    </template>

    <!-- Start Modal -->
    <div v-if="showStartModal" class="fixed inset-0 z-50 flex items-center justify-center" style="background: rgba(0,0,0,0.3)">
      <div class="card w-full max-w-sm mx-4 !p-6">
        <h3 class="text-base font-bold mb-4" style="color: var(--text-primary)">启动模拟交易</h3>
        <div class="space-y-3">
          <div>
            <label class="text-xs font-medium mb-1 block" style="color: var(--text-secondary)">交易对</label>
            <select v-model="startSymbol" class="input">
              <option v-for="sym in symbols" :key="sym.value" :value="sym.value">{{ sym.label }}</option>
            </select>
          </div>
          <div>
            <label class="text-xs font-medium mb-1 block" style="color: var(--text-secondary)">K线周期</label>
            <select v-model="startInterval" class="input">
              <option v-for="iv in intervals" :key="iv.value" :value="iv.value">{{ iv.label }}</option>
            </select>
          </div>
          <div>
            <label class="text-xs font-medium mb-1 block" style="color: var(--text-secondary)">初始资金 (USDT)</label>
            <input v-model.number="startBalance" type="number" class="input" min="100" step="100" />
          </div>
          <div class="p-2.5 rounded-lg text-xs" style="background: var(--primary-bg); color: var(--primary)">
            模拟交易使用虚拟资金，Agent 将自动分析市场并执行交易来验证策略。
          </div>
        </div>
        <div class="flex gap-2 mt-5">
          <button @click="showStartModal = false" class="btn-secondary flex-1">取消</button>
          <button @click="createAndStartSimulation" :disabled="actionLoading === 'start'" class="btn-primary flex-1">
            {{ actionLoading === 'start' ? '创建中...' : '确认启动' }}
          </button>
        </div>
      </div>
    </div>

    <!-- Risk Modal -->
    <div v-if="showRiskModal" class="fixed inset-0 z-50 flex items-center justify-center" style="background: rgba(0,0,0,0.3)">
      <div class="card w-full max-w-sm mx-4 !p-6">
        <h3 class="text-base font-bold mb-2" style="color: var(--text-primary)">风险确认</h3>
        <p class="text-xs mb-3" style="color: var(--text-secondary)">启用自主交易前，请确认风险并设定最大可接受亏损。</p>
        <div class="space-y-3">
          <div>
            <label class="text-xs font-medium mb-1 block" style="color: var(--text-secondary)">最大可接受亏损 (USDT)</label>
            <input v-model.number="riskMaxLoss" type="number" class="input" min="10" step="10" />
          </div>
          <div class="p-2.5 rounded-lg text-xs" style="background: var(--loss-light); color: var(--loss)">
            自主交易模式下 Agent 将自动执行交易，亏损达到上限时自动停止。
          </div>
        </div>
        <div class="flex gap-2 mt-5">
          <button @click="showRiskModal = false" class="btn-secondary flex-1">取消</button>
          <button @click="signRiskConfirmation" :disabled="actionLoading === 'risk'" class="btn-primary flex-1">
            {{ actionLoading === 'risk' ? '确认中...' : '确认签署' }}
          </button>
        </div>
      </div>
    </div>
  </div>
</template>

<style scoped>
@keyframes pulse {
  0%, 100% { opacity: 1; }
  50% { opacity: 0.5; }
}
</style>

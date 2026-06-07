<script setup lang="ts">
import { ref, computed, onMounted, onUnmounted } from 'vue'
import { useRouter } from 'vue-router'
import {
  TrendingUp, TrendingDown, Play, Square, Bot, Activity,
  CheckCircle2, Shield, Zap, BarChart3,
  ChevronRight, Swords, Sparkles, Award, Rocket, Lock,
  AlertTriangle
} from 'lucide-vue-next'
import api from '@/api'

const router = useRouter()

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
  profit_loss_ratio: number
  max_drawdown_percent: number
  running_days: number
  max_position_size_percent: number
  max_leverage: number
  max_daily_trades: number
  ai_confidence_threshold: number
  analysis_interval: string
  autonomous_mode_enabled: boolean
  risk_confirmation_signed: boolean
  daily_loss_percent: number
  consecutive_stop_losses: number
  last_trade_at: string | null
  created_at: string
}

interface Trade {
  id: string
  symbol: string
  direction: string
  entry_price: number
  exit_price: number | null
  quantity: number
  leverage: number
  stop_loss: number | null
  take_profit: number | null
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

interface PositionWithPnl {
  trade: Trade
  current_price: number | null
  unrealized_pnl: number | null
  unrealized_pnl_percent: number | null
}

interface LevelInfo {
  current_level: number
  current_level_name: string
  current_mode: string
  next_level: number | null
  next_level_name: string | null
  next_mode: string | null
  progress_percent: number
}

interface LevelRequirements {
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

interface DashboardData {
  has_config: boolean
  config: SimulationConfig | null
  level_info: LevelInfo | null
  open_positions: PositionWithPnl[]
  closed_trades: Trade[]
  level_requirements: LevelRequirements | null
  promotion_eligibility: { eligible: boolean } | null
  risk_confirmation_signed: boolean
}

const dashboard = ref<DashboardData | null>(null)
const loading = ref(true)
const actionLoading = ref<string | null>(null)
const showStartModal = ref(false)
const startSymbol = ref('BTC-USDT-SWAP')
const startBalance = ref(10000)
const pollTimer = ref<ReturnType<typeof setInterval> | null>(null)
const tradeTab = ref<'positions' | 'closed'>('positions')

const levelThemes = [
  { color: '#64748B', bg: '#F1F5F9', label: 'Lv.0', icon: Sparkles, name: '实习交易员', mode: '模拟盘' },
  { color: '#2563EB', bg: '#EFF6FF', label: 'Lv.1', icon: Zap, name: '初级交易员', mode: 'Demo盘' },
  { color: '#D97706', bg: '#FFFBEB', label: 'Lv.2', icon: Shield, name: '资深交易员', mode: '自主交易' },
  { color: '#7C3AED', bg: '#F5F3FF', label: 'Lv.3', icon: Award, name: '基金经理', mode: '实盘' },
]

const currentLevelTheme = computed(() => {
  const level = dashboard.value?.level_info?.current_level ?? 0
  return levelThemes[level] || levelThemes[3]
})

const balanceChange = computed(() => {
  if (!dashboard.value?.config) return { amount: 0, percent: 0 }
  const cfg = dashboard.value.config
  const diff = cfg.current_balance - cfg.initial_balance
  return { amount: diff, percent: (diff / cfg.initial_balance) * 100 }
})

const reqItems = computed(() => {
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

const isRunning = computed(() => dashboard.value?.config?.status === 'running')

async function loadDashboard() {
  loading.value = true
  try {
    const res = await api.get('/agent/dashboard')
    dashboard.value = res.data
  } catch (e: any) {
    if (e?.response?.status === 404) {
      dashboard.value = { has_config: false, config: null, level_info: null, open_positions: [], closed_trades: [], level_requirements: null, promotion_eligibility: null, risk_confirmation_signed: false }
    }
  } finally { loading.value = false }
}

async function createAndStart() {
  actionLoading.value = 'start'
  try {
    await api.post('/agent/simulation/start', { symbol: startSymbol.value, initial_balance: startBalance.value })
    showStartModal.value = false
    await loadDashboard()
  } catch (e: any) { alert(e?.response?.data?.message || e?.message || '启动失败') }
  finally { actionLoading.value = null }
}

async function toggleSimulation() {
  if (!dashboard.value?.config) return
  actionLoading.value = 'sim'
  try {
    if (isRunning.value) {
      await api.post('/agent/simulation/stop', { config_id: dashboard.value.config.id })
    } else {
      await api.post('/agent/simulation/start', { symbol: dashboard.value.config.symbol, initial_balance: dashboard.value.config.initial_balance })
    }
    await loadDashboard()
  } catch (e: any) { alert(e?.response?.data?.message || '操作失败') }
  finally { actionLoading.value = null }
}

async function handleEmergencyStop() {
  if (!dashboard.value?.config) return
  actionLoading.value = 'emergency'
  try {
    await api.post('/agent/emergency/stop', { config_id: dashboard.value.config.id })
    await loadDashboard()
  } finally { actionLoading.value = null }
}

async function handlePromotion() {
  actionLoading.value = 'promo'
  try {
    await api.post('/agent/promotion/initiate')
    await loadDashboard()
  } catch (e: any) { alert(e?.response?.data?.message || '晋级申请失败') }
  finally { actionLoading.value = null }
}

function fmtPnl(v: number | null) { if (v == null) return '--'; return `${v >= 0 ? '+' : ''}${v.toFixed(2)}%` }
function fmtPrice(v: number | null | undefined) { if (v == null) return '--'; return v.toFixed(6) }
function fmtTime(t: string) { return new Date(t).toLocaleString('zh-CN', { month: '2-digit', day: '2-digit', hour: '2-digit', minute: '2-digit' }) }

onMounted(() => { loadDashboard(); pollTimer.value = setInterval(() => loadDashboard(), 15000) })
onUnmounted(() => { if (pollTimer.value) clearInterval(pollTimer.value) })
</script>

<template>
  <div class="space-y-5">
    <!-- Header -->
    <div class="flex items-center justify-between">
      <div>
        <h1 class="text-xl font-bold" style="color: var(--text-primary)">Agent 模拟交易</h1>
        <p class="text-sm mt-0.5" style="color: var(--text-secondary)">AI Agent 自动交易验证 · 逐级进化</p>
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
      <h2 class="text-lg font-bold mb-2" style="color: var(--text-primary)">启动 AI 模拟交易</h2>
      <p class="text-sm mb-5" style="color: var(--text-secondary)">
        Agent 将自动分析市场并执行模拟交易，通过验证策略准确性来逐步晋级
      </p>

      <!-- Evolution Roadmap -->
      <div class="flex items-center justify-center gap-1 mb-6">
        <template v-for="(lv, i) in levelThemes" :key="i">
          <div class="flex flex-col items-center px-3 py-2 rounded-xl" :style="{ background: i === 0 ? 'var(--primary-bg)' : 'var(--surface-secondary)' }">
            <component :is="lv.icon" class="w-5 h-5 mb-1" :style="{ color: i === 0 ? 'var(--primary)' : 'var(--text-muted)' }" />
            <span class="text-xs font-semibold" :style="{ color: i === 0 ? 'var(--primary)' : 'var(--text-muted)' }">{{ lv.name }}</span>
            <span class="text-xs" style="color: var(--text-muted)">{{ lv.mode }}</span>
          </div>
          <ChevronRight v-if="i < 3" class="w-4 h-4" style="color: var(--text-muted)" />
        </template>
      </div>

      <button @click="showStartModal = true" class="btn-primary inline-flex items-center gap-2">
        <Rocket class="w-4 h-4" /> 开始模拟交易
      </button>
    </div>

    <!-- Main Content -->
    <template v-else-if="dashboard?.has_config">
      <!-- Evolution Roadmap Bar -->
      <div class="card !p-4">
        <div class="flex items-center gap-1">
          <template v-for="(lv, i) in levelThemes" :key="i">
            <div class="flex-1 flex items-center gap-2 p-2 rounded-lg transition-all"
              :style="{ background: (dashboard.level_info?.current_level ?? 0) >= i ? lv.bg : 'var(--surface-secondary)', opacity: (dashboard.level_info?.current_level ?? 0) >= i ? 1 : 0.4 }">
              <component :is="lv.icon" class="w-5 h-5 shrink-0" :style="{ color: (dashboard.level_info?.current_level ?? 0) >= i ? lv.color : 'var(--text-muted)' }" />
              <div class="min-w-0">
                <div class="text-xs font-bold truncate" :style="{ color: (dashboard.level_info?.current_level ?? 0) >= i ? lv.color : 'var(--text-muted)' }">{{ lv.label }} {{ lv.name }}</div>
                <div class="text-xs truncate" style="color: var(--text-muted)">{{ lv.mode }}</div>
              </div>
              <CheckCircle2 v-if="(dashboard.level_info?.current_level ?? 0) > i" class="w-4 h-4 shrink-0 ml-auto" style="color: var(--profit)" />
              <div v-else-if="(dashboard.level_info?.current_level ?? 0) === i" class="w-2 h-2 rounded-full ml-auto" style="background: var(--primary); animation: pulse 2s infinite"></div>
            </div>
            <ChevronRight v-if="i < 3" class="w-4 h-4 shrink-0" style="color: var(--text-muted)" />
          </template>
        </div>
      </div>

      <!-- Stats Row -->
      <div class="grid grid-cols-2 lg:grid-cols-4 gap-4">
        <div class="card !p-4">
          <div class="text-xs mb-1" style="color: var(--text-secondary)">账户余额</div>
          <div class="text-lg font-bold" style="color: var(--text-primary)">
            ${{ dashboard.config!.current_balance.toLocaleString(undefined, { maximumFractionDigits: 2 }) }}
          </div>
          <div class="text-xs" :style="{ color: balanceChange.percent >= 0 ? 'var(--profit)' : 'var(--loss)' }">
            {{ balanceChange.percent >= 0 ? '+' : '' }}{{ balanceChange.percent.toFixed(2) }}%
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

      <!-- Level Progress + Control -->
      <div class="grid grid-cols-1 lg:grid-cols-3 gap-4">
        <!-- Level & Requirements -->
        <div class="lg:col-span-2 card !p-5">
          <div class="flex items-center gap-3 mb-4">
            <div class="w-10 h-10 rounded-xl flex items-center justify-center" :style="{ background: currentLevelTheme.bg }">
              <component :is="currentLevelTheme.icon" class="w-5 h-5" :style="{ color: currentLevelTheme.color }" />
            </div>
            <div class="flex-1">
              <div class="flex items-center gap-2">
                <span class="text-xs font-bold px-1.5 py-0.5 rounded" :style="{ background: currentLevelTheme.bg, color: currentLevelTheme.color }">{{ currentLevelTheme.label }}</span>
                <span class="font-semibold" style="color: var(--text-primary)">{{ dashboard.level_info?.current_level_name }}</span>
                <span class="text-xs px-1.5 py-0.5 rounded-full" style="background: var(--surface-secondary); color: var(--text-muted)">{{ dashboard.level_info?.current_mode }}</span>
              </div>
              <div class="text-xs mt-0.5" style="color: var(--text-secondary)">
                {{ dashboard.level_info?.next_level ? `下一级: ${dashboard.level_info.next_level_name} (${dashboard.level_info.next_mode})` : '已满级' }}
              </div>
            </div>
            <button v-if="dashboard.promotion_eligibility?.eligible" @click="handlePromotion" :disabled="actionLoading === 'promo'" class="btn-primary text-xs !py-1.5 !px-3">
              申请晋级
            </button>
          </div>

          <!-- Progress Bar -->
          <div class="mb-4">
            <div class="flex justify-between text-xs mb-1">
              <span style="color: var(--text-muted)">升级进度</span>
              <span style="color: var(--primary)">{{ dashboard.level_info?.progress_percent.toFixed(0) }}%</span>
            </div>
            <div class="h-2 rounded-full overflow-hidden" style="background: var(--border)">
              <div class="h-full rounded-full transition-all duration-700" :style="{ background: currentLevelTheme.color, width: `${dashboard.level_info?.progress_percent || 0}%` }"></div>
            </div>
          </div>

          <!-- Requirements -->
          <div v-if="reqItems.length > 0" class="grid grid-cols-3 gap-2">
            <div v-for="item in reqItems" :key="item.label" class="p-2 rounded-lg" style="background: var(--surface-secondary)">
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
          <div v-else class="text-center py-2">
            <span class="text-xs font-semibold" style="color: var(--primary)">已达到最高等级</span>
          </div>
        </div>

        <!-- Control Panel -->
        <div class="card !p-5 space-y-3">
          <h3 class="text-sm font-semibold" style="color: var(--text-primary)">控制</h3>

          <div class="flex items-center justify-between p-3 rounded-xl" style="background: var(--surface-secondary)">
            <div>
              <div class="text-sm font-medium" style="color: var(--text-primary)">模拟交易</div>
              <div class="flex items-center gap-1 text-xs mt-0.5" :style="{ color: isRunning ? 'var(--profit)' : 'var(--text-muted)' }">
                <div class="w-1.5 h-1.5 rounded-full" :style="{ background: isRunning ? 'var(--profit)' : 'var(--text-muted)', animation: isRunning ? 'pulse 2s infinite' : 'none' }"></div>
                {{ isRunning ? '自动交易中' : '已停止' }}
              </div>
            </div>
            <button @click="toggleSimulation" :disabled="actionLoading === 'sim'"
              class="w-8 h-8 rounded-lg flex items-center justify-center"
              :style="{ background: isRunning ? 'var(--loss-light)' : 'var(--primary-bg)', color: isRunning ? 'var(--loss)' : 'var(--primary)' }">
              <component :is="isRunning ? Square : Play" class="w-4 h-4" />
            </button>
          </div>

          <div class="grid grid-cols-2 gap-2">
            <button @click="router.push('/agent/debate')" class="p-2.5 rounded-lg text-xs font-medium flex items-center justify-center gap-1.5" style="background: var(--surface-secondary); color: var(--text-primary)">
              <Swords class="w-3.5 h-3.5" style="color: var(--primary)" /> AI 辩论
            </button>
            <button @click="router.push('/agent/history')" class="p-2.5 rounded-lg text-xs font-medium flex items-center justify-center gap-1.5" style="background: var(--surface-secondary); color: var(--text-primary)">
              <BarChart3 class="w-3.5 h-3.5" style="color: var(--primary)" /> 交易历史
            </button>
          </div>

          <div class="pt-2 space-y-1.5 text-xs" style="border-top: 1px solid var(--border)">
            <div class="flex justify-between"><span style="color: var(--text-muted)">交易对</span><span style="color: var(--text-primary)">{{ dashboard.config!.symbol }}</span></div>
            <div class="flex justify-between"><span style="color: var(--text-muted)">K线周期</span><span style="color: var(--text-primary)">{{ dashboard.config!.analysis_interval || '1H' }}</span></div>
            <div class="flex justify-between"><span style="color: var(--text-muted)">仓位上限</span><span style="color: var(--text-primary)">{{ dashboard.config!.max_position_size_percent }}%</span></div>
            <div class="flex justify-between"><span style="color: var(--text-muted)">最大杠杆</span><span style="color: var(--text-primary)">{{ dashboard.config!.max_leverage }}x</span></div>
            <div class="flex justify-between"><span style="color: var(--text-muted)">日亏损限制</span><span style="color: var(--text-primary)">{{ dashboard.config!.daily_loss_percent.toFixed(1) }}%</span></div>
            <div class="flex justify-between"><span style="color: var(--text-muted)">AI置信阈值</span><span style="color: var(--text-primary)">{{ (dashboard.config!.ai_confidence_threshold * 100).toFixed(0) }}%</span></div>
            <div class="flex justify-between"><span style="color: var(--text-muted)">连续亏损</span><span style="color: var(--text-primary)">{{ dashboard.config!.consecutive_stop_losses }} 次</span></div>
          </div>
        </div>
      </div>

      <!-- Positions & Trades -->
      <div class="card !p-5">
        <div class="flex items-center gap-4 mb-3">
          <button @click="tradeTab = 'positions'" class="text-sm font-semibold pb-1 transition-colors" :style="{ color: tradeTab === 'positions' ? 'var(--primary)' : 'var(--text-muted)', borderBottom: tradeTab === 'positions' ? '2px solid var(--primary)' : '2px solid transparent' }">
            持仓 ({{ dashboard.open_positions.length }})
          </button>
          <button @click="tradeTab = 'closed'" class="text-sm font-semibold pb-1 transition-colors" :style="{ color: tradeTab === 'closed' ? 'var(--primary)' : 'var(--text-muted)', borderBottom: tradeTab === 'closed' ? '2px solid var(--primary)' : '2px solid transparent' }">
            已平仓 ({{ dashboard.closed_trades.length }})
          </button>
          <div class="flex-1"></div>
          <button @click="router.push('/agent/history')" class="text-xs flex items-center gap-0.5" style="color: var(--primary)">全部 <ChevronRight class="w-3 h-3" /></button>
        </div>

        <!-- Open Positions -->
        <div v-if="tradeTab === 'positions'">
          <div v-if="dashboard.open_positions.length === 0" class="text-center py-8" style="color: var(--text-muted)">
            <Activity class="w-6 h-6 mx-auto mb-1.5 opacity-30" />
            <p class="text-xs">暂无持仓</p>
            <p class="text-xs mt-1">Agent 自动分析后将开仓交易</p>
          </div>
          <div v-else class="space-y-2">
            <div v-for="pos in dashboard.open_positions" :key="pos.trade.id"
              class="p-3 rounded-xl" style="background: var(--surface-secondary)">
              <div class="flex items-center gap-2 mb-2">
                <div class="w-6 h-6 rounded-md flex items-center justify-center shrink-0"
                  :style="{ background: pos.trade.direction === 'long' ? 'var(--profit-light)' : 'var(--loss-light)' }">
                  <TrendingUp v-if="pos.trade.direction === 'long'" class="w-3 h-3" style="color: var(--profit)" />
                  <TrendingDown v-else class="w-3 h-3" style="color: var(--loss)" />
                </div>
                <span class="text-sm font-semibold" style="color: var(--text-primary)">{{ pos.trade.symbol }}</span>
                <span class="text-xs px-1.5 py-0.5 rounded" :style="{ background: pos.trade.direction === 'long' ? 'var(--profit-light)' : 'var(--loss-light)', color: pos.trade.direction === 'long' ? 'var(--profit)' : 'var(--loss)' }">
                  {{ pos.trade.direction === 'long' ? '做多' : '做空' }}
                </span>
                <span class="text-xs" style="color: var(--text-muted)">{{ pos.trade.leverage }}x</span>
                <span v-if="pos.trade.agent_session_id" class="text-xs px-1.5 py-0.5 rounded flex items-center gap-1" style="background: var(--primary-bg); color: var(--primary)">
                  <Swords class="w-3 h-3" /> 辩论决策
                </span>
                <span v-if="pos.trade.ai_confidence" class="text-xs ml-auto" style="color: var(--text-muted)">AI {{ (pos.trade.ai_confidence * 100).toFixed(0) }}%</span>
              </div>
              <div class="grid grid-cols-4 gap-2 text-xs">
                <div>
                  <div style="color: var(--text-muted)">入场价</div>
                  <div class="font-medium" style="color: var(--text-primary)">{{ fmtPrice(pos.trade.entry_price) }}</div>
                </div>
                <div>
                  <div style="color: var(--text-muted)">当前价</div>
                  <div class="font-medium" style="color: var(--text-primary)">{{ fmtPrice(pos.current_price) }}</div>
                </div>
                <div>
                  <div style="color: var(--text-muted)">止损 / 止盈</div>
                  <div class="font-medium" style="color: var(--text-primary)">
                    {{ fmtPrice(pos.trade.stop_loss) }} / {{ fmtPrice(pos.trade.take_profit) }}
                  </div>
                </div>
                <div>
                  <div style="color: var(--text-muted)">浮动盈亏</div>
                  <div class="font-semibold" :style="{ color: pos.unrealized_pnl_percent && pos.unrealized_pnl_percent >= 0 ? 'var(--profit)' : 'var(--loss)' }">
                    {{ pos.unrealized_pnl != null ? `$${pos.unrealized_pnl.toFixed(2)}` : '--' }}
                    <span>({{ pos.unrealized_pnl_percent != null ? fmtPnl(pos.unrealized_pnl_percent) : '--' }})</span>
                  </div>
                </div>
              </div>
              <!-- Debate Reasoning -->
              <div v-if="pos.trade.ai_reasoning?.reasoning" class="mt-2 p-2 rounded-lg text-xs" style="background: var(--primary-bg); color: var(--primary)">
                <div class="flex items-center gap-1 mb-0.5 font-medium">
                  <Swords class="w-3 h-3" /> 辩论理由
                </div>
                {{ pos.trade.ai_reasoning.reasoning }}
              </div>
              <div class="flex items-center gap-3 mt-2 text-xs" style="color: var(--text-muted)">
                <span>数量: {{ pos.trade.quantity?.toFixed(4) || '--' }}</span>
                <span>开仓: {{ fmtTime(pos.trade.opened_at) }}</span>
                <span class="flex items-center gap-1">
                  <div class="w-1.5 h-1.5 rounded-full" style="background: var(--profit); animation: pulse 2s infinite"></div>
                  持仓中
                </span>
              </div>
            </div>
          </div>
        </div>

        <!-- Closed Trades -->
        <div v-if="tradeTab === 'closed'">
          <div v-if="dashboard.closed_trades.length === 0" class="text-center py-8" style="color: var(--text-muted)">
            <Activity class="w-6 h-6 mx-auto mb-1.5 opacity-30" />
            <p class="text-xs">暂无已平仓交易</p>
          </div>
          <div v-else class="space-y-1.5">
            <div v-for="trade in dashboard.closed_trades.slice(0, 15)" :key="trade.id"
              class="p-2 rounded-lg" style="background: var(--surface-secondary)">
              <div class="flex items-center gap-2.5">
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
                <span v-if="trade.close_reason" class="text-xs px-1.5 py-0.5 rounded" style="background: var(--surface-tertiary); color: var(--text-muted)">{{ trade.close_reason }}</span>
                <span class="text-xs" style="color: var(--text-muted)">{{ fmtTime(trade.opened_at) }}</span>
              </div>
              <div v-if="trade.ai_reasoning?.reasoning" class="mt-1 text-xs truncate" style="color: var(--text-muted); padding-left: 2rem">
                {{ trade.ai_reasoning.reasoning }}
              </div>
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
              <option value="BTC-USDT-SWAP">BTC/USDT</option>
              <option value="ETH-USDT-SWAP">ETH/USDT</option>
              <option value="SOL-USDT-SWAP">SOL/USDT</option>
              <option value="DOGE-USDT-SWAP">DOGE/USDT</option>
            </select>
          </div>
          <div>
            <label class="text-xs font-medium mb-1 block" style="color: var(--text-secondary)">初始资金 (USDT)</label>
            <input v-model.number="startBalance" type="number" class="input" min="100" step="100" />
          </div>
          <div class="p-2.5 rounded-lg text-xs" style="background: var(--primary-bg); color: var(--primary)">
            Agent 将自动分析市场并执行模拟交易，止盈止损后自动重新分析开仓，形成闭环验证。
          </div>
        </div>
        <div class="flex gap-2 mt-5">
          <button @click="showStartModal = false" class="btn-secondary flex-1">取消</button>
          <button @click="createAndStart" :disabled="actionLoading === 'start'" class="btn-primary flex-1">
            {{ actionLoading === 'start' ? '创建中...' : '确认启动' }}
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

<script setup lang="ts">
import { ref, computed, onMounted } from 'vue'
import {
  TrendingUp, TrendingDown, Calendar, Filter,
  Activity, Clock, Bot, Shield, Zap
} from 'lucide-vue-next'
import api from '@/api'

interface Trade {
  id: string
  symbol: string
  direction: string
  mode: string
  entry_price: number
  exit_price: number | null
  quantity: number
  leverage: number
  stop_loss: number | null
  take_profit: number | null
  pnl: number | null
  pnl_percent: number | null
  ai_confidence: number | null
  ai_reasoning: any
  agent_session_id: string | null
  status: string
  close_reason: string | null
  reasoning: any
  opened_at: string
  closed_at: string | null
}

const trades = ref<Trade[]>([])
const loading = ref(true)
const tradeMode = ref<'all' | 'paper' | 'demo' | 'live'>('all')
const tradeType = ref<'all' | 'long' | 'short'>('all')
const tradeStatus = ref<'all' | 'open' | 'closed'>('all')
const timeRange = ref<'1d' | '7d' | '30d' | 'all'>('7d')

const filteredTrades = computed(() => {
  return trades.value.filter(trade => {
    if (tradeMode.value !== 'all' && trade.mode !== tradeMode.value) return false
    if (tradeType.value !== 'all' && trade.direction !== tradeType.value) return false
    if (tradeStatus.value !== 'all' && trade.status !== tradeStatus.value) return false
    return true
  })
})

const stats = computed(() => {
  const closed = filteredTrades.value.filter(t => t.status === 'closed')
  const wins = closed.filter(t => t.pnl != null && t.pnl > 0)
  const losses = closed.filter(t => t.pnl != null && t.pnl < 0)

  return {
    total: closed.length,
    wins: wins.length,
    losses: losses.length,
    winRate: closed.length > 0 ? (wins.length / closed.length * 100).toFixed(1) : '0',
    totalPnl: closed.reduce((s, t) => s + (t.pnl || 0), 0),
    avgPnlPct: closed.length > 0
      ? (closed.reduce((s, t) => s + (t.pnl_percent || 0), 0) / closed.length).toFixed(2)
      : '0',
  }
})

const modeLabel = (mode: string) => {
  switch (mode) {
    case 'paper': return '模拟'
    case 'demo': return 'Demo'
    case 'live': return '实盘'
    default: return mode
  }
}

const modeColor = (mode: string) => {
  switch (mode) {
    case 'paper': return { bg: 'var(--primary-bg)', color: 'var(--primary)' }
    case 'demo': return { bg: 'var(--warning-light)', color: 'var(--warning)' }
    case 'live': return { bg: 'var(--loss-light)', color: 'var(--loss)' }
    default: return { bg: 'var(--surface-secondary)', color: 'var(--text-muted)' }
  }
}

function fmtTime(t: string) {
  if (!t) return '-'
  return new Date(t).toLocaleString('zh-CN', { month: '2-digit', day: '2-digit', hour: '2-digit', minute: '2-digit' })
}

function fmtPnl(v: number | null) {
  if (v == null) return '--'
  return `${v >= 0 ? '+' : ''}${v.toFixed(2)}%`
}

function fmtPrice(v: number | null | undefined) { if (v == null) return '--'; return v.toFixed(6) }

function fmtDuration(opened: string, closed: string | null) {
  const end = closed ? new Date(closed) : new Date()
  const start = new Date(opened)
  const mins = Math.floor((end.getTime() - start.getTime()) / 60000)
  if (mins < 60) return `${mins}分钟`
  const hours = Math.floor(mins / 60)
  if (hours < 24) return `${hours}小时`
  return `${Math.floor(hours / 24)}天`
}

async function loadTrades() {
  loading.value = true
  try {
    // Load Agent simulation trades
    const simRes = await api.get('/agent/simulation/trades')
    const simTrades = (simRes.data?.trades || simRes.data?.data || simRes.data || []) as Trade[]

    // Load real OKX trades
    let realTrades: Trade[] = []
    try {
      const realRes = await api.get('/trading/history')
      const rawTrades = realRes.data?.data || realRes.data || []
      // Map OKX trade format to our Trade interface
      realTrades = (Array.isArray(rawTrades) ? rawTrades : []).map((t: any) => ({
        id: t.ordId || t.tradeId || t.id || `${t.instId}-${t.cTime}`,
        symbol: t.instId || t.symbol || '',
        direction: t.side === 'buy' ? 'long' : 'short',
        mode: 'live',
        entry_price: parseFloat(t.avgPx || t.price || '0'),
        exit_price: null,
        quantity: parseFloat(t.sz || t.size || '0'),
        leverage: parseFloat(t.lever || '1'),
        stop_loss: null,
        take_profit: null,
        pnl: t.pnl ? parseFloat(t.pnl) : null,
        pnl_percent: null,
        ai_confidence: null,
        ai_reasoning: null,
        agent_session_id: null,
        status: t.state === 'filled' || t.state === '2' ? 'closed' : 'open',
        close_reason: null,
        reasoning: null,
        opened_at: t.cTime || t.createdTime || new Date().toISOString(),
        closed_at: t.uTime || t.updatedTime || null,
      }))
    } catch {
      // Real trades may not be available if no OKX API configured
    }

    trades.value = [...simTrades, ...realTrades].sort((a, b) =>
      new Date(b.opened_at).getTime() - new Date(a.opened_at).getTime()
    )
  } catch (e) {
    console.error('Failed to load trades:', e)
    trades.value = []
  } finally {
    loading.value = false
  }
}

onMounted(() => { loadTrades() })
</script>

<template>
  <div class="space-y-5">
    <!-- Header -->
    <div>
      <h1 class="text-xl font-bold" style="color: var(--text-primary)">交易历史</h1>
      <p class="text-sm mt-0.5" style="color: var(--text-secondary)">Agent 模拟交易与真实交易记录</p>
    </div>

    <!-- Stats -->
    <div class="grid grid-cols-2 lg:grid-cols-4 gap-4">
      <div class="card !p-4">
        <div class="text-xs mb-1" style="color: var(--text-secondary)">总交易</div>
        <div class="text-lg font-bold" style="color: var(--text-primary)">{{ stats.total }}</div>
        <div class="text-xs" style="color: var(--text-muted)">{{ stats.wins }}W / {{ stats.losses }}L</div>
      </div>
      <div class="card !p-4">
        <div class="text-xs mb-1" style="color: var(--text-secondary)">胜率</div>
        <div class="text-lg font-bold" :style="{ color: parseFloat(stats.winRate) >= 50 ? 'var(--profit)' : 'var(--loss)' }">{{ stats.winRate }}%</div>
      </div>
      <div class="card !p-4">
        <div class="text-xs mb-1" style="color: var(--text-secondary)">总盈亏</div>
        <div class="text-lg font-bold" :style="{ color: stats.totalPnl >= 0 ? 'var(--profit)' : 'var(--loss)' }">
          {{ stats.totalPnl >= 0 ? '+' : '' }}{{ stats.totalPnl.toFixed(2) }}
        </div>
      </div>
      <div class="card !p-4">
        <div class="text-xs mb-1" style="color: var(--text-secondary)">平均盈亏</div>
        <div class="text-lg font-bold" :style="{ color: parseFloat(stats.avgPnlPct) >= 0 ? 'var(--profit)' : 'var(--loss)' }">
          {{ parseFloat(stats.avgPnlPct) >= 0 ? '+' : '' }}{{ stats.avgPnlPct }}%
        </div>
      </div>
    </div>

    <!-- Filters -->
    <div class="card !p-4">
      <div class="flex items-center gap-4 flex-wrap">
        <!-- Mode Filter -->
        <div class="flex items-center gap-2">
          <Bot class="w-4 h-4" style="color: var(--text-muted)" />
          <span class="text-xs" style="color: var(--text-secondary)">类型</span>
          <div class="flex gap-1">
            <button v-for="m in [
              { value: 'all', label: '全部' },
              { value: 'paper', label: '模拟' },
              { value: 'demo', label: 'Demo' },
              { value: 'live', label: '实盘' }
            ]" :key="m.value" @click="tradeMode = m.value as any"
              class="px-2.5 py-1 rounded-lg text-xs font-medium transition-all"
              :style="tradeMode === m.value ? 'background: var(--primary-bg); color: var(--primary)' : 'background: var(--surface-secondary); color: var(--text-muted)'">
              {{ m.label }}
            </button>
          </div>
        </div>

        <!-- Direction Filter -->
        <div class="flex items-center gap-2">
          <Filter class="w-4 h-4" style="color: var(--text-muted)" />
          <span class="text-xs" style="color: var(--text-secondary)">方向</span>
          <div class="flex gap-1">
            <button v-for="d in [
              { value: 'all', label: '全部' },
              { value: 'long', label: '做多' },
              { value: 'short', label: '做空' }
            ]" :key="d.value" @click="tradeType = d.value as any"
              class="px-2.5 py-1 rounded-lg text-xs font-medium transition-all"
              :style="tradeType === d.value ? 'background: var(--primary-bg); color: var(--primary)' : 'background: var(--surface-secondary); color: var(--text-muted)'">
              {{ d.label }}
            </button>
          </div>
        </div>

        <!-- Status Filter -->
        <div class="flex items-center gap-2">
          <span class="text-xs" style="color: var(--text-secondary)">状态</span>
          <div class="flex gap-1">
            <button v-for="s in [
              { value: 'all', label: '全部' },
              { value: 'open', label: '持仓中' },
              { value: 'closed', label: '已平仓' }
            ]" :key="s.value" @click="tradeStatus = s.value as any"
              class="px-2.5 py-1 rounded-lg text-xs font-medium transition-all"
              :style="tradeStatus === s.value ? 'background: var(--primary-bg); color: var(--primary)' : 'background: var(--surface-secondary); color: var(--text-muted)'">
              {{ s.label }}
            </button>
          </div>
        </div>
      </div>
    </div>

    <!-- Trade List -->
    <div class="card !p-5">
      <div v-if="loading" class="space-y-3">
        <div v-for="i in 3" :key="i" class="h-16 rounded-lg animate-pulse" style="background: var(--surface-secondary)" />
      </div>

      <div v-else-if="filteredTrades.length === 0" class="text-center py-12" style="color: var(--text-muted)">
        <Activity class="w-8 h-8 mx-auto mb-2 opacity-30" />
        <p class="text-sm">暂无交易记录</p>
        <p class="text-xs mt-1">启动模拟交易后，Agent 的交易将显示在这里</p>
      </div>

      <div v-else class="space-y-2">
        <div v-for="trade in filteredTrades" :key="trade.id"
          class="p-3 rounded-xl" style="background: var(--surface-secondary)">
          <!-- Row 1: Symbol + Direction + Mode + PnL -->
          <div class="flex items-center gap-2">
            <div class="w-7 h-7 rounded-lg flex items-center justify-center shrink-0"
              :style="{ background: trade.direction === 'long' ? 'var(--profit-light)' : 'var(--loss-light)' }">
              <TrendingUp v-if="trade.direction === 'long'" class="w-3.5 h-3.5" style="color: var(--profit)" />
              <TrendingDown v-else class="w-3.5 h-3.5" style="color: var(--loss)" />
            </div>
            <span class="text-sm font-semibold" style="color: var(--text-primary)">{{ trade.symbol }}</span>
            <span class="text-xs px-1.5 py-0.5 rounded" :style="{ background: trade.direction === 'long' ? 'var(--profit-light)' : 'var(--loss-light)', color: trade.direction === 'long' ? 'var(--profit)' : 'var(--loss)' }">
              {{ trade.direction === 'long' ? '多' : '空' }}
            </span>
            <span class="text-xs px-1.5 py-0.5 rounded" :style="{ background: modeColor(trade.mode).bg, color: modeColor(trade.mode).color }">
              {{ modeLabel(trade.mode) }}
            </span>
            <span class="text-xs" style="color: var(--text-muted)">{{ trade.leverage }}x</span>
            <span v-if="trade.agent_session_id" class="text-xs px-1.5 py-0.5 rounded flex items-center gap-1" style="background: var(--primary-bg); color: var(--primary)">
              辩论决策
            </span>
            <div class="flex-1"></div>
            <span class="text-sm font-bold" :style="{ color: trade.pnl_percent != null && trade.pnl_percent >= 0 ? 'var(--profit)' : 'var(--loss)' }">
              {{ fmtPnl(trade.pnl_percent) }}
            </span>
            <span v-if="trade.status === 'open'" class="text-xs flex items-center gap-1" style="color: var(--profit)">
              <div class="w-1.5 h-1.5 rounded-full" style="background: var(--profit); animation: pulse 2s infinite"></div>
              持仓
            </span>
            <span v-else-if="trade.close_reason" class="text-xs px-1.5 py-0.5 rounded" style="background: var(--surface-tertiary); color: var(--text-muted)">
              {{ trade.close_reason }}
            </span>
          </div>

          <!-- Row 2: Details -->
          <div class="grid grid-cols-4 gap-2 mt-2 text-xs">
            <div>
              <div style="color: var(--text-muted)">入场价</div>
              <div class="font-medium" style="color: var(--text-primary)">{{ fmtPrice(trade.entry_price) }}</div>
            </div>
            <div>
              <div style="color: var(--text-muted)">出场价</div>
              <div class="font-medium" style="color: var(--text-primary)">{{ fmtPrice(trade.exit_price) }}</div>
            </div>
            <div>
              <div style="color: var(--text-muted)">盈亏</div>
              <div class="font-medium" :style="{ color: trade.pnl != null && trade.pnl >= 0 ? 'var(--profit)' : 'var(--loss)' }">
                {{ trade.pnl != null ? `$${trade.pnl.toFixed(2)}` : '--' }}
              </div>
            </div>
            <div>
              <div style="color: var(--text-muted)">持仓时间</div>
              <div class="font-medium" style="color: var(--text-primary)">{{ fmtDuration(trade.opened_at, trade.closed_at) }}</div>
            </div>
          </div>

          <!-- Row 3: Meta -->
          <div class="flex items-center gap-3 mt-2 text-xs" style="color: var(--text-muted)">
            <span class="flex items-center gap-1"><Clock class="w-3 h-3" /> {{ fmtTime(trade.opened_at) }}</span>
            <span v-if="trade.ai_confidence">AI 置信: {{ (trade.ai_confidence * 100).toFixed(0) }}%</span>
            <span v-if="trade.stop_loss">SL: {{ fmtPrice(trade.stop_loss) }}</span>
            <span v-if="trade.take_profit">TP: {{ fmtPrice(trade.take_profit) }}</span>
          </div>

          <!-- Debate Reasoning -->
          <div v-if="trade.ai_reasoning?.reasoning" class="mt-2 p-2 rounded-lg text-xs" style="background: var(--primary-bg); color: var(--primary)">
            <div class="font-medium mb-0.5">辩论理由</div>
            {{ trade.ai_reasoning.reasoning }}
          </div>
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

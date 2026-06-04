<script setup lang="ts">
import { ref, onMounted, computed } from 'vue'
import {
  TrendingUp,
  TrendingDown,
  Calendar,
  Filter,
  Download,
  ArrowLeftRight,
  Clock,
  Target,
  Activity,
  ChevronRight,
  Eye
} from 'lucide-vue-next'
import api from '@/api'

interface Trade {
  id: string
  symbol: string
  type: 'long' | 'short'
  entryPrice: number
  exitPrice: number
  size: number
  pnl: number
  pnlPercent: number
  entryTime: string
  exitTime: string
  duration: string
  status: 'closed' | 'open'
  reason: string
  confidence: number
  sessionId?: string
}

const trades = ref<Trade[]>([
  {
    id: 'trade_1',
    symbol: 'DOGE-USDT-SWAP',
    type: 'long',
    entryPrice: 0.215,
    exitPrice: 0.238,
    size: 1000,
    pnl: 23,
    pnlPercent: 10.7,
    entryTime: new Date(Date.now() - 6 * 3600 * 1000).toISOString(),
    exitTime: new Date(Date.now() - 2 * 3600 * 1000).toISOString(),
    duration: '4h',
    status: 'closed',
    reason: '双底形态确认 + 负费率逼空',
    confidence: 0.75,
    sessionId: 'session_123'
  },
  {
    id: 'trade_2',
    symbol: 'DOGE-USDT-SWAP',
    type: 'short',
    entryPrice: 0.245,
    exitPrice: 0.232,
    size: 800,
    pnl: 10.4,
    pnlPercent: 5.3,
    entryTime: new Date(Date.now() - 24 * 3600 * 1000).toISOString(),
    exitTime: new Date(Date.now() - 20 * 3600 * 1000).toISOString(),
    duration: '4h',
    status: 'closed',
    reason: '技术指标超买 + OI下降',
    confidence: 0.68
  },
  {
    id: 'trade_3',
    symbol: 'DOGE-USDT-SWAP',
    type: 'long',
    entryPrice: 0.205,
    exitPrice: 0.21,
    size: 1500,
    pnl: 7.5,
    pnlPercent: 2.4,
    entryTime: new Date(Date.now() - 48 * 3600 * 1000).toISOString(),
    exitTime: new Date(Date.now() - 46 * 3600 * 1000).toISOString(),
    duration: '2h',
    status: 'closed',
    reason: '支撑位测试成功',
    confidence: 0.62
  },
  {
    id: 'trade_4',
    symbol: 'DOGE-USDT-SWAP',
    type: 'long',
    entryPrice: 0.228,
    exitPrice: 0,
    size: 1200,
    pnl: -5.5,
    pnlPercent: -2.0,
    entryTime: new Date(Date.now() - 3600 * 1000).toISOString(),
    exitTime: '',
    duration: '1h',
    status: 'open',
    reason: '突破确认',
    confidence: 0.7
  },
  {
    id: 'trade_5',
    symbol: 'DOGE-USDT-SWAP',
    type: 'short',
    entryPrice: 0.25,
    exitPrice: 0.24,
    size: 900,
    pnl: 9,
    pnlPercent: 4.0,
    entryTime: new Date(Date.now() - 72 * 3600 * 1000).toISOString(),
    exitTime: new Date(Date.now() - 70 * 3600 * 1000).toISOString(),
    duration: '2h',
    status: 'closed',
    reason: '阻力位遇阻回落',
    confidence: 0.65
  }
])

const loading = ref(false)
const timeRange = ref<'1d' | '7d' | '30d' | 'all'>('7d')
const tradeType = ref<'all' | 'long' | 'short'>('all')
const status = ref<'all' | 'open' | 'closed'>('all')

const filteredTrades = computed(() => {
  return trades.value.filter(trade => {
    let match = true
    
    if (tradeType.value !== 'all') {
      match = match && trade.type === tradeType.value
    }
    
    if (status.value !== 'all') {
      match = match && trade.status === status.value
    }
    
    return match
  })
})

const stats = computed(() => {
  const closedTrades = trades.value.filter(t => t.status === 'closed')
  const winningTrades = closedTrades.filter(t => t.pnl > 0)
  const losingTrades = closedTrades.filter(t => t.pnl <= 0)
  
  return {
    totalTrades: closedTrades.length,
    winningTrades: winningTrades.length,
    losingTrades: losingTrades.length,
    winRate: closedTrades.length > 0 ? (winningTrades.length / closedTrades.length * 100).toFixed(1) : '0',
    totalPnl: closedTrades.reduce((sum, t) => sum + t.pnl, 0),
    avgPnlPercent: closedTrades.length > 0 ? (closedTrades.reduce((sum, t) => sum + t.pnlPercent, 0) / closedTrades.length).toFixed(2) : '0',
    maxWin: Math.max(...closedTrades.map(t => t.pnlPercent), 0),
    maxLoss: Math.min(...closedTrades.map(t => t.pnlPercent), 0)
  }
})

function formatTime(isoString: string): string {
  if (!isoString) return '-'
  const date = new Date(isoString)
  return date.toLocaleString('zh-CN', {
    month: 'numeric',
    day: 'numeric',
    hour: '2-digit',
    minute: '2-digit'
  })
}

async function loadTrades() {
  loading.value = true
  try {
    // const res = await api.get('/agent/trades')
    // trades.value = res.data
  } catch (e) {
    console.error('Failed to load trades:', e)
  } finally {
    loading.value = false
  }
}

onMounted(() => {
  loadTrades()
})
</script>

<template>
  <div class="space-y-6">
    <!-- 页面标题 -->
    <div class="flex items-center justify-between">
      <div>
        <h1 class="font-sans text-2xl font-bold" style="color: var(--text-primary)">
          交易历史
        </h1>
        <p class="text-sm mt-1" style="color: var(--text-secondary)">
          查看 AI Agent 的历史交易记录和表现分析
        </p>
      </div>
      <button class="btn btn-secondary flex items-center gap-2">
        <Download class="w-4 h-4" />
        导出
      </button>
    </div>

    <!-- 统计卡片 -->
    <div class="grid grid-cols-2 md:grid-cols-4 gap-4">
      <div class="stat-card">
        <div class="flex items-center justify-between mb-2">
          <span class="stat-label">总交易</span>
          <Activity class="w-4 h-4" style="color: var(--text-muted)" />
        </div>
        <div class="stat-value">{{ stats.totalTrades }}</div>
      </div>
      
      <div class="stat-card">
        <div class="flex items-center justify-between mb-2">
          <span class="stat-label">胜率</span>
          <Target class="w-4 h-4" style="color: var(--profit)" />
        </div>
        <div class="stat-value" style="color: var(--profit)">{{ stats.winRate }}%</div>
      </div>
      
      <div class="stat-card">
        <div class="flex items-center justify-between mb-2">
          <span class="stat-label">总盈亏</span>
          <TrendingUp class="w-4 h-4" style="color: var(--profit)" />
        </div>
        <div class="stat-value" :style="{ color: stats.totalPnl >= 0 ? 'var(--profit)' : 'var(--loss)' }">
          {{ stats.totalPnl >= 0 ? '+' : '' }}{{ stats.totalPnl.toFixed(2) }}
        </div>
      </div>
      
      <div class="stat-card">
        <div class="flex items-center justify-between mb-2">
          <span class="stat-label">平均盈亏</span>
          <TrendingDown class="w-4 h-4" :style="{ color: parseFloat(stats.avgPnlPercent) >= 0 ? 'var(--profit)' : 'var(--loss)' }" />
        </div>
        <div class="stat-value" :style="{ color: parseFloat(stats.avgPnlPercent) >= 0 ? 'var(--profit)' : 'var(--loss)' }">
          {{ parseFloat(stats.avgPnlPercent) >= 0 ? '+' : '' }}{{ stats.avgPnlPercent }}%
        </div>
      </div>
    </div>

    <!-- 过滤器 -->
    <div class="card">
      <div class="flex items-center gap-4 flex-wrap">
        <div class="flex items-center gap-2">
          <Calendar class="w-4 h-4" style="color: var(--text-muted)" />
          <span class="text-sm" style="color: var(--text-secondary)">时间范围:</span>
          <div class="flex gap-1">
            <button
              v-for="range in [
                { value: '1d', label: '1天' },
                { value: '7d', label: '7天' },
                { value: '30d', label: '30天' },
                { value: 'all', label: '全部' }
              ]"
              :key="range.value"
              @click="timeRange = range.value as any"
              class="px-3 py-1.5 rounded-lg text-sm transition-all"
              :style="timeRange === range.value
                ? 'background: var(--primary-bg); color: var(--primary)'
                : 'background: var(--surface-tertiary); color: var(--text-secondary)'"
            >
              {{ range.label }}
            </button>
          </div>
        </div>
        
        <div class="flex items-center gap-2">
          <Filter class="w-4 h-4" style="color: var(--text-muted)" />
          <span class="text-sm" style="color: var(--text-secondary)">类型:</span>
          <div class="flex gap-1">
            <button
              v-for="type in [
                { value: 'all', label: '全部' },
                { value: 'long', label: '做多' },
                { value: 'short', label: '做空' }
              ]"
              :key="type.value"
              @click="tradeType = type.value as any"
              class="px-3 py-1.5 rounded-lg text-sm transition-all"
              :style="tradeType === type.value
                ? 'background: var(--primary-bg); color: var(--primary)'
                : 'background: var(--surface-tertiary); color: var(--text-secondary)'"
            >
              {{ type.label }}
            </button>
          </div>
        </div>
        
        <div class="flex items-center gap-2">
          <span class="text-sm" style="color: var(--text-secondary)">状态:</span>
          <div class="flex gap-1">
            <button
              v-for="s in [
                { value: 'all', label: '全部' },
                { value: 'open', label: '进行中' },
                { value: 'closed', label: '已平仓' }
              ]"
              :key="s.value"
              @click="status = s.value as any"
              class="px-3 py-1.5 rounded-lg text-sm transition-all"
              :style="status === s.value
                ? 'background: var(--primary-bg); color: var(--primary)'
                : 'background: var(--surface-tertiary); color: var(--text-secondary)'"
            >
              {{ s.label }}
            </button>
          </div>
        </div>
      </div>
    </div>

    <!-- PnL 曲线 -->
    <div class="card">
      <h3 class="font-medium mb-4" style="color: var(--text-primary)">累计盈亏曲线</h3>
      <div class="h-64 flex items-center justify-center" style="color: var(--text-muted)">
        <div class="text-center">
          <TrendingUp class="w-12 h-12 mx-auto mb-2 opacity-50" />
          <p>图表区域 - 可集成 ECharts/Chart.js</p>
        </div>
      </div>
    </div>

    <!-- 交易列表 -->
    <div class="card">
      <h3 class="font-medium mb-4" style="color: var(--text-primary)">交易记录</h3>
      
      <div v-if="loading" class="space-y-3">
        <div v-for="i in 3" :key="i" class="h-16 rounded-lg animate-pulse" style="background: var(--surface-tertiary)" />
      </div>
      
      <div v-else-if="filteredTrades.length === 0" class="py-12 text-center" style="color: var(--text-muted)">
        <Activity class="w-12 h-12 mx-auto mb-3 opacity-50" />
        <p>暂无交易记录</p>
      </div>
      
      <div v-else class="space-y-3">
        <div
          v-for="trade in filteredTrades"
          :key="trade.id"
          class="p-4 rounded-xl border transition-all cursor-pointer hover:shadow-card-hover"
          style="border-color: var(--border);"
        >
          <div class="flex items-center justify-between">
            <div class="flex items-center gap-4">
              <div
                class="w-10 h-10 rounded-xl flex items-center justify-center"
                :style="{
                  background: trade.type === 'long' ? 'var(--profit-light)' : 'var(--loss-light)'
                }"
              >
                <ArrowLeftRight
                  class="w-5 h-5"
                  :style="{ color: trade.type === 'long' ? 'var(--profit)' : 'var(--loss)' }"
                  :class="{ 'rotate-45': trade.type === 'short' }"
                />
              </div>
              
              <div>
                <div class="flex items-center gap-2">
                  <span class="font-medium" style="color: var(--text-primary)">{{ trade.symbol }}</span>
                  <span class="badge" :class="trade.type === 'long' ? 'badge-profit' : 'badge-loss'">
                    {{ trade.type === 'long' ? '做多' : '做空' }}
                  </span>
                  <span v-if="trade.status === 'open'" class="badge badge-primary">
                    进行中
                  </span>
                </div>
                <div class="text-sm mt-1" style="color: var(--text-muted)">
                  <span class="flex items-center gap-1">
                    <Clock class="w-3 h-3" />
                    {{ formatTime(trade.entryTime) }}
                  </span>
                </div>
              </div>
            </div>
            
            <div class="text-right">
              <div
                class="text-lg font-bold"
                :style="{ color: trade.pnl >= 0 ? 'var(--profit)' : 'var(--loss)' }"
              >
                {{ trade.pnl >= 0 ? '+' : '' }}{{ trade.pnl.toFixed(2) }}
                <span class="text-sm font-normal">
                  ({{ trade.pnlPercent >= 0 ? '+' : '' }}{{ trade.pnlPercent.toFixed(1) }}%)
                </span>
              </div>
              <div class="text-sm mt-1" style="color: var(--text-muted)">
                入场: {{ trade.entryPrice }} → 出场: {{ trade.exitPrice || '-' }}
              </div>
            </div>
            
            <div class="ml-4">
              <ChevronRight class="w-5 h-5" style="color: var(--text-muted)" />
            </div>
          </div>
          
          <div class="mt-3 pt-3 border-t" style="border-color: var(--border)">
            <div class="grid grid-cols-2 md:grid-cols-4 gap-4 text-sm">
              <div>
                <span style="color: var(--text-muted)">持仓时间</span>
                <div style="color: var(--text-primary)">{{ trade.duration }}</div>
              </div>
              <div>
                <span style="color: var(--text-muted)">仓位大小</span>
                <div style="color: var(--text-primary)">{{ trade.size }}</div>
              </div>
              <div>
                <span style="color: var(--text-muted)">置信度</span>
                <div style="color: var(--text-primary)">{{ (trade.confidence * 100).toFixed(0) }}%</div>
              </div>
              <div>
                <span style="color: var(--text-muted)">交易理由</span>
                <div style="color: var(--text-primary)" class="truncate">{{ trade.reason }}</div>
              </div>
            </div>
          </div>
        </div>
      </div>
    </div>
  </div>
</template>

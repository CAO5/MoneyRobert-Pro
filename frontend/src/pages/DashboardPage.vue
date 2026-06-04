<script setup lang="ts">
import { ref, computed, onMounted } from 'vue'
import api from '@/api'
import { TrendingUp, TrendingDown, Wallet, Target, Activity, ArrowUpRight, ArrowDownRight, RefreshCw, ChevronRight } from 'lucide-vue-next'

const metrics = ref({ total_equity: 0, today_pnl: 0, open_positions: 0, active_strategies: 0 })
const positions = ref<any[]>([])
const loading = ref(true)
const refreshing = ref(false)

const loadData = async () => {
  try {
    const [metricsRes, positionsRes] = await Promise.all([
      api.get('/dashboard/metrics'),
      api.get('/dashboard/positions'),
    ])
    metrics.value = metricsRes.data
    positions.value = Array.isArray(positionsRes.data) ? positionsRes.data : (positionsRes.data.positions || positionsRes.data.items || [])
  } catch (e) {
    console.error('Failed to load dashboard data', e)
  } finally {
    loading.value = false
    refreshing.value = false
  }
}

const handleRefresh = async () => {
  refreshing.value = true
  await loadData()
}

onMounted(loadData)

const statCards = computed(() => [
  {
    label: '总权益',
    value: metrics.value.total_equity,
    prefix: '$',
    icon: Wallet,
    color: 'primary',
    change: '+2.5%',
    positive: true
  },
  {
    label: '今日盈亏',
    value: metrics.value.today_pnl,
    prefix: '$',
    icon: metrics.value.today_pnl >= 0 ? TrendingUp : TrendingDown,
    color: metrics.value.today_pnl >= 0 ? 'profit' : 'loss',
    change: metrics.value.today_pnl >= 0 ? '+$128.50' : '-$45.20',
    positive: metrics.value.today_pnl >= 0
  },
  {
    label: '持仓数量',
    value: metrics.value.open_positions,
    prefix: '',
    icon: Target,
    color: 'neutral',
    change: '3 多 / 2 空',
    positive: true
  },
  {
    label: '活跃策略',
    value: metrics.value.active_strategies,
    prefix: '',
    icon: Activity,
    color: 'primary',
    change: '运行中',
    positive: true
  },
])

function formatNumber(n: number, prefix = ''): string {
  if (typeof n !== 'number' || isNaN(n)) return `${prefix}0.00`
  return `${prefix}${n.toLocaleString('en-US', { minimumFractionDigits: 2, maximumFractionDigits: 2 })}`
}

function formatPrice(n: number): string {
  if (typeof n !== 'number' || isNaN(n)) return '0.00'
  return n.toLocaleString('en-US', { minimumFractionDigits: 4, maximumFractionDigits: 4 })
}
</script>

<template>
  <div class="space-y-6 animate-fade-in">
    <!-- Header -->
    <div class="flex items-center justify-between">
      <div>
        <h1 class="text-2xl font-bold" style="color: var(--text-primary)">仪表盘</h1>
        <p class="text-sm mt-1" style="color: var(--text-secondary)">您的投资组合概览与实时数据</p>
      </div>
      <button
        @click="handleRefresh"
        :disabled="refreshing"
        class="btn btn-secondary"
      >
        <RefreshCw class="w-4 h-4" :class="{ 'animate-spin': refreshing }" />
        刷新数据
      </button>
    </div>

    <!-- Stats Grid -->
    <div v-if="loading" class="grid grid-cols-1 sm:grid-cols-2 lg:grid-cols-4 gap-4">
      <div v-for="i in 4" :key="i" class="card p-5 animate-pulse">
        <div class="h-4 w-20 rounded mb-3" style="background: var(--surface-tertiary)"></div>
        <div class="h-8 w-32 rounded" style="background: var(--surface-tertiary)"></div>
      </div>
    </div>

    <div v-else class="grid grid-cols-1 sm:grid-cols-2 lg:grid-cols-4 gap-4">
      <div
        v-for="(card, index) in statCards"
        :key="card.label"
        class="card p-5 group cursor-default animate-fade-in-up"
        :style="{ animationDelay: `${index * 100}ms` }"
      >
        <div class="flex items-start justify-between mb-3">
          <span class="text-sm font-medium" style="color: var(--text-secondary)">{{ card.label }}</span>
          <div
            class="w-10 h-10 rounded-xl flex items-center justify-center transition-transform group-hover:scale-110"
            :style="{
              background: card.color === 'primary' ? 'var(--primary-bg)' :
                         card.color === 'profit' ? 'var(--profit-light)' :
                         card.color === 'loss' ? 'var(--loss-light)' : 'var(--surface-tertiary)',
              color: card.color === 'primary' ? 'var(--primary)' :
                     card.color === 'profit' ? 'var(--profit)' :
                     card.color === 'loss' ? 'var(--loss)' : 'var(--text-secondary)'
            }"
          >
            <component :is="card.icon" class="w-5 h-5" />
          </div>
        </div>
        <div class="flex items-end justify-between">
          <div>
            <div class="text-2xl font-bold font-mono" style="color: var(--text-primary)">
              {{ formatNumber(card.value, card.prefix) }}
            </div>
            <div
              class="text-xs font-medium mt-1"
              :style="{ color: card.positive ? 'var(--profit)' : 'var(--loss)' }"
            >
              {{ card.change }}
            </div>
          </div>
        </div>
      </div>
    </div>

    <!-- Main Content Grid -->
    <div class="grid grid-cols-1 lg:grid-cols-3 gap-6">
      <!-- Positions Table -->
      <div class="lg:col-span-2 card">
        <div class="flex items-center justify-between p-5" style="border-bottom: 1px solid var(--border)">
          <h2 class="text-lg font-semibold" style="color: var(--text-primary)">持仓摘要</h2>
          <router-link to="/trading" class="text-sm font-medium flex items-center gap-1" style="color: var(--primary)">
            查看全部 <ChevronRight class="w-4 h-4" />
          </router-link>
        </div>

        <div v-if="positions.length === 0" class="p-12 text-center">
          <Target class="w-16 h-16 mx-auto mb-4" style="color: var(--text-muted); opacity: 0.3" />
          <p class="text-sm" style="color: var(--text-muted)">暂无持仓</p>
          <router-link to="/trading" class="btn btn-primary mt-4">
            开始交易
          </router-link>
        </div>

        <div v-else class="table-container border-0 rounded-none">
          <table class="table">
            <thead>
              <tr>
                <th>交易对</th>
                <th>方向</th>
                <th class="text-right">数量</th>
                <th class="text-right">入场价</th>
                <th class="text-right">未实现盈亏</th>
              </tr>
            </thead>
            <tbody>
              <tr v-for="pos in positions" :key="pos.symbol">
                <td>
                  <div class="font-semibold" style="color: var(--text-primary)">{{ pos.symbol }}</div>
                </td>
                <td>
                  <span class="badge" :class="pos.side === 'long' ? 'badge-profit' : 'badge-loss'">
                    {{ pos.side === 'long' ? '做多' : '做空' }}
                  </span>
                </td>
                <td class="text-right font-mono" style="color: var(--text-primary)">{{ pos.size }}</td>
                <td class="text-right font-mono" style="color: var(--text-secondary)">{{ formatPrice(pos.entry_price) }}</td>
                <td class="text-right font-mono font-semibold" :style="{ color: pos.unrealized_pnl >= 0 ? 'var(--profit)' : 'var(--loss)' }">
                  {{ formatNumber(pos.unrealized_pnl, '$') }}
                </td>
              </tr>
            </tbody>
          </table>
        </div>
      </div>

      <!-- Quick Actions -->
      <div class="card">
        <div class="p-5" style="border-bottom: 1px solid var(--border)">
          <h2 class="text-lg font-semibold" style="color: var(--text-primary)">快捷操作</h2>
        </div>
        <div class="p-5 space-y-3">
          <router-link to="/trading" class="btn btn-primary w-full py-3">
            <ArrowUpRight class="w-4 h-4" />
            开始交易
          </router-link>
          <router-link to="/market" class="btn btn-secondary w-full py-3">
            <Activity class="w-4 h-4" />
            行情分析
          </router-link>
          <router-link to="/ai" class="btn btn-secondary w-full py-3">
            <TrendingUp class="w-4 h-4" />
            AI 辩论分析
          </router-link>
        </div>

        <!-- Market Status -->
        <div class="p-5" style="border-top: 1px solid var(--border)">
          <h3 class="text-sm font-semibold mb-3" style="color: var(--text-secondary)">市场状态</h3>
          <div class="space-y-2">
            <div class="flex items-center justify-between text-sm">
              <span style="color: var(--text-secondary)">BTC/USDT</span>
              <span class="font-mono font-semibold" style="color: var(--profit)">+2.34%</span>
            </div>
            <div class="flex items-center justify-between text-sm">
              <span style="color: var(--text-secondary)">ETH/USDT</span>
              <span class="font-mono font-semibold" style="color: var(--profit)">+1.87%</span>
            </div>
            <div class="flex items-center justify-between text-sm">
              <span style="color: var(--text-secondary)">DOGE/USDT</span>
              <span class="font-mono font-semibold" style="color: var(--loss)">-0.52%</span>
            </div>
          </div>
        </div>
      </div>
    </div>
  </div>
</template>

<script setup lang="ts">
import { ref, computed, onMounted } from 'vue'
import api from '@/api'
import { TrendingUp, TrendingDown, Wallet, Target, Activity, ArrowUpRight, ArrowDownRight } from 'lucide-vue-next'

const metrics = ref({ total_equity: 0, today_pnl: 0, open_positions: 0, active_strategies: 0 })
const positions = ref<any[]>([])
const loading = ref(true)

onMounted(async () => {
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
  }
})

const statCards = computed(() => [
  { label: '总权益', value: metrics.value.total_equity, prefix: '$', icon: Wallet, color: 'var(--gold)' },
  { label: '今日盈亏', value: metrics.value.today_pnl, prefix: '$', icon: metrics.value.today_pnl >= 0 ? TrendingUp : TrendingDown, color: metrics.value.today_pnl >= 0 ? 'var(--profit)' : 'var(--loss)' },
  { label: '持仓数', value: metrics.value.open_positions, prefix: '', icon: Target, color: 'var(--text-primary)' },
  { label: '活跃策略', value: metrics.value.active_strategies, prefix: '', icon: Activity, color: 'var(--text-primary)' },
])

function formatNumber(n: number, prefix = ''): string {
  if (typeof n !== 'number' || isNaN(n)) return `${prefix}0.00`
  return `${prefix}${n.toLocaleString('en-US', { minimumFractionDigits: 2, maximumFractionDigits: 2 })}`
}
</script>

<template>
  <div class="space-y-6">
    <div>
      <h1 class="font-display text-2xl font-bold" style="color: var(--text-primary)">仪表盘</h1>
      <p class="text-sm mt-1" style="color: var(--text-secondary)">您的投资组合概览</p>
    </div>

    <div v-if="loading" class="grid grid-cols-4 gap-4">
      <div v-for="i in 4" :key="i" class="card animate-pulse h-28"></div>
    </div>

    <div v-else class="grid grid-cols-4 gap-4">
      <div v-for="card in statCards" :key="card.label" class="card group cursor-default">
        <div class="flex items-center justify-between mb-3">
          <span class="text-sm" style="color: var(--text-secondary)">{{ card.label }}</span>
          <div class="w-8 h-8 rounded-lg flex items-center justify-center" :style="{ background: card.color + '15', color: card.color }">
            <component :is="card.icon" class="w-4 h-4" />
          </div>
        </div>
        <div class="stat-value" :style="{ color: card.color }">{{ formatNumber(card.value, card.prefix) }}</div>
      </div>
    </div>

    <div class="grid grid-cols-3 gap-6">
      <div class="col-span-2 card">
        <h2 class="text-lg font-semibold mb-4" style="color: var(--text-primary)">持仓摘要</h2>
        <div v-if="positions.length === 0" class="py-12 text-center" style="color: var(--text-muted)">
          <Target class="w-12 h-12 mx-auto mb-3 opacity-30" />
          <p>暂无持仓</p>
        </div>
        <table v-else class="w-full">
          <thead>
            <tr class="text-xs uppercase" style="color: var(--text-muted)">
              <th class="text-left py-3 font-medium">交易对</th>
              <th class="text-left py-3 font-medium">方向</th>
              <th class="text-right py-3 font-medium">数量</th>
              <th class="text-right py-3 font-medium">入场价</th>
              <th class="text-right py-3 font-medium">未实现盈亏</th>
            </tr>
          </thead>
          <tbody>
            <tr v-for="pos in positions" :key="pos.symbol" class="border-t" style="border-color: var(--border)">
              <td class="py-3 font-medium">{{ pos.symbol }}</td>
              <td class="py-3">
                <span class="badge" :class="pos.side === 'long' ? 'badge-profit' : 'badge-loss'">
                  {{ pos.side === 'long' ? '做多' : '做空' }}
                </span>
              </td>
              <td class="py-3 text-right font-mono">{{ pos.size }}</td>
              <td class="py-3 text-right font-mono">{{ pos.entry_price }}</td>
              <td class="py-3 text-right font-mono" :style="{ color: pos.unrealized_pnl >= 0 ? 'var(--profit)' : 'var(--loss)' }">
                {{ formatNumber(pos.unrealized_pnl, '$') }}
              </td>
            </tr>
          </tbody>
        </table>
      </div>

      <div class="card">
        <h2 class="text-lg font-semibold mb-4" style="color: var(--text-primary)">快捷操作</h2>
        <div class="space-y-3">
          <router-link to="/trading" class="btn-primary w-full py-3 flex items-center justify-center gap-2">
            <ArrowUpRight class="w-4 h-4" /> 开始交易
          </router-link>
          <router-link to="/ai" class="btn-secondary w-full py-3 flex items-center justify-center gap-2">
            AI 分析
          </router-link>
          <router-link to="/strategies" class="btn-secondary w-full py-3 flex items-center justify-center gap-2">
            策略管理
          </router-link>
        </div>
      </div>
    </div>
  </div>
</template>

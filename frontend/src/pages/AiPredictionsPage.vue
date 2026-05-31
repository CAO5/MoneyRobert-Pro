<script setup lang="ts">
import { ref, onMounted } from 'vue'
import api from '@/api'
import { Target, CheckCircle, XCircle } from 'lucide-vue-next'

const predictions = ref<any[]>([])
const stats = ref({ total: 0, correct: 0, win_rate: 0 })
const loading = ref(true)

onMounted(async () => {
  try {
    const [predRes, statRes] = await Promise.all([api.get('/ai/prediction'), api.get('/ai/prediction/statistics')])
    predictions.value = predRes.data.items || predRes.data.predictions || predRes.data || []
    stats.value = statRes.data.statistics || statRes.data || stats.value
  } catch (e) {
    console.error('Failed to load predictions', e)
  } finally {
    loading.value = false
  }
})

function statusIcon(s: string) {
  if (s === 'correct' || s === 'hit') return CheckCircle
  if (s === 'incorrect' || s === 'miss') return XCircle
  return Target
}

function statusColor(s: string) {
  if (s === 'correct' || s === 'hit') return 'var(--profit)'
  if (s === 'incorrect' || s === 'miss') return 'var(--loss)'
  return 'var(--gold)'
}
</script>

<template>
  <div class="space-y-6">
    <div class="flex items-center gap-3">
      <Target class="w-6 h-6" style="color: var(--gold)" />
      <h1 class="font-display text-2xl font-bold" style="color: var(--text-primary)">AI 预测</h1>
    </div>

    <div v-if="loading" class="grid grid-cols-3 gap-4">
      <div v-for="i in 3" :key="i" class="card animate-pulse h-24"></div>
    </div>

    <template v-else>
      <div class="grid grid-cols-3 gap-4">
        <div class="card">
          <div class="text-sm mb-2" style="color: var(--text-secondary)">总预测数</div>
          <div class="stat-value" style="color: var(--text-primary)">{{ stats.total }}</div>
        </div>
        <div class="card">
          <div class="text-sm mb-2" style="color: var(--text-secondary)">正确数</div>
          <div class="stat-value" style="color: var(--profit)">{{ stats.correct }}</div>
        </div>
        <div class="card">
          <div class="text-sm mb-2" style="color: var(--text-secondary)">胜率</div>
          <div class="stat-value" style="color: var(--gold)">{{ stats.win_rate?.toFixed(1) }}%</div>
        </div>
      </div>

      <div class="card">
        <h2 class="text-lg font-semibold mb-4" style="color: var(--text-primary)">预测列表</h2>
        <div v-if="predictions.length === 0" class="py-12 text-center" style="color: var(--text-muted)">暂无预测</div>
        <div v-else class="space-y-3">
          <div v-for="p in predictions" :key="p.id" class="flex items-center justify-between p-3 rounded-lg" style="background: var(--bg-primary)">
            <div class="flex items-center gap-3">
              <component :is="statusIcon(p.status)" class="w-5 h-5" :style="{ color: statusColor(p.status) }" />
              <div>
                <div class="font-medium" style="color: var(--text-primary)">{{ p.symbol }}</div>
                <div class="text-xs" style="color: var(--text-muted)">{{ new Date(p.created_at).toLocaleString('zh-CN') }}</div>
              </div>
            </div>
            <div class="flex items-center gap-4 text-sm">
              <span class="badge" :class="p.direction === 'long' ? 'badge-profit' : 'badge-loss'">{{ p.direction === 'long' ? '多' : '空' }}</span>
              <span class="font-mono" style="color: var(--gold)">{{ p.confidence }}%</span>
              <span class="badge badge-neutral">{{ p.risk_level }}</span>
              <span class="badge" :class="statusColor(p.status) === 'var(--profit)' ? 'badge-profit' : statusColor(p.status) === 'var(--loss)' ? 'badge-loss' : 'badge-gold'">{{ p.status }}</span>
            </div>
          </div>
        </div>
      </div>
    </template>
  </div>
</template>

<script setup lang="ts">
import { ref, onMounted } from 'vue'
import api from '@/api'
import { Brain, TrendingUp, Shield } from 'lucide-vue-next'

const symbol = ref('BTCUSDT')
const interval = ref('1H')
const activeTab = ref('technical')
const result = ref<any>(null)
const loading = ref(false)

const intervals = ['1m', '5m', '15m', '30m', '1H', '4H', '1D', '1W']
const tabs = [
  { key: 'technical', label: '技术分析' },
  { key: 'funding', label: '资金分析' },
  { key: 'sentiment', label: '情绪分析' },
  { key: 'comprehensive', label: '综合分析' },
]

const endpoints: Record<string, string> = {
  technical: '/ai/analyze/technical',
  funding: '/ai/analyze/funding',
  sentiment: '/ai/analyze/sentiment',
  comprehensive: '/ai/analyze/comprehensive',
}

async function analyze() {
  loading.value = true
  result.value = null
  try {
    const { data } = await api.post(endpoints[activeTab.value], { symbol: symbol.value, interval: interval.value })
    result.value = data
  } catch (e) {
    console.error('Analysis failed', e)
  } finally {
    loading.value = false
  }
}

onMounted(() => { analyze() })

function directionColor(d: string) {
  if (d === 'long' || d === '多') return 'var(--profit)'
  if (d === 'short' || d === '空') return 'var(--loss)'
  return 'var(--gold)'
}
</script>

<template>
  <div class="space-y-6">
    <div class="flex items-center gap-3">
      <Brain class="w-6 h-6" style="color: var(--gold)" />
      <h1 class="font-display text-2xl font-bold" style="color: var(--text-primary)">AI 分析</h1>
    </div>

    <div class="card flex flex-wrap items-end gap-4">
      <div class="flex-1 min-w-[200px]">
        <label class="text-sm mb-1 block" style="color: var(--text-secondary)">交易对</label>
        <input v-model="symbol" class="input-field" placeholder="BTCUSDT" />
      </div>
      <div>
        <label class="text-sm mb-1 block" style="color: var(--text-secondary)">时间周期</label>
        <div class="flex gap-1">
          <button v-for="iv in intervals" :key="iv" @click="interval = iv"
            class="px-3 py-1.5 rounded text-xs font-medium transition-colors"
            :style="interval === iv ? 'background: var(--gold); color: var(--bg-primary)' : 'background: var(--border); color: var(--text-secondary)'">
            {{ iv }}
          </button>
        </div>
      </div>
      <button @click="analyze" class="btn-primary flex items-center gap-2" :disabled="loading">
        <TrendingUp class="w-4 h-4" /> 分析
      </button>
    </div>

    <div class="flex gap-2">
      <button v-for="tab in tabs" :key="tab.key" @click="activeTab = tab.key; analyze()"
        class="px-4 py-2 rounded-lg text-sm font-medium transition-colors"
        :style="activeTab === tab.key ? 'background: var(--gold); color: var(--bg-primary)' : 'background: var(--bg-card); color: var(--text-secondary); border: 1px solid var(--border)'">
        {{ tab.label }}
      </button>
    </div>

    <div v-if="loading" class="card animate-pulse h-64"></div>

    <div v-else-if="result" class="card space-y-4">
      <div v-if="activeTab === 'comprehensive' && result.direction" class="space-y-4">
        <div class="flex items-center gap-4">
          <span class="text-sm" style="color: var(--text-secondary)">方向</span>
          <span class="text-xl font-bold" :style="{ color: directionColor(result.direction) }">
            {{ result.direction === 'long' ? '做多' : result.direction === 'short' ? '做空' : '中性' }}
          </span>
        </div>
        <div>
          <div class="flex justify-between text-sm mb-1">
            <span style="color: var(--text-secondary)">置信度</span>
            <span class="font-mono" style="color: var(--gold)">{{ result.confidence }}%</span>
          </div>
          <div class="h-2 rounded-full" style="background: var(--border)">
            <div class="h-full rounded-full transition-all" :style="{ width: result.confidence + '%', background: 'var(--gold)' }"></div>
          </div>
        </div>
        <div class="grid grid-cols-2 gap-4">
          <div class="card" style="background: var(--bg-primary)">
            <div class="text-xs mb-1" style="color: var(--text-muted)">风险等级</div>
            <div class="flex items-center gap-2"><Shield class="w-4 h-4" style="color: var(--gold)" /><span style="color: var(--text-primary)">{{ result.risk_level }}</span></div>
          </div>
          <div class="card" style="background: var(--bg-primary)">
            <div class="text-xs mb-1" style="color: var(--text-muted)">杠杆建议</div>
            <span class="font-mono" style="color: var(--gold)">{{ result.leverage_suggestion }}x</span>
          </div>
          <div class="card" style="background: var(--bg-primary)">
            <div class="text-xs mb-1" style="color: var(--text-muted)">入场区间</div>
            <span class="font-mono" style="color: var(--text-primary)">{{ result.entry_range }}</span>
          </div>
          <div class="card" style="background: var(--bg-primary)">
            <div class="text-xs mb-1" style="color: var(--text-muted)">止损 / 止盈</div>
            <span class="font-mono" style="color: var(--loss)">{{ result.stop_loss }}</span>
            <span style="color: var(--text-muted)"> / </span>
            <span class="font-mono" style="color: var(--profit)">{{ result.take_profit }}</span>
          </div>
        </div>
      </div>
      <div v-else class="whitespace-pre-wrap text-sm" style="color: var(--text-secondary)">{{ typeof result === 'string' ? result : JSON.stringify(result, null, 2) }}</div>
    </div>
  </div>
</template>

<script setup lang="ts">
import { ref, onMounted } from 'vue'
import api from '@/api'
import { Bot, Power, PowerOff } from 'lucide-vue-next'

const configs = ref<any[]>([])
const sessions = ref<any[]>([])
const loading = ref(true)
const showForm = ref(false)
const form = ref({
  symbol: 'BTCUSDT', mode: 'paper' as 'paper' | 'live',
  max_position_size: 1000, max_leverage: 10, risk_ratio: 0.02,
  stop_loss_pct: 5, take_profit_pct: 10, ai_confidence_threshold: 0.7,
})

async function loadData() {
  try {
    const [cfgRes, sessRes] = await Promise.all([api.get('/auto-trading/configs'), api.get('/auto-trading/sessions')])
    configs.value = cfgRes.data.items || cfgRes.data.configs || cfgRes.data || []
    sessions.value = sessRes.data.items || sessRes.data.sessions || sessRes.data || []
  } catch (e) {
    console.error('Failed to load auto trading data', e)
  } finally {
    loading.value = false
  }
}

async function createConfig() {
  try {
    await api.post('/auto-trading/configs', form.value)
    showForm.value = false
    await loadData()
  } catch (e) {
    console.error('Create config failed', e)
  }
}

async function toggleConfig(id: string, enabled: boolean) {
  try {
    await api.post(`/auto-trading/configs/${id}/${enabled ? 'disable' : 'enable'}`)
    await loadData()
  } catch (e) {
    console.error('Toggle failed', e)
  }
}

onMounted(() => { loadData() })
</script>

<template>
  <div class="space-y-6">
    <div class="flex items-center justify-between">
      <div class="flex items-center gap-3">
        <Bot class="w-6 h-6" style="color: var(--primary)" />
        <h1 class="font-sans text-2xl font-bold" style="color: var(--text-primary)">自动交易</h1>
      </div>
      <button @click="showForm = !showForm" class="btn-primary flex items-center gap-2">
        <Power class="w-4 h-4" /> 新建配置
      </button>
    </div>

    <div v-if="showForm" class="card space-y-3">
      <div class="grid grid-cols-3 gap-3">
        <div>
          <label class="text-sm mb-1 block" style="color: var(--text-secondary)">交易对</label>
          <input v-model="form.symbol" class="input" />
        </div>
        <div>
          <label class="text-sm mb-1 block" style="color: var(--text-secondary)">模式</label>
          <div class="grid grid-cols-2 gap-2">
            <button @click="form.mode = 'paper'" class="py-2 rounded-lg text-sm" :style="form.mode === 'paper' ? 'background: var(--primary); color: var(--surface-secondary)' : 'background: var(--border); color: var(--text-secondary)'">模拟</button>
            <button @click="form.mode = 'live'" class="py-2 rounded-lg text-sm" :style="form.mode === 'live' ? 'background: var(--loss); color: #fff' : 'background: var(--border); color: var(--text-secondary)'">实盘</button>
          </div>
        </div>
        <div>
          <label class="text-sm mb-1 block" style="color: var(--text-secondary)">最大仓位</label>
          <input v-model.number="form.max_position_size" type="number" class="input" />
        </div>
        <div>
          <label class="text-sm mb-1 block" style="color: var(--text-secondary)">最大杠杆</label>
          <input v-model.number="form.max_leverage" type="number" class="input" />
        </div>
        <div>
          <label class="text-sm mb-1 block" style="color: var(--text-secondary)">风险比例</label>
          <input v-model.number="form.risk_ratio" type="number" step="0.01" class="input" />
        </div>
        <div>
          <label class="text-sm mb-1 block" style="color: var(--text-secondary)">止损 %</label>
          <input v-model.number="form.stop_loss_pct" type="number" class="input" />
        </div>
        <div>
          <label class="text-sm mb-1 block" style="color: var(--text-secondary)">止盈 %</label>
          <input v-model.number="form.take_profit_pct" type="number" class="input" />
        </div>
        <div>
          <label class="text-sm mb-1 block" style="color: var(--text-secondary)">AI置信度阈值</label>
          <input v-model.number="form.ai_confidence_threshold" type="number" step="0.05" min="0" max="1" class="input" />
        </div>
      </div>
      <div class="flex gap-2">
        <button @click="createConfig" class="btn-primary">确认创建</button>
        <button @click="showForm = false" class="btn-secondary">取消</button>
      </div>
    </div>

    <div v-if="loading" class="grid grid-cols-2 gap-4">
      <div v-for="i in 2" :key="i" class="card animate-pulse h-32"></div>
    </div>

    <template v-else>
      <div class="card">
        <h2 class="text-lg font-semibold mb-4" style="color: var(--text-primary)">配置列表</h2>
        <div v-if="configs.length === 0" class="py-8 text-center" style="color: var(--text-muted)">暂无配置</div>
        <div v-else class="space-y-3">
          <div v-for="c in configs" :key="c.id" class="flex items-center justify-between p-3 rounded-lg" style="background: var(--surface-secondary)">
            <div class="space-y-1">
              <div class="font-medium" style="color: var(--text-primary)">{{ c.symbol }}</div>
              <div class="text-xs space-x-3" style="color: var(--text-muted)">
                <span>模式: <span class="badge" :class="c.mode === 'live' ? 'badge-loss' : 'badge-primary'">{{ c.mode === 'live' ? '实盘' : '模拟' }}</span></span>
                <span>杠杆: {{ c.max_leverage }}x</span>
                <span>风险: {{ c.risk_ratio }}</span>
              </div>
            </div>
            <button @click="toggleConfig(c.id, c.enabled)" class="p-2 rounded-lg transition-colors hover:bg-[var(--surface-tertiary)]"
              :style="{ color: c.enabled ? 'var(--profit)' : 'var(--loss)' }">
              <component :is="c.enabled ? Power : PowerOff" class="w-5 h-5" />
            </button>
          </div>
        </div>
      </div>

      <div class="card">
        <h2 class="text-lg font-semibold mb-4" style="color: var(--text-primary)">会话列表</h2>
        <div v-if="sessions.length === 0" class="py-8 text-center" style="color: var(--text-muted)">暂无会话</div>
        <div v-else class="space-y-2">
          <div v-for="s in sessions" :key="s.id" class="flex items-center justify-between p-3 rounded-lg" style="background: var(--surface-secondary)">
            <div>
              <span class="font-medium" style="color: var(--text-primary)">{{ s.symbol || s.id }}</span>
              <span class="badge ml-2" :class="s.status === 'running' ? 'badge-profit' : 'badge-neutral'">{{ s.status }}</span>
            </div>
            <span class="text-xs font-mono" style="color: var(--text-muted)">{{ new Date(s.created_at).toLocaleString('zh-CN') }}</span>
          </div>
        </div>
      </div>
    </template>
  </div>
</template>

<script setup lang="ts">
import { ref, onMounted } from 'vue'
import api from '@/api'
import { TrendingUp, Play, Pause, Square, Trash2 } from 'lucide-vue-next'

const strategies = ref<any[]>([])
const loading = ref(true)
const showForm = ref(false)
const form = ref({ name: '', symbol: '', type: '', params: '{}' })

async function loadStrategies() {
  try {
    const { data } = await api.get('/strategies')
    strategies.value = data.items || data.strategies || data || []
  } catch (e) {
    console.error('Failed to load strategies', e)
  } finally {
    loading.value = false
  }
}

async function createStrategy() {
  try {
    await api.post('/strategies', { ...form.value, params: JSON.parse(form.value.params) })
    showForm.value = false
    form.value = { name: '', symbol: '', type: '', params: '{}' }
    await loadStrategies()
  } catch (e) {
    console.error('Create failed', e)
  }
}

async function action(id: string, action: string) {
  try {
    await api.post(`/strategies/${id}/${action}`)
    await loadStrategies()
  } catch (e) {
    console.error('Action failed', e)
  }
}

async function remove(id: string) {
  try {
    await api.delete(`/strategies/${id}`)
    await loadStrategies()
  } catch (e) {
    console.error('Delete failed', e)
  }
}

function statusBadge(s: string) {
  if (s === 'running') return 'badge-profit'
  if (s === 'paused') return 'badge-primary'
  if (s === 'cancelled') return 'badge-loss'
  return 'badge-neutral'
}

onMounted(() => { loadStrategies() })
</script>

<template>
  <div class="space-y-6">
    <div class="flex items-center justify-between">
      <div class="flex items-center gap-3">
        <TrendingUp class="w-6 h-6" style="color: var(--primary)" />
        <h1 class="text-2xl font-bold" style="color: var(--text-primary)">策略管理</h1>
      </div>
      <button @click="showForm = !showForm" class="btn-primary flex items-center gap-2">
        <Play class="w-4 h-4" /> 创建策略
      </button>
    </div>

    <div v-if="showForm" class="card space-y-3">
      <div class="grid grid-cols-2 gap-3">
        <div>
          <label class="text-sm mb-1 block" style="color: var(--text-secondary)">名称</label>
          <input v-model="form.name" class="input" />
        </div>
        <div>
          <label class="text-sm mb-1 block" style="color: var(--text-secondary)">交易对</label>
          <input v-model="form.symbol" class="input" placeholder="BTCUSDT" />
        </div>
        <div>
          <label class="text-sm mb-1 block" style="color: var(--text-secondary)">类型</label>
          <input v-model="form.type" class="input" placeholder="grid, dca..." />
        </div>
        <div>
          <label class="text-sm mb-1 block" style="color: var(--text-secondary)">参数 (JSON)</label>
          <input v-model="form.params" class="input font-mono text-sm" />
        </div>
      </div>
      <div class="flex gap-2">
        <button @click="createStrategy" class="btn-primary">确认创建</button>
        <button @click="showForm = false" class="btn-secondary">取消</button>
      </div>
    </div>

    <div v-if="loading" class="grid grid-cols-3 gap-4">
      <div v-for="i in 3" :key="i" class="card animate-pulse h-40"></div>
    </div>

    <div v-else-if="strategies.length === 0" class="card py-12 text-center" style="color: var(--text-muted)">暂无策略</div>

    <div v-else class="grid grid-cols-3 gap-4">
      <div v-for="s in strategies" :key="s.id" class="card space-y-3">
        <div class="flex items-center justify-between">
          <h3 class="font-semibold" style="color: var(--text-primary)">{{ s.name }}</h3>
          <span class="badge" :class="statusBadge(s.status)">{{ s.status }}</span>
        </div>
        <div class="text-sm space-y-1">
          <div class="flex justify-between"><span style="color: var(--text-muted)">交易对</span><span style="color: var(--text-secondary)">{{ s.symbol }}</span></div>
          <div class="flex justify-between"><span style="color: var(--text-muted)">类型</span><span style="color: var(--text-secondary)">{{ s.type }}</span></div>
          <div class="flex justify-between"><span style="color: var(--text-muted)">创建时间</span><span class="font-mono text-xs" style="color: var(--text-secondary)">{{ new Date(s.created_at).toLocaleDateString() }}</span></div>
        </div>
        <div class="flex gap-1 pt-2 border-t" style="border-color: var(--border)">
          <button v-if="s.status !== 'running'" @click="action(s.id, 'execute')" class="p-1.5 rounded hover:bg-[var(--surface-tertiary)]" style="color: var(--profit)"><Play class="w-4 h-4" /></button>
          <button v-if="s.status === 'running'" @click="action(s.id, 'pause')" class="p-1.5 rounded hover:bg-[var(--surface-tertiary)]" style="color: var(--primary)"><Pause class="w-4 h-4" /></button>
          <button v-if="s.status === 'paused'" @click="action(s.id, 'resume')" class="p-1.5 rounded hover:bg-[var(--surface-tertiary)]" style="color: var(--profit)"><Play class="w-4 h-4" /></button>
          <button v-if="s.status === 'running'" @click="action(s.id, 'cancel')" class="p-1.5 rounded hover:bg-[var(--surface-tertiary)]" style="color: var(--loss)"><Square class="w-4 h-4" /></button>
          <button @click="remove(s.id)" class="p-1.5 rounded hover:bg-[var(--surface-tertiary)] ml-auto" style="color: var(--loss)"><Trash2 class="w-4 h-4" /></button>
        </div>
      </div>
    </div>
  </div>
</template>

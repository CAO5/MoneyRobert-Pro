<script setup lang="ts">
import { ref, onMounted } from 'vue'
import api from '@/api'
import { Settings, Key } from 'lucide-vue-next'

const apiKeys = ref<any[]>([])
const loading = ref(true)
const showForm = ref(false)
const form = ref({ name: '', api_key: '', api_secret: '' })

async function loadApiKeys() {
  try {
    const { data } = await api.get('/api-keys')
    apiKeys.value = data.items || data.keys || data || []
  } catch (e) {
    console.error('Failed to load API keys', e)
  } finally {
    loading.value = false
  }
}

async function addKey() {
  try {
    await api.post('/api-keys', form.value)
    showForm.value = false
    form.value = { name: '', api_key: '', api_secret: '' }
    await loadApiKeys()
  } catch (e) {
    console.error('Add key failed', e)
  }
}

onMounted(() => { loadApiKeys() })
</script>

<template>
  <div class="space-y-6">
    <div class="flex items-center gap-3">
      <Settings class="w-6 h-6" style="color: var(--gold)" />
      <h1 class="font-display text-2xl font-bold" style="color: var(--text-primary)">个人设置</h1>
    </div>

    <div class="card">
      <div class="flex items-center justify-between mb-4">
        <div class="flex items-center gap-2">
          <Key class="w-5 h-5" style="color: var(--gold)" />
          <h2 class="text-lg font-semibold" style="color: var(--text-primary)">API 密钥管理</h2>
        </div>
        <button @click="showForm = !showForm" class="btn-primary">添加密钥</button>
      </div>

      <div v-if="showForm" class="p-4 rounded-lg mb-4 space-y-3" style="background: var(--bg-primary)">
        <div>
          <label class="text-sm mb-1 block" style="color: var(--text-secondary)">名称</label>
          <input v-model="form.name" class="input-field" placeholder="如：Binance Main" />
        </div>
        <div>
          <label class="text-sm mb-1 block" style="color: var(--text-secondary)">API Key</label>
          <input v-model="form.api_key" class="input-field font-mono text-sm" />
        </div>
        <div>
          <label class="text-sm mb-1 block" style="color: var(--text-secondary)">API Secret</label>
          <input v-model="form.api_secret" type="password" class="input-field font-mono text-sm" />
        </div>
        <div class="flex gap-2">
          <button @click="addKey" class="btn-primary">确认添加</button>
          <button @click="showForm = false" class="btn-secondary">取消</button>
        </div>
      </div>

      <div v-if="loading" class="space-y-3">
        <div v-for="i in 3" :key="i" class="h-16 rounded-lg animate-pulse" style="background: var(--bg-primary)"></div>
      </div>

      <div v-else-if="apiKeys.length === 0" class="py-8 text-center" style="color: var(--text-muted)">暂无 API 密钥</div>

      <div v-else class="space-y-3">
        <div v-for="k in apiKeys" :key="k.id" class="flex items-center justify-between p-3 rounded-lg" style="background: var(--bg-primary)">
          <div class="space-y-1">
            <div class="flex items-center gap-2">
              <span class="font-medium" style="color: var(--text-primary)">{{ k.name }}</span>
              <span class="badge" :class="k.is_active ? 'badge-profit' : 'badge-loss'">{{ k.is_active ? '活跃' : '停用' }}</span>
            </div>
            <div class="text-xs" style="color: var(--text-muted)">
              <span class="font-mono">{{ k.api_key?.slice(0, 8) }}****</span>
              <span class="ml-3">{{ new Date(k.created_at).toLocaleDateString() }}</span>
            </div>
          </div>
        </div>
      </div>
    </div>
  </div>
</template>

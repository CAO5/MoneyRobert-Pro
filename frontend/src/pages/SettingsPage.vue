<script setup lang="ts">
import { ref, onMounted, computed } from 'vue'
import api from '@/api'
import { Settings, Key, Trash2, RefreshCw, CheckCircle, XCircle, Loader2, Eye, EyeOff, Brain, Edit2, Star, Zap } from 'lucide-vue-next'

interface ApiKey {
  id: string
  name: string
  key_type: string
  api_key: string
  api_secret?: string
  passphrase?: string
  is_active: boolean
  is_demo?: boolean
  provider?: string
  created_at: string
  updated_at?: string
}

interface AiProvider {
  id: string
  provider: string
  api_key: string
  base_url: string
  model: string
  max_tokens: number
  temperature: number
  is_active: boolean
  is_default: boolean
  created_at: string
  updated_at?: string
}

const PROVIDER_PRESETS: Record<string, { base_url: string; model: string }> = {
  openai: { base_url: 'https://api.openai.com/v1', model: 'gpt-4o-mini' },
  deepseek: { base_url: 'https://api.deepseek.com/v1', model: 'deepseek-chat' },
  anthropic: { base_url: 'https://api.anthropic.com/v1', model: 'claude-3-haiku-20240307' },
  custom: { base_url: '', model: '' }
}

const PROVIDER_LABELS: Record<string, string> = {
  openai: 'OpenAI',
  deepseek: 'DeepSeek',
  anthropic: 'Anthropic',
  custom: '自定义'
}

const apiKeys = ref<ApiKey[]>([])
const aiProviders = ref<AiProvider[]>([])
const loading = ref(true)
const providersLoading = ref(true)
const showForm = ref(false)
const submitting = ref(false)
const testingId = ref<string | null>(null)
const testResult = ref<{ id: string; success: boolean; message: string } | null>(null)

const form = ref<{
  name: string
  key_type: string
  api_key: string
  api_secret: string
  passphrase: string
  is_demo: boolean
}>({
  name: '',
  key_type: 'exchange',
  api_key: '',
  api_secret: '',
  passphrase: '',
  is_demo: false
})

const showSecret = ref(false)

// AI Provider form
const showProviderForm = ref(false)
const editingProviderId = ref<string | null>(null)
const providerSubmitting = ref(false)
const providerTestingId = ref<string | null>(null)
const providerTestResult = ref<{ id: string; success: boolean; message: string } | null>(null)

const providerForm = ref<{
  provider: string
  api_key: string
  base_url: string
  model: string
  max_tokens: number
  temperature: number
}>({
  provider: 'openai',
  api_key: '',
  base_url: 'https://api.openai.com/v1',
  model: 'gpt-4o-mini',
  max_tokens: 4096,
  temperature: 0.7
})

const showProviderSecret = ref(false)

const exchangeKeys = computed(() => apiKeys.value.filter(k => k.key_type === 'exchange'))
const aiKeys = computed(() => apiKeys.value.filter(k => k.key_type === 'ai_provider'))

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

async function loadAiProviders() {
  try {
    const { data } = await api.get('/ai/providers')
    aiProviders.value = data.items || data.providers || data || []
  } catch (e) {
    console.error('Failed to load AI providers', e)
  } finally {
    providersLoading.value = false
  }
}

async function addKey() {
  if (!form.value.name || !form.value.api_key || !form.value.api_secret) {
    alert('请填写所有必填字段')
    return
  }
  
  submitting.value = true
  try {
    await api.post('/api-keys', {
      name: form.value.name,
      key_type: form.value.key_type,
      api_key: form.value.api_key,
      api_secret: form.value.api_secret,
      passphrase: form.value.passphrase,
      is_demo: form.value.is_demo
    })
    showForm.value = false
    form.value = { name: '', key_type: 'exchange', api_key: '', api_secret: '', passphrase: '', is_demo: true }
    await loadApiKeys()
  } catch (e: any) {
    console.error('Add key failed', e)
    alert('添加失败: ' + (e.response?.data?.message || e.message))
  } finally {
    submitting.value = false
  }
}

async function deleteKey(id: string) {
  if (!confirm('确定要删除这个 API 密钥吗？')) return
  try {
    await api.delete(`/api-keys/${id}`)
    await loadApiKeys()
  } catch (e: any) {
    console.error('Delete key failed', e)
    alert('删除失败: ' + (e.response?.data?.message || e.message))
  }
}

async function toggleKey(id: string) {
  try {
    await api.post(`/api-keys/${id}/toggle`)
    await loadApiKeys()
  } catch (e: any) {
    console.error('Toggle key failed', e)
    alert('切换状态失败: ' + (e.response?.data?.message || e.message))
  }
}

async function testKey(id: string) {
  testingId.value = id
  testResult.value = null
  try {
    const { data } = await api.post(`/api-keys/${id}/test`)
    testResult.value = { id, success: data.success, message: data.message }
  } catch (e: any) {
    testResult.value = { 
      id, 
      success: false, 
      message: e.response?.data?.message || '测试失败，请检查密钥是否正确' 
    }
  } finally {
    testingId.value = null
  }
}

// AI Provider CRUD
function openProviderForm(provider?: AiProvider) {
  if (provider) {
    editingProviderId.value = provider.id
    providerForm.value = {
      provider: provider.provider,
      api_key: provider.api_key,
      base_url: provider.base_url,
      model: provider.model,
      max_tokens: provider.max_tokens,
      temperature: provider.temperature
    }
  } else {
    editingProviderId.value = null
    providerForm.value = {
      provider: 'openai',
      api_key: '',
      base_url: 'https://api.openai.com/v1',
      model: 'gpt-4o-mini',
      max_tokens: 4096,
      temperature: 0.7
    }
  }
  showProviderForm.value = true
}

function onProviderTypeChange() {
  const preset = PROVIDER_PRESETS[providerForm.value.provider]
  if (preset) {
    providerForm.value.base_url = preset.base_url
    providerForm.value.model = preset.model
  }
}

async function saveProvider() {
  if (!providerForm.value.api_key) {
    alert('请填写 API Key')
    return
  }
  if (!providerForm.value.base_url || !providerForm.value.model) {
    alert('请填写 Base URL 和模型名称')
    return
  }
  
  providerSubmitting.value = true
  try {
    const payload = { ...providerForm.value }
    if (editingProviderId.value) {
      await api.put(`/ai/providers/${editingProviderId.value}`, payload)
    } else {
      await api.post('/ai/providers', payload)
    }
    showProviderForm.value = false
    editingProviderId.value = null
    await loadAiProviders()
  } catch (e: any) {
    console.error('Save AI provider failed', e)
    alert('保存失败: ' + (e.response?.data?.message || e.message))
  } finally {
    providerSubmitting.value = false
  }
}

async function deleteProvider(id: string) {
  if (!confirm('确定要删除这个 AI 提供商配置吗？')) return
  try {
    await api.delete(`/ai/providers/${id}`)
    await loadAiProviders()
  } catch (e: any) {
    console.error('Delete AI provider failed', e)
    alert('删除失败: ' + (e.response?.data?.message || e.message))
  }
}

async function testProvider(id: string) {
  providerTestingId.value = id
  providerTestResult.value = null
  try {
    const { data } = await api.post(`/ai/providers/${id}/test`)
    providerTestResult.value = { id, success: data.success ?? true, message: data.message || '连接成功' }
  } catch (e: any) {
    providerTestResult.value = {
      id,
      success: false,
      message: e.response?.data?.message || e.message || '连接测试失败'
    }
  } finally {
    providerTestingId.value = null
  }
}

async function setDefaultProvider(id: string) {
  try {
    await api.put(`/ai/providers/${id}`, { is_default: true })
    await loadAiProviders()
  } catch (e: any) {
    console.error('Set default provider failed', e)
    alert('设置默认失败: ' + (e.response?.data?.message || e.message))
  }
}

function formatDate(dateStr: string) {
  if (!dateStr) return ''
  return new Date(dateStr).toLocaleDateString('zh-CN', {
    year: 'numeric',
    month: '2-digit',
    day: '2-digit'
  })
}

function maskKey(key: string) {
  if (!key || key.length < 8) return '****'
  return key.slice(0, 8) + '****'
}

onMounted(() => {
  loadApiKeys()
  loadAiProviders()
})
</script>

<template>
  <div class="space-y-6">
    <div class="flex items-center gap-3">
      <Settings class="w-6 h-6" style="color: var(--gold)" />
      <h1 class="font-display text-2xl font-bold" style="color: var(--text-primary)">个人设置</h1>
    </div>

    <!-- API Keys Section -->
    <div class="card">
      <div class="flex items-center justify-between mb-4">
        <div class="flex items-center gap-2">
          <Key class="w-5 h-5" style="color: var(--gold)" />
          <h2 class="text-lg font-semibold" style="color: var(--text-primary)">API 密钥管理</h2>
        </div>
        <button @click="showForm = !showForm" class="btn-primary">
          {{ showForm ? '取消' : '添加密钥' }}
        </button>
      </div>

      <!-- Add Form -->
      <div v-if="showForm" class="p-4 rounded-lg mb-6 space-y-4" style="background: var(--bg-primary)">
        <div class="grid grid-cols-1 md:grid-cols-2 gap-4">
          <div>
            <label class="text-sm mb-1 block" style="color: var(--text-secondary)">名称 *</label>
            <input v-model="form.name" class="input-field" placeholder="如：OKX 模拟盘" />
          </div>
          <div>
            <label class="text-sm mb-1 block" style="color: var(--text-secondary)">密钥类型 *</label>
            <select v-model="form.key_type" class="input-field">
              <option value="exchange">交易所 (OKX/Binance)</option>
              <option value="ai_provider">AI 提供商</option>
            </select>
          </div>
        </div>

        <div>
          <label class="text-sm mb-1 block" style="color: var(--text-secondary)">API Key *</label>
          <input v-model="form.api_key" class="input-field font-mono text-sm" placeholder="输入 API Key" />
        </div>

        <div>
          <label class="text-sm mb-1 block" style="color: var(--text-secondary)">API Secret *</label>
          <div class="relative">
            <input 
              v-model="form.api_secret" 
              :type="showSecret ? 'text' : 'password'" 
              class="input-field font-mono text-sm pr-10" 
              placeholder="输入 API Secret" 
            />
            <button 
              @click="showSecret = !showSecret" 
              class="absolute right-3 top-1/2 -translate-y-1/2 text-gray-400 hover:text-gray-300"
            >
              <EyeOff v-if="showSecret" class="w-4 h-4" />
              <Eye v-else class="w-4 h-4" />
            </button>
          </div>
        </div>

        <div v-if="form.key_type === 'exchange'">
          <label class="text-sm mb-1 block" style="color: var(--text-secondary)">Passphrase (OKX 专用)</label>
          <input v-model="form.passphrase" type="password" class="input-field font-mono text-sm" placeholder="输入 Passphrase" />
        </div>

        <div v-if="form.key_type === 'exchange'" class="flex items-center gap-2">
          <input type="checkbox" id="is_demo" v-model="form.is_demo" class="w-4 h-4 accent-yellow-500" />
          <label for="is_demo" class="text-sm" style="color: var(--text-secondary)">模拟盘密钥（仅 OKX 模拟盘创建的 API Key 需勾选）</label>
        </div>

        <div class="flex gap-2 pt-2">
          <button @click="addKey" class="btn-primary flex items-center gap-2" :disabled="submitting">
            <Loader2 v-if="submitting" class="w-4 h-4 animate-spin" />
            {{ submitting ? '添加中...' : '确认添加' }}
          </button>
          <button @click="showForm = false" class="btn-secondary">取消</button>
        </div>
      </div>

      <!-- Loading State -->
      <div v-if="loading" class="space-y-3">
        <div v-for="i in 3" :key="i" class="h-20 rounded-lg animate-pulse" style="background: var(--bg-primary)"></div>
      </div>

      <!-- Exchange Keys -->
      <div v-else>
        <h3 v-if="exchangeKeys.length > 0" class="text-sm font-medium mb-3" style="color: var(--text-secondary)">
          交易所密钥 ({{ exchangeKeys.length }})
        </h3>
        
        <div v-if="exchangeKeys.length === 0 && !loading" class="py-6 text-center" style="color: var(--text-muted)">
          暂无交易所 API 密钥
        </div>

        <div v-else class="space-y-3">
          <div 
            v-for="k in exchangeKeys" 
            :key="k.id" 
            class="p-4 rounded-lg" 
            style="background: var(--bg-primary)"
          >
            <div class="flex items-start justify-between">
              <div class="space-y-2 flex-1">
                <div class="flex items-center gap-2 flex-wrap">
                  <span class="font-medium" style="color: var(--text-primary)">{{ k.name }}</span>
                  <span 
                    class="badge text-xs" 
                    :class="k.is_demo ? 'bg-yellow-500/20 text-yellow-400' : 'bg-green-500/20 text-green-400'"
                  >
                    {{ k.is_demo ? '模拟盘' : '实盘' }}
                  </span>
                  <span 
                    class="badge text-xs" 
                    :class="k.is_active ? 'badge-profit' : 'badge-loss'"
                  >
                    {{ k.is_active ? '活跃' : '停用' }}
                  </span>
                </div>
                <div class="text-xs" style="color: var(--text-muted)">
                  <span class="font-mono bg-gray-700/50 px-2 py-0.5 rounded">{{ maskKey(k.api_key) }}</span>
                  <span class="ml-3">创建于 {{ formatDate(k.created_at) }}</span>
                </div>
                
                <!-- Test Result -->
                <div v-if="testResult?.id === k.id" class="mt-2 p-2 rounded text-sm"
                  :class="testResult.success ? 'bg-green-500/20 text-green-400' : 'bg-red-500/20 text-red-400'">
                  <div class="flex items-center gap-1">
                    <CheckCircle v-if="testResult.success" class="w-4 h-4" />
                    <XCircle v-else class="w-4 h-4" />
                    {{ testResult.message }}
                  </div>
                </div>
              </div>

              <div class="flex items-center gap-2 ml-4">
                <button 
                  @click="testKey(k.id)" 
                  class="btn-icon"
                  :disabled="testingId === k.id || !k.is_active"
                  :title="k.is_active ? '测试连接' : '请先启用密钥'"
                >
                  <Loader2 v-if="testingId === k.id" class="w-4 h-4 animate-spin" />
                  <RefreshCw v-else class="w-4 h-4" />
                </button>
                <button 
                  @click="toggleKey(k.id)" 
                  class="btn-icon"
                  :title="k.is_active ? '停用' : '启用'"
                >
                  <XCircle v-if="k.is_active" class="w-4 h-4 text-red-400" />
                  <CheckCircle v-else class="w-4 h-4 text-green-400" />
                </button>
                <button @click="deleteKey(k.id)" class="btn-icon hover:text-red-400" title="删除">
                  <Trash2 class="w-4 h-4" />
                </button>
              </div>
            </div>
          </div>
        </div>

        <!-- AI Provider Keys (legacy from api-keys) -->
        <h3 v-if="aiKeys.length > 0" class="text-sm font-medium mb-3 mt-6" style="color: var(--text-secondary)">
          AI 提供商密钥 ({{ aiKeys.length }})
        </h3>
        
        <div class="space-y-3">
          <div 
            v-for="k in aiKeys" 
            :key="k.id" 
            class="p-4 rounded-lg" 
            style="background: var(--bg-primary)"
          >
            <div class="flex items-start justify-between">
              <div class="space-y-2 flex-1">
                <div class="flex items-center gap-2 flex-wrap">
                  <span class="font-medium" style="color: var(--text-primary)">{{ k.name }}</span>
                  <span class="badge bg-purple-500/20 text-purple-400 text-xs">
                    {{ k.provider || 'OpenAI' }}
                  </span>
                  <span 
                    class="badge text-xs" 
                    :class="k.is_active ? 'badge-profit' : 'badge-loss'"
                  >
                    {{ k.is_active ? '活跃' : '停用' }}
                  </span>
                </div>
                <div class="text-xs" style="color: var(--text-muted)">
                  <span class="font-mono bg-gray-700/50 px-2 py-0.5 rounded">{{ maskKey(k.api_key) }}</span>
                  <span class="ml-3">创建于 {{ formatDate(k.created_at) }}</span>
                </div>
              </div>

              <div class="flex items-center gap-2 ml-4">
                <button 
                  @click="testKey(k.id)" 
                  class="btn-icon"
                  :disabled="testingId === k.id || !k.is_active"
                >
                  <Loader2 v-if="testingId === k.id" class="w-4 h-4 animate-spin" />
                  <RefreshCw v-else class="w-4 h-4" />
                </button>
                <button 
                  @click="toggleKey(k.id)" 
                  class="btn-icon"
                >
                  <XCircle v-if="k.is_active" class="w-4 h-4 text-red-400" />
                  <CheckCircle v-else class="w-4 h-4 text-green-400" />
                </button>
                <button @click="deleteKey(k.id)" class="btn-icon hover:text-red-400">
                  <Trash2 class="w-4 h-4" />
                </button>
              </div>
            </div>
          </div>
        </div>
      </div>
    </div>

    <!-- AI Model Configuration Section -->
    <div class="card">
      <div class="flex items-center justify-between mb-4">
        <div class="flex items-center gap-2">
          <Brain class="w-5 h-5" style="color: var(--gold)" />
          <h2 class="text-lg font-semibold" style="color: var(--text-primary)">AI 模型配置</h2>
        </div>
        <button @click="openProviderForm()" class="btn-primary">
          添加提供商
        </button>
      </div>

      <!-- Provider Add/Edit Form -->
      <div v-if="showProviderForm" class="p-4 rounded-lg mb-6 space-y-4" style="background: var(--bg-primary)">
        <div class="grid grid-cols-1 md:grid-cols-2 gap-4">
          <div>
            <label class="text-sm mb-1 block" style="color: var(--text-secondary)">提供商类型 *</label>
            <select v-model="providerForm.provider" @change="onProviderTypeChange" class="input-field">
              <option value="openai">OpenAI</option>
              <option value="deepseek">DeepSeek</option>
              <option value="anthropic">Anthropic</option>
              <option value="custom">自定义</option>
            </select>
          </div>
          <div>
            <label class="text-sm mb-1 block" style="color: var(--text-secondary)">模型名称 *</label>
            <input v-model="providerForm.model" class="input-field font-mono text-sm" placeholder="如：gpt-4o-mini" />
          </div>
        </div>

        <div>
          <label class="text-sm mb-1 block" style="color: var(--text-secondary)">API Key *</label>
          <div class="relative">
            <input 
              v-model="providerForm.api_key" 
              :type="showProviderSecret ? 'text' : 'password'" 
              class="input-field font-mono text-sm pr-10" 
              placeholder="输入 API Key" 
            />
            <button 
              @click="showProviderSecret = !showProviderSecret" 
              class="absolute right-3 top-1/2 -translate-y-1/2 text-gray-400 hover:text-gray-300"
            >
              <EyeOff v-if="showProviderSecret" class="w-4 h-4" />
              <Eye v-else class="w-4 h-4" />
            </button>
          </div>
        </div>

        <div>
          <label class="text-sm mb-1 block" style="color: var(--text-secondary)">Base URL *</label>
          <input v-model="providerForm.base_url" class="input-field font-mono text-sm" placeholder="https://api.openai.com/v1" />
        </div>

        <div class="grid grid-cols-1 md:grid-cols-2 gap-4">
          <div>
            <label class="text-sm mb-1 block" style="color: var(--text-secondary)">最大 Tokens</label>
            <input v-model.number="providerForm.max_tokens" type="number" class="input-field" placeholder="4096" min="1" max="128000" />
          </div>
          <div>
            <label class="text-sm mb-1 block" style="color: var(--text-secondary)">Temperature</label>
            <input v-model.number="providerForm.temperature" type="number" class="input-field" placeholder="0.7" min="0" max="2" step="0.1" />
          </div>
        </div>

        <div class="flex gap-2 pt-2">
          <button @click="saveProvider" class="btn-primary flex items-center gap-2" :disabled="providerSubmitting">
            <Loader2 v-if="providerSubmitting" class="w-4 h-4 animate-spin" />
            {{ providerSubmitting ? '保存中...' : (editingProviderId ? '确认修改' : '确认添加') }}
          </button>
          <button @click="showProviderForm = false" class="btn-secondary">取消</button>
        </div>
      </div>

      <!-- Loading State -->
      <div v-if="providersLoading" class="space-y-3">
        <div v-for="i in 2" :key="i" class="h-24 rounded-lg animate-pulse" style="background: var(--bg-primary)"></div>
      </div>

      <!-- Provider List -->
      <div v-else>
        <div v-if="aiProviders.length === 0" class="py-6 text-center" style="color: var(--text-muted)">
          暂无 AI 模型配置，点击"添加提供商"开始配置
        </div>

        <div v-else class="space-y-3">
          <div 
            v-for="p in aiProviders" 
            :key="p.id" 
            class="p-4 rounded-lg" 
            style="background: var(--bg-primary)"
          >
            <div class="flex items-start justify-between">
              <div class="space-y-2 flex-1">
                <div class="flex items-center gap-2 flex-wrap">
                  <span class="font-medium" style="color: var(--text-primary)">
                    {{ PROVIDER_LABELS[p.provider] || p.provider }}
                  </span>
                  <span class="badge bg-blue-500/20 text-blue-400 text-xs font-mono">
                    {{ p.model }}
                  </span>
                  <span 
                    class="badge text-xs" 
                    :class="p.is_active ? 'badge-profit' : 'badge-loss'"
                  >
                    {{ p.is_active ? '活跃' : '停用' }}
                  </span>
                  <span 
                    v-if="p.is_default"
                    class="badge bg-yellow-500/20 text-yellow-400 text-xs flex items-center gap-1"
                  >
                    <Star class="w-3 h-3" />
                    默认
                  </span>
                </div>
                <div class="text-xs space-y-1" style="color: var(--text-muted)">
                  <div>
                    <span class="font-mono bg-gray-700/50 px-2 py-0.5 rounded">{{ maskKey(p.api_key) }}</span>
                  </div>
                  <div class="flex flex-wrap gap-x-4 gap-y-1">
                    <span>Base URL: <span class="font-mono">{{ p.base_url }}</span></span>
                    <span>Max Tokens: {{ p.max_tokens }}</span>
                    <span>Temperature: {{ p.temperature }}</span>
                  </div>
                  <div>创建于 {{ formatDate(p.created_at) }}</div>
                </div>
                
                <!-- Test Result -->
                <div v-if="providerTestResult?.id === p.id" class="mt-2 p-2 rounded text-sm"
                  :class="providerTestResult.success ? 'bg-green-500/20 text-green-400' : 'bg-red-500/20 text-red-400'">
                  <div class="flex items-center gap-1">
                    <CheckCircle v-if="providerTestResult.success" class="w-4 h-4" />
                    <XCircle v-else class="w-4 h-4" />
                    {{ providerTestResult.message }}
                  </div>
                </div>
              </div>

              <div class="flex items-center gap-2 ml-4">
                <button 
                  @click="testProvider(p.id)" 
                  class="btn-icon"
                  :disabled="providerTestingId === p.id"
                  title="测试连接"
                >
                  <Loader2 v-if="providerTestingId === p.id" class="w-4 h-4 animate-spin" />
                  <Zap v-else class="w-4 h-4" />
                </button>
                <button 
                  v-if="!p.is_default"
                  @click="setDefaultProvider(p.id)" 
                  class="btn-icon"
                  title="设为默认"
                >
                  <Star class="w-4 h-4" />
                </button>
                <button 
                  @click="openProviderForm(p)" 
                  class="btn-icon"
                  title="编辑"
                >
                  <Edit2 class="w-4 h-4" />
                </button>
                <button @click="deleteProvider(p.id)" class="btn-icon hover:text-red-400" title="删除">
                  <Trash2 class="w-4 h-4" />
                </button>
              </div>
            </div>
          </div>
        </div>
      </div>
    </div>
  </div>
</template>

<script setup lang="ts">
import { ref, onMounted, computed } from 'vue'
import api from '@/api'
import { Settings, Key, Trash2, RefreshCw, CheckCircle, XCircle, Loader2, Eye, EyeOff, Brain, Edit2, Star, Zap, Plus, ChevronDown, Globe, Wifi, WifiOff } from 'lucide-vue-next'

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

const form = ref({
  name: '',
  key_type: 'exchange',
  api_key: '',
  api_secret: '',
  passphrase: '',
  is_demo: false
})

const showSecret = ref(false)
const showProviderForm = ref(false)
const editingProviderId = ref<string | null>(null)
const providerSubmitting = ref(false)
const providerTestingId = ref<string | null>(null)
const providerTestResult = ref<{ id: string; success: boolean; message: string } | null>(null)

const providerForm = ref({
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

// Proxy config
interface ProxyConfig {
  enabled: boolean
  url: string
  proxy_type: string
  test_url: string
}

interface ProxyTestResult {
  success: boolean
  message: string
  latency_ms: number | null
}

const proxyConfig = ref<ProxyConfig>({ enabled: false, url: '', proxy_type: 'socks5', test_url: 'https://www.okx.com' })
const proxyLoading = ref(true)
const proxySaving = ref(false)
const proxyTesting = ref(false)
const proxyTestResult = ref<ProxyTestResult | null>(null)

async function loadApiKeys() {
  try {
    const { data } = await api.get('/api-keys')
    apiKeys.value = data.items || data.keys || data || []
  } catch (e) { console.error('Failed to load API keys', e) }
  finally { loading.value = false }
}

async function loadAiProviders() {
  try {
    const { data } = await api.get('/ai/providers')
    aiProviders.value = data.items || data.providers || data || []
  } catch (e) { console.error('Failed to load AI providers', e) }
  finally { providersLoading.value = false }
}

async function addKey() {
  if (!form.value.name || !form.value.api_key || !form.value.api_secret) { alert('请填写所有必填字段'); return }
  submitting.value = true
  try {
    await api.post('/api-keys', form.value)
    showForm.value = false
    form.value = { name: '', key_type: 'exchange', api_key: '', api_secret: '', passphrase: '', is_demo: false }
    await loadApiKeys()
  } catch (e: any) { alert('添加失败: ' + (e.response?.data?.message || e.message)) }
  finally { submitting.value = false }
}

async function deleteKey(id: string) {
  if (!confirm('确定要删除这个 API 密钥吗？')) return
  try { await api.delete(`/api-keys/${id}`); await loadApiKeys() }
  catch (e: any) { alert('删除失败: ' + (e.response?.data?.message || e.message)) }
}

async function toggleKey(id: string) {
  try { await api.post(`/api-keys/${id}/toggle`); await loadApiKeys() }
  catch (e: any) { alert('切换状态失败: ' + (e.response?.data?.message || e.message)) }
}

async function testKey(id: string) {
  testingId.value = id; testResult.value = null
  try {
    const { data } = await api.post(`/api-keys/${id}/test`)
    testResult.value = { id, success: data.success, message: data.message }
  } catch (e: any) { testResult.value = { id, success: false, message: e.response?.data?.message || '测试失败' } }
  finally { testingId.value = null }
}

function openProviderForm(provider?: AiProvider) {
  if (provider) {
    editingProviderId.value = provider.id
    providerForm.value = { provider: provider.provider, api_key: provider.api_key, base_url: provider.base_url, model: provider.model, max_tokens: provider.max_tokens, temperature: provider.temperature }
  } else {
    editingProviderId.value = null
    providerForm.value = { provider: 'openai', api_key: '', base_url: 'https://api.openai.com/v1', model: 'gpt-4o-mini', max_tokens: 4096, temperature: 0.7 }
  }
  showProviderForm.value = true
}

function onProviderTypeChange() {
  const preset = PROVIDER_PRESETS[providerForm.value.provider]
  if (preset) { providerForm.value.base_url = preset.base_url; providerForm.value.model = preset.model }
}

async function saveProvider() {
  if (!providerForm.value.api_key) { alert('请填写 API Key'); return }
  if (!providerForm.value.base_url || !providerForm.value.model) { alert('请填写 Base URL 和模型名称'); return }
  providerSubmitting.value = true
  try {
    if (editingProviderId.value) await api.put(`/ai/providers/${editingProviderId.value}`, providerForm.value)
    else await api.post('/ai/providers', providerForm.value)
    showProviderForm.value = false; editingProviderId.value = null; await loadAiProviders()
  } catch (e: any) { alert('保存失败: ' + (e.response?.data?.message || e.message)) }
  finally { providerSubmitting.value = false }
}

async function deleteProvider(id: string) {
  if (!confirm('确定要删除这个 AI 提供商配置吗？')) return
  try { await api.delete(`/ai/providers/${id}`); await loadAiProviders() }
  catch (e: any) { alert('删除失败: ' + (e.response?.data?.message || e.message)) }
}

async function testProvider(id: string) {
  providerTestingId.value = id; providerTestResult.value = null
  try {
    const { data } = await api.post(`/ai/providers/${id}/test`)
    providerTestResult.value = { id, success: data.success ?? true, message: data.message || '连接成功' }
  } catch (e: any) { providerTestResult.value = { id, success: false, message: e.response?.data?.message || '连接测试失败' } }
  finally { providerTestingId.value = null }
}

async function setDefaultProvider(id: string) {
  try { await api.put(`/ai/providers/${id}`, { is_default: true }); await loadAiProviders() }
  catch (e: any) { alert('设置默认失败: ' + (e.response?.data?.message || e.message)) }
}

function formatDate(dateStr: string) { return dateStr ? new Date(dateStr).toLocaleDateString('zh-CN', { year: 'numeric', month: '2-digit', day: '2-digit' }) : '' }
function maskKey(key: string) { return !key || key.length < 8 ? '****' : key.slice(0, 8) + '****' }

onMounted(() => { loadApiKeys(); loadAiProviders(); loadProxyConfig() })

async function loadProxyConfig() {
  try {
    const { data } = await api.get('/system/proxy')
    proxyConfig.value = data
  } catch (e) { console.error('Failed to load proxy config', e) }
  finally { proxyLoading.value = false }
}

async function saveProxyConfig() {
  proxySaving.value = true
  try {
    const { data } = await api.put('/system/proxy', proxyConfig.value)
    proxyConfig.value = data
  } catch (e: any) { alert('保存失败: ' + (e.response?.data?.message || e.message)) }
  finally { proxySaving.value = false }
}

async function testProxyConnection() {
  proxyTesting.value = true
  proxyTestResult.value = null
  try {
    const { data } = await api.put('/system/proxy/test')
    proxyTestResult.value = data
  } catch (e: any) { proxyTestResult.value = { success: false, message: e.response?.data?.message || '测试失败', latency_ms: null } }
  finally { proxyTesting.value = false }
}
</script>

<template>
  <div class="space-y-6 animate-fade-in">
    <!-- Header -->
    <div>
      <h1 class="text-2xl font-bold" style="color: var(--text-primary)">系统设置</h1>
      <p class="text-sm mt-1" style="color: var(--text-secondary)">管理代理配置、API 密钥和 AI 模型配置</p>
    </div>

    <!-- Proxy Configuration Section -->
    <div class="card">
      <div class="flex items-center justify-between p-5" style="border-bottom: 1px solid var(--border)">
        <div class="flex items-center gap-2">
          <Globe class="w-5 h-5" style="color: var(--primary)" />
          <h2 class="text-lg font-semibold" style="color: var(--text-primary)">代理配置</h2>
          <span class="badge" :class="proxyConfig.enabled ? 'badge-profit' : 'badge-loss'">
            <Wifi v-if="proxyConfig.enabled" class="w-3 h-3" style="display: inline" />
            <WifiOff v-else class="w-3 h-3" style="display: inline" />
            {{ proxyConfig.enabled ? '已启用' : '未启用' }}
          </span>
        </div>
      </div>

      <div v-if="proxyLoading" class="p-5 space-y-3">
        <div v-for="i in 2" :key="i" class="h-16 rounded-lg animate-pulse" style="background: var(--surface-tertiary)"></div>
      </div>

      <div v-else class="p-5 space-y-4">
        <div class="flex items-center gap-3 p-3 rounded-lg" style="background: var(--surface-secondary); border: 1px solid var(--border)">
          <input type="checkbox" id="proxy_enabled" v-model="proxyConfig.enabled" class="w-5 h-5 rounded cursor-pointer" style="accent-color: var(--primary)" />
          <label for="proxy_enabled" class="cursor-pointer">
            <span class="font-semibold" style="color: var(--text-primary)">启用网络代理</span>
            <span class="text-xs block" style="color: var(--text-muted)">后端访问 OKX 等 API 时通过代理连接，无需 Docker 配置代理</span>
          </label>
        </div>

        <div class="grid grid-cols-1 md:grid-cols-2 gap-4">
          <div>
            <label class="label">代理类型</label>
            <div class="relative">
              <select v-model="proxyConfig.proxy_type" class="input pr-10 appearance-none" :disabled="!proxyConfig.enabled">
                <option value="socks5">SOCKS5</option>
                <option value="http">HTTP</option>
                <option value="https">HTTPS</option>
              </select>
              <ChevronDown class="w-4 h-4 absolute right-3 top-1/2 -translate-y-1/2 pointer-events-none" style="color: var(--text-muted)" />
            </div>
          </div>
          <div>
            <label class="label">测试 URL</label>
            <input v-model="proxyConfig.test_url" class="input font-mono text-sm" placeholder="https://www.okx.com" :disabled="!proxyConfig.enabled" />
          </div>
        </div>

        <div>
          <label class="label">代理地址</label>
          <input v-model="proxyConfig.url" class="input font-mono text-sm" :placeholder="proxyConfig.proxy_type === 'socks5' ? 'socks5://127.0.0.1:10809' : 'http://127.0.0.1:7890'" :disabled="!proxyConfig.enabled" />
          <p class="text-xs mt-1" style="color: var(--text-muted)">
            {{ proxyConfig.proxy_type === 'socks5' ? '格式: socks5://IP:端口 (如 socks5://127.0.0.1:10809)' : '格式: http://IP:端口 (如 http://127.0.0.1:7890)' }}
          </p>
        </div>

        <div v-if="proxyTestResult" class="p-3 rounded-lg text-sm" :style="{ background: proxyTestResult.success ? 'rgba(0,200,83,0.1)' : 'rgba(255,23,68,0.1)', border: proxyTestResult.success ? '1px solid rgba(0,200,83,0.3)' : '1px solid rgba(255,23,68,0.3)' }">
          <div class="flex items-center gap-2">
            <CheckCircle v-if="proxyTestResult.success" class="w-4 h-4" style="color: var(--profit)" />
            <XCircle v-else class="w-4 h-4" style="color: var(--loss)" />
            <span :style="{ color: proxyTestResult.success ? 'var(--profit)' : 'var(--loss)' }">{{ proxyTestResult.message }}</span>
            <span v-if="proxyTestResult.latency_ms" class="text-xs" style="color: var(--text-muted)">({{ proxyTestResult.latency_ms }}ms)</span>
          </div>
        </div>

        <div class="flex gap-2 pt-2">
          <button @click="saveProxyConfig" class="btn btn-primary" :disabled="proxySaving">
            <Loader2 v-if="proxySaving" class="w-4 h-4 animate-spin" />
            {{ proxySaving ? '保存中...' : '保存配置' }}
          </button>
          <button @click="testProxyConnection" class="btn btn-secondary" :disabled="proxyTesting || !proxyConfig.enabled">
            <Loader2 v-if="proxyTesting" class="w-4 h-4 animate-spin" />
            <Zap v-else class="w-4 h-4" />
            {{ proxyTesting ? '测试中...' : '测试连接' }}
          </button>
        </div>
      </div>
    </div>

    <!-- API Keys Section -->
    <div class="card">
      <div class="flex items-center justify-between p-5" style="border-bottom: 1px solid var(--border)">
        <div class="flex items-center gap-2">
          <Key class="w-5 h-5" style="color: var(--primary)" />
          <h2 class="text-lg font-semibold" style="color: var(--text-primary)">API 密钥管理</h2>
        </div>
        <button @click="showForm = !showForm" class="btn btn-primary btn-sm">
          <Plus class="w-4 h-4" />
          {{ showForm ? '取消' : '添加密钥' }}
        </button>
      </div>

      <!-- Add Form -->
      <div v-if="showForm" class="p-5 space-y-4" style="border-bottom: 1px solid var(--border); background: var(--surface-secondary)">
        <div class="grid grid-cols-1 md:grid-cols-2 gap-4">
          <div>
            <label class="label">名称 *</label>
            <input v-model="form.name" class="input" placeholder="如：OKX 模拟盘" />
          </div>
          <div>
            <label class="label">密钥类型 *</label>
            <div class="relative">
              <select v-model="form.key_type" class="input pr-10 appearance-none">
                <option value="exchange">交易所 (OKX/Binance)</option>
                <option value="ai_provider">AI 提供商</option>
              </select>
              <ChevronDown class="w-4 h-4 absolute right-3 top-1/2 -translate-y-1/2 pointer-events-none" style="color: var(--text-muted)" />
            </div>
          </div>
        </div>
        <div>
          <label class="label">API Key *</label>
          <input v-model="form.api_key" class="input font-mono text-sm" placeholder="输入 API Key" />
        </div>
        <div>
          <label class="label">API Secret *</label>
          <div class="relative">
            <input v-model="form.api_secret" :type="showSecret ? 'text' : 'password'" class="input font-mono text-sm pr-10" placeholder="输入 API Secret" />
            <button @click="showSecret = !showSecret" class="absolute right-3 top-1/2 -translate-y-1/2" style="color: var(--text-muted)">
              <EyeOff v-if="showSecret" class="w-4 h-4" /><Eye v-else class="w-4 h-4" />
            </button>
          </div>
        </div>
        <div v-if="form.key_type === 'exchange'">
          <label class="label">Passphrase (OKX 专用)</label>
          <input v-model="form.passphrase" type="password" class="input font-mono text-sm" placeholder="输入 Passphrase" />
        </div>
        <div v-if="form.key_type === 'exchange'" class="flex items-center gap-2">
          <input type="checkbox" id="is_demo" v-model="form.is_demo" class="w-4 h-4 rounded" style="accent-color: var(--primary)" />
          <label for="is_demo" class="text-sm" style="color: var(--text-secondary)">模拟盘密钥</label>
        </div>
        <div class="flex gap-2 pt-2">
          <button @click="addKey" class="btn btn-primary" :disabled="submitting">
            <Loader2 v-if="submitting" class="w-4 h-4 animate-spin" />
            {{ submitting ? '添加中...' : '确认添加' }}
          </button>
          <button @click="showForm = false" class="btn btn-secondary">取消</button>
        </div>
      </div>

      <!-- Loading State -->
      <div v-if="loading" class="p-5 space-y-3">
        <div v-for="i in 2" :key="i" class="h-20 rounded-lg animate-pulse" style="background: var(--surface-tertiary)"></div>
      </div>

      <!-- Exchange Keys -->
      <div v-else class="p-5">
        <h3 v-if="exchangeKeys.length > 0" class="text-sm font-semibold mb-3" style="color: var(--text-secondary)">交易所密钥 ({{ exchangeKeys.length }})</h3>
        <div v-if="exchangeKeys.length === 0" class="py-8 text-center">
          <Key class="w-12 h-12 mx-auto mb-3" style="color: var(--text-muted); opacity: 0.3" />
          <p class="text-sm" style="color: var(--text-muted)">暂无交易所 API 密钥</p>
        </div>
        <div v-else class="space-y-3">
          <div v-for="k in exchangeKeys" :key="k.id" class="p-4 rounded-lg" style="background: var(--surface-secondary); border: 1px solid var(--border)">
            <div class="flex items-start justify-between">
              <div class="flex-1">
                <div class="flex items-center gap-2 flex-wrap mb-2">
                  <span class="font-semibold" style="color: var(--text-primary)">{{ k.name }}</span>
                  <span class="badge" :class="k.is_demo ? 'badge-warning' : 'badge-profit'">{{ k.is_demo ? '模拟盘' : '实盘' }}</span>
                  <span class="badge" :class="k.is_active ? 'badge-profit' : 'badge-loss'">{{ k.is_active ? '活跃' : '停用' }}</span>
                </div>
                <div class="text-xs" style="color: var(--text-muted)">
                  <span class="font-mono px-2 py-0.5 rounded" style="background: var(--surface-tertiary)">{{ maskKey(k.api_key) }}</span>
                  <span class="ml-3">创建于 {{ formatDate(k.created_at) }}</span>
                </div>
                <div v-if="testResult?.id === k.id" class="mt-2 p-2 rounded text-sm" :class="testResult.success ? 'badge-profit' : 'badge-loss'" style="display: flex; align-items: center; gap: 4px; width: fit-content">
                  <CheckCircle v-if="testResult.success" class="w-4 h-4" /><XCircle v-else class="w-4 h-4" />
                  {{ testResult.message }}
                </div>
              </div>
              <div class="flex items-center gap-1 ml-4">
                <button @click="testKey(k.id)" class="btn btn-ghost btn-sm" :disabled="testingId === k.id || !k.is_active">
                  <Loader2 v-if="testingId === k.id" class="w-4 h-4 animate-spin" /><RefreshCw v-else class="w-4 h-4" />
                </button>
                <button @click="toggleKey(k.id)" class="btn btn-ghost btn-sm">
                  <XCircle v-if="k.is_active" class="w-4 h-4" style="color: var(--loss)" /><CheckCircle v-else class="w-4 h-4" style="color: var(--profit)" />
                </button>
                <button @click="deleteKey(k.id)" class="btn btn-ghost btn-sm" style="color: var(--loss)">
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
      <div class="flex items-center justify-between p-5" style="border-bottom: 1px solid var(--border)">
        <div class="flex items-center gap-2">
          <Brain class="w-5 h-5" style="color: var(--primary)" />
          <h2 class="text-lg font-semibold" style="color: var(--text-primary)">AI 模型配置</h2>
        </div>
        <button @click="openProviderForm()" class="btn btn-primary btn-sm">
          <Plus class="w-4 h-4" />
          添加提供商
        </button>
      </div>

      <!-- Provider Add/Edit Form -->
      <div v-if="showProviderForm" class="p-5 space-y-4" style="border-bottom: 1px solid var(--border); background: var(--surface-secondary)">
        <div class="grid grid-cols-1 md:grid-cols-2 gap-4">
          <div>
            <label class="label">提供商类型 *</label>
            <div class="relative">
              <select v-model="providerForm.provider" @change="onProviderTypeChange" class="input pr-10 appearance-none">
                <option value="openai">OpenAI</option>
                <option value="deepseek">DeepSeek</option>
                <option value="anthropic">Anthropic</option>
                <option value="custom">自定义</option>
              </select>
              <ChevronDown class="w-4 h-4 absolute right-3 top-1/2 -translate-y-1/2 pointer-events-none" style="color: var(--text-muted)" />
            </div>
          </div>
          <div>
            <label class="label">模型名称 *</label>
            <input v-model="providerForm.model" class="input font-mono text-sm" placeholder="如：gpt-4o-mini" />
          </div>
        </div>
        <div>
          <label class="label">API Key *</label>
          <div class="relative">
            <input v-model="providerForm.api_key" :type="showProviderSecret ? 'text' : 'password'" class="input font-mono text-sm pr-10" placeholder="输入 API Key" />
            <button @click="showProviderSecret = !showProviderSecret" class="absolute right-3 top-1/2 -translate-y-1/2" style="color: var(--text-muted)">
              <EyeOff v-if="showProviderSecret" class="w-4 h-4" /><Eye v-else class="w-4 h-4" />
            </button>
          </div>
        </div>
        <div>
          <label class="label">Base URL *</label>
          <input v-model="providerForm.base_url" class="input font-mono text-sm" placeholder="https://api.openai.com/v1" />
        </div>
        <div class="grid grid-cols-1 md:grid-cols-2 gap-4">
          <div>
            <label class="label">最大 Tokens</label>
            <input v-model.number="providerForm.max_tokens" type="number" class="input" placeholder="4096" min="1" max="128000" />
          </div>
          <div>
            <label class="label">Temperature</label>
            <input v-model.number="providerForm.temperature" type="number" class="input" placeholder="0.7" min="0" max="2" step="0.1" />
          </div>
        </div>
        <div class="flex gap-2 pt-2">
          <button @click="saveProvider" class="btn btn-primary" :disabled="providerSubmitting">
            <Loader2 v-if="providerSubmitting" class="w-4 h-4 animate-spin" />
            {{ providerSubmitting ? '保存中...' : (editingProviderId ? '确认修改' : '确认添加') }}
          </button>
          <button @click="showProviderForm = false" class="btn btn-secondary">取消</button>
        </div>
      </div>

      <!-- Loading State -->
      <div v-if="providersLoading" class="p-5 space-y-3">
        <div v-for="i in 2" :key="i" class="h-24 rounded-lg animate-pulse" style="background: var(--surface-tertiary)"></div>
      </div>

      <!-- Provider List -->
      <div v-else class="p-5">
        <div v-if="aiProviders.length === 0" class="py-8 text-center">
          <Brain class="w-12 h-12 mx-auto mb-3" style="color: var(--text-muted); opacity: 0.3" />
          <p class="text-sm" style="color: var(--text-muted)">暂无 AI 模型配置</p>
        </div>
        <div v-else class="space-y-3">
          <div v-for="p in aiProviders" :key="p.id" class="p-4 rounded-lg" style="background: var(--surface-secondary); border: 1px solid var(--border)">
            <div class="flex items-start justify-between">
              <div class="flex-1">
                <div class="flex items-center gap-2 flex-wrap mb-2">
                  <span class="font-semibold" style="color: var(--text-primary)">{{ PROVIDER_LABELS[p.provider] || p.provider }}</span>
                  <span class="badge badge-primary font-mono">{{ p.model }}</span>
                  <span class="badge" :class="p.is_active ? 'badge-profit' : 'badge-loss'">{{ p.is_active ? '活跃' : '停用' }}</span>
                  <span v-if="p.is_default" class="badge badge-warning flex items-center gap-1">
                    <Star class="w-3 h-3" /> 默认
                  </span>
                </div>
                <div class="text-xs space-y-1" style="color: var(--text-muted)">
                  <div><span class="font-mono px-2 py-0.5 rounded" style="background: var(--surface-tertiary)">{{ maskKey(p.api_key) }}</span></div>
                  <div class="flex flex-wrap gap-x-4 gap-y-1">
                    <span>Base URL: <span class="font-mono">{{ p.base_url }}</span></span>
                    <span>Max Tokens: {{ p.max_tokens }}</span>
                    <span>Temperature: {{ p.temperature }}</span>
                  </div>
                </div>
                <div v-if="providerTestResult?.id === p.id" class="mt-2 p-2 rounded text-sm" :class="providerTestResult.success ? 'badge-profit' : 'badge-loss'" style="display: flex; align-items: center; gap: 4px; width: fit-content">
                  <CheckCircle v-if="providerTestResult.success" class="w-4 h-4" /><XCircle v-else class="w-4 h-4" />
                  {{ providerTestResult.message }}
                </div>
              </div>
              <div class="flex items-center gap-1 ml-4">
                <button @click="testProvider(p.id)" class="btn btn-ghost btn-sm" :disabled="providerTestingId === p.id">
                  <Loader2 v-if="providerTestingId === p.id" class="w-4 h-4 animate-spin" /><Zap v-else class="w-4 h-4" />
                </button>
                <button v-if="!p.is_default" @click="setDefaultProvider(p.id)" class="btn btn-ghost btn-sm">
                  <Star class="w-4 h-4" />
                </button>
                <button @click="openProviderForm(p)" class="btn btn-ghost btn-sm">
                  <Edit2 class="w-4 h-4" />
                </button>
                <button @click="deleteProvider(p.id)" class="btn btn-ghost btn-sm" style="color: var(--loss)">
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

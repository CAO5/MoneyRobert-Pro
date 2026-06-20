<script setup lang="ts">
import { ref, onMounted } from 'vue'
import api from '@/api'
import { Newspaper, ExternalLink, RefreshCw } from 'lucide-vue-next'

const news = ref<any[]>([])
const loading = ref(true)
const fetching = ref(false)
const filter = ref('')
const message = ref('')
const error = ref('')

async function loadNews() {
  loading.value = true
  error.value = ''
  try {
    const { data } = await api.get('/news', { params: filter.value ? { symbol: filter.value } : {} })
    news.value = data.items || []
  } catch (e) {
    console.error('Failed to load news', e)
    error.value = '新闻加载失败，请稍后重试'
  } finally {
    loading.value = false
  }
}

async function fetchNews() {
  fetching.value = true
  message.value = ''
  error.value = ''
  try {
    const { data } = await api.post('/news/fetch')
    const failed = (data.sources || []).filter((source: any) => source.error).length
    message.value = `新增 ${data.inserted || 0} 条，跳过 ${data.duplicates || 0} 条重复新闻${failed ? `，${failed} 个来源暂不可用` : ''}`
    await loadNews()
  } catch (e: any) {
    error.value = e.response?.data?.message || '新闻抓取失败，请检查网络或代理配置'
  } finally {
    fetching.value = false
  }
}

function sentimentBadge(score: number | null) {
  if (score != null && score > 0.6) return 'badge-profit'
  if (score != null && score < 0.4) return 'badge-loss'
  return 'badge-neutral'
}

function sentimentLabel(score: number | null) {
  if (score == null) return '未知'
  if (score > 0.6) return '偏多'
  if (score < 0.4) return '偏空'
  return '中性'
}

onMounted(loadNews)
</script>

<template>
  <div class="space-y-6">
    <div class="flex items-center justify-between">
      <div class="flex items-center gap-3">
        <Newspaper class="w-6 h-6" style="color: var(--primary)" />
        <h1 class="text-2xl font-bold" style="color: var(--text-primary)">新闻资讯</h1>
      </div>
      <div class="flex gap-2">
        <input v-model="filter" @keyup.enter="loadNews" class="input w-48" placeholder="按交易对筛选，如 BTC" />
        <button @click="fetchNews" class="btn-secondary flex items-center gap-2" :disabled="fetching">
          <RefreshCw class="w-4 h-4" :class="{ 'animate-spin': fetching }" />
          {{ fetching ? '抓取中...' : '抓取最新' }}
        </button>
      </div>
    </div>

    <div v-if="message" class="card py-3 text-sm" style="color: var(--profit)">{{ message }}</div>
    <div v-if="error" class="card py-3 text-sm" style="color: var(--loss)">{{ error }}</div>

    <div v-if="loading" class="space-y-3">
      <div v-for="i in 5" :key="i" class="card animate-pulse h-24"></div>
    </div>

    <div v-else-if="news.length === 0" class="card py-12 text-center" style="color: var(--text-muted)">
      暂无新闻，点击“抓取最新”获取资讯
    </div>

    <div v-else class="space-y-3">
      <div v-for="item in news" :key="item.id" class="card flex items-start justify-between gap-4">
        <div class="flex-1 space-y-2">
          <h3 class="font-semibold" style="color: var(--text-primary)">{{ item.title }}</h3>
          <div class="flex flex-wrap items-center gap-3 text-xs" style="color: var(--text-muted)">
            <span>{{ item.source }}</span>
            <span>{{ new Date(item.published_at).toLocaleString('zh-CN') }}</span>
            <span class="badge" :class="sentimentBadge(item.sentiment)">{{ sentimentLabel(item.sentiment) }}</span>
            <span v-for="symbol in item.related_symbols || []" :key="symbol" class="badge badge-neutral">{{ symbol }}</span>
          </div>
        </div>
        <a v-if="item.url" :href="item.url" target="_blank" rel="noopener noreferrer"
          class="p-2 rounded-lg transition-colors hover:bg-[var(--surface-tertiary)] flex-shrink-0"
          style="color: var(--text-secondary)">
          <ExternalLink class="w-4 h-4" />
        </a>
      </div>
    </div>
  </div>
</template>

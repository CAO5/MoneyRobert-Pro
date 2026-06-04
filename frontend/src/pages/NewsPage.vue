<script setup lang="ts">
import { ref, onMounted } from 'vue'
import api from '@/api'
import { Newspaper, ExternalLink } from 'lucide-vue-next'

const news = ref<any[]>([])
const loading = ref(true)
const filter = ref('')

async function loadNews() {
  try {
    const { data } = await api.get('/news', { params: filter.value ? { symbol: filter.value } : {} })
    news.value = data.items || data.news || data || []
  } catch (e) {
    console.error('Failed to load news', e)
  } finally {
    loading.value = false
  }
}

function sentimentBadge(s: string) {
  if (s === 'positive' || s === 'bullish') return 'badge-profit'
  if (s === 'negative' || s === 'bearish') return 'badge-loss'
  return 'badge-neutral'
}

onMounted(() => { loadNews() })
</script>

<template>
  <div class="space-y-6">
    <div class="flex items-center justify-between">
      <div class="flex items-center gap-3">
        <Newspaper class="w-6 h-6" style="color: var(--primary)" />
        <h1 class="text-2xl font-bold" style="color: var(--text-primary)">新闻资讯</h1>
      </div>
      <div class="flex gap-2">
        <input v-model="filter" @keyup.enter="loadNews" class="input w-48" placeholder="按交易对筛选" />
        <button @click="loadNews" class="btn-secondary">刷新</button>
      </div>
    </div>

    <div v-if="loading" class="space-y-3">
      <div v-for="i in 5" :key="i" class="card animate-pulse h-24"></div>
    </div>

    <div v-else-if="news.length === 0" class="card py-12 text-center" style="color: var(--text-muted)">暂无新闻</div>

    <div v-else class="space-y-3">
      <div v-for="n in news" :key="n.id" class="card flex items-start justify-between gap-4">
        <div class="flex-1 space-y-2">
          <h3 class="font-semibold" style="color: var(--text-primary)">{{ n.title }}</h3>
          <div class="flex items-center gap-3 text-xs" style="color: var(--text-muted)">
            <span>{{ n.source }}</span>
            <span>{{ new Date(n.published_at).toLocaleString('zh-CN') }}</span>
            <span class="badge" :class="sentimentBadge(n.sentiment)">{{ n.sentiment }}</span>
          </div>
        </div>
        <a v-if="n.url" :href="n.url" target="_blank" class="p-2 rounded-lg transition-colors hover:bg-[var(--surface-tertiary)] flex-shrink-0" style="color: var(--text-secondary)">
          <ExternalLink class="w-4 h-4" />
        </a>
      </div>
    </div>
  </div>
</template>

<script setup lang="ts">
import { ref, onMounted } from 'vue'
import api from '@/api'
import { FileText, Download } from 'lucide-vue-next'

const reports = ref<any[]>([])
const loading = ref(true)

async function loadReports() {
  try {
    const { data } = await api.get('/reports')
    reports.value = data.items || data.reports || data || []
  } catch (e) {
    console.error('Failed to load reports', e)
  } finally {
    loading.value = false
  }
}

async function exportReport(id: string) {
  try {
    const { data } = await api.post(`/reports/${id}/export`, {}, { responseType: 'blob' })
    const url = window.URL.createObjectURL(new Blob([data]))
    const link = document.createElement('a')
    link.href = url
    link.setAttribute('download', `report-${id}`)
    document.body.appendChild(link)
    link.click()
    link.remove()
    window.URL.revokeObjectURL(url)
  } catch (e) {
    console.error('Export failed', e)
  }
}

function statusBadge(s: string) {
  if (s === 'completed') return 'badge-profit'
  if (s === 'failed') return 'badge-loss'
  if (s === 'processing') return 'badge-gold'
  return 'badge-neutral'
}

onMounted(() => { loadReports() })
</script>

<template>
  <div class="space-y-6">
    <div class="flex items-center gap-3">
      <FileText class="w-6 h-6" style="color: var(--gold)" />
      <h1 class="font-display text-2xl font-bold" style="color: var(--text-primary)">报告中心</h1>
    </div>

    <div v-if="loading" class="space-y-3">
      <div v-for="i in 4" :key="i" class="card animate-pulse h-20"></div>
    </div>

    <div v-else-if="reports.length === 0" class="card py-12 text-center" style="color: var(--text-muted)">暂无报告</div>

    <div v-else class="space-y-3">
      <div v-for="r in reports" :key="r.id" class="card flex items-center justify-between">
        <div class="space-y-1">
          <div class="flex items-center gap-3">
            <h3 class="font-semibold" style="color: var(--text-primary)">{{ r.title }}</h3>
            <span class="badge badge-neutral">{{ r.format }}</span>
            <span class="badge" :class="statusBadge(r.status)">{{ r.status }}</span>
          </div>
          <div class="text-xs" style="color: var(--text-muted)">{{ new Date(r.created_at).toLocaleString('zh-CN') }}</div>
        </div>
        <button @click="exportReport(r.id)" :disabled="r.status !== 'completed'" class="btn-secondary flex items-center gap-2" :style="r.status !== 'completed' ? 'opacity: 0.5; cursor: not-allowed' : ''">
          <Download class="w-4 h-4" /> 导出
        </button>
      </div>
    </div>
  </div>
</template>

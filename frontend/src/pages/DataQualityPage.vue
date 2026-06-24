<script setup lang="ts">
import { ref, onMounted } from 'vue'
import { DataQualityApi, type QualityOverviewItem, type QualityAlert } from '@/api'
import {
  Database, RefreshCw, AlertTriangle, CheckCircle, AlertCircle,
  Activity, Zap, Clock, BarChart3
} from 'lucide-vue-next'

// =========================================================
// 状态
// =========================================================

const overview = ref<QualityOverviewItem[]>([])
const alerts = ref<QualityAlert[]>([])
const stats = ref({ total: 0, healthy: 0, warning: 0, critical: 0 })
const alertStats = ref({ total: 0, critical: 0, warning: 0 })
const loading = ref(false)
const scanning = ref(false)
const error = ref('')

// =========================================================
// 方法
// =========================================================

async function loadOverview() {
  loading.value = true
  error.value = ''
  try {
    const data = await DataQualityApi.getOverview()
    overview.value = data.overview || []
    stats.value = {
      total: data.total_sources || 0,
      healthy: data.healthy || 0,
      warning: data.warning || 0,
      critical: data.critical || 0,
    }
  } catch (e: unknown) {
    error.value = e instanceof Error ? e.message : '加载质量概览失败'
  } finally {
    loading.value = false
  }
}

async function loadAlerts() {
  try {
    const data = await DataQualityApi.getAlerts()
    alerts.value = data.alerts || []
    alertStats.value = {
      total: data.total_alerts || 0,
      critical: data.critical || 0,
      warning: data.warning || 0,
    }
  } catch (e: unknown) {
    // 告警加载失败不阻塞主页面
  }
}

async function runScan() {
  scanning.value = true
  error.value = ''
  try {
    await DataQualityApi.triggerScan({ lookback_hours: 1 })
    await Promise.all([loadOverview(), loadAlerts()])
  } catch (e: unknown) {
    error.value = e instanceof Error ? e.message : '扫描失败'
  } finally {
    scanning.value = false
  }
}

function gradeColor(grade: string): string {
  switch (grade) {
    case 'A': return 'var(--profit)'
    case 'B': return 'var(--primary)'
    case 'C': return 'var(--warning)'
    case 'D': return 'var(--loss)'
    default: return 'var(--text-muted)'
  }
}

function gradeLabel(grade: string): string {
  switch (grade) {
    case 'A': return '优秀'
    case 'B': return '良好'
    case 'C': return '警告'
    case 'D': return '异常'
    default: return grade
  }
}

function formatFreshness(sec: number | null): string {
  if (sec == null) return '未知'
  if (sec < 60) return sec.toFixed(0) + ' 秒'
  if (sec < 3600) return (sec / 60).toFixed(0) + ' 分钟'
  if (sec < 86400) return (sec / 3600).toFixed(1) + ' 小时'
  return (sec / 86400).toFixed(1) + ' 天'
}

function formatPct(v: number, digits = 2): string {
  return (v * 100).toFixed(digits) + '%'
}

function formatTime(t: string): string {
  return new Date(t).toLocaleString('zh-CN', {
    month: '2-digit', day: '2-digit', hour: '2-digit', minute: '2-digit',
  })
}

onMounted(() => {
  loadOverview()
  loadAlerts()
})
</script>

<template>
  <div class="space-y-6 animate-fade-in">
    <!-- Header -->
    <div class="flex items-center justify-between">
      <div>
        <h1 class="text-2xl font-bold" style="color: var(--text-primary)">数据质量监控</h1>
        <p class="text-sm mt-1" style="color: var(--text-secondary)">
          新鲜度、缺口率、覆盖率、异常值的自动扫描与告警
        </p>
      </div>
      <div class="flex gap-2">
        <button @click="runScan" class="btn btn-primary" :disabled="scanning">
          <Zap class="w-4 h-4" :class="scanning ? 'animate-pulse' : ''" />
          {{ scanning ? '扫描中...' : '手动扫描' }}
        </button>
        <button @click="() => { loadOverview(); loadAlerts() }" class="btn btn-secondary">
          <RefreshCw class="w-4 h-4" :class="loading ? 'animate-spin' : ''" />
          刷新
        </button>
      </div>
    </div>

    <!-- 错误提示 -->
    <div v-if="error" class="card p-4 flex items-center gap-3" style="border-left: 3px solid var(--loss)">
      <AlertTriangle class="w-5 h-5" style="color: var(--loss)" />
      <span class="text-sm" style="color: var(--loss)">{{ error }}</span>
    </div>

    <!-- 统计卡片 -->
    <div class="grid grid-cols-2 md:grid-cols-4 gap-4">
      <div class="card p-4">
        <div class="flex items-center gap-2 mb-1">
          <Database class="w-4 h-4" style="color: var(--text-muted)" />
          <span class="text-xs" style="color: var(--text-muted)">数据源总数</span>
        </div>
        <div class="text-2xl font-bold" style="color: var(--text-primary)">{{ stats.total }}</div>
      </div>
      <div class="card p-4">
        <div class="flex items-center gap-2 mb-1">
          <CheckCircle class="w-4 h-4" style="color: var(--profit)" />
          <span class="text-xs" style="color: var(--text-muted)">健康 (A/B)</span>
        </div>
        <div class="text-2xl font-bold" style="color: var(--profit)">{{ stats.healthy }}</div>
      </div>
      <div class="card p-4">
        <div class="flex items-center gap-2 mb-1">
          <AlertTriangle class="w-4 h-4" style="color: var(--warning)" />
          <span class="text-xs" style="color: var(--text-muted)">警告 (C)</span>
        </div>
        <div class="text-2xl font-bold" style="color: var(--warning)">{{ stats.warning }}</div>
      </div>
      <div class="card p-4">
        <div class="flex items-center gap-2 mb-1">
          <AlertCircle class="w-4 h-4" style="color: var(--loss)" />
          <span class="text-xs" style="color: var(--text-muted)">异常 (D)</span>
        </div>
        <div class="text-2xl font-bold" style="color: var(--loss)">{{ stats.critical }}</div>
      </div>
    </div>

    <!-- 告警摘要 -->
    <div v-if="alertStats.total > 0" class="card p-4 flex items-center gap-4" style="border-left: 3px solid var(--warning)">
      <AlertTriangle class="w-5 h-5" style="color: var(--warning)" />
      <div class="flex-1">
        <span class="text-sm font-medium" style="color: var(--text-primary)">
          {{ alertStats.total }} 个数据源需要关注
        </span>
        <span class="text-xs ml-3" style="color: var(--text-muted)">
          严重: {{ alertStats.critical }} / 警告: {{ alertStats.warning }}
        </span>
      </div>
    </div>

    <!-- 质量概览表 -->
    <div class="card">
      <div class="p-5" style="border-bottom: 1px solid var(--border)">
        <h2 class="text-lg font-semibold flex items-center gap-2" style="color: var(--text-primary)">
          <BarChart3 class="w-5 h-5" />
          数据源质量概览
        </h2>
      </div>
      <div v-if="overview.length === 0 && !loading" class="p-8 text-center" style="color: var(--text-muted)">
        暂无质量数据，点击"手动扫描"开始
      </div>
      <div v-else class="overflow-x-auto">
        <table class="w-full text-sm">
          <thead>
            <tr style="border-bottom: 1px solid var(--border)">
              <th class="text-left py-3 px-4" style="color: var(--text-muted)">交易对</th>
              <th class="text-left py-3 px-4" style="color: var(--text-muted)">数据源</th>
              <th class="text-center py-3 px-4" style="color: var(--text-muted)">等级</th>
              <th class="text-right py-3 px-4" style="color: var(--text-muted)">新鲜度</th>
              <th class="text-right py-3 px-4" style="color: var(--text-muted)">覆盖率</th>
              <th class="text-right py-3 px-4" style="color: var(--text-muted)">缺口率</th>
              <th class="text-right py-3 px-4" style="color: var(--text-muted)">异常值率</th>
              <th class="text-right py-3 px-4" style="color: var(--text-muted)">实际/期望</th>
              <th class="text-left py-3 px-4" style="color: var(--text-muted)">扫描时间</th>
            </tr>
          </thead>
          <tbody>
            <tr
              v-for="item in overview"
              :key="item.symbol + item.data_source"
              style="border-bottom: 1px solid var(--border)"
              class="hover:bg-[var(--surface)]"
            >
              <td class="py-2 px-4" style="color: var(--text-primary)">{{ item.symbol }}</td>
              <td class="py-2 px-4" style="color: var(--text-secondary)">{{ item.data_source }}</td>
              <td class="text-center py-2 px-4">
                <span
                  class="text-xs font-bold px-2 py-1 rounded"
                  :style="{
                    background: gradeColor(item.quality_grade) + '20',
                    color: gradeColor(item.quality_grade)
                  }"
                >
                  {{ item.quality_grade }} - {{ gradeLabel(item.quality_grade) }}
                </span>
              </td>
              <td class="text-right py-2 px-4" :style="{ color: (item.freshness_sec ?? 999) > 300 ? 'var(--loss)' : 'var(--text-primary)' }">
                {{ formatFreshness(item.freshness_sec) }}
              </td>
              <td class="text-right py-2 px-4" :style="{ color: item.coverage_ratio >= 0.8 ? 'var(--profit)' : 'var(--loss)' }">
                {{ formatPct(item.coverage_ratio) }}
              </td>
              <td class="text-right py-2 px-4" :style="{ color: item.gap_ratio < 0.1 ? 'var(--profit)' : 'var(--warning)' }">
                {{ formatPct(item.gap_ratio) }}
              </td>
              <td class="text-right py-2 px-4" :style="{ color: item.outlier_ratio < 0.05 ? 'var(--profit)' : 'var(--warning)' }">
                {{ formatPct(item.outlier_ratio) }}
              </td>
              <td class="text-right py-2 px-4" style="color: var(--text-secondary)">
                {{ item.actual_points }} / {{ item.expected_points }}
              </td>
              <td class="py-2 px-4 text-xs" style="color: var(--text-muted)">
                {{ formatTime(item.snapshot_time) }}
              </td>
            </tr>
          </tbody>
        </table>
      </div>
    </div>
  </div>
</template>

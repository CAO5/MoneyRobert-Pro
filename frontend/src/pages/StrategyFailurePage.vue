<script setup lang="ts">
import { ref, onMounted } from 'vue'
import { BacktestApi } from '@/api'
import {
  ShieldAlert, RefreshCw, AlertTriangle, CheckCircle, Clock,
  TrendingDown, AlertCircle, Zap, Activity
} from 'lucide-vue-next'

// =========================================================
// 状态
// =========================================================

const alerts = ref<Array<Record<string, unknown>>>([])
const loading = ref(false)
const error = ref('')
const detecting = ref(false)

const filterSeverity = ref('')
const filterStatus = ref('active')

// =========================================================
// 方法
// =========================================================

async function loadAlerts() {
  loading.value = true
  error.value = ''
  try {
    const data = await BacktestApi.listFailureAlerts({
      severity: filterSeverity.value || undefined,
      status: filterStatus.value || undefined,
      limit: 100,
    })
    alerts.value = data.alerts || data || []
  } catch (e: unknown) {
    error.value = e instanceof Error ? e.message : '加载告警失败'
  } finally {
    loading.value = false
  }
}

async function runDetection() {
  detecting.value = true
  error.value = ''
  try {
    await BacktestApi.detectStrategyFailures({ lookback_days: 30 })
    await loadAlerts()
  } catch (e: unknown) {
    error.value = e instanceof Error ? e.message : '检测失败'
  } finally {
    detecting.value = false
  }
}

async function acknowledge(alertId: string) {
  try {
    await BacktestApi.acknowledgeAlert(alertId)
    await loadAlerts()
  } catch (e: unknown) {
    error.value = e instanceof Error ? e.message : '确认失败'
  }
}

async function resolve(alertId: string) {
  try {
    await BacktestApi.resolveAlert(alertId)
    await loadAlerts()
  } catch (e: unknown) {
    error.value = e instanceof Error ? e.message : '解决失败'
  }
}

function severityMeta(severity: string): { label: string; color: string; icon: typeof AlertTriangle } {
  switch (severity) {
    case 'critical':
      return { label: '严重', color: 'var(--loss)', icon: AlertCircle }
    case 'warning':
      return { label: '警告', color: 'var(--warning)', icon: AlertTriangle }
    case 'info':
      return { label: '提示', color: 'var(--primary)', icon: Activity }
    default:
      return { label: severity, color: 'var(--text-muted)', icon: AlertTriangle }
  }
}

function detectionTypeLabel(type: string): string {
  const map: Record<string, string> = {
    drawdown_breach: '回撤突破',
    calibration_drift: '校准漂移',
    win_rate_drop: '胜率下降',
    profit_factor_drop: '盈亏比下降',
    correlation_breakdown: '相关性断裂',
    regime_shift: '市场状态切换',
  }
  return map[type] || type
}

function formatTime(t: string): string {
  return new Date(t).toLocaleString('zh-CN', {
    month: '2-digit', day: '2-digit', hour: '2-digit', minute: '2-digit',
  })
}

onMounted(() => {
  loadAlerts()
})
</script>

<template>
  <div class="space-y-6 animate-fade-in">
    <!-- Header -->
    <div class="flex items-center justify-between">
      <div>
        <h1 class="text-2xl font-bold" style="color: var(--text-primary)">策略失效告警</h1>
        <p class="text-sm mt-1" style="color: var(--text-secondary)">
          回撤突破、校准漂移、胜率下降、盈亏比下降、市场状态切换的自动检测
        </p>
      </div>
      <div class="flex gap-2">
        <button @click="runDetection" class="btn btn-primary" :disabled="detecting">
          <Zap class="w-4 h-4" :class="detecting ? 'animate-pulse' : ''" />
          {{ detecting ? '检测中...' : '运行检测' }}
        </button>
        <button @click="loadAlerts" class="btn btn-secondary">
          <RefreshCw class="w-4 h-4" :class="loading ? 'animate-spin' : ''" />
          刷新
        </button>
      </div>
    </div>

    <!-- 过滤器 -->
    <div class="card p-4 flex items-center gap-4 flex-wrap">
      <div class="flex items-center gap-2">
        <label class="text-sm" style="color: var(--text-secondary)">严重程度</label>
        <select v-model="filterSeverity" @change="loadAlerts" class="input" style="width: 120px">
          <option value="">全部</option>
          <option value="critical">严重</option>
          <option value="warning">警告</option>
          <option value="info">提示</option>
        </select>
      </div>
      <div class="flex items-center gap-2">
        <label class="text-sm" style="color: var(--text-secondary)">状态</label>
        <select v-model="filterStatus" @change="loadAlerts" class="input" style="width: 120px">
          <option value="">全部</option>
          <option value="active">活跃</option>
          <option value="acknowledged">已确认</option>
          <option value="resolved">已解决</option>
        </select>
      </div>
      <div class="ml-auto text-sm" style="color: var(--text-muted)">
        共 {{ alerts.length }} 条告警
      </div>
    </div>

    <!-- 错误提示 -->
    <div v-if="error" class="card p-4 flex items-center gap-3" style="border-left: 3px solid var(--loss)">
      <AlertTriangle class="w-5 h-5" style="color: var(--loss)" />
      <span class="text-sm" style="color: var(--loss)">{{ error }}</span>
    </div>

    <!-- 告警列表 -->
    <div v-if="alerts.length === 0 && !loading" class="card p-12 text-center">
      <CheckCircle class="w-12 h-12 mx-auto mb-3" style="color: var(--profit)" />
      <p class="text-sm" style="color: var(--text-muted)">暂无告警，策略运行正常</p>
    </div>

    <div v-else class="space-y-3">
      <div
        v-for="alert in alerts"
        :key="(alert.alert_id as string) || (alert.id as string)"
        class="card p-4"
        :style="{ borderLeft: `3px solid ${severityMeta(alert.severity as string).color}` }"
      >
        <div class="flex items-start justify-between">
          <div class="flex items-start gap-3 flex-1">
            <component
              :is="severityMeta(alert.severity as string).icon"
              class="w-5 h-5 mt-0.5"
              :style="{ color: severityMeta(alert.severity as string).color }"
            />
            <div class="flex-1">
              <div class="flex items-center gap-2 mb-1">
                <span class="font-semibold" style="color: var(--text-primary)">
                  {{ detectionTypeLabel(alert.detection_type as string) }}
                </span>
                <span
                  class="text-xs px-2 py-0.5 rounded"
                  :style="{
                    background: severityMeta(alert.severity as string).color + '20',
                    color: severityMeta(alert.severity as string).color
                  }"
                >
                  {{ severityMeta(alert.severity as string).label }}
                </span>
                <span
                  v-if="alert.status"
                  class="text-xs px-2 py-0.5 rounded"
                  :style="{
                    background: 'var(--surface)',
                    color: alert.status === 'resolved' ? 'var(--profit)' : alert.status === 'acknowledged' ? 'var(--primary)' : 'var(--warning)'
                  }"
                >
                  {{ alert.status === 'resolved' ? '已解决' : alert.status === 'acknowledged' ? '已确认' : '活跃' }}
                </span>
              </div>
              <p class="text-sm" style="color: var(--text-secondary)">
                {{ alert.message || alert.description || '无描述' }}
              </p>
              <div class="flex items-center gap-4 mt-2 text-xs" style="color: var(--text-muted)">
                <span v-if="alert.symbol" class="flex items-center gap-1">
                  <Activity class="w-3 h-3" />
                  {{ alert.symbol }}
                </span>
                <span v-if="alert.detected_at || alert.created_at" class="flex items-center gap-1">
                  <Clock class="w-3 h-3" />
                  {{ formatTime((alert.detected_at || alert.created_at) as string) }}
                </span>
                <span v-if="alert.current_value != null && alert.threshold != null">
                  当前: {{ (alert.current_value as number).toFixed(4) }} / 阈值: {{ (alert.threshold as number).toFixed(4) }}
                </span>
              </div>
            </div>
          </div>

          <!-- 操作按钮 -->
          <div v-if="alert.status === 'active'" class="flex gap-2 ml-4">
            <button
              @click="acknowledge((alert.alert_id || alert.id) as string)"
              class="btn btn-sm btn-secondary"
            >
              确认
            </button>
            <button
              @click="resolve((alert.alert_id || alert.id) as string)"
              class="btn btn-sm btn-primary"
            >
              解决
            </button>
          </div>
        </div>
      </div>
    </div>
  </div>
</template>

<script setup lang="ts">
import { onMounted, onUnmounted, ref } from 'vue'
import api from '@/api'

interface AssetStatus {
  asset: string
  sample: null | {
    cdfp_value: number | null
    spot_imbalance: number | null
    perp_imbalance: number | null
    same_direction_bins: number | null
    market_regime: string
    volume_quality: string
    quality_status: string
    available_time: number
  }
  five_minute: null | { log_return: number; completed_time: number }
}

interface Status {
  state: string
  factor_version: string
  started_time: number | null
  assets: AssetStatus[]
}

interface AssetReport {
  samples: number
  scored_samples: number
  nonoverlapping_samples: number
  score_distribution: Record<string, number>
  exclusions: Record<string, { count: number; fraction_of_all_samples: number | null }>
  conditional_returns_nonoverlapping: Record<string, { count: number; mean_log_return: number | null }>
  out_of_sample: {
    count: number
    incremental_cdfp_mse_reduction: number | null
    cdfp_only_mse_reduction_vs: Record<string, number | null>
  }
}

interface Report {
  status: string
  assets: Record<string, AssetReport>
}

const status = ref<Status>()
const report = ref<Report>()
const statusError = ref('')
const reportError = ref('')
const refreshing = ref(false)
const reporting = ref(false)
let timer: number | undefined

const labels: Record<string, string> = {
  DISABLED: '采集未启用', STARTING: '正在连接', COLLECTING: '采集中', FINISHED: '采集已结束',
  STALE: '采集已中断', WARMUP: '预热中', NORMAL: '非极端', EXTREME: '极端',
  GAP: '数据缺口', UNKNOWN: '门槛待建立', LOW: '低活跃', OK: '通过',
  AWAITING_SAMPLE: '等待样本', REVIEWABLE: '有可评估样本',
}
const comparisons = {
  momentum_only: '价格动量', spot_only: '现货流', perp_only: '永续流',
}
const groups = ['[-1,-0.6)', '[-0.6,-0.2)', '[-0.2,0.2)', '[0.2,0.6)', '[0.6,1]']

function formatTime(value: number | null | undefined) {
  return value == null ? '—' : new Date(value).toLocaleString('zh-CN', { hour12: false })
}

function formatNumber(value: number | null | undefined) {
  return value == null ? '—' : value.toFixed(3)
}

function formatPercent(value: number | null | undefined) {
  return value == null ? '—' : `${(value * 100).toFixed(2)}%`
}

async function refreshStatus() {
  if (refreshing.value) return
  refreshing.value = true
  try {
    status.value = (await api.get<Status>('/research/cdfp/status')).data
    statusError.value = ''
  } catch {
    statusError.value = '无法获取 CDFP 采集状态'
  } finally {
    refreshing.value = false
  }
}

async function loadReport() {
  reporting.value = true
  reportError.value = ''
  try {
    report.value = (await api.get<Report>('/research/cdfp/report')).data
  } catch {
    reportError.value = '无法获取 CDFP 研究报告'
  } finally {
    reporting.value = false
  }
}

onMounted(() => {
  void refreshStatus()
  timer = window.setInterval(refreshStatus, 10_000)
})
onUnmounted(() => window.clearInterval(timer))
</script>

<template>
  <div class="space-y-6">
    <header class="flex flex-wrap items-start justify-between gap-3">
      <div>
        <h1 class="text-2xl font-bold">CDFP 研究</h1>
        <p class="mt-1 text-sm" style="color: var(--text-secondary)">
          OKX 现货与永续同向成交压力 · 每 10 秒更新
        </p>
      </div>
      <button class="btn btn-secondary" :disabled="refreshing" @click="refreshStatus">刷新状态</button>
    </header>

    <section class="card space-y-3 p-4">
      <p v-if="statusError" role="alert" style="color: var(--loss)">{{ statusError }}</p>
      <template v-else-if="status">
        <div class="flex flex-wrap items-center gap-3">
          <h2 class="font-semibold">{{ labels[status.state] ?? status.state }}</h2>
          <span class="text-sm" style="color: var(--text-secondary)">{{ status.factor_version }}</span>
          <span v-if="status.started_time" class="text-sm" style="color: var(--text-secondary)">
            启动于 {{ formatTime(status.started_time) }}
          </span>
        </div>
        <div class="overflow-x-auto">
          <table class="w-full whitespace-nowrap text-left text-sm">
            <thead style="color: var(--text-secondary)">
              <tr>
                <th v-for="column in ['资产', 'CDFP', '现货失衡', '永续失衡', '同向窗口', '质量 / 活跃度 / 行情', '可用时间', '最近已完成 5 分钟收益']"
                    :key="column" scope="col" class="px-2 py-2">{{ column }}</th>
              </tr>
            </thead>
            <tbody class="tabular-nums">
              <tr v-for="item in status.assets" :key="item.asset">
                <th scope="row" class="px-2 py-2">{{ item.asset }}</th>
                <td class="px-2 py-2">{{ formatNumber(item.sample?.cdfp_value) }}</td>
                <td class="px-2 py-2">{{ formatNumber(item.sample?.spot_imbalance) }}</td>
                <td class="px-2 py-2">{{ formatNumber(item.sample?.perp_imbalance) }}</td>
                <td class="px-2 py-2">{{ item.sample ? (item.sample.same_direction_bins ?? '—') : '—' }}</td>
                <td class="px-2 py-2">{{ item.sample
                  ? [item.sample.quality_status, item.sample.volume_quality, item.sample.market_regime].map(key => labels[key] ?? key).join(' / ')
                  : '—' }}</td>
                <td class="px-2 py-2">{{ formatTime(item.sample?.available_time) }}</td>
                <td class="px-2 py-2" :title="formatTime(item.five_minute?.completed_time)">
                  {{ formatPercent(item.five_minute?.log_return) }}
                </td>
              </tr>
            </tbody>
          </table>
        </div>
      </template>
      <p v-else>正在读取采集状态…</p>
    </section>

    <section class="card space-y-4 p-4">
      <div class="flex flex-wrap items-center justify-between gap-3">
        <h2 class="font-semibold">5 分钟研究报告</h2>
        <button class="btn btn-secondary" :disabled="reporting" @click="loadReport">
          {{ reporting ? '正在计算…' : '查看阶段报告' }}
        </button>
      </div>
      <p v-if="reportError" role="alert" style="color: var(--loss)">{{ reportError }}</p>
      <template v-if="report">
        <p class="text-sm" style="color: var(--text-secondary)" role="status">
          {{ labels[report.status] ?? report.status }}
        </p>
        <div class="grid gap-4 lg:grid-cols-3">
          <article v-for="(result, asset) in report.assets" :key="asset"
                   class="space-y-3 rounded-lg border p-3 text-sm" style="border-color: var(--border)">
            <h3 class="font-semibold">{{ asset }}</h3>
            <div>{{ result.scored_samples }} / {{ result.samples }} 条有分数 · {{ result.nonoverlapping_samples }} 条非重叠样本</div>
            <dl class="grid grid-cols-2 gap-x-2 gap-y-1">
              <dt>缺口比例</dt><dd>{{ formatPercent(result.exclusions.data_gap?.fraction_of_all_samples) }}</dd>
              <dt>低活跃比例</dt><dd>{{ formatPercent(result.exclusions.low_volume?.fraction_of_all_samples) }}</dd>
              <dt>极端行情比例</dt><dd>{{ formatPercent(result.exclusions.extreme_market?.fraction_of_all_samples) }}</dd>
              <dt>样本外样本数</dt><dd>{{ result.out_of_sample.count }}</dd>
              <dt>联合基准 MSE 降幅</dt><dd>{{ formatPercent(result.out_of_sample.incremental_cdfp_mse_reduction) }}</dd>
              <template v-for="(label, key) in comparisons" :key="key">
                <dt>相对{{ label }} MSE 降幅</dt>
                <dd>{{ formatPercent(result.out_of_sample.cdfp_only_mse_reduction_vs[key]) }}</dd>
              </template>
            </dl>
            <table class="w-full text-left tabular-nums">
              <thead>
                <tr><th scope="col">分数组</th><th scope="col">频数</th><th scope="col">平均收益</th></tr>
              </thead>
              <tbody>
                <tr v-for="group in groups" :key="group">
                  <th scope="row" class="font-normal">{{ group }}</th>
                  <td>{{ result.score_distribution[group] }}</td>
                  <td>{{ formatPercent(result.conditional_returns_nonoverlapping[group]?.mean_log_return) }}</td>
                </tr>
              </tbody>
            </table>
          </article>
        </div>
      </template>
    </section>
  </div>
</template>

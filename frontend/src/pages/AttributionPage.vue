<script setup lang="ts">
import { ref, onMounted } from 'vue'
import { BacktestApi } from '@/api'
import {
  FlaskConical, RefreshCw, AlertTriangle, TrendingUp, TrendingDown,
  DollarSign, Percent, BarChart3, PieChart
} from 'lucide-vue-next'

// =========================================================
// 状态
// =========================================================

const jobs = ref<Array<{ job_id: string; job_name: string }>>([])
const selectedJobId = ref('')
const loading = ref(false)
const error = ref('')

const attributions = ref<unknown[]>([])
const summary = ref<Record<string, unknown> | null>(null)

// =========================================================
// 方法
// =========================================================

async function loadJobs() {
  try {
    const jobList = await BacktestApi.listJobs()
    jobs.value = jobList.map(j => ({ job_id: j.job_id, job_name: j.job_name }))
    if (jobs.value.length > 0 && !selectedJobId.value) {
      selectedJobId.value = jobs.value[0].job_id
      await loadAttributions()
    }
  } catch (e: unknown) {
    error.value = e instanceof Error ? e.message : '加载回测任务失败'
  }
}

async function loadAttributions() {
  if (!selectedJobId.value) return
  loading.value = true
  error.value = ''
  try {
    const [attrData, sumData] = await Promise.all([
      BacktestApi.listAttributions(selectedJobId.value),
      BacktestApi.getAttributionSummary(selectedJobId.value).catch(() => null),
    ])
    attributions.value = attrData.attributions || attrData || []
    summary.value = sumData
  } catch (e: unknown) {
    error.value = e instanceof Error ? e.message : '加载归因数据失败'
  } finally {
    loading.value = false
  }
}

function formatNum(v: number | undefined | null, digits = 2): string {
  if (v == null || isNaN(v)) return '-'
  if (Math.abs(v) >= 1e6) return (v / 1e6).toFixed(digits) + 'M'
  if (Math.abs(v) >= 1e3) return (v / 1e3).toFixed(digits) + 'K'
  return v.toFixed(digits)
}

function formatPct(v: number | undefined | null, digits = 2): string {
  if (v == null || isNaN(v)) return '-'
  return (v * 100).toFixed(digits) + '%'
}

function formatTime(t: string): string {
  return new Date(t).toLocaleString('zh-CN', {
    month: '2-digit', day: '2-digit', hour: '2-digit', minute: '2-digit',
  })
}

onMounted(() => {
  loadJobs()
})
</script>

<template>
  <div class="space-y-6 animate-fade-in">
    <!-- Header -->
    <div class="flex items-center justify-between">
      <div>
        <h1 class="text-2xl font-bold" style="color: var(--text-primary)">归因分析</h1>
        <p class="text-sm mt-1" style="color: var(--text-secondary)">
          交易后盈亏分解：毛盈亏 → 手续费/滑点/资金费率/冲击 → 净盈亏
        </p>
      </div>
      <button @click="loadAttributions" class="btn btn-secondary" :disabled="!selectedJobId">
        <RefreshCw class="w-4 h-4" :class="loading ? 'animate-spin' : ''" />
        刷新
      </button>
    </div>

    <!-- Job 选择器 -->
    <div class="card p-4 flex items-center gap-4">
      <label class="text-sm font-medium" style="color: var(--text-secondary)">回测任务</label>
      <select v-model="selectedJobId" @change="loadAttributions" class="input" style="min-width: 300px">
        <option value="">请选择任务</option>
        <option v-for="job in jobs" :key="job.job_id" :value="job.job_id">
          {{ job.job_name }} ({{ job.job_id.substring(0, 8) }})
        </option>
      </select>
    </div>

    <!-- 错误提示 -->
    <div v-if="error" class="card p-4 flex items-center gap-3" style="border-left: 3px solid var(--loss)">
      <AlertTriangle class="w-5 h-5" style="color: var(--loss)" />
      <span class="text-sm" style="color: var(--loss)">{{ error }}</span>
    </div>

    <!-- 汇总卡片 -->
    <div v-if="summary" class="grid grid-cols-2 md:grid-cols-4 gap-4">
      <div class="card p-4">
        <div class="flex items-center gap-2 mb-1">
          <DollarSign class="w-4 h-4" style="color: var(--text-muted)" />
          <span class="text-xs" style="color: var(--text-muted)">总毛盈亏</span>
        </div>
        <div class="text-xl font-bold" :style="{
          color: (summary.total_gross_pnl as number) >= 0 ? 'var(--profit)' : 'var(--loss)'
        }">
          {{ formatNum(summary.total_gross_pnl as number) }}
        </div>
      </div>
      <div class="card p-4">
        <div class="flex items-center gap-2 mb-1">
          <Percent class="w-4 h-4" style="color: var(--text-muted)" />
          <span class="text-xs" style="color: var(--text-muted)">总成本</span>
        </div>
        <div class="text-xl font-bold" style="color: var(--loss)">
          {{ formatNum(summary.total_cost as number) }}
        </div>
      </div>
      <div class="card p-4">
        <div class="flex items-center gap-2 mb-1">
          <DollarSign class="w-4 h-4" style="color: var(--text-muted)" />
          <span class="text-xs" style="color: var(--text-muted)">总净盈亏</span>
        </div>
        <div class="text-xl font-bold" :style="{
          color: (summary.total_net_pnl as number) >= 0 ? 'var(--profit)' : 'var(--loss)'
        }">
          {{ formatNum(summary.total_net_pnl as number) }}
        </div>
      </div>
      <div class="card p-4">
        <div class="flex items-center gap-2 mb-1">
          <PieChart class="w-4 h-4" style="color: var(--text-muted)" />
          <span class="text-xs" style="color: var(--text-muted)">成本占比</span>
        </div>
        <div class="text-xl font-bold" style="color: var(--warning)">
          {{ formatPct(summary.cost_ratio as number) }}
        </div>
      </div>
    </div>

    <!-- 归因列表 -->
    <div class="card">
      <div class="p-5" style="border-bottom: 1px solid var(--border)">
        <h2 class="text-lg font-semibold flex items-center gap-2" style="color: var(--text-primary)">
          <BarChart3 class="w-5 h-5" />
          交易归因明细
        </h2>
      </div>
      <div v-if="attributions.length === 0 && !loading" class="p-8 text-center" style="color: var(--text-muted)">
        {{ selectedJobId ? '暂无归因数据' : '请选择回测任务' }}
      </div>
      <div v-else class="overflow-x-auto">
        <table class="w-full text-sm">
          <thead>
            <tr style="border-bottom: 1px solid var(--border)">
              <th class="text-left py-3 px-4" style="color: var(--text-muted)">交易对</th>
              <th class="text-right py-3 px-4" style="color: var(--text-muted)">毛盈亏</th>
              <th class="text-right py-3 px-4" style="color: var(--text-muted)">手续费</th>
              <th class="text-right py-3 px-4" style="color: var(--text-muted)">滑点</th>
              <th class="text-right py-3 px-4" style="color: var(--text-muted)">资金费率</th>
              <th class="text-right py-3 px-4" style="color: var(--text-muted)">净盈亏</th>
              <th class="text-left py-3 px-4" style="color: var(--text-muted)">标签</th>
            </tr>
          </thead>
          <tbody>
            <tr
              v-for="(attr, idx) in attributions.slice(0, 50)"
              :key="idx"
              style="border-bottom: 1px solid var(--border)"
              class="hover:bg-[var(--surface)]"
            >
              <td class="py-2 px-4" style="color: var(--text-primary)">
                {{ (attr as Record<string, unknown>).symbol || '-' }}
              </td>
              <td class="text-right py-2 px-4" :style="{
                color: ((attr as Record<string, number>).gross_pnl) >= 0 ? 'var(--profit)' : 'var(--loss)'
              }">
                {{ formatNum((attr as Record<string, number>).gross_pnl) }}
              </td>
              <td class="text-right py-2 px-4" style="color: var(--loss)">
                {{ formatNum((attr as Record<string, number>).fee_cost) }}
              </td>
              <td class="text-right py-2 px-4" style="color: var(--loss)">
                {{ formatNum((attr as Record<string, number>).slippage_cost) }}
              </td>
              <td class="text-right py-2 px-4" style="color: var(--loss)">
                {{ formatNum((attr as Record<string, number>).funding_cost) }}
              </td>
              <td class="text-right py-2 px-4 font-semibold" :style="{
                color: ((attr as Record<string, number>).net_pnl) >= 0 ? 'var(--profit)' : 'var(--loss)'
              }">
                {{ formatNum((attr as Record<string, number>).net_pnl) }}
              </td>
              <td class="py-2 px-4">
                <span class="text-xs px-2 py-0.5 rounded" style="background: var(--surface); color: var(--text-secondary)">
                  {{ (attr as Record<string, unknown>).attribution_label || '-' }}
                </span>
              </td>
            </tr>
          </tbody>
        </table>
      </div>
    </div>
  </div>
</template>

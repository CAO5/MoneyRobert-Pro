<script setup lang="ts">
import { ref, computed, onMounted } from 'vue'
import { BacktestApi, type BacktestJobSummary, type BacktestJobDetail, type BacktestReport, type TrustLevelResponse } from '@/api'
import {
  FlaskConical, RefreshCw, Play, ShieldCheck, AlertTriangle, CheckCircle,
  XCircle, TrendingUp, TrendingDown, FileText, ChevronRight, Loader2
} from 'lucide-vue-next'

// =========================================================
// 状态
// =========================================================

const jobs = ref<BacktestJobSummary[]>([])
const selectedJob = ref<BacktestJobDetail | null>(null)
const report = ref<BacktestReport | null>(null)
const trustLevel = ref<TrustLevelResponse | null>(null)
const loading = ref(false)
const detailLoading = ref(false)
const assessing = ref(false)
const starting = ref(false)
const error = ref('')
const selectedJobId = ref('')

// =========================================================
// 计算属性与辅助函数
// =========================================================

/// 任务状态中文映射与颜色
function statusMeta(status: string): { label: string; color: string } {
  switch (status) {
    case 'created': return { label: '已创建', color: 'var(--text-muted)' }
    case 'running': return { label: '运行中', color: 'var(--profit)' }
    case 'completed': return { label: '已完成', color: 'var(--profit)' }
    case 'failed': return { label: '失败', color: 'var(--loss)' }
    case 'cancelled': return { label: '已取消', color: 'var(--warning)' }
    default: return { label: status, color: 'var(--text-muted)' }
  }
}

/// 可信等级中文映射与颜色
function trustMeta(level: string): { label: string; color: string; desc: string } {
  switch (level) {
    case 'display_only':
      return { label: '仅展示', color: 'var(--loss)', desc: '结果不可用于策略比较或晋级' }
    case 'comparable':
      return { label: '可比较', color: 'var(--warning)', desc: '结果可用于策略比较，不可直接晋级' }
    case 'promotion_eligible':
      return { label: '可晋级', color: 'var(--profit)', desc: '结果可用于策略晋级与实盘授权' }
    default:
      return { label: '未评估', color: 'var(--text-muted)', desc: '尚未进行可信等级评估' }
  }
}

/// 格式化百分比
function pct(v?: number, digits = 2): string {
  if (v == null) return '—'
  return (v * 100).toFixed(digits) + '%'
}

/// 格式化数字
function num(v?: number, digits = 2): string {
  if (v == null) return '—'
  return v.toFixed(digits)
}

/// 格式化时间
function formatTime(t?: string): string {
  if (!t) return '—'
  return new Date(t).toLocaleString('zh-CN', {
    month: '2-digit', day: '2-digit', hour: '2-digit', minute: '2-digit',
  })
}

// =========================================================
// 方法
// =========================================================

async function loadJobs() {
  loading.value = true
  error.value = ''
  try {
    jobs.value = await BacktestApi.listJobs()
    if (jobs.value.length && !selectedJobId.value) {
      await selectJob(jobs.value[0].job_id)
    }
  } catch (e: unknown) {
    error.value = e instanceof Error ? e.message : '加载回测任务列表失败'
  } finally {
    loading.value = false
  }
}

async function selectJob(jobId: string) {
  selectedJobId.value = jobId
  selectedJob.value = null
  report.value = null
  trustLevel.value = null
  detailLoading.value = true
  error.value = ''
  try {
    // 并行加载详情、报告、可信等级
    const [job, rpt, trust] = await Promise.allSettled([
      BacktestApi.getJob(jobId),
      BacktestApi.getReport(jobId),
      BacktestApi.getTrustLevel(jobId),
    ])
    if (job.status === 'fulfilled') selectedJob.value = job.value
    if (rpt.status === 'fulfilled') report.value = rpt.value
    if (trust.status === 'fulfilled') trustLevel.value = trust.value
  } catch (e: unknown) {
    error.value = e instanceof Error ? e.message : '加载任务详情失败'
  } finally {
    detailLoading.value = false
  }
}

async function startJob() {
  if (!selectedJobId.value) return
  starting.value = true
  error.value = ''
  try {
    await BacktestApi.startJob(selectedJobId.value)
    // 刷新任务状态
    await selectJob(selectedJobId.value)
  } catch (e: unknown) {
    error.value = e instanceof Error ? e.message : '启动回测失败'
  } finally {
    starting.value = false
  }
}

async function assessTrust() {
  if (!selectedJobId.value) return
  assessing.value = true
  error.value = ''
  try {
    trustLevel.value = await BacktestApi.assessTrustLevel(selectedJobId.value)
  } catch (e: unknown) {
    error.value = e instanceof Error ? e.message : '评估可信等级失败'
  } finally {
    assessing.value = false
  }
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
        <h1 class="text-2xl font-bold" style="color: var(--text-primary)">回测中心</h1>
        <p class="text-sm mt-1" style="color: var(--text-secondary)">回测任务管理与可信等级门禁</p>
      </div>
      <button @click="loadJobs" class="btn btn-secondary">
        <RefreshCw class="w-4 h-4" :class="loading ? 'animate-spin' : ''" />
        刷新
      </button>
    </div>

    <!-- 错误提示 -->
    <div v-if="error" class="card p-4 flex items-center gap-3" style="border-left: 3px solid var(--loss)">
      <AlertTriangle class="w-5 h-5" style="color: var(--loss)" />
      <span class="text-sm" style="color: var(--loss)">{{ error }}</span>
    </div>

    <div class="grid grid-cols-1 lg:grid-cols-3 gap-6">
      <!-- 左列：任务列表 -->
      <div class="card">
        <div class="p-4" style="border-bottom: 1px solid var(--border)">
          <h2 class="font-semibold flex items-center gap-2" style="color: var(--text-primary)">
            <FlaskConical class="w-4 h-4" />
            回测任务
          </h2>
        </div>
        <div v-if="loading" class="flex items-center justify-center py-16">
          <div class="spinner"></div>
        </div>
        <div v-else-if="jobs.length" class="divide-y" style="border-color: var(--border)">
          <div
            v-for="j in jobs"
            :key="j.job_id"
            @click="selectJob(j.job_id)"
            class="p-4 cursor-pointer transition-colors"
            :style="selectedJobId === j.job_id ? { background: 'var(--surface-secondary)' } : {}"
          >
            <div class="flex items-center justify-between mb-1">
              <span class="font-medium text-sm" style="color: var(--text-primary)">{{ j.job_name }}</span>
              <span
                class="px-2 py-0.5 rounded text-xs font-medium"
                :style="{ background: statusMeta(j.status).color + '20', color: statusMeta(j.status).color }"
              >
                {{ statusMeta(j.status).label }}
              </span>
            </div>
            <div class="flex items-center gap-3 text-xs" style="color: var(--text-muted)">
              <span>{{ formatTime(j.start_time) }}</span>
              <span v-if="j.total_return_pct != null" class="font-mono" :style="{ color: j.total_return_pct >= 0 ? 'var(--profit)' : 'var(--loss)' }">
                {{ j.total_return_pct >= 0 ? '+' : '' }}{{ j.total_return_pct.toFixed(2) }}%
              </span>
              <span v-if="j.sharpe_ratio != null" class="font-mono">SR: {{ j.sharpe_ratio.toFixed(2) }}</span>
            </div>
          </div>
        </div>
        <div v-else class="py-16 text-center">
          <p class="text-sm" style="color: var(--text-muted)">暂无回测任务</p>
        </div>
      </div>

      <!-- 右列：任务详情 -->
      <div class="lg:col-span-2 space-y-6">
        <div v-if="detailLoading" class="card flex items-center justify-center py-24">
          <div class="spinner"></div>
          <span class="ml-3 text-sm" style="color: var(--text-muted)">加载任务详情...</span>
        </div>

        <template v-else-if="selectedJob">
          <!-- 任务配置 -->
          <div class="card">
            <div class="p-5 flex items-center justify-between" style="border-bottom: 1px solid var(--border)">
              <h2 class="text-lg font-semibold" style="color: var(--text-primary)">{{ selectedJob.job_name }}</h2>
              <div class="flex items-center gap-2">
                <span
                  class="px-3 py-1 rounded-lg text-xs font-medium"
                  :style="{ background: statusMeta(selectedJob.status).color + '20', color: statusMeta(selectedJob.status).color }"
                >
                  {{ statusMeta(selectedJob.status).label }}
                </span>
                <button
                  v-if="selectedJob.status === 'created'"
                  @click="startJob"
                  :disabled="starting"
                  class="btn btn-primary btn-sm"
                >
                  <Play v-if="!starting" class="w-3.5 h-3.5" />
                  <Loader2 v-else class="w-3.5 h-3.5 animate-spin" />
                  {{ starting ? '启动中' : '启动' }}
                </button>
              </div>
            </div>
            <div class="p-5 grid grid-cols-2 md:grid-cols-4 gap-4 text-sm">
              <div>
                <div class="text-xs mb-1" style="color: var(--text-muted)">标的</div>
                <div class="font-medium" style="color: var(--text-primary)">{{ selectedJob.assets.join(', ') }}</div>
              </div>
              <div>
                <div class="text-xs mb-1" style="color: var(--text-muted)">时间范围</div>
                <div class="font-medium text-xs" style="color: var(--text-secondary)">
                  {{ formatTime(selectedJob.start_time) }} ~ {{ formatTime(selectedJob.end_time) }}
                </div>
              </div>
              <div>
                <div class="text-xs mb-1" style="color: var(--text-muted)">初始资金</div>
                <div class="font-mono font-medium" style="color: var(--text-primary)">${{ selectedJob.initial_equity.toLocaleString() }}</div>
              </div>
              <div>
                <div class="text-xs mb-1" style="color: var(--text-muted)">数据频率</div>
                <div class="font-medium" style="color: var(--text-primary)">{{ selectedJob.data_frequency }}</div>
              </div>
              <div>
                <div class="text-xs mb-1" style="color: var(--text-muted)">手续费 (Taker/Maker)</div>
                <div class="font-mono font-medium" style="color: var(--text-primary)">
                  {{ selectedJob.fee_taker_bps }} / {{ selectedJob.fee_maker_bps }} bps
                </div>
              </div>
              <div>
                <div class="text-xs mb-1" style="color: var(--text-muted)">滑点</div>
                <div class="font-mono font-medium" style="color: var(--text-primary)">{{ selectedJob.slippage_bps }} bps</div>
              </div>
              <div>
                <div class="text-xs mb-1" style="color: var(--text-muted)">最大杠杆</div>
                <div class="font-mono font-medium" style="color: var(--text-primary)">{{ selectedJob.max_total_leverage }}x</div>
              </div>
              <div>
                <div class="text-xs mb-1" style="color: var(--text-muted)">日亏损上限</div>
                <div class="font-mono font-medium" style="color: var(--loss)">{{ pct(selectedJob.max_daily_loss_pct, 1) }}</div>
              </div>
            </div>
          </div>

          <!-- 绩效报告 -->
          <div v-if="report" class="card">
            <div class="p-5" style="border-bottom: 1px solid var(--border)">
              <h2 class="text-lg font-semibold flex items-center gap-2" style="color: var(--text-primary)">
                <FileText class="w-5 h-5" />
                绩效报告
              </h2>
            </div>
            <div class="p-5 grid grid-cols-2 md:grid-cols-4 gap-4">
              <div class="p-3 rounded-lg" style="background: var(--surface-secondary)">
                <div class="text-xs mb-1" style="color: var(--text-muted)">总收益</div>
                <div class="text-lg font-mono font-semibold" :style="{ color: (report.total_return ?? 0) >= 0 ? 'var(--profit)' : 'var(--loss)' }">
                  {{ report.total_return != null ? (report.total_return * 100).toFixed(2) + '%' : '—' }}
                </div>
              </div>
              <div class="p-3 rounded-lg" style="background: var(--surface-secondary)">
                <div class="text-xs mb-1" style="color: var(--text-muted)">Sharpe</div>
                <div class="text-lg font-mono font-semibold" style="color: var(--text-primary)">
                  {{ num(report.sharpe_ratio) }}
                </div>
              </div>
              <div class="p-3 rounded-lg" style="background: var(--surface-secondary)">
                <div class="text-xs mb-1" style="color: var(--text-muted)">最大回撤</div>
                <div class="text-lg font-mono font-semibold" style="color: var(--loss)">
                  {{ report.max_drawdown != null ? (report.max_drawdown * 100).toFixed(2) + '%' : '—' }}
                </div>
              </div>
              <div class="p-3 rounded-lg" style="background: var(--surface-secondary)">
                <div class="text-xs mb-1" style="color: var(--text-muted)">胜率</div>
                <div class="text-lg font-mono font-semibold" style="color: var(--text-primary)">
                  {{ report.win_rate != null ? (report.win_rate * 100).toFixed(1) + '%' : '—' }}
                </div>
              </div>
              <div class="p-3 rounded-lg" style="background: var(--surface-secondary)">
                <div class="text-xs mb-1" style="color: var(--text-muted)">总交易数</div>
                <div class="text-lg font-mono font-semibold" style="color: var(--text-primary)">
                  {{ report.total_trades ?? '—' }}
                </div>
              </div>
              <div class="p-3 rounded-lg" style="background: var(--surface-secondary)">
                <div class="text-xs mb-1" style="color: var(--text-muted)">盈亏比</div>
                <div class="text-lg font-mono font-semibold" style="color: var(--text-primary)">
                  {{ num(report.profit_factor) }}
                </div>
              </div>
              <div class="p-3 rounded-lg" style="background: var(--surface-secondary)">
                <div class="text-xs mb-1" style="color: var(--text-muted)">总手续费</div>
                <div class="text-lg font-mono font-semibold" style="color: var(--text-primary)">
                  {{ report.total_fee != null ? '$' + report.total_fee.toFixed(2) : '—' }}
                </div>
              </div>
              <div class="p-3 rounded-lg" style="background: var(--surface-secondary)">
                <div class="text-xs mb-1" style="color: var(--text-muted)">年均收益</div>
                <div class="text-lg font-mono font-semibold" :style="{ color: (report.annualized_return ?? 0) >= 0 ? 'var(--profit)' : 'var(--loss)' }">
                  {{ report.annualized_return != null ? (report.annualized_return * 100).toFixed(2) + '%' : '—' }}
                </div>
              </div>
            </div>
          </div>

          <!-- 可信等级门禁 -->
          <div class="card">
            <div class="p-5 flex items-center justify-between" style="border-bottom: 1px solid var(--border)">
              <h2 class="text-lg font-semibold flex items-center gap-2" style="color: var(--text-primary)">
                <ShieldCheck class="w-5 h-5" />
                可信等级门禁
              </h2>
              <button
                @click="assessTrust"
                :disabled="assessing"
                class="btn btn-secondary btn-sm"
              >
                <RefreshCw v-if="!assessing" class="w-3.5 h-3.5" />
                <Loader2 v-else class="w-3.5 h-3.5 animate-spin" />
                {{ assessing ? '评估中' : (trustLevel ? '重新评估' : '触发评估') }}
              </button>
            </div>

            <div v-if="trustLevel" class="p-5 space-y-5">
              <!-- 可信等级徽章 -->
              <div class="flex items-center gap-4">
                <div
                  class="px-4 py-2 rounded-lg text-base font-semibold"
                  :style="{ background: trustMeta(trustLevel.trust_level).color + '20', color: trustMeta(trustLevel.trust_level).color }"
                >
                  {{ trustMeta(trustLevel.trust_level).label }}
                </div>
                <div class="text-sm" style="color: var(--text-secondary)">
                  {{ trustMeta(trustLevel.trust_level).desc }}
                </div>
              </div>

              <!-- 门禁检查项 -->
              <div>
                <div class="text-xs font-medium mb-3" style="color: var(--text-secondary)">门禁检查</div>
                <div class="grid grid-cols-2 gap-3">
                  <div class="flex items-center gap-2 p-2.5 rounded-lg" style="background: var(--surface-secondary)">
                    <CheckCircle v-if="trustLevel.test_coverage_passed" class="w-4 h-4" style="color: var(--profit)" />
                    <XCircle v-else class="w-4 h-4" style="color: var(--loss)" />
                    <span class="text-sm" style="color: var(--text-primary)">测试覆盖</span>
                  </div>
                  <div class="flex items-center gap-2 p-2.5 rounded-lg" style="background: var(--surface-secondary)">
                    <CheckCircle v-if="trustLevel.capital_conservation_passed" class="w-4 h-4" style="color: var(--profit)" />
                    <XCircle v-else class="w-4 h-4" style="color: var(--loss)" />
                    <span class="text-sm" style="color: var(--text-primary)">资金守恒</span>
                  </div>
                  <div class="flex items-center gap-2 p-2.5 rounded-lg" style="background: var(--surface-secondary)">
                    <CheckCircle v-if="trustLevel.slippage_accounted" class="w-4 h-4" style="color: var(--profit)" />
                    <XCircle v-else class="w-4 h-4" style="color: var(--loss)" />
                    <span class="text-sm" style="color: var(--text-primary)">滑点入账</span>
                  </div>
                  <div class="flex items-center gap-2 p-2.5 rounded-lg" style="background: var(--surface-secondary)">
                    <CheckCircle v-if="trustLevel.sample_size_sufficient" class="w-4 h-4" style="color: var(--profit)" />
                    <XCircle v-else class="w-4 h-4" style="color: var(--loss)" />
                    <span class="text-sm" style="color: var(--text-primary)">样本量充足</span>
                  </div>
                  <div class="flex items-center gap-2 p-2.5 rounded-lg" style="background: var(--surface-secondary)">
                    <CheckCircle v-if="trustLevel.walk_forward_validated" class="w-4 h-4" style="color: var(--profit)" />
                    <XCircle v-else class="w-4 h-4" style="color: var(--loss)" />
                    <span class="text-sm" style="color: var(--text-primary)">Walk-forward</span>
                  </div>
                  <div class="flex items-center gap-2 p-2.5 rounded-lg" style="background: var(--surface-secondary)">
                    <CheckCircle v-if="trustLevel.calibration_healthy" class="w-4 h-4" style="color: var(--profit)" />
                    <XCircle v-else class="w-4 h-4" style="color: var(--loss)" />
                    <span class="text-sm" style="color: var(--text-primary)">校准健康</span>
                  </div>
                </div>
              </div>

              <!-- 评估指标 -->
              <div class="grid grid-cols-3 gap-4">
                <div>
                  <div class="text-xs mb-1" style="color: var(--text-muted)">总交易数</div>
                  <div class="font-mono font-semibold" style="color: var(--text-primary)">{{ trustLevel.total_trades }}</div>
                </div>
                <div>
                  <div class="text-xs mb-1" style="color: var(--text-muted)">测试通过率</div>
                  <div class="font-mono font-semibold" :style="{ color: trustLevel.test_pass_rate >= 0.8 ? 'var(--profit)' : 'var(--loss)' }">
                    {{ (trustLevel.test_pass_rate * 100).toFixed(0) }}%
                  </div>
                </div>
                <div>
                  <div class="text-xs mb-1" style="color: var(--text-muted)">数据覆盖率</div>
                  <div class="font-mono font-semibold" :style="{ color: trustLevel.data_coverage_ratio >= 0.95 ? 'var(--profit)' : 'var(--loss)' }">
                    {{ (trustLevel.data_coverage_ratio * 100).toFixed(1) }}%
                  </div>
                </div>
              </div>

              <!-- 晋级资格 -->
              <div class="p-3 rounded-lg flex items-center gap-3" :style="{ background: trustLevel.promotion_eligible ? 'var(--profit-light)' : 'var(--surface-secondary)' }">
                <CheckCircle v-if="trustLevel.promotion_eligible" class="w-5 h-5" style="color: var(--profit)" />
                <XCircle v-else class="w-5 h-5" style="color: var(--loss)" />
                <div>
                  <div class="text-sm font-medium" style="color: var(--text-primary)">
                    {{ trustLevel.promotion_eligible ? '满足晋级条件' : '不满足晋级条件' }}
                  </div>
                  <div v-if="!trustLevel.promotion_eligible && trustLevel.promotion_blockers" class="text-xs mt-0.5" style="color: var(--text-muted)">
                    {{ JSON.stringify(trustLevel.promotion_blockers) }}
                  </div>
                </div>
              </div>

              <!-- 评估时间 -->
              <div class="text-xs" style="color: var(--text-muted)">
                评估时间: {{ formatTime(trustLevel.assessed_at) }}
              </div>
            </div>

            <!-- 未评估占位 -->
            <div v-else class="py-16 text-center">
              <ShieldCheck class="w-12 h-12 mx-auto mb-3" style="color: var(--text-muted); opacity: 0.3" />
              <p class="text-sm" style="color: var(--text-muted)">尚未进行可信等级评估</p>
              <p class="text-xs mt-1" style="color: var(--text-muted)">点击"触发评估"按钮开始</p>
            </div>
          </div>
        </template>

        <!-- 无选中任务占位 -->
        <div v-else class="card">
          <div class="py-24 text-center">
            <FlaskConical class="w-12 h-12 mx-auto mb-3" style="color: var(--text-muted); opacity: 0.3" />
            <p class="text-sm" style="color: var(--text-muted)">选择左侧任务查看详情</p>
          </div>
        </div>
      </div>
    </div>
  </div>
</template>

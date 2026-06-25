<script setup lang="ts">
import { ref, onMounted, computed } from 'vue'
import { BacktestApi, CounterfactualApi, type CounterfactualExplanation } from '@/api'
import {
  GitCompare, RefreshCw, AlertTriangle, TrendingUp, TrendingDown,
  DollarSign, Clock, ArrowLeftRight, Scale, Sparkles, Search
} from 'lucide-vue-next'

// =========================================================
// 状态
// =========================================================

const jobs = ref<Array<{ job_id: string; job_name: string }>>([])
const selectedJobId = ref('')
const loading = ref(false)
const generating = ref(false)
const error = ref('')

const explanations = ref<CounterfactualExplanation[]>([])
const selectedExplanation = ref<CounterfactualExplanation | null>(null)

// 生成表单
const generateForm = ref({
  symbol: 'BTC-USDT-SWAP',
  direction: 'long',
  actual_pnl: 100,
  gross_pnl: 120,
  entry_time: '',
  exit_time: '',
  holding_period_sec: 3600,
  fee_cost: 5,
  slippage_cost: 3,
  funding_cost: 2,
  impact_cost: 10,
  benchmark_return: 0.05,
  market_regime: '',
  signal_confidence: 0.7,
  save: true,
})

// 查询模式
const queryMode = ref<'generate' | 'query'>('generate')
const queryAttributionId = ref('')

// =========================================================
// 计算属性
// =========================================================

const scenarioIcons: Record<string, unknown> = {
  no_trade: DollarSign,
  earlier_exit: Clock,
  later_exit: Clock,
  opposite_direction: ArrowLeftRight,
  reduced_size: Scale,
}

const scenarioLabels: Record<string, string> = {
  no_trade: '若不交易',
  earlier_exit: '若提前退出',
  later_exit: '若延后退出',
  opposite_direction: '若反向操作',
  reduced_size: '若减半仓位',
}

const scenarioColors: Record<string, string> = {
  no_trade: 'var(--text-muted)',
  earlier_exit: 'var(--warning)',
  later_exit: 'var(--primary)',
  opposite_direction: 'var(--loss)',
  reduced_size: 'var(--profit)',
}

const sortedExplanations = computed(() => {
  const order = ['no_trade', 'earlier_exit', 'later_exit', 'opposite_direction', 'reduced_size']
  return [...explanations.value].sort((a, b) => {
    return order.indexOf(a.scenario_type) - order.indexOf(b.scenario_type)
  })
})

const bestScenario = computed(() => {
  if (explanations.value.length === 0) return null
  return [...explanations.value].sort((a, b) => {
    const aDelta = a.pnl_delta ?? -Infinity
    const bDelta = b.pnl_delta ?? -Infinity
    return bDelta - aDelta
  })[0]
})

// =========================================================
// 方法
// =========================================================

async function loadJobs() {
  try {
    const jobList = await BacktestApi.listJobs()
    jobs.value = jobList.map(j => ({ job_id: j.job_id, job_name: j.job_name }))
  } catch (e: unknown) {
    error.value = e instanceof Error ? e.message : '加载回测任务失败'
  }
}

async function generateCounterfactuals() {
  generating.value = true
  error.value = ''
  try {
    const now = new Date()
    const entryTime = generateForm.value.entry_time || new Date(now.getTime() - 3600 * 1000).toISOString()
    const exitTime = generateForm.value.exit_time || now.toISOString()

    const data = await CounterfactualApi.generate({
      job_id: selectedJobId.value || undefined,
      symbol: generateForm.value.symbol,
      direction: generateForm.value.direction,
      actual_pnl: Number(generateForm.value.actual_pnl),
      gross_pnl: Number(generateForm.value.gross_pnl),
      entry_time: entryTime,
      exit_time: exitTime || undefined,
      holding_period_sec: Number(generateForm.value.holding_period_sec) || undefined,
      fee_cost: Number(generateForm.value.fee_cost),
      slippage_cost: Number(generateForm.value.slippage_cost),
      funding_cost: Number(generateForm.value.funding_cost),
      impact_cost: Number(generateForm.value.impact_cost),
      benchmark_return: generateForm.value.benchmark_return ? Number(generateForm.value.benchmark_return) : undefined,
      market_regime: generateForm.value.market_regime || undefined,
      signal_confidence: Number(generateForm.value.signal_confidence) || undefined,
      save: generateForm.value.save,
    })
    explanations.value = data.explanations || []
  } catch (e: unknown) {
    error.value = e instanceof Error ? e.message : '生成反事实解释失败'
  } finally {
    generating.value = false
  }
}

async function queryByAttribution() {
  if (!queryAttributionId.value) {
    error.value = '请输入归因 ID'
    return
  }
  loading.value = true
  error.value = ''
  try {
    const data = await CounterfactualApi.listByAttribution(queryAttributionId.value)
    explanations.value = data.explanations || []
  } catch (e: unknown) {
    error.value = e instanceof Error ? e.message : '查询反事实解释失败'
  } finally {
    loading.value = false
  }
}

async function queryByJob() {
  if (!selectedJobId.value) {
    error.value = '请选择回测任务'
    return
  }
  loading.value = true
  error.value = ''
  try {
    const data = await CounterfactualApi.listByJob(selectedJobId.value)
    explanations.value = data.explanations || []
  } catch (e: unknown) {
    error.value = e instanceof Error ? e.message : '查询反事实解释失败'
  } finally {
    loading.value = false
  }
}

function viewExplanation(cf: CounterfactualExplanation) {
  selectedExplanation.value = selectedExplanation.value?.explanation_id === cf.explanation_id ? null : cf
}

function formatNum(v: number | undefined | null, digits = 2): string {
  if (v == null || isNaN(v)) return '-'
  return v.toFixed(digits)
}

function formatPct(v: number | undefined | null, digits = 2): string {
  if (v == null || isNaN(v)) return '-'
  return (v * 100).toFixed(digits) + '%'
}

function pnlColor(v: number | undefined | null): string {
  if (v == null || isNaN(v)) return 'var(--text-muted)'
  return v >= 0 ? 'var(--profit)' : 'var(--loss)'
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
        <h1 class="text-2xl font-bold" style="color: var(--text-primary)">反事实解释</h1>
        <p class="text-sm mt-1" style="color: var(--text-secondary)">
          对每笔交易回答"若不做/早退/晚退/反向/减仓会怎样"
        </p>
      </div>
      <button @click="generateCounterfactuals" class="btn btn-secondary" :disabled="generating">
        <RefreshCw class="w-4 h-4" :class="generating ? 'animate-spin' : ''" />
        刷新
      </button>
    </div>

    <!-- 错误提示 -->
    <div v-if="error" class="card p-4 flex items-center gap-3" style="border-left: 3px solid var(--loss)">
      <AlertTriangle class="w-5 h-5" style="color: var(--loss)" />
      <span class="text-sm" style="color: var(--loss)">{{ error }}</span>
      <button @click="error = ''" class="ml-auto text-xs" style="color: var(--text-muted)">关闭</button>
    </div>

    <!-- 模式切换 -->
    <div class="card p-4">
      <div class="flex gap-2 mb-4">
        <button @click="queryMode = 'generate'"
                class="px-4 py-2 rounded text-sm font-medium transition"
                :style="{ background: queryMode === 'generate' ? 'var(--primary)' : 'var(--surface)', color: queryMode === 'generate' ? 'white' : 'var(--text-secondary)' }">
          <Sparkles class="w-4 h-4 inline mr-1" />
          生成反事实
        </button>
        <button @click="queryMode = 'query'"
                class="px-4 py-2 rounded text-sm font-medium transition"
                :style="{ background: queryMode === 'query' ? 'var(--primary)' : 'var(--surface)', color: queryMode === 'query' ? 'white' : 'var(--text-secondary)' }">
          <Search class="w-4 h-4 inline mr-1" />
          查询历史
        </button>
      </div>

      <!-- 生成模式 -->
      <div v-if="queryMode === 'generate'" class="space-y-4">
        <div class="grid grid-cols-1 md:grid-cols-4 gap-3">
          <div>
            <label class="block text-xs font-medium mb-1" style="color: var(--text-muted)">交易对</label>
            <input v-model="generateForm.symbol" class="input" placeholder="BTC-USDT-SWAP" />
          </div>
          <div>
            <label class="block text-xs font-medium mb-1" style="color: var(--text-muted)">方向</label>
            <select v-model="generateForm.direction" class="input">
              <option value="long">做多</option>
              <option value="short">做空</option>
            </select>
          </div>
          <div>
            <label class="block text-xs font-medium mb-1" style="color: var(--text-muted)">实际净盈亏</label>
            <input v-model.number="generateForm.actual_pnl" type="number" class="input" />
          </div>
          <div>
            <label class="block text-xs font-medium mb-1" style="color: var(--text-muted)">毛盈亏</label>
            <input v-model.number="generateForm.gross_pnl" type="number" class="input" />
          </div>
        </div>

        <div class="grid grid-cols-2 md:grid-cols-4 gap-3">
          <div>
            <label class="block text-xs font-medium mb-1" style="color: var(--text-muted)">手续费</label>
            <input v-model.number="generateForm.fee_cost" type="number" class="input" />
          </div>
          <div>
            <label class="block text-xs font-medium mb-1" style="color: var(--text-muted)">滑点成本</label>
            <input v-model.number="generateForm.slippage_cost" type="number" class="input" />
          </div>
          <div>
            <label class="block text-xs font-medium mb-1" style="color: var(--text-muted)">资金费率</label>
            <input v-model.number="generateForm.funding_cost" type="number" class="input" />
          </div>
          <div>
            <label class="block text-xs font-medium mb-1" style="color: var(--text-muted)">冲击成本</label>
            <input v-model.number="generateForm.impact_cost" type="number" class="input" />
          </div>
        </div>

        <div class="grid grid-cols-2 md:grid-cols-3 gap-3">
          <div>
            <label class="block text-xs font-medium mb-1" style="color: var(--text-muted)">持仓时间（秒）</label>
            <input v-model.number="generateForm.holding_period_sec" type="number" class="input" />
          </div>
          <div>
            <label class="block text-xs font-medium mb-1" style="color: var(--text-muted)">基准收益</label>
            <input v-model.number="generateForm.benchmark_return" type="number" step="0.01" class="input" />
          </div>
          <div>
            <label class="block text-xs font-medium mb-1" style="color: var(--text-muted)">信号置信度</label>
            <input v-model.number="generateForm.signal_confidence" type="number" step="0.01" min="0" max="1" class="input" />
          </div>
        </div>

        <div class="flex items-center gap-3">
          <label class="flex items-center gap-2 text-sm" style="color: var(--text-secondary)">
            <input v-model="generateForm.save" type="checkbox" />
            保存到数据库
          </label>
          <button @click="generateCounterfactuals" class="btn btn-primary" :disabled="generating">
            <GitCompare class="w-4 h-4" :class="generating ? 'animate-pulse' : ''" />
            {{ generating ? '生成中...' : '生成反事实解释' }}
          </button>
        </div>
      </div>

      <!-- 查询模式 -->
      <div v-else class="space-y-4">
        <div class="grid grid-cols-1 md:grid-cols-2 gap-4">
          <div>
            <label class="block text-xs font-medium mb-1" style="color: var(--text-muted)">按归因 ID 查询</label>
            <div class="flex gap-2">
              <input v-model="queryAttributionId" class="input" placeholder="UUID" />
              <button @click="queryByAttribution" class="btn btn-secondary text-xs">
                <Search class="w-4 h-4" />
                查询
              </button>
            </div>
          </div>
          <div>
            <label class="block text-xs font-medium mb-1" style="color: var(--text-muted)">按回测任务查询</label>
            <div class="flex gap-2">
              <select v-model="selectedJobId" class="input" style="min-width: 200px">
                <option value="">请选择任务</option>
                <option v-for="job in jobs" :key="job.job_id" :value="job.job_id">
                  {{ job.job_name }} ({{ job.job_id.substring(0, 8) }})
                </option>
              </select>
              <button @click="queryByJob" class="btn btn-secondary text-xs" :disabled="!selectedJobId">
                <Search class="w-4 h-4" />
                查询
              </button>
            </div>
          </div>
        </div>
      </div>
    </div>

    <!-- 最佳场景提示 -->
    <div v-if="bestScenario && explanations.length > 0" class="card p-4 flex items-center gap-3" style="border-left: 3px solid var(--profit)">
      <TrendingUp class="w-5 h-5" style="color: var(--profit)" />
      <div class="text-sm" style="color: var(--text-secondary)">
        最优反事实场景：<strong :style="{ color: scenarioColors[bestScenario.scenario_type] }">{{ scenarioLabels[bestScenario.scenario_type] }}</strong>
        ，预计盈亏 <strong :style="{ color: pnlColor(bestScenario.counterfactual_pnl) }">{{ formatNum(bestScenario.counterfactual_pnl) }}</strong>
        ，相比实际 <strong>{{ formatNum(bestScenario.pnl_delta) }}</strong>
      </div>
    </div>

    <!-- 反事实场景列表 -->
    <div v-if="explanations.length > 0" class="space-y-4">
      <!-- 场景对比卡片 -->
      <div class="grid grid-cols-1 md:grid-cols-5 gap-3">
        <div v-for="cf in sortedExplanations" :key="cf.explanation_id"
             class="card p-4 cursor-pointer transition hover:opacity-80"
             :style="{ borderLeft: `3px solid ${scenarioColors[cf.scenario_type]}` }"
             @click="viewExplanation(cf)">
          <div class="flex items-center gap-2 mb-2">
            <component :is="scenarioIcons[cf.scenario_type] || GitCompare" class="w-4 h-4" :style="{ color: scenarioColors[cf.scenario_type] }" />
            <span class="text-xs font-medium" :style="{ color: scenarioColors[cf.scenario_type] }">
              {{ scenarioLabels[cf.scenario_type] || cf.scenario_type }}
            </span>
          </div>
          <div class="text-lg font-bold" :style="{ color: pnlColor(cf.counterfactual_pnl) }">
            {{ formatNum(cf.counterfactual_pnl) }}
          </div>
          <div class="text-xs mt-1" style="color: var(--text-muted)">
            差值：<span :style="{ color: pnlColor(cf.pnl_delta) }">{{ formatNum(cf.pnl_delta) }}</span>
          </div>
          <div class="text-xs mt-1" style="color: var(--text-muted)">
            置信度：{{ formatPct(cf.confidence) }}
          </div>
        </div>
      </div>

      <!-- 详细解释 -->
      <div v-if="selectedExplanation" class="card p-5">
        <div class="flex items-center justify-between mb-3">
          <h3 class="text-sm font-semibold flex items-center gap-2" style="color: var(--text-primary)">
            <component :is="scenarioIcons[selectedExplanation.scenario_type] || GitCompare" class="w-4 h-4" :style="{ color: scenarioColors[selectedExplanation.scenario_type] }" />
            {{ scenarioLabels[selectedExplanation.scenario_type] }} - 详细解释
          </h3>
          <button @click="selectedExplanation = null" class="text-xs" style="color: var(--text-muted)">收起</button>
        </div>

        <!-- 自然语言解释 -->
        <div class="card p-4 mb-3" style="background: var(--surface)">
          <p class="text-sm" style="color: var(--text-secondary)">{{ selectedExplanation.explanation }}</p>
        </div>

        <!-- 关键驱动因素 -->
        <div v-if="Array.isArray(selectedExplanation.key_drivers) && selectedExplanation.key_drivers.length > 0" class="mb-3">
          <h4 class="text-xs font-semibold mb-2" style="color: var(--text-muted)">关键驱动因素</h4>
          <div class="flex flex-wrap gap-2">
            <span v-for="(driver, idx) in (selectedExplanation.key_drivers as Array<Record<string, unknown>>)" :key="idx"
                  class="text-xs px-2 py-1 rounded" style="background: var(--surface); color: var(--text-secondary)">
              {{ driver.driver || driver.name || JSON.stringify(driver) }}: {{ driver.value ?? '-' }}
            </span>
          </div>
        </div>

        <!-- 证据 -->
        <div v-if="selectedExplanation.evidence && typeof selectedExplanation.evidence === 'object'" class="mb-3">
          <h4 class="text-xs font-semibold mb-2" style="color: var(--text-muted)">证据</h4>
          <pre class="text-xs p-3 rounded overflow-x-auto" style="background: var(--surface); color: var(--text-secondary)">{{ JSON.stringify(selectedExplanation.evidence, null, 2) }}</pre>
        </div>

        <!-- 假设输入 -->
        <div v-if="selectedExplanation.what_if_inputs" class="mb-3">
          <h4 class="text-xs font-semibold mb-2" style="color: var(--text-muted)">假设输入参数</h4>
          <pre class="text-xs p-3 rounded overflow-x-auto" style="background: var(--surface); color: var(--text-secondary)">{{ JSON.stringify(selectedExplanation.what_if_inputs, null, 2) }}</pre>
        </div>

        <!-- 数值对比 -->
        <div class="grid grid-cols-2 md:grid-cols-4 gap-3">
          <div class="card p-3">
            <div class="text-xs" style="color: var(--text-muted)">反事实盈亏</div>
            <div class="text-lg font-bold" :style="{ color: pnlColor(selectedExplanation.counterfactual_pnl) }">
              {{ formatNum(selectedExplanation.counterfactual_pnl) }}
            </div>
          </div>
          <div class="card p-3">
            <div class="text-xs" style="color: var(--text-muted)">实际盈亏</div>
            <div class="text-lg font-bold" :style="{ color: pnlColor(selectedExplanation.actual_pnl) }">
              {{ formatNum(selectedExplanation.actual_pnl) }}
            </div>
          </div>
          <div class="card p-3">
            <div class="text-xs" style="color: var(--text-muted)">盈亏差值</div>
            <div class="text-lg font-bold" :style="{ color: pnlColor(selectedExplanation.pnl_delta) }">
              {{ formatNum(selectedExplanation.pnl_delta) }}
            </div>
          </div>
          <div class="card p-3">
            <div class="text-xs" style="color: var(--text-muted)">置信度</div>
            <div class="text-lg font-bold" style="color: var(--primary)">
              {{ formatPct(selectedExplanation.confidence) }}
            </div>
          </div>
        </div>
      </div>

      <!-- 完整对比表 -->
      <div class="card">
        <div class="p-5" style="border-bottom: 1px solid var(--border)">
          <h2 class="text-lg font-semibold flex items-center gap-2" style="color: var(--text-primary)">
            <GitCompare class="w-5 h-5" />
            场景对比表
          </h2>
        </div>
        <div class="overflow-x-auto">
          <table class="w-full text-sm">
            <thead>
              <tr style="border-bottom: 1px solid var(--border)">
                <th class="text-left py-3 px-4" style="color: var(--text-muted)">场景</th>
                <th class="text-left py-3 px-4" style="color: var(--text-muted)">描述</th>
                <th class="text-right py-3 px-4" style="color: var(--text-muted)">反事实盈亏</th>
                <th class="text-right py-3 px-4" style="color: var(--text-muted)">实际盈亏</th>
                <th class="text-right py-3 px-4" style="color: var(--text-muted)">差值</th>
                <th class="text-right py-3 px-4" style="color: var(--text-muted)">置信度</th>
              </tr>
            </thead>
            <tbody>
              <tr v-for="cf in sortedExplanations" :key="cf.explanation_id"
                  style="border-bottom: 1px solid var(--border)"
                  class="cursor-pointer hover:bg-[var(--surface)]"
                  @click="viewExplanation(cf)">
                <td class="py-2 px-4">
                  <span class="text-xs font-medium px-2 py-0.5 rounded"
                        :style="{ background: scenarioColors[cf.scenario_type] + '20', color: scenarioColors[cf.scenario_type] }">
                    {{ scenarioLabels[cf.scenario_type] || cf.scenario_type }}
                  </span>
                </td>
                <td class="py-2 px-4 text-xs" style="color: var(--text-secondary)">
                  {{ cf.scenario_description || '-' }}
                </td>
                <td class="text-right py-2 px-4 font-mono" :style="{ color: pnlColor(cf.counterfactual_pnl) }">
                  {{ formatNum(cf.counterfactual_pnl) }}
                </td>
                <td class="text-right py-2 px-4 font-mono" :style="{ color: pnlColor(cf.actual_pnl) }">
                  {{ formatNum(cf.actual_pnl) }}
                </td>
                <td class="text-right py-2 px-4 font-mono" :style="{ color: pnlColor(cf.pnl_delta) }">
                  {{ formatNum(cf.pnl_delta) }}
                </td>
                <td class="text-right py-2 px-4 font-mono" style="color: var(--primary)">
                  {{ formatPct(cf.confidence) }}
                </td>
              </tr>
            </tbody>
          </table>
        </div>
      </div>
    </div>

    <!-- 空状态 -->
    <div v-else-if="!loading && !generating && !error" class="card p-12 text-center">
      <GitCompare class="w-12 h-12 mx-auto mb-3 opacity-30" />
      <p class="text-sm" style="color: var(--text-muted)">
        {{ queryMode === 'generate' ? '填写交易参数，点击"生成反事实解释"开始' : '输入归因 ID 或选择回测任务查询' }}
      </p>
    </div>
  </div>
</template>

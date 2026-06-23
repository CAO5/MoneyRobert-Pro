<script setup lang="ts">
import { ref, computed, onMounted } from 'vue'
import { SignalApi, type DecisionCardResponse, type CreateDecisionCardRequest } from '@/api'
import {
  Layers, RefreshCw, TrendingUp, TrendingDown, Minus, AlertTriangle,
  CheckCircle, Clock, Activity, Gauge, ShieldAlert, Database, ChevronRight, Target
} from 'lucide-vue-next'

// =========================================================
// 状态
// =========================================================

const cards = ref<DecisionCardResponse[]>([])
const latestCard = ref<DecisionCardResponse | null>(null)
const loading = ref(false)
const generating = ref(false)
const error = ref('')

// 生成表单
const form = ref({
  symbol: 'BTC-USDT-SWAP',
  target_horizon_sec: 3600,
  p_up: 0.45,
  p_down: 0.35,
  p_flat: 0.20,
  q10: -0.02,
  q50: 0.005,
  q90: 0.03,
  expected_volatility: 0.015,
  model_version: 'v1.0.0',
  market_regime: 'trending',
  expected_value: 0.008,
  position_suggestion: 0.05,
  worst_case: -0.025,
  risk_budget_used: 0.3,
  data_freshness_sec: 120,
})

// 预测周期预设
const horizonPresets = [
  { label: '15分钟', value: 900 },
  { label: '1小时', value: 3600 },
  { label: '4小时', value: 14400 },
  { label: '1天', value: 86400 },
]

// 市场状态选项
const regimeOptions = ['trending', 'ranging', 'volatile', 'breakout', 'unknown']

// =========================================================
// 计算属性
// =========================================================

/// p_flat 自动计算（确保概率之和为 1）
const probSum = computed(() => form.value.p_up + form.value.p_down + form.value.p_flat)
const probValid = computed(() => Math.abs(probSum.value - 1.0) <= 0.05)

/// 建议动作的中文映射与颜色
function actionMeta(action: string): { label: string; color: string; icon: typeof TrendingUp } {
  switch (action) {
    case 'open_long':
      return { label: '做多', color: 'var(--profit)', icon: TrendingUp }
    case 'open_short':
      return { label: '做空', color: 'var(--loss)', icon: TrendingDown }
    case 'close':
      return { label: '平仓', color: 'var(--warning)', icon: Minus }
    case 'reduce':
      return { label: '减仓', color: 'var(--warning)', icon: Minus }
    default:
      return { label: '观望', color: 'var(--text-muted)', icon: Minus }
  }
}

/// 格式化百分比
function pct(v: number, digits = 2): string {
  return (v * 100).toFixed(digits) + '%'
}

/// 格式化时间
function formatTime(t: string): string {
  return new Date(t).toLocaleString('zh-CN', {
    month: '2-digit', day: '2-digit', hour: '2-digit', minute: '2-digit',
  })
}

/// 数据新鲜度等级
function freshnessLevel(sec?: number): { label: string; color: string } {
  if (sec == null) return { label: '未知', color: 'var(--text-muted)' }
  if (sec < 300) return { label: '新鲜', color: 'var(--profit)' }
  if (sec < 1800) return { label: '正常', color: 'var(--warning)' }
  return { label: '过期', color: 'var(--loss)' }
}

// =========================================================
// 方法
// =========================================================

async function generateCard() {
  if (!probValid.value) {
    error.value = `概率之和应为 1.0，当前为 ${probSum.value.toFixed(3)}`
    return
  }
  generating.value = true
  error.value = ''
  try {
    const req: CreateDecisionCardRequest = {
      symbol: form.value.symbol,
      target_horizon_sec: form.value.target_horizon_sec,
      p_up: form.value.p_up,
      p_down: form.value.p_down,
      p_flat: form.value.p_flat,
      q10: form.value.q10,
      q50: form.value.q50,
      q90: form.value.q90,
      expected_volatility: form.value.expected_volatility,
      model_version: form.value.model_version,
      market_regime: form.value.market_regime || undefined,
      expected_value: form.value.expected_value,
      position_suggestion: form.value.position_suggestion,
      worst_case: form.value.worst_case,
      risk_budget_used: form.value.risk_budget_used,
      data_freshness_sec: form.value.data_freshness_sec,
    }
    const card = await SignalApi.createDecisionCard(req)
    latestCard.value = card
    // 刷新历史列表
    await loadCards()
  } catch (e: unknown) {
    error.value = e instanceof Error ? e.message : '生成决策卡失败'
  } finally {
    generating.value = false
  }
}

async function loadCards() {
  loading.value = true
  try {
    cards.value = await SignalApi.listDecisionCards(50)
    if (!latestCard.value && cards.value.length) {
      latestCard.value = cards.value[0]
    }
  } catch (e: unknown) {
    error.value = e instanceof Error ? e.message : '加载决策卡列表失败'
  } finally {
    loading.value = false
  }
}

function selectCard(card: DecisionCardResponse) {
  latestCard.value = card
}

/// 自动平衡概率：调整 p_up 或 p_down 后自动修正 p_flat
function rebalanceProb(changed: 'up' | 'down') {
  if (changed === 'up') {
    form.value.p_flat = Math.max(0, 1 - form.value.p_up - form.value.p_down)
  } else {
    form.value.p_flat = Math.max(0, 1 - form.value.p_up - form.value.p_down)
  }
}

onMounted(() => {
  loadCards()
})
</script>

<template>
  <div class="space-y-6 animate-fade-in">
    <!-- Header -->
    <div class="flex items-center justify-between">
      <div>
        <h1 class="text-2xl font-bold" style="color: var(--text-primary)">概率决策卡</h1>
        <p class="text-sm mt-1" style="color: var(--text-secondary)">
          将交易决策转化为概率分布 + 净期望 + CVaR + 失效条件的可审计对象
        </p>
      </div>
      <button @click="loadCards" class="btn btn-secondary">
        <RefreshCw class="w-4 h-4" :class="loading ? 'animate-spin' : ''" />
        刷新
      </button>
    </div>

    <!-- 错误提示 -->
    <div v-if="error" class="card p-4 flex items-center gap-3" style="border-left: 3px solid var(--loss)">
      <AlertTriangle class="w-5 h-5" style="color: var(--loss)" />
      <span class="text-sm" style="color: var(--loss)">{{ error }}</span>
    </div>

    <div class="grid grid-cols-1 lg:grid-cols-2 gap-6">
      <!-- 左列：生成表单 -->
      <div class="card">
        <div class="p-5" style="border-bottom: 1px solid var(--border)">
          <h2 class="text-lg font-semibold flex items-center gap-2" style="color: var(--text-primary)">
            <Layers class="w-5 h-5" />
            生成决策卡
          </h2>
        </div>
        <div class="p-5 space-y-4">
          <!-- Symbol & Horizon -->
          <div class="grid grid-cols-2 gap-4">
            <div>
              <label class="block text-sm font-medium mb-1.5" style="color: var(--text-secondary)">交易对</label>
              <input v-model="form.symbol" class="input" placeholder="BTC-USDT-SWAP" />
            </div>
            <div>
              <label class="block text-sm font-medium mb-1.5" style="color: var(--text-secondary)">预测周期</label>
              <div class="flex gap-1.5 flex-wrap">
                <button
                  v-for="h in horizonPresets"
                  :key="h.value"
                  @click="form.target_horizon_sec = h.value"
                  class="btn btn-sm"
                  :class="form.target_horizon_sec === h.value ? 'btn-primary' : 'btn-secondary'"
                >
                  {{ h.label }}
                </button>
              </div>
            </div>
          </div>

          <!-- 概率分布 -->
          <div>
            <label class="block text-sm font-medium mb-2" style="color: var(--text-secondary)">
              概率分布
              <span class="ml-2 text-xs" :style="{ color: probValid ? 'var(--profit)' : 'var(--loss)' }">
                和 = {{ probSum.toFixed(3) }}
              </span>
            </label>
            <div class="grid grid-cols-3 gap-3">
              <div>
                <div class="text-xs mb-1 flex items-center gap-1" style="color: var(--profit)">
                  <TrendingUp class="w-3 h-3" /> P(上涨)
                </div>
                <input
                  v-model.number="form.p_up"
                  type="number" step="0.01" min="0" max="1"
                  class="input"
                  @input="rebalanceProb('up')"
                />
              </div>
              <div>
                <div class="text-xs mb-1 flex items-center gap-1" style="color: var(--loss)">
                  <TrendingDown class="w-3 h-3" /> P(下跌)
                </div>
                <input
                  v-model.number="form.p_down"
                  type="number" step="0.01" min="0" max="1"
                  class="input"
                  @input="rebalanceProb('down')"
                />
              </div>
              <div>
                <div class="text-xs mb-1 flex items-center gap-1" style="color: var(--text-muted)">
                  <Minus class="w-3 h-3" /> P(震荡)
                </div>
                <input
                  v-model.number="form.p_flat"
                  type="number" step="0.01" min="0" max="1"
                  class="input"
                  disabled
                />
              </div>
            </div>
            <!-- 概率可视化条 -->
            <div class="mt-2 h-3 rounded-full overflow-hidden flex" style="background: var(--surface-tertiary)">
              <div :style="{ width: (form.p_up * 100) + '%', background: 'var(--profit)' }"></div>
              <div :style="{ width: (form.p_flat * 100) + '%', background: 'var(--text-muted)' }"></div>
              <div :style="{ width: (form.p_down * 100) + '%', background: 'var(--loss)' }"></div>
            </div>
          </div>

          <!-- 收益分位数 -->
          <div>
            <label class="block text-sm font-medium mb-2" style="color: var(--text-secondary)">收益分位数</label>
            <div class="grid grid-cols-3 gap-3">
              <div>
                <div class="text-xs mb-1" style="color: var(--text-muted)">Q10（悲观）</div>
                <input v-model.number="form.q10" type="number" step="0.001" class="input" />
              </div>
              <div>
                <div class="text-xs mb-1" style="color: var(--text-muted)">Q50（中位）</div>
                <input v-model.number="form.q50" type="number" step="0.001" class="input" />
              </div>
              <div>
                <div class="text-xs mb-1" style="color: var(--text-muted)">Q90（乐观）</div>
                <input v-model.number="form.q90" type="number" step="0.001" class="input" />
              </div>
            </div>
          </div>

          <!-- EV & 风险 -->
          <div class="grid grid-cols-2 gap-4">
            <div>
              <label class="block text-sm font-medium mb-1.5" style="color: var(--text-secondary)">
                净期望 EV（扣除费用/滑点后）
              </label>
              <input v-model.number="form.expected_value" type="number" step="0.001" class="input" />
            </div>
            <div>
              <label class="block text-sm font-medium mb-1.5" style="color: var(--text-secondary)">
                仓位建议（0-1）
              </label>
              <input v-model.number="form.position_suggestion" type="number" step="0.01" min="0" max="1" class="input" />
            </div>
          </div>

          <div class="grid grid-cols-2 gap-4">
            <div>
              <label class="block text-sm font-medium mb-1.5" style="color: var(--text-secondary)">
                最坏情形 CVaR
              </label>
              <input v-model.number="form.worst_case" type="number" step="0.001" class="input" />
            </div>
            <div>
              <label class="block text-sm font-medium mb-1.5" style="color: var(--text-secondary)">
                已用风险预算
              </label>
              <input v-model.number="form.risk_budget_used" type="number" step="0.01" min="0" max="1" class="input" />
            </div>
          </div>

          <!-- 模型版本 & 市场状态 -->
          <div class="grid grid-cols-2 gap-4">
            <div>
              <label class="block text-sm font-medium mb-1.5" style="color: var(--text-secondary)">模型版本</label>
              <input v-model="form.model_version" class="input" placeholder="v1.0.0" />
            </div>
            <div>
              <label class="block text-sm font-medium mb-1.5" style="color: var(--text-secondary)">市场状态</label>
              <select v-model="form.market_regime" class="input">
                <option v-for="r in regimeOptions" :key="r" :value="r">{{ r }}</option>
              </select>
            </div>
          </div>

          <!-- 数据新鲜度 -->
          <div>
            <label class="block text-sm font-medium mb-1.5" style="color: var(--text-secondary)">
              数据新鲜度（秒）
            </label>
            <input v-model.number="form.data_freshness_sec" type="number" step="10" min="0" class="input" />
          </div>

          <!-- 生成按钮 -->
          <button
            @click="generateCard"
            :disabled="generating || !probValid"
            class="btn btn-primary w-full"
          >
            <Activity v-if="!generating" class="w-4 h-4" />
            <RefreshCw v-else class="w-4 h-4 animate-spin" />
            {{ generating ? '生成中...' : '生成决策卡' }}
          </button>
        </div>
      </div>

      <!-- 右列：最新决策卡展示 -->
      <div class="space-y-6">
        <div v-if="latestCard" class="card">
          <div class="p-5" style="border-bottom: 1px solid var(--border)">
            <h2 class="text-lg font-semibold flex items-center gap-2" style="color: var(--text-primary)">
              <CheckCircle class="w-5 h-5" style="color: var(--profit)" />
              最新决策卡
            </h2>
          </div>
          <div class="p-5 space-y-5">
            <!-- 概要信息 -->
            <div class="flex items-center justify-between">
              <div>
                <div class="text-xs" style="color: var(--text-muted)">{{ latestCard.symbol }}</div>
                <div class="text-sm" style="color: var(--text-secondary)">{{ formatTime(latestCard.generated_at) }}</div>
              </div>
              <div
                class="px-4 py-2 rounded-lg flex items-center gap-2 font-semibold"
                :style="{
                  background: actionMeta(latestCard.suggested_action).color + '20',
                  color: actionMeta(latestCard.suggested_action).color,
                }"
              >
                <component :is="actionMeta(latestCard.suggested_action).icon" class="w-5 h-5" />
                {{ actionMeta(latestCard.suggested_action).label }}
              </div>
            </div>

            <!-- 概率分布可视化 -->
            <div>
              <div class="text-xs font-medium mb-2" style="color: var(--text-secondary)">概率分布</div>
              <div class="h-6 rounded-lg overflow-hidden flex" style="background: var(--surface-tertiary)">
                <div
                  class="flex items-center justify-center text-xs font-semibold text-white"
                  :style="{ width: (latestCard.p_up * 100) + '%', background: 'var(--profit)' }"
                >
                  {{ latestCard.p_up > 0.1 ? pct(latestCard.p_up, 1) : '' }}
                </div>
                <div
                  class="flex items-center justify-center text-xs font-semibold text-white"
                  :style="{ width: (latestCard.p_flat * 100) + '%', background: 'var(--text-muted)' }"
                >
                  {{ latestCard.p_flat > 0.1 ? pct(latestCard.p_flat, 1) : '' }}
                </div>
                <div
                  class="flex items-center justify-center text-xs font-semibold text-white"
                  :style="{ width: (latestCard.p_down * 100) + '%', background: 'var(--loss)' }"
                >
                  {{ latestCard.p_down > 0.1 ? pct(latestCard.p_down, 1) : '' }}
                </div>
              </div>
            </div>

            <!-- 关键指标网格 -->
            <div class="grid grid-cols-2 gap-3">
              <div class="p-3 rounded-lg" style="background: var(--surface-secondary)">
                <div class="flex items-center gap-1.5 text-xs mb-1" style="color: var(--text-muted)">
                  <Gauge class="w-3.5 h-3.5" /> 净期望 EV
                </div>
                <div class="text-lg font-mono font-semibold" :style="{ color: latestCard.expected_value >= 0 ? 'var(--profit)' : 'var(--loss)' }">
                  {{ pct(latestCard.expected_value, 3) }}
                </div>
              </div>
              <div class="p-3 rounded-lg" style="background: var(--surface-secondary)">
                <div class="flex items-center gap-1.5 text-xs mb-1" style="color: var(--text-muted)">
                  <ShieldAlert class="w-3.5 h-3.5" /> 最坏情形 CVaR
                </div>
                <div class="text-lg font-mono font-semibold" style="color: var(--loss)">
                  {{ latestCard.worst_case != null ? pct(latestCard.worst_case, 3) : '—' }}
                </div>
              </div>
              <div class="p-3 rounded-lg" style="background: var(--surface-secondary)">
                <div class="flex items-center gap-1.5 text-xs mb-1" style="color: var(--text-muted)">
                  <Target class="w-3.5 h-3.5" /> 仓位建议
                </div>
                <div class="text-lg font-mono font-semibold" style="color: var(--text-primary)">
                  {{ pct(latestCard.position_suggestion, 1) }}
                </div>
              </div>
              <div class="p-3 rounded-lg" style="background: var(--surface-secondary)">
                <div class="flex items-center gap-1.5 text-xs mb-1" style="color: var(--text-muted)">
                  <Clock class="w-3.5 h-3.5" /> 数据新鲜度
                </div>
                <div class="text-lg font-mono font-semibold" :style="{ color: freshnessLevel(latestCard.data_freshness_sec).color }">
                  {{ latestCard.data_freshness_sec != null ? Math.round(latestCard.data_freshness_sec) + 's' : '—' }}
                </div>
                <div class="text-xs" :style="{ color: freshnessLevel(latestCard.data_freshness_sec).color }">
                  {{ freshnessLevel(latestCard.data_freshness_sec).label }}
                </div>
              </div>
            </div>

            <!-- 收益区间 -->
            <div v-if="latestCard.q10 != null || latestCard.q90 != null">
              <div class="text-xs font-medium mb-2" style="color: var(--text-secondary)">收益区间（分位数）</div>
              <div class="flex items-center justify-between text-sm font-mono">
                <span style="color: var(--loss)">Q10: {{ latestCard.q10 != null ? pct(latestCard.q10, 2) : '—' }}</span>
                <span style="color: var(--text-muted)">Q50: {{ latestCard.q50 != null ? pct(latestCard.q50, 2) : '—' }}</span>
                <span style="color: var(--profit)">Q90: {{ latestCard.q90 != null ? pct(latestCard.q90, 2) : '—' }}</span>
              </div>
            </div>

            <!-- 适用状态 & 风险预算 -->
            <div class="flex items-center gap-4 text-sm">
              <div v-if="latestCard.applicable_regime" class="flex items-center gap-1.5">
                <span style="color: var(--text-muted)">适用状态:</span>
                <span class="font-medium" style="color: var(--text-primary)">{{ latestCard.applicable_regime }}</span>
              </div>
              <div v-if="latestCard.risk_budget_used != null" class="flex items-center gap-1.5">
                <span style="color: var(--text-muted)">风险预算:</span>
                <span class="font-mono font-medium" style="color: var(--text-primary)">{{ pct(latestCard.risk_budget_used, 1) }}</span>
              </div>
            </div>

            <!-- 失效条件 -->
            <div v-if="latestCard.invalidation_conditions">
              <div class="text-xs font-medium mb-2 flex items-center gap-1.5" style="color: var(--warning)">
                <AlertTriangle class="w-3.5 h-3.5" /> 失效条件
              </div>
              <pre class="text-xs p-3 rounded-lg overflow-x-auto" style="background: var(--surface-secondary); color: var(--text-secondary)">{{ JSON.stringify(latestCard.invalidation_conditions, null, 2) }}</pre>
            </div>

            <!-- 模型版本 & 数据血缘 -->
            <div class="flex items-center gap-2 text-xs" style="color: var(--text-muted)">
              <Database class="w-3.5 h-3.5" />
              模型版本: <span class="font-mono" style="color: var(--text-secondary)">{{ latestCard.model_version }}</span>
            </div>
          </div>
        </div>

        <!-- 无决策卡占位 -->
        <div v-else class="card">
          <div class="py-20 text-center">
            <Layers class="w-12 h-12 mx-auto mb-3" style="color: var(--text-muted); opacity: 0.3" />
            <p class="text-sm" style="color: var(--text-muted)">尚未生成决策卡</p>
            <p class="text-xs mt-1" style="color: var(--text-muted)">填写左侧表单后点击"生成决策卡"</p>
          </div>
        </div>
      </div>
    </div>

    <!-- 历史决策卡列表 -->
    <div class="card">
      <div class="p-5" style="border-bottom: 1px solid var(--border)">
        <h2 class="text-lg font-semibold" style="color: var(--text-primary)">历史决策卡</h2>
      </div>
      <div v-if="loading" class="flex items-center justify-center py-16">
        <div class="spinner"></div>
      </div>
      <div v-else-if="cards.length" class="table-container border-0 rounded-none">
        <table class="table">
          <thead>
            <tr>
              <th>时间</th>
              <th>交易对</th>
              <th>建议动作</th>
              <th class="text-right">P(涨)</th>
              <th class="text-right">P(跌)</th>
              <th class="text-right">净期望</th>
              <th class="text-right">CVaR</th>
              <th class="text-right">仓位</th>
              <th class="text-right">操作</th>
            </tr>
          </thead>
          <tbody>
            <tr
              v-for="c in cards"
              :key="c.card_id"
              :class="latestCard?.card_id === c.card_id ? 'opacity-100' : ''"
              :style="latestCard?.card_id === c.card_id ? { background: 'var(--surface-secondary)' } : {}"
            >
              <td class="text-sm" style="color: var(--text-secondary)">{{ formatTime(c.generated_at) }}</td>
              <td class="font-semibold">{{ c.symbol }}</td>
              <td>
                <span
                  class="inline-flex items-center gap-1 px-2 py-0.5 rounded text-xs font-medium"
                  :style="{
                    background: actionMeta(c.suggested_action).color + '20',
                    color: actionMeta(c.suggested_action).color,
                  }"
                >
                  <component :is="actionMeta(c.suggested_action).icon" class="w-3 h-3" />
                  {{ actionMeta(c.suggested_action).label }}
                </span>
              </td>
              <td class="text-right font-mono" style="color: var(--profit)">{{ pct(c.p_up, 1) }}</td>
              <td class="text-right font-mono" style="color: var(--loss)">{{ pct(c.p_down, 1) }}</td>
              <td class="text-right font-mono font-semibold" :style="{ color: c.expected_value >= 0 ? 'var(--profit)' : 'var(--loss)' }">
                {{ pct(c.expected_value, 3) }}
              </td>
              <td class="text-right font-mono" style="color: var(--loss)">
                {{ c.worst_case != null ? pct(c.worst_case, 2) : '—' }}
              </td>
              <td class="text-right font-mono" style="color: var(--text-primary)">{{ pct(c.position_suggestion, 1) }}</td>
              <td class="text-right">
                <button @click="selectCard(c)" class="btn btn-sm btn-secondary">
                  查看
                  <ChevronRight class="w-3 h-3" />
                </button>
              </td>
            </tr>
          </tbody>
        </table>
      </div>
      <div v-else class="py-16 text-center">
        <p class="text-sm" style="color: var(--text-muted)">暂无历史决策卡</p>
      </div>
    </div>
  </div>
</template>

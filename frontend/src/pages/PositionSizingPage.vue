<script setup lang="ts">
import { ref, computed } from 'vue'
import { BacktestApi } from '@/api'
import {
  Target, Zap, AlertTriangle, CheckCircle, TrendingUp,
  DollarSign, Percent, Activity, Gauge
} from 'lucide-vue-next'

// =========================================================
// 状态
// =========================================================

const form = ref({
  entry_price: 65000,
  win_probability: 0.55,
  avg_win: 0.03,
  avg_loss: 0.02,
  asset_volatility: 0.60,
  stop_loss_pct: 0.02,
  kelly_fraction: 0.25,
  volatility_target: 0.15,
  max_risk_per_trade: 0.005,
  max_position_pct: 0.10,
  max_leverage: 3.0,
  min_position_pct: 0.01,
})

const result = ref<Record<string, unknown> | null>(null)
const loading = ref(false)
const error = ref('')

// =========================================================
// 计算属性
// =========================================================

/// 盈亏比
const payoffRatio = computed(() => {
  if (form.value.avg_loss === 0) return 0
  return form.value.avg_win / form.value.avg_loss
})

/// 期望值
const expectedValue = computed(() => {
  return form.value.win_probability * form.value.avg_win - (1 - form.value.win_probability) * form.value.avg_loss
})

/// Kelly 原始公式预览
const kellyPreview = computed(() => {
  const p = form.value.win_probability
  const b = payoffRatio.value
  if (b === 0) return 0
  return (p * b - (1 - p)) / b
})

// =========================================================
// 方法
// =========================================================

async function calculate() {
  loading.value = true
  error.value = ''
  try {
    result.value = await BacktestApi.calculatePosition({
      entry_price: form.value.entry_price,
      win_probability: form.value.win_probability,
      avg_win: form.value.avg_win,
      avg_loss: form.value.avg_loss,
      asset_volatility: form.value.asset_volatility,
      stop_loss_pct: form.value.stop_loss_pct || undefined,
      kelly_fraction: form.value.kelly_fraction,
      volatility_target: form.value.volatility_target,
      max_risk_per_trade: form.value.max_risk_per_trade,
      max_position_pct: form.value.max_position_pct,
      max_leverage: form.value.max_leverage,
      min_position_pct: form.value.min_position_pct,
    })
  } catch (e: unknown) {
    error.value = e instanceof Error ? e.message : '计算失败'
  } finally {
    loading.value = false
  }
}

function formatPct(v: number, digits = 2): string {
  return (v * 100).toFixed(digits) + '%'
}

function formatNum(v: number, digits = 4): string {
  return v.toFixed(digits)
}

const methodLabel: Record<string, string> = {
  kelly: 'Fractional Kelly',
  volatility_target: '波动率目标',
  risk_budget: '单笔风险预算',
  conservative_min: '保守策略（三者取最小）',
}
</script>

<template>
  <div class="space-y-6 animate-fade-in">
    <!-- Header -->
    <div class="flex items-center justify-between">
      <div>
        <h1 class="text-2xl font-bold" style="color: var(--text-primary)">仓位计算器</h1>
        <p class="text-sm mt-1" style="color: var(--text-secondary)">
          Fractional Kelly + 波动率目标 + 单笔风险预算，三者取最小（保守策略）
        </p>
      </div>
      <button @click="calculate" class="btn btn-primary" :disabled="loading">
        <Zap class="w-4 h-4" :class="loading ? 'animate-pulse' : ''" />
        {{ loading ? '计算中...' : '计算仓位' }}
      </button>
    </div>

    <!-- 错误提示 -->
    <div v-if="error" class="card p-4 flex items-center gap-3" style="border-left: 3px solid var(--loss)">
      <AlertTriangle class="w-5 h-5" style="color: var(--loss)" />
      <span class="text-sm" style="color: var(--loss)">{{ error }}</span>
    </div>

    <div class="grid grid-cols-1 lg:grid-cols-2 gap-6">
      <!-- 左列：输入参数 -->
      <div class="card">
        <div class="p-5" style="border-bottom: 1px solid var(--border)">
          <h2 class="text-lg font-semibold flex items-center gap-2" style="color: var(--text-primary)">
            <Target class="w-5 h-5" />
            交易参数
          </h2>
        </div>
        <div class="p-5 space-y-4">
          <div class="grid grid-cols-2 gap-4">
            <div>
              <label class="block text-xs font-medium mb-1" style="color: var(--text-muted)">入场价格</label>
              <input v-model.number="form.entry_price" type="number" step="100" class="input" />
            </div>
            <div>
              <label class="block text-xs font-medium mb-1" style="color: var(--text-muted)">胜率</label>
              <input v-model.number="form.win_probability" type="number" step="0.01" min="0" max="1" class="input" />
            </div>
          </div>
          <div class="grid grid-cols-2 gap-4">
            <div>
              <label class="block text-xs font-medium mb-1" style="color: var(--text-muted)">平均盈利</label>
              <input v-model.number="form.avg_win" type="number" step="0.005" class="input" />
            </div>
            <div>
              <label class="block text-xs font-medium mb-1" style="color: var(--text-muted)">平均亏损</label>
              <input v-model.number="form.avg_loss" type="number" step="0.005" class="input" />
            </div>
          </div>
          <div class="grid grid-cols-2 gap-4">
            <div>
              <label class="block text-xs font-medium mb-1" style="color: var(--text-muted)">资产波动率(年化)</label>
              <input v-model.number="form.asset_volatility" type="number" step="0.05" class="input" />
            </div>
            <div>
              <label class="block text-xs font-medium mb-1" style="color: var(--text-muted)">止损比例</label>
              <input v-model.number="form.stop_loss_pct" type="number" step="0.005" class="input" />
            </div>
          </div>
        </div>
      </div>

      <!-- 右列：风险配置 -->
      <div class="card">
        <div class="p-5" style="border-bottom: 1px solid var(--border)">
          <h2 class="text-lg font-semibold flex items-center gap-2" style="color: var(--text-primary)">
            <Gauge class="w-5 h-5" />
            风险配置
          </h2>
        </div>
        <div class="p-5 space-y-4">
          <div>
            <label class="block text-xs font-medium mb-1" style="color: var(--text-muted)">Kelly 分数 (0-1)</label>
            <input v-model.number="form.kelly_fraction" type="number" step="0.05" min="0" max="1" class="input" />
            <div class="text-xs mt-1" style="color: var(--text-muted)">0.25 = 1/4 Kelly（推荐），1.0 = 全 Kelly</div>
          </div>
          <div class="grid grid-cols-2 gap-4">
            <div>
              <label class="block text-xs font-medium mb-1" style="color: var(--text-muted)">波动率目标</label>
              <input v-model.number="form.volatility_target" type="number" step="0.01" class="input" />
            </div>
            <div>
              <label class="block text-xs font-medium mb-1" style="color: var(--text-muted)">单笔最大风险</label>
              <input v-model.number="form.max_risk_per_trade" type="number" step="0.001" class="input" />
            </div>
          </div>
          <div class="grid grid-cols-3 gap-3">
            <div>
              <label class="block text-xs font-medium mb-1" style="color: var(--text-muted)">最大仓位</label>
              <input v-model.number="form.max_position_pct" type="number" step="0.01" class="input" />
            </div>
            <div>
              <label class="block text-xs font-medium mb-1" style="color: var(--text-muted)">最大杠杆</label>
              <input v-model.number="form.max_leverage" type="number" step="0.5" class="input" />
            </div>
            <div>
              <label class="block text-xs font-medium mb-1" style="color: var(--text-muted)">最小仓位</label>
              <input v-model.number="form.min_position_pct" type="number" step="0.005" class="input" />
            </div>
          </div>
        </div>
      </div>
    </div>

    <!-- 预览指标 -->
    <div class="grid grid-cols-3 gap-4">
      <div class="card p-4">
        <div class="text-xs" style="color: var(--text-muted)">盈亏比</div>
        <div class="text-xl font-bold" :style="{
          color: payoffRatio >= 1 ? 'var(--profit)' : 'var(--warning)'
        }">
          {{ formatNum(payoffRatio) }}
        </div>
      </div>
      <div class="card p-4">
        <div class="text-xs" style="color: var(--text-muted)">期望值</div>
        <div class="text-xl font-bold" :style="{
          color: expectedValue >= 0 ? 'var(--profit)' : 'var(--loss)'
        }">
          {{ formatPct(expectedValue) }}
        </div>
      </div>
      <div class="card p-4">
        <div class="text-xs" style="color: var(--text-muted)">全 Kelly 预览</div>
        <div class="text-xl font-bold" :style="{
          color: kellyPreview > 0 ? 'var(--primary)' : 'var(--text-muted)'
        }">
          {{ formatPct(kellyPreview) }}
        </div>
      </div>
    </div>

    <!-- 计算结果 -->
    <div v-if="result">
      <div class="card p-6" :style="{
        borderLeft: `3px solid var(--primary)`
      }">
        <div class="flex items-center gap-3 mb-4">
          <CheckCircle class="w-8 h-8" style="color: var(--primary)" />
          <div>
            <div class="text-lg font-bold" style="color: var(--text-primary)">建议仓位</div>
            <div class="text-sm" style="color: var(--text-secondary)">
              方法：{{ methodLabel[(result.result as Record<string, string>).method] || (result.result as Record<string, string>).method }}
            </div>
          </div>
        </div>

        <!-- 主要结果 -->
        <div class="grid grid-cols-2 md:grid-cols-4 gap-4 mb-4">
          <div class="p-4 rounded" style="background: var(--surface)">
            <div class="text-xs" style="color: var(--text-muted)">仓位占比</div>
            <div class="text-2xl font-bold" style="color: var(--primary)">
              {{ formatPct((result.result as Record<string, number>).position_pct) }}
            </div>
          </div>
          <div class="p-4 rounded" style="background: var(--surface)">
            <div class="text-xs" style="color: var(--text-muted)">杠杆</div>
            <div class="text-2xl font-bold" style="color: var(--text-primary)">
              {{ formatNum((result.result as Record<string, number>).leverage, 2) }}x
            </div>
          </div>
          <div class="p-4 rounded" style="background: var(--surface)">
            <div class="text-xs" style="color: var(--text-muted)">止损价</div>
            <div class="text-2xl font-bold" style="color: var(--loss)">
              {{ (result.result as Record<string, number>).stop_loss_price ? (result.result as Record<string, number>).stop_loss_price.toFixed(2) : '-' }}
            </div>
          </div>
          <div class="p-4 rounded" style="background: var(--surface)">
            <div class="text-xs" style="color: var(--text-muted)">Kelly 原始</div>
            <div class="text-2xl font-bold" style="color: var(--text-secondary)">
              {{ formatPct((result.result as Record<string, number>).kelly_raw) }}
            </div>
          </div>
        </div>

        <!-- 三种方法对比 -->
        <div class="grid grid-cols-3 gap-3">
          <div class="p-3 rounded text-center" style="background: var(--surface)">
            <Activity class="w-4 h-4 mx-auto mb-1" style="color: var(--primary)" />
            <div class="text-xs" style="color: var(--text-muted)">Kelly 目标</div>
            <div class="font-bold" style="color: var(--text-primary)">
              {{ formatPct((result.result as Record<string, number>).vol_target_pct) }}
            </div>
          </div>
          <div class="p-3 rounded text-center" style="background: var(--surface)">
            <TrendingUp class="w-4 h-4 mx-auto mb-1" style="color: var(--primary)" />
            <div class="text-xs" style="color: var(--text-muted)">波动率目标</div>
            <div class="font-bold" style="color: var(--text-primary)">
              {{ formatPct((result.result as Record<string, number>).vol_target_pct) }}
            </div>
          </div>
          <div class="p-3 rounded text-center" style="background: var(--surface)">
            <DollarSign class="w-4 h-4 mx-auto mb-1" style="color: var(--primary)" />
            <div class="text-xs" style="color: var(--text-muted)">风险预算</div>
            <div class="font-bold" style="color: var(--text-primary)">
              {{ formatPct((result.result as Record<string, number>).risk_based_pct) }}
            </div>
          </div>
        </div>

        <!-- 调整原因 -->
        <div class="mt-4 p-3 rounded text-sm" style="background: var(--surface); color: var(--text-secondary)">
          <strong style="color: var(--text-primary)">调整原因：</strong>
          {{ (result.result as Record<string, string>).reason }}
        </div>
      </div>
    </div>
  </div>
</template>

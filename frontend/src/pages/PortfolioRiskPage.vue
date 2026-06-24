<script setup lang="ts">
import { ref } from 'vue'
import { BacktestApi } from '@/api'
import {
  Shield, Zap, AlertTriangle, CheckCircle, TrendingDown,
  DollarSign, Percent, Activity
} from 'lucide-vue-next'

// =========================================================
// 状态
// =========================================================

const assets = ref<Array<{ symbol: string; position_pct: number; volatility: number; avg_daily_volume: number }>>([
  { symbol: 'BTC-USDT-SWAP', position_pct: 0.30, volatility: 0.60, avg_daily_volume: 5000000000 },
  { symbol: 'ETH-USDT-SWAP', position_pct: 0.20, volatility: 0.70, avg_daily_volume: 3000000000 },
  { symbol: 'SOL-USDT-SWAP', position_pct: 0.10, volatility: 0.90, avg_daily_volume: 1000000000 },
])

const correlations = ref<Array<[string, string, number]>>([
  ['BTC-USDT-SWAP', 'ETH-USDT-SWAP', 0.85],
  ['BTC-USDT-SWAP', 'SOL-USDT-SWAP', 0.65],
  ['ETH-USDT-SWAP', 'SOL-USDT-SWAP', 0.70],
])

const config = ref({
  max_portfolio_cvar: 0.05,
  max_risk_concentration: 0.30,
  max_volume_participation: 0.01,
  high_correlation_threshold: 0.70,
  max_correlated_exposure: 0.20,
})

const result = ref<Record<string, unknown> | null>(null)
const loading = ref(false)
const error = ref('')

// =========================================================
// 方法
// =========================================================

function addAsset() {
  assets.value.push({ symbol: '', position_pct: 0.05, volatility: 0.80, avg_daily_volume: 500000000 })
}

function removeAsset(idx: number) {
  assets.value.splice(idx, 1)
}

async function check() {
  loading.value = true
  error.value = ''
  try {
    result.value = await BacktestApi.checkPortfolioRisk({
      assets: assets.value,
      correlations: correlations.value,
      ...config.value,
    })
  } catch (e: unknown) {
    error.value = e instanceof Error ? e.message : '检查失败'
  } finally {
    loading.value = false
  }
}

function formatPct(v: number, digits = 2): string {
  return (v * 100).toFixed(digits) + '%'
}

function formatNum(v: number, digits = 2): string {
  if (Math.abs(v) >= 1e9) return (v / 1e9).toFixed(digits) + 'B'
  if (Math.abs(v) >= 1e6) return (v / 1e6).toFixed(digits) + 'M'
  if (Math.abs(v) >= 1e3) return (v / 1e3).toFixed(digits) + 'K'
  return v.toFixed(digits)
}
</script>

<template>
  <div class="space-y-6 animate-fade-in">
    <!-- Header -->
    <div class="flex items-center justify-between">
      <div>
        <h1 class="text-2xl font-bold" style="color: var(--text-primary)">组合风险管理</h1>
        <p class="text-sm mt-1" style="color: var(--text-secondary)">
          CVaR、风险贡献集中度、流动性约束、相关性限制的综合检查
        </p>
      </div>
      <button @click="check" class="btn btn-primary" :disabled="loading">
        <Zap class="w-4 h-4" :class="loading ? 'animate-pulse' : ''" />
        {{ loading ? '检查中...' : '运行检查' }}
      </button>
    </div>

    <!-- 错误提示 -->
    <div v-if="error" class="card p-4 flex items-center gap-3" style="border-left: 3px solid var(--loss)">
      <AlertTriangle class="w-5 h-5" style="color: var(--loss)" />
      <span class="text-sm" style="color: var(--loss)">{{ error }}</span>
    </div>

    <div class="grid grid-cols-1 lg:grid-cols-2 gap-6">
      <!-- 左列：资产配置 -->
      <div class="card">
        <div class="p-5" style="border-bottom: 1px solid var(--border)">
          <h2 class="text-lg font-semibold flex items-center gap-2" style="color: var(--text-primary)">
            <Activity class="w-5 h-5" />
            资产配置
          </h2>
        </div>
        <div class="p-5 space-y-3">
          <div
            v-for="(asset, idx) in assets"
            :key="idx"
            class="grid grid-cols-5 gap-2 items-center"
          >
            <input
              v-model="asset.symbol"
              class="input"
              placeholder="BTC-USDT-SWAP"
              style="font-size: 12px"
            />
            <div>
              <input
                v-model.number="asset.position_pct"
                type="number"
                step="0.01"
                class="input"
                style="font-size: 12px"
              />
              <div class="text-[10px]" style="color: var(--text-muted)">仓位%</div>
            </div>
            <div>
              <input
                v-model.number="asset.volatility"
                type="number"
                step="0.05"
                class="input"
                style="font-size: 12px"
              />
              <div class="text-[10px]" style="color: var(--text-muted)">波动率</div>
            </div>
            <div>
              <input
                v-model.number="asset.avg_daily_volume"
                type="number"
                class="input"
                style="font-size: 12px"
              />
              <div class="text-[10px]" style="color: var(--text-muted)">日均量</div>
            </div>
            <button @click="removeAsset(idx)" class="btn btn-sm btn-secondary" style="padding: 4px 8px">
              <TrendingDown class="w-3 h-3" />
            </button>
          </div>
          <button @click="addAsset" class="btn btn-sm btn-secondary w-full">
            + 添加资产
          </button>

          <!-- 总仓位 -->
          <div class="flex justify-between text-sm pt-2" style="border-top: 1px solid var(--border)">
            <span style="color: var(--text-muted)">总仓位</span>
            <span class="font-bold" :style="{
              color: assets.reduce((s, a) => s + a.position_pct, 0) > 1 ? 'var(--loss)' : 'var(--text-primary)'
            }">
              {{ formatPct(assets.reduce((s, a) => s + a.position_pct, 0)) }}
            </span>
          </div>
        </div>
      </div>

      <!-- 右列：风险参数 -->
      <div class="card">
        <div class="p-5" style="border-bottom: 1px solid var(--border)">
          <h2 class="text-lg font-semibold flex items-center gap-2" style="color: var(--text-primary)">
            <Shield class="w-5 h-5" />
            风险参数
          </h2>
        </div>
        <div class="p-5 space-y-4">
          <div>
            <label class="block text-xs font-medium mb-1" style="color: var(--text-muted)">最大 CVaR 预算</label>
            <input v-model.number="config.max_portfolio_cvar" type="number" step="0.01" class="input" />
            <div class="text-xs mt-1" style="color: var(--text-muted)">组合 CVaR 不超过 NAV 的此比例</div>
          </div>
          <div>
            <label class="block text-xs font-medium mb-1" style="color: var(--text-muted)">单资产风险贡献上限</label>
            <input v-model.number="config.max_risk_concentration" type="number" step="0.05" class="input" />
          </div>
          <div>
            <label class="block text-xs font-medium mb-1" style="color: var(--text-muted)">流动性约束</label>
            <input v-model.number="config.max_volume_participation" type="number" step="0.005" class="input" />
            <div class="text-xs mt-1" style="color: var(--text-muted)">最多吃掉成交量的此比例</div>
          </div>
          <div class="grid grid-cols-2 gap-3">
            <div>
              <label class="block text-xs font-medium mb-1" style="color: var(--text-muted)">高相关阈值</label>
              <input v-model.number="config.high_correlation_threshold" type="number" step="0.05" class="input" />
            </div>
            <div>
              <label class="block text-xs font-medium mb-1" style="color: var(--text-muted)">高相关最大敞口</label>
              <input v-model.number="config.max_correlated_exposure" type="number" step="0.05" class="input" />
            </div>
          </div>
        </div>
      </div>
    </div>

    <!-- 检查结果 -->
    <div v-if="result">
      <!-- 状态指示 -->
      <div class="card p-5 flex items-center gap-4" :style="{
        borderLeft: `3px solid ${result.passed ? 'var(--profit)' : 'var(--loss)'}`
      }">
        <component
          :is="result.passed ? CheckCircle : AlertTriangle"
          class="w-8 h-8"
          :style="{ color: result.passed ? 'var(--profit)' : 'var(--loss)' }"
        />
        <div>
          <div class="text-lg font-bold" :style="{
            color: result.passed ? 'var(--profit)' : 'var(--loss)'
          }">
            {{ result.passed ? '所有检查通过' : '存在风险约束违反' }}
          </div>
          <div v-if="(result.violations as string[]).length > 0" class="text-sm mt-1" style="color: var(--text-secondary)">
            {{ (result.violations as string[]).length }} 项违反
          </div>
        </div>
      </div>

      <!-- 指标卡片 -->
      <div class="grid grid-cols-2 md:grid-cols-3 gap-4">
        <div class="card p-4">
          <div class="flex items-center gap-2 mb-1">
            <DollarSign class="w-4 h-4" style="color: var(--text-muted)" />
            <span class="text-xs" style="color: var(--text-muted)">组合 CVaR</span>
          </div>
          <div class="text-xl font-bold" :style="{
            color: (result.portfolio_cvar as number) <= config.max_portfolio_cvar ? 'var(--profit)' : 'var(--loss)'
          }">
            {{ formatPct(result.portfolio_cvar as number) }}
          </div>
          <div class="text-xs mt-1" style="color: var(--text-muted)">限制: {{ formatPct(config.max_portfolio_cvar) }}</div>
        </div>
        <div class="card p-4">
          <div class="flex items-center gap-2 mb-1">
            <Percent class="w-4 h-4" style="color: var(--text-muted)" />
            <span class="text-xs" style="color: var(--text-muted)">组合波动率</span>
          </div>
          <div class="text-xl font-bold" style="color: var(--text-primary)">
            {{ formatPct(result.portfolio_volatility as number) }}
          </div>
        </div>
        <div class="card p-4">
          <div class="flex items-center gap-2 mb-1">
            <Shield class="w-4 h-4" style="color: var(--text-muted)" />
            <span class="text-xs" style="color: var(--text-muted)">违反数</span>
          </div>
          <div class="text-xl font-bold" :style="{
            color: (result.violations as string[]).length === 0 ? 'var(--profit)' : 'var(--loss)'
          }">
            {{ (result.violations as string[]).length }}
          </div>
        </div>
      </div>

      <!-- 违反详情 -->
      <div v-if="(result.violations as string[]).length > 0" class="card p-5">
        <h3 class="text-sm font-semibold mb-3" style="color: var(--loss)">违反的约束</h3>
        <div class="space-y-2">
          <div
            v-for="(v, idx) in (result.violations as string[])"
            :key="idx"
            class="flex items-center gap-2 text-sm p-2 rounded"
            style="background: var(--surface)"
          >
            <AlertTriangle class="w-4 h-4" style="color: var(--loss)" />
            <span style="color: var(--text-secondary)">{{ v }}</span>
          </div>
        </div>
      </div>

      <!-- 建议调整 -->
      <div v-if="result.adjusted_positions && Object.keys(result.adjusted_positions as Record<string, number>).length > 0" class="card p-5">
        <h3 class="text-sm font-semibold mb-3" style="color: var(--primary)">建议仓位调整</h3>
        <div class="grid grid-cols-2 md:grid-cols-4 gap-3">
          <div
            v-for="(pct, sym) in (result.adjusted_positions as Record<string, number>)"
            :key="sym"
            class="p-3 rounded"
            style="background: var(--surface)"
          >
            <div class="text-xs" style="color: var(--text-muted)">{{ sym }}</div>
            <div class="font-bold" style="color: var(--primary)">{{ formatPct(pct) }}</div>
          </div>
        </div>
      </div>
    </div>
  </div>
</template>

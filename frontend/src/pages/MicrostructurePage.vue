<script setup lang="ts">
import { ref, onMounted } from 'vue'
import {
  MicrostructureApi,
  type OrderbookSnapshot,
  type CvdResponse,
  type LiquidationSummary,
} from '@/api'
import {
  BookOpen, Activity, Zap, TrendingUp, TrendingDown, RefreshCw,
  AlertTriangle, DollarSign, BarChart3
} from 'lucide-vue-next'

// =========================================================
// 状态
// =========================================================

const activeTab = ref<'orderbook' | 'cvd' | 'liquidations' | 'basis'>('orderbook')
const symbol = ref('BTC-USDT-SWAP')
const loading = ref(false)
const error = ref('')

const symbols = [
  'BTC-USDT-SWAP', 'ETH-USDT-SWAP', 'SOL-USDT-SWAP', 'DOGE-USDT-SWAP',
  'XRP-USDT-SWAP', 'ADA-USDT-SWAP', 'AVAX-USDT-SWAP', 'DOT-USDT-SWAP',
  'LINK-USDT-SWAP', 'MATIC-USDT-SWAP', 'ARB-USDT-SWAP', 'OP-USDT-SWAP',
]

// 订单簿
const orderbook = ref<OrderbookSnapshot | null>(null)

// CVD
const cvdData = ref<CvdResponse | null>(null)
const cvdLimit = ref(500)

// 清算
const liquidationData = ref<LiquidationSummary | null>(null)
const liquidationLimit = ref(100)

// 基差
const basisData = ref<{ symbol: string; count: number; basis_data: unknown[] } | null>(null)

// =========================================================
// 方法
// =========================================================

async function loadOrderbook() {
  loading.value = true
  error.value = ''
  try {
    orderbook.value = await MicrostructureApi.getLatestOrderbook(symbol.value)
  } catch (e: unknown) {
    error.value = e instanceof Error ? e.message : '获取订单簿失败'
  } finally {
    loading.value = false
  }
}

async function loadCvd() {
  loading.value = true
  error.value = ''
  try {
    cvdData.value = await MicrostructureApi.computeCvd(symbol.value, {
      limit: cvdLimit.value,
    })
  } catch (e: unknown) {
    error.value = e instanceof Error ? e.message : '获取 CVD 失败'
  } finally {
    loading.value = false
  }
}

async function loadLiquidations() {
  loading.value = true
  error.value = ''
  try {
    liquidationData.value = await MicrostructureApi.listLiquidations({
      limit: liquidationLimit.value,
    })
  } catch (e: unknown) {
    error.value = e instanceof Error ? e.message : '获取清算数据失败'
  } finally {
    loading.value = false
  }
}

async function loadBasis() {
  loading.value = true
  error.value = ''
  try {
    basisData.value = await MicrostructureApi.listBasis(symbol.value, 50)
  } catch (e: unknown) {
    error.value = e instanceof Error ? e.message : '获取基差数据失败'
  } finally {
    loading.value = false
  }
}

async function refresh() {
  if (activeTab.value === 'orderbook') await loadOrderbook()
  else if (activeTab.value === 'cvd') await loadCvd()
  else if (activeTab.value === 'liquidations') await loadLiquidations()
  else if (activeTab.value === 'basis') await loadBasis()
}

function switchTab(tab: 'orderbook' | 'cvd' | 'liquidations' | 'basis') {
  activeTab.value = tab
  refresh()
}

function formatNum(v: number, digits = 2): string {
  if (Math.abs(v) >= 1e9) return (v / 1e9).toFixed(digits) + 'B'
  if (Math.abs(v) >= 1e6) return (v / 1e6).toFixed(digits) + 'M'
  if (Math.abs(v) >= 1e3) return (v / 1e3).toFixed(digits) + 'K'
  return v.toFixed(digits)
}

function formatTime(t: string): string {
  return new Date(t).toLocaleString('zh-CN', {
    month: '2-digit', day: '2-digit', hour: '2-digit', minute: '2-digit', second: '2-digit',
  })
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

onMounted(() => {
  loadOrderbook()
})
</script>

<template>
  <div class="space-y-6 animate-fade-in">
    <!-- Header -->
    <div class="flex items-center justify-between">
      <div>
        <h1 class="text-2xl font-bold" style="color: var(--text-primary)">微结构数据</h1>
        <p class="text-sm mt-1" style="color: var(--text-secondary)">
          订单簿深度、逐笔成交、清算事件、基差数据的实时监控
        </p>
      </div>
      <button @click="refresh" class="btn btn-secondary">
        <RefreshCw class="w-4 h-4" :class="loading ? 'animate-spin' : ''" />
        刷新
      </button>
    </div>

    <!-- Symbol 选择器 -->
    <div class="card p-4 flex items-center gap-4">
      <label class="text-sm font-medium" style="color: var(--text-secondary)">交易对</label>
      <select v-model="symbol" @change="refresh" class="input" style="width: 200px">
        <option v-for="s in symbols" :key="s" :value="s">{{ s }}</option>
      </select>

      <!-- Tab 切换 -->
      <div class="flex gap-1 ml-auto">
        <button
          v-for="tab in [
            { key: 'orderbook', label: '订单簿', icon: BookOpen },
            { key: 'cvd', label: 'CVD', icon: Activity },
            { key: 'liquidations', label: '清算', icon: Zap },
            { key: 'basis', label: '基差', icon: BarChart3 },
          ]"
          :key="tab.key"
          @click="switchTab(tab.key as 'orderbook' | 'cvd' | 'liquidations' | 'basis')"
          class="btn btn-sm"
          :class="activeTab === tab.key ? 'btn-primary' : 'btn-secondary'"
        >
          <component :is="tab.icon" class="w-4 h-4" />
          {{ tab.label }}
        </button>
      </div>
    </div>

    <!-- 错误提示 -->
    <div v-if="error" class="card p-4 flex items-center gap-3" style="border-left: 3px solid var(--loss)">
      <AlertTriangle class="w-5 h-5" style="color: var(--loss)" />
      <span class="text-sm" style="color: var(--loss)">{{ error }}</span>
    </div>

    <!-- 订单簿 -->
    <div v-if="activeTab === 'orderbook'" class="space-y-4">
      <div v-if="orderbook" class="grid grid-cols-2 md:grid-cols-4 gap-4">
        <div class="card p-4">
          <div class="text-xs" style="color: var(--text-muted)">买一价</div>
          <div class="text-xl font-bold" style="color: var(--profit)">{{ orderbook.best_bid.toFixed(2) }}</div>
        </div>
        <div class="card p-4">
          <div class="text-xs" style="color: var(--text-muted)">卖一价</div>
          <div class="text-xl font-bold" style="color: var(--loss)">{{ orderbook.best_ask.toFixed(2) }}</div>
        </div>
        <div class="card p-4">
          <div class="text-xs" style="color: var(--text-muted)">价差 (bps)</div>
          <div class="text-xl font-bold" style="color: var(--text-primary)">{{ orderbook.spread_bps.toFixed(2) }}</div>
        </div>
        <div class="card p-4">
          <div class="text-xs" style="color: var(--text-muted)">中间价</div>
          <div class="text-xl font-bold" style="color: var(--text-primary)">{{ orderbook.mid_price.toFixed(2) }}</div>
        </div>
      </div>

      <div v-if="orderbook" class="card p-5">
        <h3 class="text-sm font-semibold mb-3" style="color: var(--text-secondary)">深度不平衡度</h3>
        <div class="flex items-center gap-4">
          <div class="flex-1">
            <div class="flex justify-between text-xs mb-1">
              <span style="color: var(--profit)">买盘</span>
              <span style="color: var(--loss)">卖盘</span>
            </div>
            <div class="h-4 rounded-full overflow-hidden flex" style="background: var(--surface)">
              <div
                class="h-full transition-all"
                :style="{
                  width: ((1 + orderbook.depth_imbalance_5) / 2 * 100) + '%',
                  background: 'var(--profit)'
                }"
              />
              <div class="flex-1" style="background: var(--loss)" />
            </div>
          </div>
          <div class="text-lg font-bold" :style="{
            color: orderbook.depth_imbalance_5 > 0 ? 'var(--profit)' : 'var(--loss)'
          }">
            {{ (orderbook.depth_imbalance_5 * 100).toFixed(1) }}%
          </div>
        </div>
        <div class="text-xs mt-2" style="color: var(--text-muted)">
          更新时间：{{ formatTime(orderbook.timestamp) }}
        </div>
      </div>
    </div>

    <!-- CVD -->
    <div v-if="activeTab === 'cvd'" class="space-y-4">
      <div v-if="cvdData" class="grid grid-cols-2 md:grid-cols-4 gap-4">
        <div class="card p-4">
          <div class="text-xs" style="color: var(--text-muted)">CVD</div>
          <div class="text-xl font-bold" :style="{
            color: cvdData.cvd >= 0 ? 'var(--profit)' : 'var(--loss)'
          }">
            {{ cvdData.cvd >= 0 ? '+' : '' }}{{ formatNum(cvdData.cvd) }}
          </div>
        </div>
        <div class="card p-4">
          <div class="text-xs" style="color: var(--text-muted)">成交笔数</div>
          <div class="text-xl font-bold" style="color: var(--text-primary)">{{ cvdData.tick_count }}</div>
        </div>
        <div class="card p-4">
          <div class="text-xs" style="color: var(--text-muted)">买入量</div>
          <div class="text-xl font-bold" style="color: var(--profit)">{{ formatNum(cvdData.buy_volume) }}</div>
        </div>
        <div class="card p-4">
          <div class="text-xs" style="color: var(--text-muted)">卖出量</div>
          <div class="text-xl font-bold" style="color: var(--loss)">{{ formatNum(cvdData.sell_volume) }}</div>
        </div>
      </div>

      <div v-if="cvdData" class="card p-5">
        <h3 class="text-sm font-semibold mb-3" style="color: var(--text-secondary)">买卖力量对比</h3>
        <div class="space-y-3">
          <div>
            <div class="flex justify-between text-xs mb-1">
              <span style="color: var(--profit)">主动买入 ({{ formatNum(cvdData.buy_notional) }} USDT)</span>
              <span style="color: var(--loss)">主动卖出 ({{ formatNum(cvdData.sell_notional) }} USDT)</span>
            </div>
            <div class="h-3 rounded-full overflow-hidden flex" style="background: var(--surface)">
              <div
                class="h-full"
                :style="{
                  width: (cvdData.buy_notional / (cvdData.buy_notional + cvdData.sell_notional) * 100) + '%',
                  background: 'var(--profit)'
                }"
              />
              <div class="flex-1" style="background: var(--loss)" />
            </div>
          </div>
          <div class="text-xs" style="color: var(--text-muted)">
            时间范围：{{ formatTime(cvdData.start_time) }} ~ {{ formatTime(cvdData.end_time) }}
          </div>
        </div>
      </div>
    </div>

    <!-- 清算 -->
    <div v-if="activeTab === 'liquidations'" class="space-y-4">
      <div v-if="liquidationData" class="grid grid-cols-2 md:grid-cols-4 gap-4">
        <div class="card p-4">
          <div class="text-xs" style="color: var(--text-muted)">清算事件数</div>
          <div class="text-xl font-bold" style="color: var(--text-primary)">{{ liquidationData.count }}</div>
        </div>
        <div class="card p-4">
          <div class="text-xs" style="color: var(--text-muted)">总清算额</div>
          <div class="text-xl font-bold" style="color: var(--text-primary)">{{ formatNum(liquidationData.total_notional) }} USDT</div>
        </div>
        <div class="card p-4">
          <div class="text-xs" style="color: var(--text-muted)">多头清算</div>
          <div class="text-xl font-bold" style="color: var(--loss)">{{ formatNum(liquidationData.long_liquidation_notional) }} USDT</div>
        </div>
        <div class="card p-4">
          <div class="text-xs" style="color: var(--text-muted)">空头清算</div>
          <div class="text-xl font-bold" style="color: var(--profit)">{{ formatNum(liquidationData.short_liquidation_notional) }} USDT</div>
        </div>
      </div>

      <div v-if="liquidationData" class="card p-5">
        <h3 class="text-sm font-semibold mb-3" style="color: var(--text-secondary)">多空清算对比</h3>
        <div class="h-4 rounded-full overflow-hidden flex" style="background: var(--surface)">
          <div
            class="h-full"
            :style="{
              width: (liquidationData.long_liquidation_notional / (liquidationData.total_notional || 1) * 100) + '%',
              background: 'var(--loss)'
            }"
          />
          <div class="flex-1" style="background: var(--profit)" />
        </div>
        <div class="flex justify-between text-xs mt-2">
          <span style="color: var(--loss)">多头被清算</span>
          <span style="color: var(--profit)">空头被清算</span>
        </div>
        <div class="text-xs mt-3" style="color: var(--text-muted)">
          时间范围：{{ formatTime(liquidationData.start_time) }} ~ {{ formatTime(liquidationData.end_time) }}
        </div>
      </div>
    </div>

    <!-- 基差 -->
    <div v-if="activeTab === 'basis'" class="space-y-4">
      <div v-if="basisData" class="card p-5">
        <h3 class="text-sm font-semibold mb-3" style="color: var(--text-secondary)">
          基差数据 ({{ basisData.count }} 条)
        </h3>
        <div v-if="basisData.count === 0" class="text-center py-8" style="color: var(--text-muted)">
          暂无基差数据
        </div>
        <div v-else class="overflow-x-auto">
          <table class="w-full text-sm">
            <thead>
              <tr style="border-bottom: 1px solid var(--border)">
                <th class="text-left py-2 px-3" style="color: var(--text-muted)">时间</th>
                <th class="text-right py-2 px-3" style="color: var(--text-muted)">现货价</th>
                <th class="text-right py-2 px-3" style="color: var(--text-muted)">永续价</th>
                <th class="text-right py-2 px-3" style="color: var(--text-muted)">基差</th>
                <th class="text-right py-2 px-3" style="color: var(--text-muted)">基差%</th>
                <th class="text-right py-2 px-3" style="color: var(--text-muted)">资金费率</th>
                <th class="text-right py-2 px-3" style="color: var(--text-muted)">年化</th>
              </tr>
            </thead>
            <tbody>
              <tr
                v-for="(item, idx) in (basisData.basis_data as Array<Record<string, number | string | null>>).slice(0, 20)"
                :key="idx"
                style="border-bottom: 1px solid var(--border)"
              >
                <td class="py-2 px-3" style="color: var(--text-secondary)">{{ formatTime(item.timestamp as string) }}</td>
                <td class="text-right py-2 px-3" style="color: var(--text-primary)">{{ (item.spot_price as number)?.toFixed(2) || '-' }}</td>
                <td class="text-right py-2 px-3" style="color: var(--text-primary)">{{ (item.perp_price as number)?.toFixed(2) || '-' }}</td>
                <td class="text-right py-2 px-3" :style="{ color: (item.perp_basis as number) >= 0 ? 'var(--profit)' : 'var(--loss)' }">
                  {{ (item.perp_basis as number)?.toFixed(2) || '-' }}
                </td>
                <td class="text-right py-2 px-3" :style="{ color: (item.perp_basis_pct as number) >= 0 ? 'var(--profit)' : 'var(--loss)' }">
                  {{ ((item.perp_basis_pct as number) * 100)?.toFixed(3) || '-' }}%
                </td>
                <td class="text-right py-2 px-3" style="color: var(--text-secondary)">
                  {{ item.funding_rate != null ? ((item.funding_rate as number) * 100).toFixed(4) + '%' : '-' }}
                </td>
                <td class="text-right py-2 px-3" style="color: var(--text-secondary)">
                  {{ item.funding_rate_annualized != null ? ((item.funding_rate_annualized as number) * 100).toFixed(2) + '%' : '-' }}
                </td>
              </tr>
            </tbody>
          </table>
        </div>
      </div>
    </div>
  </div>
</template>

<script setup lang="ts">
import { ref, onMounted, onUnmounted, computed, watch } from 'vue'
import api from '@/api'
import { BarChart3, TrendingUp, TrendingDown, RefreshCw, Wifi, WifiOff, ChevronDown } from 'lucide-vue-next'
import CandlestickChart from '@/components/CandlestickChart.vue'
import IndicatorChart from '@/components/IndicatorChart.vue'
import { useWebSocket } from '@/composables/useWebSocket'

const tickers = ref<any[]>([])
const fundingRates = ref<any[]>([])
const longShortRatio = ref<any[]>([])
const klineData = ref<any[]>([])
const fundingRateHistory = ref<any[]>([])
const longShortHistory = ref<any[]>([])
const openInterestHistory = ref<any[]>([])
const popularSymbols = ref<string[]>([])
const loading = ref(true)
const chartLoading = ref(false)
const selectedSymbol = ref('BTC-USDT-SWAP')
const selectedInterval = ref('1H')
const lastUpdateTime = ref<Date | null>(null)

const intervals = ['1m', '5m', '15m', '30m', '1H', '4H', '1D']

function formatSymbol(symbol: string): string {
  return symbol.replace('-USDT-SWAP', '/USDT').replace('-USDT', '/USDT').replace('-BUSD-SWAP', '/BUSD').replace('-BUSD', '/BUSD')
}

const coinNames: Record<string, string> = {
  'BTC': '比特币', 'ETH': '以太坊', 'SOL': 'Solana', 'DOGE': '狗狗币', 'XRP': '瑞波币',
  'ADA': '艾达币', 'AVAX': 'Avalanche', 'DOT': '波卡', 'LINK': 'Chainlink',
}

function getCoinName(symbol: string): string {
  const coin = symbol.split('-')[0]
  return coinNames[coin] || coin
}

const { connected: wsConnected, connect: wsConnect, disconnect: wsDisconnect, on: wsOn, off: wsOff } = useWebSocket()

function toUTCTimestamp(t: string | number): number {
  if (typeof t === 'number') return t
  const str = String(t)
  const isoStr = str.includes('T') ? str : str.replace(' ', 'T') + 'Z'
  return Math.floor(new Date(isoStr).getTime() / 1000)
}

const fundingChartData = computed(() =>
  fundingRateHistory.value.map((f: any) => ({
    time: toUTCTimestamp(f.funding_time || f.created_at),
    value: (f.funding_rate || 0) * 100,
    color: f.funding_rate >= 0 ? 'rgba(16, 185, 129, 0.6)' : 'rgba(239, 68, 68, 0.6)',
  }))
)

const longShortChartData = computed(() =>
  longShortHistory.value.map((r: any) => ({
    time: toUTCTimestamp(r.timestamp),
    value: r.long_short_ratio || r.ratio || 0,
  }))
)

const openInterestChartData = computed(() =>
  openInterestHistory.value.map((o: any) => ({
    time: toUTCTimestamp(o.timestamp),
    value: o.open_interest || 0,
  }))
)

async function loadMarketData() {
  loading.value = true
  try {
    const [tickersRes, fundingRes, ratioRes, symbolsRes] = await Promise.allSettled([
      api.get('/market/tickers'),
      api.get('/market/funding-rates'),
      api.get('/market/long-short-ratio'),
      api.get('/market/popular-symbols'),
    ])
    if (tickersRes.status === 'fulfilled') tickers.value = tickersRes.value.data.tickers || tickersRes.value.data || []
    if (fundingRes.status === 'fulfilled') fundingRates.value = fundingRes.value.data.rates || fundingRes.value.data || []
    if (ratioRes.status === 'fulfilled') longShortRatio.value = ratioRes.value.data.ratios || ratioRes.value.data || []
    if (symbolsRes.status === 'fulfilled') popularSymbols.value = symbolsRes.value.data.symbols || symbolsRes.value.data || []
    if (popularSymbols.value.length && !popularSymbols.value.includes(selectedSymbol.value)) {
      selectedSymbol.value = popularSymbols.value[0]
    }
  } catch (e) {
    console.error('Failed to load market data', e)
  } finally {
    loading.value = false
  }
}

async function loadChartData() {
  chartLoading.value = true
  try {
    const symbol = selectedSymbol.value
    const interval = selectedInterval.value
    const [klinesRes, fundingRes, ratioRes, oiRes] = await Promise.allSettled([
      api.get(`/market/klines/${symbol}`, { params: { interval, limit: 200 } }),
      api.get(`/market/funding-rate/${symbol}`, { params: { limit: 100 } }),
      api.get(`/market/long-short-ratio/${symbol}`, { params: { limit: 100 } }),
      api.get(`/market/open-interest/${symbol}`, { params: { limit: 100 } }),
    ])
    if (klinesRes.status === 'fulfilled') {
      const rawKlines = klinesRes.value.data.data || klinesRes.value.data || []
      klineData.value = rawKlines.map((k: any) => ({
        time: toUTCTimestamp(k.open_time || k.created_at),
        open: Number(k.open), high: Number(k.high), low: Number(k.low),
        close: Number(k.close), volume: Number(k.volume),
      }))
    } else { klineData.value = [] }
    if (fundingRes.status === 'fulfilled') fundingRateHistory.value = fundingRes.value.data.data || fundingRes.value.data || []
    else { fundingRateHistory.value = [] }
    if (ratioRes.status === 'fulfilled') longShortHistory.value = ratioRes.value.data.data || ratioRes.value.data || []
    else { longShortHistory.value = [] }
    if (oiRes.status === 'fulfilled') openInterestHistory.value = oiRes.value.data.data || oiRes.value.data || []
    else { openInterestHistory.value = [] }
    lastUpdateTime.value = new Date()
  } catch (e) {
    console.error('Failed to load chart data', e)
  } finally {
    chartLoading.value = false
  }
}

function onWsTicker(data: any) {
  const idx = tickers.value.findIndex((t: any) => t.symbol === data.symbol)
  const ticker = { symbol: data.symbol, last: data.last, open_24h: data.open_24h, high_24h: data.high_24h, low_24h: data.low_24h, volume_24h: data.volume_24h, best_bid: data.best_bid, best_ask: data.best_ask, change_percent_24h: data.change_percent_24h }
  if (idx >= 0) tickers.value[idx] = ticker
  else tickers.value.push(ticker)
  lastUpdateTime.value = new Date()
}

function onWsKlineUpdate(data: any) {
  if (data.symbol === selectedSymbol.value) loadChartData()
}

function onWsFundingRate(data: any) {
  const idx = fundingRates.value.findIndex((f: any) => f.symbol === data.symbol)
  const rate = { symbol: data.symbol, funding_rate: data.funding_rate, funding_time: data.funding_time }
  if (idx >= 0) fundingRates.value[idx] = rate
  else fundingRates.value.push(rate)
  lastUpdateTime.value = new Date()
}

let pollTimer: ReturnType<typeof setInterval> | null = null
function startPolling() { if (!pollTimer) pollTimer = setInterval(() => { loadChartData(); loadMarketData() }, 30000) }
function stopPolling() { if (pollTimer) { clearInterval(pollTimer); pollTimer = null } }

function formatTime(t: string) {
  return new Date(t).toLocaleString('zh-CN', { month: '2-digit', day: '2-digit', hour: '2-digit', minute: '2-digit' })
}

function formatLastUpdate() {
  if (!lastUpdateTime.value) return ''
  return lastUpdateTime.value.toLocaleTimeString('zh-CN', { hour: '2-digit', minute: '2-digit', second: '2-digit' })
}

watch([selectedSymbol, selectedInterval], () => loadChartData())
watch(wsConnected, (val) => { if (val) stopPolling(); else startPolling() })

onMounted(async () => {
  await loadMarketData()
  loadChartData()
  wsOn('ticker', onWsTicker)
  wsOn('kline_update', onWsKlineUpdate)
  wsOn('funding_rate', onWsFundingRate)
  wsConnect()
  if (!wsConnected.value) startPolling()
})

onUnmounted(() => {
  wsOff('ticker', onWsTicker)
  wsOff('kline_update', onWsKlineUpdate)
  wsOff('funding_rate', onWsFundingRate)
  wsDisconnect()
  stopPolling()
})
</script>

<template>
  <div class="space-y-6 animate-fade-in">
    <!-- Header -->
    <div class="flex items-center justify-between">
      <div>
        <h1 class="text-2xl font-bold" style="color: var(--text-primary)">行情分析</h1>
        <p class="text-sm mt-1" style="color: var(--text-secondary)">实时市场数据与技术指标</p>
      </div>
      <div class="flex items-center gap-3">
        <div class="flex items-center gap-2 px-3 py-1.5 rounded-full text-xs font-medium" :style="{ background: wsConnected ? 'var(--profit-light)' : 'var(--warning-light)', color: wsConnected ? 'var(--profit)' : 'var(--warning)' }">
          <Wifi v-if="wsConnected" class="w-3.5 h-3.5" />
          <WifiOff v-else class="w-3.5 h-3.5" />
          {{ wsConnected ? '实时连接' : '轮询模式' }}
        </div>
        <button @click="loadMarketData(); loadChartData()" class="btn btn-secondary">
          <RefreshCw class="w-4 h-4" />
          刷新
        </button>
      </div>
    </div>

    <!-- K线图 -->
    <div class="card">
      <div class="flex flex-col sm:flex-row sm:items-center justify-between gap-4 p-5" style="border-bottom: 1px solid var(--border)">
        <h2 class="text-lg font-semibold" style="color: var(--text-primary)">K 线图</h2>
        <div class="flex items-center gap-3">
          <!-- Symbol Select -->
          <div class="relative">
            <select v-model="selectedSymbol" class="input pr-10 appearance-none" style="min-width: 140px">
              <option v-for="s in popularSymbols" :key="s" :value="s">{{ formatSymbol(s) }}</option>
              <option v-if="!popularSymbols.length" value="BTC-USDT-SWAP">BTC/USDT</option>
            </select>
            <ChevronDown class="w-4 h-4 absolute right-3 top-1/2 -translate-y-1/2 pointer-events-none" style="color: var(--text-muted)" />
          </div>
          <!-- Interval Tabs -->
          <div class="tabs">
            <button
              v-for="iv in intervals"
              :key="iv"
              @click="selectedInterval = iv"
              class="tab"
              :class="selectedInterval === iv ? 'active' : ''"
            >
              {{ iv }}
            </button>
          </div>
        </div>
      </div>

      <div v-if="chartLoading" class="flex items-center justify-center py-24">
        <div class="spinner"></div>
        <span class="ml-3 text-sm" style="color: var(--text-muted)">加载图表数据...</span>
      </div>
      <div v-else-if="klineData.length" class="p-4">
        <CandlestickChart :data="klineData" :height="420" :show-volume="true" />
      </div>
      <div v-else class="py-24 text-center">
        <BarChart3 class="w-12 h-12 mx-auto mb-3" style="color: var(--text-muted); opacity: 0.3" />
        <p class="text-sm" style="color: var(--text-muted)">暂无 K 线数据</p>
      </div>
    </div>

    <!-- 指标图表 -->
    <div class="grid grid-cols-1 lg:grid-cols-3 gap-6">
      <div class="card">
        <div class="p-4" style="border-bottom: 1px solid var(--border)">
          <h3 class="font-semibold" style="color: var(--text-primary)">资金费率走势</h3>
        </div>
        <div v-if="chartLoading" class="flex items-center justify-center py-16"><div class="spinner"></div></div>
        <div v-else-if="fundingChartData.length" class="p-4">
          <IndicatorChart :data="fundingChartData" title="" :height="180" type="histogram" />
        </div>
        <div v-else class="py-16 text-center">
          <p class="text-sm" style="color: var(--text-muted)">暂无数据</p>
        </div>
      </div>

      <div class="card">
        <div class="p-4" style="border-bottom: 1px solid var(--border)">
          <h3 class="font-semibold" style="color: var(--text-primary)">多空比走势</h3>
        </div>
        <div v-if="chartLoading" class="flex items-center justify-center py-16"><div class="spinner"></div></div>
        <div v-else-if="longShortChartData.length" class="p-4">
          <IndicatorChart :data="longShortChartData" title="" :height="180" line-color="#8B5CF6" />
        </div>
        <div v-else class="py-16 text-center">
          <p class="text-sm" style="color: var(--text-muted)">暂无数据</p>
        </div>
      </div>

      <div class="card">
        <div class="p-4" style="border-bottom: 1px solid var(--border)">
          <h3 class="font-semibold" style="color: var(--text-primary)">持仓量走势</h3>
        </div>
        <div v-if="chartLoading" class="flex items-center justify-center py-16"><div class="spinner"></div></div>
        <div v-else-if="openInterestChartData.length" class="p-4">
          <IndicatorChart :data="openInterestChartData" title="" :height="180" line-color="#EC4899" />
        </div>
        <div v-else class="py-16 text-center">
          <p class="text-sm" style="color: var(--text-muted)">暂无数据</p>
        </div>
      </div>
    </div>

    <!-- 行情列表 -->
    <div class="card">
      <div class="p-5" style="border-bottom: 1px solid var(--border)">
        <h2 class="text-lg font-semibold" style="color: var(--text-primary)">行情列表</h2>
      </div>
      <div class="table-container border-0 rounded-none">
        <table class="table">
          <thead>
            <tr>
              <th>交易对</th>
              <th class="text-right">最新价</th>
              <th class="text-right">涨跌幅</th>
              <th class="text-right">24H成交量</th>
              <th class="text-right">操作</th>
            </tr>
          </thead>
          <tbody>
            <tr v-for="t in tickers" :key="t.symbol">
              <td>
                <div class="font-semibold" style="color: var(--text-primary)">{{ formatSymbol(t.symbol) }}</div>
                <div class="text-xs" style="color: var(--text-muted)">{{ getCoinName(t.symbol) }}</div>
              </td>
              <td class="text-right font-mono" style="color: var(--text-primary)">{{ t.last_price || t.last }}</td>
              <td class="text-right">
                <div class="flex items-center justify-end gap-1">
                  <TrendingUp v-if="(t.price_change_percent || t.change_percent_24h) >= 0" class="w-4 h-4" style="color: var(--profit)" />
                  <TrendingDown v-else class="w-4 h-4" style="color: var(--loss)" />
                  <span class="font-mono font-semibold" :style="{ color: (t.price_change_percent || t.change_percent_24h) >= 0 ? 'var(--profit)' : 'var(--loss)' }">
                    {{ (t.price_change_percent || t.change_percent_24h) >= 0 ? '+' : '' }}{{ (t.price_change_percent || t.change_percent_24h)?.toFixed(2) }}%
                  </span>
                </div>
              </td>
              <td class="text-right font-mono" style="color: var(--text-secondary)">{{ Number(t.volume_24h)?.toLocaleString() }}</td>
              <td class="text-right">
                <button
                  @click="selectedSymbol = t.symbol"
                  class="btn btn-sm"
                  :class="selectedSymbol === t.symbol ? 'btn-primary' : 'btn-secondary'"
                >
                  {{ selectedSymbol === t.symbol ? '查看中' : '查看' }}
                </button>
              </td>
            </tr>
          </tbody>
        </table>
      </div>
    </div>

    <!-- 资金费率和多空比 -->
    <div class="grid grid-cols-1 lg:grid-cols-2 gap-6">
      <div class="card">
        <div class="p-5" style="border-bottom: 1px solid var(--border)">
          <h2 class="text-lg font-semibold" style="color: var(--text-primary)">资金费率</h2>
        </div>
        <div class="table-container border-0 rounded-none">
          <table class="table">
            <thead>
              <tr>
                <th>交易对</th>
                <th class="text-right">费率</th>
                <th class="text-right">下次结算</th>
              </tr>
            </thead>
            <tbody>
              <tr v-for="f in fundingRates" :key="f.symbol">
                <td class="font-semibold">{{ formatSymbol(f.symbol) }}</td>
                <td class="text-right font-mono font-semibold" :style="{ color: f.funding_rate >= 0 ? 'var(--profit)' : 'var(--loss)' }">
                  {{ (f.funding_rate * 100)?.toFixed(4) }}%
                </td>
                <td class="text-right font-mono text-sm" style="color: var(--text-secondary)">{{ formatTime(f.funding_time || f.next_funding_time) }}</td>
              </tr>
            </tbody>
          </table>
        </div>
      </div>

      <div class="card">
        <div class="p-5" style="border-bottom: 1px solid var(--border)">
          <h2 class="text-lg font-semibold" style="color: var(--text-primary)">多空比</h2>
        </div>
        <div class="p-5 space-y-4">
          <div v-for="r in longShortRatio" :key="r.symbol">
            <div class="flex justify-between text-sm mb-2">
              <span class="font-medium" style="color: var(--text-primary)">{{ formatSymbol(r.symbol) }}</span>
              <span class="font-mono font-semibold" :style="{ color: (r.ratio || r.long_short_ratio) >= 1 ? 'var(--profit)' : 'var(--loss)' }">
                {{ (r.ratio || r.long_short_ratio)?.toFixed(2) }}
              </span>
            </div>
            <div class="h-2 rounded-full overflow-hidden" style="background: var(--surface-tertiary)">
              <div
                class="h-full rounded-full transition-all"
                :style="{ width: Math.min((r.ratio || r.long_short_ratio || 0) / ((r.ratio || r.long_short_ratio || 0) + 1) * 100, 100) + '%', background: (r.ratio || r.long_short_ratio) >= 1 ? 'var(--profit)' : 'var(--loss)' }"
              ></div>
            </div>
          </div>
        </div>
      </div>
    </div>
  </div>
</template>

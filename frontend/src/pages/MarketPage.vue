<script setup lang="ts">
import { ref, onMounted, onUnmounted, computed, watch } from 'vue'
import api from '@/api'
import { BarChart3, TrendingUp, TrendingDown, RefreshCw, Wifi, WifiOff } from 'lucide-vue-next'
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
  return symbol
    .replace('-USDT-SWAP', '/USDT')
    .replace('-USDT', '/USDT')
    .replace('-BUSD-SWAP', '/BUSD')
    .replace('-BUSD', '/BUSD')
}

const coinNames: Record<string, string> = {
  'BTC': '比特币', 'ETH': '以太坊', 'SOL': 'Solana',
  'DOGE': '狗狗币', 'XRP': '瑞波币', 'ADA': '艾达币',
  'AVAX': 'Avalanche', 'DOT': '波卡', 'LINK': 'Chainlink',
  'MATIC': 'Polygon', 'UNI': 'Uniswap', 'ATOM': 'Cosmos',
  'LTC': '莱特币', 'FIL': 'Filecoin', 'APT': 'Aptos',
  'ARB': 'Arbitrum', 'OP': 'Optimism', 'NEAR': 'NEAR',
  'SUI': 'Sui', 'PEPE': 'Pepe',
}

function getCoinName(symbol: string): string {
  const coin = symbol.split('-')[0]
  return coinNames[coin] || coin
}

const { connected: wsConnected, connect: wsConnect, disconnect: wsDisconnect, on: wsOn, off: wsOff } = useWebSocket()

function toUTCTimestamp(t: string | number): number {
  if (typeof t === 'number') return t
  const str = String(t)
  const isoStr = str.includes('T') ? str : str.replace(' ', 'T') + '+08:00'
  return Math.floor(new Date(isoStr).getTime() / 1000) as unknown as number
}

const fundingChartData = computed(() =>
  fundingRateHistory.value.map((f: any) => ({
    time: toUTCTimestamp(f.funding_time || f.created_at),
    value: (f.funding_rate || 0) * 100,
    color: f.funding_rate >= 0 ? 'rgba(0, 200, 83, 0.5)' : 'rgba(255, 23, 68, 0.5)',
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
    const [tickersRes, fundingRes, ratioRes, symbolsRes] = await Promise.all([
      api.get('/market/tickers'),
      api.get('/market/funding-rates'),
      api.get('/market/long-short-ratio'),
      api.get('/market/popular-symbols'),
    ])
    tickers.value = tickersRes.data.tickers || tickersRes.data || []
    fundingRates.value = fundingRes.data.rates || fundingRes.data || []
    longShortRatio.value = ratioRes.data.ratios || ratioRes.data || []
    popularSymbols.value = symbolsRes.data.symbols || symbolsRes.data || []
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
    const [klinesRes, fundingRes, ratioRes, oiRes] = await Promise.all([
      api.get(`/market/klines/${symbol}`, { params: { interval, limit: 200 } }),
      api.get(`/market/funding-rate/${symbol}`, { params: { limit: 100 } }),
      api.get(`/market/long-short-ratio/${symbol}`, { params: { limit: 100 } }),
      api.get(`/market/open-interest/${symbol}`, { params: { limit: 100 } }),
    ])
    const rawKlines = klinesRes.data.data || klinesRes.data || []
    klineData.value = rawKlines.map((k: any) => ({
      time: toUTCTimestamp(k.open_time || k.created_at),
      open: Number(k.open),
      high: Number(k.high),
      low: Number(k.low),
      close: Number(k.close),
      volume: Number(k.volume),
    }))
    fundingRateHistory.value = fundingRes.data.data || fundingRes.data || []
    longShortHistory.value = ratioRes.data.data || ratioRes.data || []
    openInterestHistory.value = oiRes.data.data || oiRes.data || []
    lastUpdateTime.value = new Date()
  } catch (e) {
    console.error('Failed to load chart data', e)
  } finally {
    chartLoading.value = false
  }
}

function onWsTicker(data: any) {
  const idx = tickers.value.findIndex((t: any) => t.symbol === data.symbol)
  const ticker = {
    symbol: data.symbol,
    last: data.last,
    open_24h: data.open_24h,
    high_24h: data.high_24h,
    low_24h: data.low_24h,
    volume_24h: data.volume_24h,
    best_bid: data.best_bid,
    best_ask: data.best_ask,
    change_percent_24h: data.change_percent_24h,
  }
  if (idx >= 0) {
    tickers.value[idx] = ticker
  } else {
    tickers.value.push(ticker)
  }
  lastUpdateTime.value = new Date()
}

function onWsKlineUpdate(data: any) {
  if (data.symbol === selectedSymbol.value) {
    loadChartData()
  }
}

function onWsFundingRate(data: any) {
  const idx = fundingRates.value.findIndex((f: any) => f.symbol === data.symbol)
  const rate = {
    symbol: data.symbol,
    funding_rate: data.funding_rate,
    funding_time: data.funding_time,
  }
  if (idx >= 0) {
    fundingRates.value[idx] = rate
  } else {
    fundingRates.value.push(rate)
  }
  if (data.symbol === selectedSymbol.value) {
    fundingRateHistory.value.push({
      funding_rate: data.funding_rate,
      funding_time: data.funding_time,
      created_at: new Date().toISOString(),
    })
    if (fundingRateHistory.value.length > 100) {
      fundingRateHistory.value = fundingRateHistory.value.slice(-100)
    }
  }
  lastUpdateTime.value = new Date()
}

let pollTimer: ReturnType<typeof setInterval> | null = null

function startPolling() {
  if (pollTimer) return
  pollTimer = setInterval(() => {
    loadChartData()
    loadMarketData()
  }, 30000)
}

function stopPolling() {
  if (pollTimer) {
    clearInterval(pollTimer)
    pollTimer = null
  }
}

function formatTime(t: string) {
  return new Date(t).toLocaleString('zh-CN', { month: '2-digit', day: '2-digit', hour: '2-digit', minute: '2-digit' })
}

function formatLastUpdate() {
  if (!lastUpdateTime.value) return ''
  return lastUpdateTime.value.toLocaleTimeString('zh-CN', { hour: '2-digit', minute: '2-digit', second: '2-digit' })
}

watch([selectedSymbol, selectedInterval], () => {
  loadChartData()
})

watch(wsConnected, (val) => {
  if (val) {
    stopPolling()
  } else {
    startPolling()
  }
})

onMounted(async () => {
  await loadMarketData()
  loadChartData()

  wsOn('ticker', onWsTicker)
  wsOn('kline_update', onWsKlineUpdate)
  wsOn('funding_rate', onWsFundingRate)
  wsConnect()

  if (!wsConnected.value) {
    startPolling()
  }
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
  <div class="space-y-6">
    <div class="flex items-center justify-between">
      <div class="flex items-center gap-3">
        <BarChart3 class="w-6 h-6" style="color: var(--gold)" />
        <h1 class="font-display text-2xl font-bold" style="color: var(--text-primary)">市场数据</h1>
        <div class="flex items-center gap-1.5 ml-3 text-xs px-2 py-1 rounded-full" :style="wsConnected ? 'background: rgba(0,200,83,0.1); color: var(--profit)' : 'background: rgba(255,23,68,0.1); color: var(--loss)'">
          <Wifi v-if="wsConnected" class="w-3 h-3" />
          <WifiOff v-else class="w-3 h-3" />
          {{ wsConnected ? '实时' : '轮询' }}
        </div>
        <span v-if="lastUpdateTime" class="text-xs" style="color: var(--text-muted)">更新于 {{ formatLastUpdate() }}</span>
      </div>
      <button @click="loadMarketData(); loadChartData()" class="btn-secondary flex items-center gap-2 text-sm px-4 py-2">
        <RefreshCw class="w-4 h-4" />
        刷新
      </button>
    </div>

    <div v-if="loading" class="grid grid-cols-4 gap-4">
      <div v-for="i in 8" :key="i" class="card animate-pulse h-20"></div>
    </div>

    <template v-else>
      <div class="card">
        <div class="flex items-center justify-between mb-4">
          <h2 class="text-lg font-semibold" style="color: var(--text-primary)">K 线图</h2>
          <div class="flex items-center gap-3">
            <select
              v-model="selectedSymbol"
              class="input-field text-sm py-2 px-3"
              style="width: auto; min-width: 160px"
            >
              <option v-for="s in popularSymbols" :key="s" :value="s">{{ formatSymbol(s) }}</option>
              <option v-if="!popularSymbols.length" value="BTC-USDT-SWAP">BTC/USDT</option>
            </select>
            <div class="flex gap-1">
              <button
                v-for="iv in intervals"
                :key="iv"
                @click="selectedInterval = iv"
                class="px-3 py-1.5 rounded text-xs font-medium transition-all"
                :style="selectedInterval === iv
                  ? 'background: var(--gold-glow); color: var(--gold); border: 1px solid var(--border-accent)'
                  : 'background: var(--bg-primary); color: var(--text-secondary); border: 1px solid var(--border)'"
              >
                {{ iv }}
              </button>
            </div>
          </div>
        </div>

        <div v-if="chartLoading" class="flex items-center justify-center py-20">
          <div class="loading-spinner"></div>
          <span class="ml-3 text-sm" style="color: var(--text-muted)">加载图表数据...</span>
        </div>
        <div v-else-if="klineData.length">
          <CandlestickChart :data="klineData" :height="480" :show-volume="true" />
        </div>
        <div v-else class="py-20 text-center" style="color: var(--text-muted)">暂无 K 线数据</div>
      </div>

      <div class="grid grid-cols-3 gap-6">
        <div class="card">
          <h2 class="text-lg font-semibold mb-3" style="color: var(--text-primary)">资金费率走势</h2>
          <div v-if="chartLoading" class="flex items-center justify-center py-12">
            <div class="loading-spinner"></div>
          </div>
          <div v-else-if="fundingChartData.length">
            <IndicatorChart :data="fundingChartData" title="" :height="220" type="histogram" />
          </div>
          <div v-else class="py-12 text-center" style="color: var(--text-muted)">暂无资金费率数据</div>
        </div>

        <div class="card">
          <h2 class="text-lg font-semibold mb-3" style="color: var(--text-primary)">多空比走势</h2>
          <div v-if="chartLoading" class="flex items-center justify-center py-12">
            <div class="loading-spinner"></div>
          </div>
          <div v-else-if="longShortChartData.length">
            <IndicatorChart :data="longShortChartData" title="" :height="220" line-color="#8B5CF6" />
          </div>
          <div v-else class="py-12 text-center" style="color: var(--text-muted)">暂无多空比数据</div>
        </div>

        <div class="card">
          <h2 class="text-lg font-semibold mb-3" style="color: var(--text-primary)">持仓量走势</h2>
          <div v-if="chartLoading" class="flex items-center justify-center py-12">
            <div class="loading-spinner"></div>
          </div>
          <div v-else-if="openInterestChartData.length">
            <IndicatorChart :data="openInterestChartData" title="" :height="220" line-color="#EC4899" />
          </div>
          <div v-else class="py-12 text-center" style="color: var(--text-muted)">暂无持仓量数据</div>
        </div>
      </div>

      <div class="card">
        <h2 class="text-lg font-semibold mb-4" style="color: var(--text-primary)">行情列表</h2>
        <div class="overflow-x-auto">
          <table class="w-full">
            <thead>
              <tr class="text-xs uppercase" style="color: var(--text-muted)">
                <th class="text-left py-3 font-medium">交易对</th>
                <th class="text-right py-3 font-medium">最新价</th>
                <th class="text-right py-3 font-medium">涨跌幅</th>
                <th class="text-right py-3 font-medium">24H成交量</th>
                <th class="text-right py-3 font-medium">操作</th>
              </tr>
            </thead>
            <tbody>
              <tr v-for="t in tickers" :key="t.symbol" class="border-t" style="border-color: var(--border)">
                <td class="py-3">
                  <div class="font-medium" style="color: var(--text-primary)">{{ formatSymbol(t.symbol) }}</div>
                  <div class="text-xs" style="color: var(--text-muted)">{{ getCoinName(t.symbol) }}</div>
                </td>
                <td class="py-3 text-right font-mono" style="color: var(--text-primary)">{{ t.last_price || t.last }}</td>
                <td class="py-3 text-right font-mono flex items-center justify-end gap-1">
                  <TrendingUp v-if="(t.price_change_percent || t.change_percent_24h) >= 0" class="w-3 h-3" style="color: var(--profit)" />
                  <TrendingDown v-else class="w-3 h-3" style="color: var(--loss)" />
                  <span :style="{ color: (t.price_change_percent || t.change_percent_24h) >= 0 ? 'var(--profit)' : 'var(--loss)' }">
                    {{ (t.price_change_percent || t.change_percent_24h) >= 0 ? '+' : '' }}{{ (t.price_change_percent || t.change_percent_24h)?.toFixed(2) }}%
                  </span>
                </td>
                <td class="py-3 text-right font-mono" style="color: var(--text-secondary)">{{ Number(t.volume_24h)?.toLocaleString() }}</td>
                <td class="py-3 text-right">
                  <button
                    @click="selectedSymbol = t.symbol"
                    class="text-xs px-3 py-1 rounded transition-all"
                    :style="selectedSymbol === t.symbol
                      ? 'background: var(--gold-glow); color: var(--gold)'
                      : 'background: var(--bg-primary); color: var(--text-secondary)'"
                  >
                    {{ selectedSymbol === t.symbol ? '查看中' : '查看' }}
                  </button>
                </td>
              </tr>
            </tbody>
          </table>
        </div>
      </div>

      <div class="grid grid-cols-2 gap-6">
        <div class="card">
          <h2 class="text-lg font-semibold mb-4" style="color: var(--text-primary)">资金费率</h2>
          <table class="w-full">
            <thead>
              <tr class="text-xs uppercase" style="color: var(--text-muted)">
                <th class="text-left py-3 font-medium">交易对</th>
                <th class="text-right py-3 font-medium">费率</th>
                <th class="text-right py-3 font-medium">下次结算</th>
              </tr>
            </thead>
            <tbody>
              <tr v-for="f in fundingRates" :key="f.symbol" class="border-t" style="border-color: var(--border)">
                <td class="py-3 font-medium" style="color: var(--text-primary)">{{ formatSymbol(f.symbol) }}</td>
                <td class="py-3 text-right font-mono" :style="{ color: f.funding_rate >= 0 ? 'var(--profit)' : 'var(--loss)' }">
                  {{ (f.funding_rate * 100)?.toFixed(4) }}%
                </td>
                <td class="py-3 text-right font-mono text-sm" style="color: var(--text-secondary)">{{ formatTime(f.funding_time || f.next_funding_time) }}</td>
              </tr>
            </tbody>
          </table>
        </div>

        <div class="card">
          <h2 class="text-lg font-semibold mb-4" style="color: var(--text-primary)">多空比</h2>
          <div class="space-y-4">
            <div v-for="r in longShortRatio" :key="r.symbol" class="space-y-1">
              <div class="flex justify-between text-sm">
                <span style="color: var(--text-primary)">{{ formatSymbol(r.symbol) }}</span>
                <span class="font-mono" style="color: var(--text-secondary)">{{ (r.ratio || r.long_short_ratio)?.toFixed(2) }}</span>
              </div>
              <div class="h-2 rounded-full overflow-hidden" style="background: var(--border)">
                <div class="h-full rounded-full" :style="{ width: Math.min((r.ratio || r.long_short_ratio || 0) / ((r.ratio || r.long_short_ratio || 0) + 1) * 100, 100) + '%', background: 'var(--profit)' }"></div>
              </div>
            </div>
          </div>
        </div>
      </div>
    </template>
  </div>
</template>

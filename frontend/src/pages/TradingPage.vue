<script setup lang="ts">
import { ref, onMounted, onUnmounted, computed, watch } from 'vue'
import api from '@/api'
import { ArrowLeftRight, Plus, X, RefreshCw, TrendingUp, TrendingDown, AlertCircle, ChevronDown, Shield, History, Activity } from 'lucide-vue-next'

const positions = ref<any[]>([])
const orders = ref<any[]>([])
const tradeHistory = ref<any[]>([])
const balance = ref<any>(null)
const loading = ref(true)
const historyLoading = ref(false)
const submitting = ref(false)
const error = ref('')
const activeTab = ref<'positions' | 'history'>('positions')

// SL/TP modal state
const showSlTpModal = ref(false)
const slTpForm = ref({
  symbol: '',
  side: '',
  pos_side: '',
  size: 0,
  stop_loss: 0,
  take_profit: 0,
})
const slTpSubmitting = ref(false)

const form = ref({
  symbol: 'BTC-USDT-SWAP',
  side: 'long' as 'long' | 'short',
  type: 'market' as 'market' | 'limit',
  quantity: 0.01,
  price: 0,
  leverage: 10,
  stop_loss: 0,
  take_profit: 0,
})

let pollTimer: ReturnType<typeof setInterval> | null = null

// Current ticker price for selected symbol
const currentTicker = ref<any>(null)
const tickerLoading = ref(false)

const popularSymbols = [
  'BTC-USDT-SWAP', 'ETH-USDT-SWAP', 'SOL-USDT-SWAP',
  'DOGE-USDT-SWAP', 'XRP-USDT-SWAP', 'ADA-USDT-SWAP',
  'AVAX-USDT-SWAP', 'DOT-USDT-SWAP', 'LINK-USDT-SWAP',
  'LTC-USDT-SWAP', 'UNI-USDT-SWAP', 'ATOM-USDT-SWAP',
]

function formatSymbol(symbol: string): string {
  return symbol.replace('-USDT-SWAP', '/USDT').replace('-USDT', '/USDT')
}

function formatPnl(val: string | number): string {
  const num = typeof val === 'string' ? parseFloat(val) : val
  if (isNaN(num)) return '0.00'
  return num >= 0 ? `+${num.toFixed(2)}` : num.toFixed(2)
}

function formatPnlPercent(val: string | number): string {
  const num = typeof val === 'string' ? parseFloat(val) : val
  if (isNaN(num)) return '0.00%'
  // OKX upl_ratio is already a percentage (e.g., 0.05 means 5%)
  const pct = (num * 100).toFixed(2)
  return num >= 0 ? `+${pct}%` : `${pct}%`
}

function formatPrice(val: string | number): string {
  const num = typeof val === 'string' ? parseFloat(val) : val
  if (isNaN(num)) return '0.00'
  if (num >= 1000) return num.toLocaleString('en-US', { minimumFractionDigits: 2, maximumFractionDigits: 2 })
  if (num >= 1) return num.toFixed(4)
  return num.toFixed(6)
}

const totalEquity = computed(() => {
  if (!balance.value) return '0.00'
  if (Array.isArray(balance.value)) {
    const eq = balance.value[0]?.totalEq || balance.value[0]?.eq || balance.value[0]?.total_eq || '0'
    return parseFloat(eq).toLocaleString('en-US', { minimumFractionDigits: 2, maximumFractionDigits: 2 })
  }
  const eq = balance.value.totalEq || balance.value.eq || balance.value.total_eq || balance.value.total_equity || '0'
  return parseFloat(eq).toLocaleString('en-US', { minimumFractionDigits: 2, maximumFractionDigits: 2 })
})

// Extract detailed balance data from OKX response (camelCase from OKX / serialized Rust structs)
const balanceDetail = computed(() => {
  const raw = balance.value
  if (!raw) return null
  const b = Array.isArray(raw) ? raw[0] : raw
  if (!b) return null

  // OKX balance structure: top-level has totalEq/imr/mmr/mgnRatio/notionalUsd,
  // details[] has per-currency data (availBal, frozenBal, eq, upl, cashBal, etc.)
  const usdtDetail = (b.details || []).find((d: any) => d.ccy === 'USDT') || b.details?.[0] || {}

  return {
    totalEq: parseFloat(b.totalEq || b.eq || b.total_eq || '0'),
    availBal: parseFloat(usdtDetail.availBal || usdtDetail.avail_bal || b.availBal || b.avail_bal || '0'),
    frozenBal: parseFloat(usdtDetail.frozenBal || usdtDetail.frozen_bal || b.frozenBal || b.frozen_bal || '0'),
    cashBal: parseFloat(usdtDetail.cashBal || usdtDetail.cash_bal || b.cashBal || b.cash_bal || '0'),
    upl: parseFloat(usdtDetail.upl || b.upl || '0'),
    marginRatio: parseFloat(b.mgnRatio || b.mgn_ratio || '0'),
    notionalUsd: parseFloat(b.notionalUsd || b.notional_usd || '0'),
    imr: parseFloat(b.imr || '0'),
    mmr: parseFloat(b.mmr || '0'),
    ordFroz: parseFloat(b.ordFroz || usdtDetail.ordFroz || b.ord_froz || usdtDetail.ord_froz || '0'),
  }
})

const totalUpl = computed(() => {
  // Prefer balance-level upl (from OKX account data), fallback to sum of position upl
  if (balanceDetail.value && balanceDetail.value.upl !== 0) {
    return balanceDetail.value.upl
  }
  return positions.value.reduce((acc, p) => acc + parseFloat(p.upl || '0'), 0)
})

async function loadData() {
  error.value = ''
  try {
    const [posRes, ordRes, balRes] = await Promise.allSettled([
      api.get('/trading/positions'),
      api.get('/trading/orders'),
      api.get('/trading/balance'),
    ])

    if (posRes.status === 'fulfilled') {
      positions.value = posRes.value.data.positions || posRes.value.data.data || posRes.value.data || []
      if (posRes.value.data.error) error.value = posRes.value.data.error
    } else {
      positions.value = []
    }

    if (ordRes.status === 'fulfilled') {
      const ordData = ordRes.value.data
      if (ordData?.orders?.data && Array.isArray(ordData.orders.data)) {
        orders.value = ordData.orders.data
      } else if (ordData?.orders && Array.isArray(ordData.orders)) {
        orders.value = ordData.orders
      } else if (Array.isArray(ordData)) {
        orders.value = ordData
      } else {
        orders.value = []
      }
    } else {
      orders.value = []
    }

    if (balRes.status === 'fulfilled') {
      balance.value = balRes.value.data
      // Check if balance data indicates no API key configured (all zeros in fallback)
      const b = balRes.value.data
      const bData = Array.isArray(b) ? b[0] : b
      const hasZeroBal = bData && (bData.totalEq === '0' || bData.total_eq === '0')
        && (bData.details?.[0]?.availBal === '0' || bData.details?.[0]?.avail_bal === '0' || !bData.details?.length)
      if (hasZeroBal) {
        if (!error.value) error.value = '未检测到交易所数据，请确认已在系统设置中配置API Key'
      }
    }
  } catch (e: any) {
    console.error('Failed to load trading data', e)
    error.value = e.response?.data?.message || e.message || '加载数据失败'
  } finally {
    loading.value = false
  }
}

async function loadTicker() {
  tickerLoading.value = true
  try {
    const symbol = form.value.symbol
    const res = await api.get(`/trading/ticker/${symbol}`)
    const data = res.data?.data?.[0] || res.data?.data || res.data
    currentTicker.value = data || null
  } catch {
    currentTicker.value = null
  } finally {
    tickerLoading.value = false
  }
}

async function loadTradeHistory() {
  historyLoading.value = true
  try {
    const res = await api.get('/trading/trades')
    const data = res.data
    if (data?.trades?.data && Array.isArray(data.trades.data)) {
      tradeHistory.value = data.trades.data
    } else if (data?.trades && Array.isArray(data.trades)) {
      tradeHistory.value = data.trades
    } else if (Array.isArray(data)) {
      tradeHistory.value = data
    } else {
      tradeHistory.value = []
    }
  } catch {
    tradeHistory.value = []
  } finally {
    historyLoading.value = false
  }
}

const currentPrice = computed(() => {
  if (!currentTicker.value) return null
  return parseFloat(currentTicker.value.last || currentTicker.value.lastPx || '0')
})

const priceChange24h = computed(() => {
  if (!currentTicker.value) return null
  const last = parseFloat(currentTicker.value.last || '0')
  const open = parseFloat(currentTicker.value.open24h || currentTicker.value.sodUtc0 || '0')
  if (!open) return null
  return ((last - open) / open) * 100
})

async function submitOrder() {
  // Validate limit order price
  if (form.value.type === 'limit' && (!form.value.price || form.value.price <= 0)) {
    error.value = '限价单必须填写有效价格'
    return
  }
  // Validate quantity
  if (!form.value.quantity || form.value.quantity <= 0) {
    error.value = '数量必须大于0'
    return
  }
  submitting.value = true
  error.value = ''
  try {
    await api.post('/trading/orders', {
      symbol: form.value.symbol,
      side: form.value.side,
      type: form.value.type,
      quantity: form.value.quantity,
      price: form.value.type === 'limit' ? form.value.price : undefined,
      leverage: form.value.leverage,
      stop_loss: form.value.stop_loss || undefined,
      take_profit: form.value.take_profit || undefined,
    })
    await loadData()
  } catch (e: any) {
    console.error('Order failed', e)
    error.value = e.response?.data?.message || e.message || '下单失败'
  } finally {
    submitting.value = false
  }
}

async function cancelOrder(instId: string, ordId: string) {
  try {
    await api.post(`/trading/orders/${instId}:${ordId}/cancel`)
    await loadData()
  } catch (e: any) {
    console.error('Cancel failed', e)
    error.value = e.response?.data?.message || e.message || '撤单失败'
  }
}

function openSlTpModal(p: any) {
  slTpForm.value = {
    symbol: p.inst_id,
    side: p.side,
    pos_side: p.pos_side || '',
    size: p.size,
    stop_loss: 0,
    take_profit: 0,
  }
  showSlTpModal.value = true
}

async function submitSlTp() {
  if (!slTpForm.value.stop_loss && !slTpForm.value.take_profit) {
    error.value = '止损价和止盈价至少填写一个'
    return
  }
  slTpSubmitting.value = true
  error.value = ''
  try {
    await api.post('/trading/positions/sl-tp', {
      symbol: slTpForm.value.symbol,
      side: slTpForm.value.side,
      pos_side: slTpForm.value.pos_side || undefined,
      size: slTpForm.value.size,
      stop_loss: slTpForm.value.stop_loss || undefined,
      take_profit: slTpForm.value.take_profit || undefined,
    })
    showSlTpModal.value = false
    await loadData()
  } catch (e: any) {
    console.error('Set SL/TP failed', e)
    error.value = e.response?.data?.message || e.message || '设置止盈止损失败'
  } finally {
    slTpSubmitting.value = false
  }
}

async function closePosition(instId: string, side: string, size: number) {
  submitting.value = true
  error.value = ''
  try {
    await api.post('/trading/orders', {
      symbol: instId,
      side: side === 'long' ? 'short' : 'long',
      type: 'market',
      quantity: size,
      reduce_only: true,
    })
    await loadData()
  } catch (e: any) {
    console.error('Close position failed', e)
    error.value = e.response?.data?.message || e.message || '平仓失败'
  } finally {
    submitting.value = false
  }
}

onMounted(() => { loadData(); loadTicker(); loadTradeHistory(); pollTimer = setInterval(loadData, 30000) })
onUnmounted(() => { if (pollTimer) { clearInterval(pollTimer); pollTimer = null } })

// Reload ticker when symbol changes
watch(() => form.value.symbol, () => { loadTicker() })
</script>

<template>
  <div class="space-y-6 animate-fade-in">
    <!-- Header -->
    <div class="flex items-center justify-between">
      <div>
        <h1 class="text-2xl font-bold" style="color: var(--text-primary)">交易中心</h1>
        <p class="text-sm mt-1" style="color: var(--text-secondary)">合约交易与持仓管理</p>
      </div>
      <button @click="loadData()" class="btn btn-secondary">
        <RefreshCw class="w-4 h-4" />
        刷新数据
      </button>
    </div>

    <!-- Error Alert -->
    <div v-if="error" class="card p-4 flex items-start gap-3" style="border-left: 3px solid var(--loss); background: var(--loss-light)">
      <AlertCircle class="w-5 h-5 flex-shrink-0" style="color: var(--loss)" />
      <p class="text-sm font-medium" style="color: var(--loss)">{{ error }}</p>
    </div>

    <!-- Account Summary -->
    <div class="grid grid-cols-2 sm:grid-cols-4 lg:grid-cols-8 gap-4">
      <div class="card !p-4">
        <div class="text-xs mb-1" style="color: var(--text-secondary)">账户权益</div>
        <div class="text-lg font-bold font-mono" style="color: var(--text-primary)">${{ totalEquity }}</div>
        <div class="text-xs" style="color: var(--text-muted)">总权益</div>
      </div>
      <div class="card !p-4">
        <div class="text-xs mb-1" style="color: var(--text-secondary)">可用余额</div>
        <div class="text-lg font-bold font-mono" style="color: var(--primary)">
          ${{ balanceDetail ? balanceDetail.availBal.toLocaleString('en-US', { minimumFractionDigits: 2, maximumFractionDigits: 2 }) : '0.00' }}
        </div>
        <div class="text-xs" style="color: var(--text-muted)">可开仓资金</div>
      </div>
      <div class="card !p-4">
        <div class="text-xs mb-1" style="color: var(--text-secondary)">未实现盈亏</div>
        <div class="text-lg font-bold font-mono" :style="{ color: totalUpl >= 0 ? 'var(--profit)' : 'var(--loss)' }">
          {{ totalUpl >= 0 ? '+' : '' }}${{ totalUpl.toLocaleString('en-US', { minimumFractionDigits: 2, maximumFractionDigits: 2 }) }}
        </div>
        <div class="text-xs" style="color: var(--text-muted)">浮动盈亏</div>
      </div>
      <div class="card !p-4">
        <div class="text-xs mb-1" style="color: var(--text-secondary)">保证金占用</div>
        <div class="text-lg font-bold font-mono" style="color: var(--text-primary)">
          ${{ balanceDetail ? balanceDetail.imr.toLocaleString('en-US', { minimumFractionDigits: 2, maximumFractionDigits: 2 }) : '0.00' }}
        </div>
        <div class="text-xs" style="color: var(--text-muted)">初始保证金</div>
      </div>
      <div class="card !p-4">
        <div class="text-xs mb-1" style="color: var(--text-secondary)">维持保证金</div>
        <div class="text-lg font-bold font-mono" :style="{ color: balanceDetail && balanceDetail.mmr > 0 && balanceDetail.mmr < balanceDetail.imr * 0.5 ? 'var(--warning)' : 'var(--text-primary)' }">
          ${{ balanceDetail ? balanceDetail.mmr.toLocaleString('en-US', { minimumFractionDigits: 2, maximumFractionDigits: 2 }) : '0.00' }}
        </div>
        <div class="text-xs" :style="{ color: balanceDetail && balanceDetail.mmr > 0 && balanceDetail.mmr < balanceDetail.imr * 0.5 ? 'var(--warning)' : 'var(--text-muted)' }">
          {{ balanceDetail && balanceDetail.mmr > 0 ? '强平线' : '最低保证金' }}
        </div>
      </div>
      <div class="card !p-4">
        <div class="text-xs mb-1" style="color: var(--text-secondary)">名价值</div>
        <div class="text-lg font-bold font-mono" style="color: var(--text-primary)">
          ${{ balanceDetail ? balanceDetail.notionalUsd.toLocaleString('en-US', { minimumFractionDigits: 2, maximumFractionDigits: 2 }) : '0.00' }}
        </div>
        <div class="text-xs" style="color: var(--text-muted)">持仓价值</div>
      </div>
      <div class="card !p-4">
        <div class="text-xs mb-1" style="color: var(--text-secondary)">冻结资金</div>
        <div class="text-lg font-bold font-mono" style="color: var(--text-secondary)">
          ${{ balanceDetail ? balanceDetail.frozenBal.toLocaleString('en-US', { minimumFractionDigits: 2, maximumFractionDigits: 2 }) : '0.00' }}
        </div>
        <div class="text-xs" style="color: var(--text-muted)">挂单冻结</div>
      </div>
      <div class="card !p-4">
        <div class="text-xs mb-1" style="color: var(--text-secondary)">持仓数 / 委托数</div>
        <div class="text-lg font-bold font-mono" style="color: var(--text-primary)">
          {{ positions.length }} / {{ orders.length }}
        </div>
        <div class="text-xs" :style="{ color: balanceDetail && balanceDetail.marginRatio > 0 && balanceDetail.marginRatio < 0.15 ? 'var(--loss)' : 'var(--text-muted)' }">
          {{ balanceDetail && balanceDetail.marginRatio > 0 ? `保证金率 ${(balanceDetail.marginRatio * 100).toFixed(1)}%` : '无持仓' }}
        </div>
      </div>
    </div>

    <!-- Main Content -->
    <div v-if="loading" class="grid grid-cols-1 lg:grid-cols-3 gap-6">
      <div class="card p-6 animate-pulse"><div class="h-64" style="background: var(--surface-tertiary)"></div></div>
      <div class="lg:col-span-2 card p-6 animate-pulse"><div class="h-64" style="background: var(--surface-tertiary)"></div></div>
    </div>

    <div v-else class="grid grid-cols-1 lg:grid-cols-3 gap-6">
      <!-- Order Form -->
      <div class="card">
        <div class="p-5" style="border-bottom: 1px solid var(--border)">
          <h2 class="text-lg font-semibold" style="color: var(--text-primary)">下单</h2>
        </div>
        <div class="p-5 space-y-4">
          <!-- Current Price Display -->
          <div v-if="currentPrice" class="p-3 rounded-lg" style="background: var(--surface-secondary)">
            <div class="flex items-center justify-between">
              <div>
                <div class="text-xs" style="color: var(--text-muted)">当前价格</div>
                <div class="text-xl font-bold font-mono" :style="{ color: priceChange24h && priceChange24h >= 0 ? 'var(--profit)' : 'var(--loss)' }">
                  ${{ formatPrice(currentPrice) }}
                </div>
              </div>
              <div v-if="priceChange24h !== null" class="text-right">
                <div class="text-xs" style="color: var(--text-muted)">24h涨跌</div>
                <div class="text-sm font-semibold font-mono" :style="{ color: priceChange24h >= 0 ? 'var(--profit)' : 'var(--loss)' }">
                  {{ priceChange24h >= 0 ? '+' : '' }}{{ priceChange24h.toFixed(2) }}%
                </div>
              </div>
            </div>
            <div v-if="currentTicker" class="flex gap-4 mt-2 text-xs" style="color: var(--text-muted)">
              <span>24h高 <span class="font-mono" style="color: var(--text-secondary)">{{ formatPrice(currentTicker.high24h) }}</span></span>
              <span>24h低 <span class="font-mono" style="color: var(--text-secondary)">{{ formatPrice(currentTicker.low24h) }}</span></span>
              <span>24h量 <span class="font-mono" style="color: var(--text-secondary)">{{ currentTicker.vol24h ? parseFloat(currentTicker.vol24h).toLocaleString() : '-' }}</span></span>
            </div>
          </div>
          <div v-else-if="tickerLoading" class="p-3 rounded-lg animate-pulse" style="background: var(--surface-secondary); height: 72px"></div>

          <!-- Symbol Select -->
          <div>
            <label class="label">交易对</label>
            <div class="relative">
              <select v-model="form.symbol" class="input pr-10 appearance-none">
                <option v-for="s in popularSymbols" :key="s" :value="s">{{ formatSymbol(s) }}</option>
              </select>
              <ChevronDown class="w-4 h-4 absolute right-3 top-1/2 -translate-y-1/2 pointer-events-none" style="color: var(--text-muted)" />
            </div>
          </div>

          <!-- Side Toggle -->
          <div>
            <label class="label">方向</label>
            <div class="grid grid-cols-2 gap-2">
              <button
                @click="form.side = 'long'"
                class="py-3 rounded-lg text-sm font-semibold transition-all"
                :style="form.side === 'long' ? 'background: var(--profit); color: white' : 'background: var(--surface-tertiary); color: var(--text-secondary)'"
              >
                做多
              </button>
              <button
                @click="form.side = 'short'"
                class="py-3 rounded-lg text-sm font-semibold transition-all"
                :style="form.side === 'short' ? 'background: var(--loss); color: white' : 'background: var(--surface-tertiary); color: var(--text-secondary)'"
              >
                做空
              </button>
            </div>
          </div>

          <!-- Order Type -->
          <div>
            <label class="label">订单类型</label>
            <div class="grid grid-cols-2 gap-2">
              <button
                @click="form.type = 'market'"
                class="py-2.5 rounded-lg text-sm font-medium transition-all"
                :style="form.type === 'market' ? 'background: var(--primary); color: white' : 'background: var(--surface-tertiary); color: var(--text-secondary)'"
              >
                市价
              </button>
              <button
                @click="form.type = 'limit'"
                class="py-2.5 rounded-lg text-sm font-medium transition-all"
                :style="form.type === 'limit' ? 'background: var(--primary); color: white' : 'background: var(--surface-tertiary); color: var(--text-secondary)'"
              >
                限价
              </button>
            </div>
          </div>

          <!-- Limit Price -->
          <div v-if="form.type === 'limit'">
            <label class="label">价格</label>
            <input v-model.number="form.price" type="number" class="input" placeholder="输入限价" />
          </div>

          <!-- Quantity -->
          <div>
            <label class="label">数量（张）</label>
            <input v-model.number="form.quantity" type="number" step="1" min="1" class="input" />
          </div>

          <!-- Leverage -->
          <div>
            <label class="label flex justify-between">
              <span>杠杆</span>
              <span class="font-mono font-semibold" style="color: var(--primary)">{{ form.leverage }}x</span>
            </label>
            <input v-model.number="form.leverage" type="range" min="1" max="125" class="w-full h-2 rounded-lg appearance-none cursor-pointer" style="background: var(--surface-tertiary)" />
          </div>

          <!-- SL/TP -->
          <div class="grid grid-cols-2 gap-3">
            <div>
              <label class="label text-xs">止损价</label>
              <input v-model.number="form.stop_loss" type="number" class="input text-sm" placeholder="选填" />
            </div>
            <div>
              <label class="label text-xs">止盈价</label>
              <input v-model.number="form.take_profit" type="number" class="input text-sm" placeholder="选填" />
            </div>
          </div>

          <!-- Submit Button -->
          <button
            @click="submitOrder"
            :disabled="submitting"
            class="btn w-full py-3 font-semibold"
            :style="form.side === 'long' ? 'background: var(--profit); color: white' : 'background: var(--loss); color: white'"
          >
            <Plus class="w-4 h-4" />
            {{ submitting ? '提交中...' : (form.side === 'long' ? '确认做多' : '确认做空') }}
          </button>
        </div>
      </div>

      <!-- Positions, Orders & History -->
      <div class="lg:col-span-2 space-y-6">
        <!-- Tab Navigation -->
        <div class="card">
          <div class="flex" style="border-bottom: 1px solid var(--border)">
            <button @click="activeTab = 'positions'" class="px-5 py-3.5 text-sm font-semibold transition-colors relative"
              :style="{ color: activeTab === 'positions' ? 'var(--primary)' : 'var(--text-muted)' }">
              持仓与委托
              <span v-if="positions.length > 0 || orders.length > 0" class="ml-1 text-xs px-1.5 py-0.5 rounded-full" style="background: var(--primary-bg); color: var(--primary)">{{ positions.length + orders.length }}</span>
              <div v-if="activeTab === 'positions'" class="absolute bottom-0 left-0 right-0 h-0.5" style="background: var(--primary)"></div>
            </button>
            <button @click="activeTab = 'history'" class="px-5 py-3.5 text-sm font-semibold transition-colors relative"
              :style="{ color: activeTab === 'history' ? 'var(--primary)' : 'var(--text-muted)' }">
              <History class="w-4 h-4 inline -mt-0.5" />
              交易历史
              <div v-if="activeTab === 'history'" class="absolute bottom-0 left-0 right-0 h-0.5" style="background: var(--primary)"></div>
            </button>
          </div>

          <!-- Positions Tab Content -->
          <div v-if="activeTab === 'positions'">
            <!-- Positions Section -->
            <div v-if="positions.length === 0" class="p-12 text-center">
              <ArrowLeftRight class="w-12 h-12 mx-auto mb-3" style="color: var(--text-muted); opacity: 0.3" />
              <p class="text-sm" style="color: var(--text-muted)">暂无持仓</p>
            </div>
            <div v-else class="table-container border-0 rounded-none">
              <table class="table">
                <thead>
                  <tr>
                    <th>交易对</th>
                    <th>方向</th>
                    <th class="text-right">数量</th>
                    <th class="text-right">均价</th>
                    <th class="text-right">标记价</th>
                    <th class="text-right">强平价</th>
                    <th class="text-right">杠杆</th>
                    <th class="text-right">保证金</th>
                    <th class="text-right">未实现盈亏</th>
                    <th class="text-right">操作</th>
                  </tr>
                </thead>
                <tbody>
                  <tr v-for="p in positions" :key="p.inst_id">
                    <td class="font-semibold">{{ formatSymbol(p.inst_id || '') }}</td>
                    <td>
                      <span class="badge" :class="p.side === 'long' ? 'badge-profit' : 'badge-loss'">
                        {{ p.side === 'long' ? '多' : '空' }}
                      </span>
                    </td>
                    <td class="text-right font-mono">{{ p.size }}</td>
                    <td class="text-right font-mono" style="color: var(--text-secondary)">{{ formatPrice(p.avg_px) }}</td>
                    <td class="text-right font-mono" style="color: var(--text-secondary)">{{ formatPrice(p.mark_px) }}</td>
                    <td class="text-right font-mono" :style="{ color: p.liq_px ? 'var(--warning)' : 'var(--text-muted)' }">{{ p.liq_px ? formatPrice(p.liq_px) : '-' }}</td>
                    <td class="text-right font-mono font-semibold" style="color: var(--primary)">{{ p.lever }}x</td>
                    <td class="text-right font-mono" style="color: var(--text-secondary)">{{ formatPrice(p.margin) }}</td>
                    <td class="text-right font-mono font-semibold" :style="{ color: parseFloat(p.upl) >= 0 ? 'var(--profit)' : 'var(--loss)' }">
                      {{ formatPnl(p.upl) }}
                      <span class="text-xs ml-1">{{ formatPnlPercent(p.upl_ratio) }}</span>
                    </td>
                    <td class="text-right">
                      <div class="flex items-center justify-end gap-1.5">
                        <button @click="openSlTpModal(p)" class="btn btn-secondary btn-sm" title="止盈止损">
                          <Shield class="w-3.5 h-3.5" />
                          止盈止损
                        </button>
                        <button @click="closePosition(p.inst_id, p.side, p.size)" class="btn btn-danger btn-sm">
                          平仓
                        </button>
                      </div>
                    </td>
                  </tr>
                </tbody>
              </table>
            </div>

            <!-- Pending Orders (below positions) -->
            <div v-if="orders.length > 0" style="border-top: 1px solid var(--border)">
              <div class="p-4">
                <h3 class="text-sm font-semibold mb-3" style="color: var(--text-secondary)">当前委托 ({{ orders.length }})</h3>
                <div class="table-container border-0 rounded-none">
                  <table class="table">
                    <thead>
                      <tr>
                        <th>交易对</th>
                        <th>方向</th>
                        <th>类型</th>
                        <th class="text-right">数量</th>
                        <th class="text-right">价格</th>
                        <th class="text-right">操作</th>
                      </tr>
                    </thead>
                    <tbody>
                      <tr v-for="o in orders" :key="o.ordId || o.id">
                        <td class="font-semibold">{{ formatSymbol(o.instId || '') }}</td>
                        <td>
                          <span class="badge" :class="o.side === 'buy' ? 'badge-profit' : 'badge-loss'">
                            {{ o.side === 'buy' ? '买入' : '卖出' }}
                          </span>
                        </td>
                        <td style="color: var(--text-secondary)">{{ o.ordType === 'limit' ? '限价' : '市价' }}</td>
                        <td class="text-right font-mono">{{ o.sz }}</td>
                        <td class="text-right font-mono" style="color: var(--text-secondary)">{{ o.px ? formatPrice(o.px) : '-' }}</td>
                        <td class="text-right">
                          <button @click="cancelOrder(o.instId, o.ordId)" class="btn btn-ghost btn-sm" style="color: var(--loss)">
                            <X class="w-4 h-4" />
                            撤单
                          </button>
                        </td>
                      </tr>
                    </tbody>
                  </table>
                </div>
              </div>
            </div>
          </div>

          <!-- Trade History Tab Content -->
          <div v-if="activeTab === 'history'">
            <div v-if="historyLoading" class="p-12 text-center">
              <div class="animate-pulse space-y-3">
                <div v-for="i in 3" :key="i" class="h-12 rounded-lg" style="background: var(--surface-secondary)"></div>
              </div>
            </div>
            <div v-else-if="tradeHistory.length === 0" class="p-12 text-center">
              <Activity class="w-12 h-12 mx-auto mb-3" style="color: var(--text-muted); opacity: 0.3" />
              <p class="text-sm" style="color: var(--text-muted)">暂无交易历史</p>
            </div>
            <div v-else class="table-container border-0 rounded-none">
              <table class="table">
                <thead>
                  <tr>
                    <th>时间</th>
                    <th>交易对</th>
                    <th>方向</th>
                    <th>类型</th>
                    <th class="text-right">数量</th>
                    <th class="text-right">成交价</th>
                    <th class="text-right">手续费</th>
                    <th>状态</th>
                  </tr>
                </thead>
                <tbody>
                  <tr v-for="t in tradeHistory" :key="t.ordId || t.id">
                    <td class="text-xs" style="color: var(--text-muted)">{{ t.cTime ? new Date(parseInt(t.cTime)).toLocaleString('zh-CN', { month: '2-digit', day: '2-digit', hour: '2-digit', minute: '2-digit' }) : '-' }}</td>
                    <td class="font-semibold">{{ formatSymbol(t.instId || '') }}</td>
                    <td>
                      <span class="badge" :class="t.side === 'buy' ? 'badge-profit' : 'badge-loss'">
                        {{ t.side === 'buy' ? '买入' : '卖出' }}
                      </span>
                    </td>
                    <td style="color: var(--text-secondary)">{{ t.ordType === 'limit' ? '限价' : t.ordType === 'market' ? '市价' : t.ordType || '-' }}</td>
                    <td class="text-right font-mono">{{ t.sz || t.accFillSz || '-' }}</td>
                    <td class="text-right font-mono" style="color: var(--text-secondary)">{{ t.avgPx ? formatPrice(t.avgPx) : (t.px ? formatPrice(t.px) : '-') }}</td>
                    <td class="text-right font-mono text-xs" style="color: var(--text-muted)">{{ t.fee ? parseFloat(t.fee).toFixed(4) : '-' }}</td>
                    <td>
                      <span class="text-xs px-1.5 py-0.5 rounded" :style="{ background: t.state === 'filled' || t.state === '2' ? 'var(--profit-light)' : 'var(--surface-tertiary)', color: t.state === 'filled' || t.state === '2' ? 'var(--profit)' : 'var(--text-muted)' }">
                        {{ t.state === 'filled' || t.state === '2' ? '已成交' : t.state === 'canceled' ? '已撤单' : t.state || '-' }}
                      </span>
                    </td>
                  </tr>
                </tbody>
              </table>
            </div>
          </div>
        </div>
      </div>
    </div>

    <!-- SL/TP Modal -->
    <div v-if="showSlTpModal" class="fixed inset-0 z-50 flex items-center justify-center" style="background: rgba(0,0,0,0.3)">
      <div class="card w-full max-w-sm mx-4 !p-6">
        <h3 class="text-base font-bold mb-4" style="color: var(--text-primary)">设置止盈止损</h3>
        <div class="space-y-3">
          <div class="flex items-center gap-2 p-2.5 rounded-lg" style="background: var(--surface-secondary)">
            <span class="text-sm font-semibold" style="color: var(--text-primary)">{{ formatSymbol(slTpForm.symbol) }}</span>
            <span class="text-xs px-1.5 py-0.5 rounded" :class="slTpForm.side === 'long' ? 'badge-profit' : 'badge-loss'">
              {{ slTpForm.side === 'long' ? '多' : '空' }}
            </span>
            <span class="text-xs" style="color: var(--text-muted)">{{ slTpForm.size }} 张</span>
          </div>
          <div>
            <label class="label">止损价</label>
            <input v-model.number="slTpForm.stop_loss" type="number" class="input" placeholder="触发止损的价格" step="any" min="0" />
            <p class="text-xs mt-1" style="color: var(--text-muted)">
              {{ slTpForm.side === 'long' ? '低于此价将触发卖出止损' : '高于此价将触发买入止损' }}
            </p>
          </div>
          <div>
            <label class="label">止盈价</label>
            <input v-model.number="slTpForm.take_profit" type="number" class="input" placeholder="触发止盈的价格" step="any" min="0" />
            <p class="text-xs mt-1" style="color: var(--text-muted)">
              {{ slTpForm.side === 'long' ? '高于此价将触发卖出止盈' : '低于此价将触发买入止盈' }}
            </p>
          </div>
        </div>
        <div class="flex gap-2 mt-5">
          <button @click="showSlTpModal = false" class="btn-secondary flex-1">取消</button>
          <button @click="submitSlTp" :disabled="slTpSubmitting" class="btn-primary flex-1">
            {{ slTpSubmitting ? '提交中...' : '确认设置' }}
          </button>
        </div>
      </div>
    </div>
  </div>
</template>

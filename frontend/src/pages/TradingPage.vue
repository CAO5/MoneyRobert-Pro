<script setup lang="ts">
import { ref, onMounted, computed } from 'vue'
import api from '@/api'
import { ArrowLeftRight, Plus, X, RefreshCw, TrendingUp, TrendingDown, Wallet, AlertCircle, ChevronDown } from 'lucide-vue-next'

const positions = ref<any[]>([])
const orders = ref<any[]>([])
const balance = ref<any>(null)
const loading = ref(true)
const submitting = ref(false)
const error = ref('')

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
    const eq = balance.value[0]?.total_eq || balance.value[0]?.eq || '0'
    return parseFloat(eq).toLocaleString('en-US', { minimumFractionDigits: 2, maximumFractionDigits: 2 })
  }
  const eq = balance.value.total_eq || balance.value.eq || balance.value.total_equity || '0'
  return parseFloat(eq).toLocaleString('en-US', { minimumFractionDigits: 2, maximumFractionDigits: 2 })
})

const totalUpl = computed(() => {
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
    }
  } catch (e: any) {
    console.error('Failed to load trading data', e)
    error.value = e.response?.data?.message || e.message || '加载数据失败'
  } finally {
    loading.value = false
  }
}

async function submitOrder() {
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

async function closePosition(instId: string, side: string, size: number) {
  submitting.value = true
  error.value = ''
  try {
    await api.post('/trading/orders', {
      symbol: instId,
      side: side === 'long' ? 'short' : 'long',
      type: 'market',
      quantity: size,
    })
    await loadData()
  } catch (e: any) {
    console.error('Close position failed', e)
    error.value = e.response?.data?.message || e.message || '平仓失败'
  } finally {
    submitting.value = false
  }
}

onMounted(loadData)
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
    <div class="grid grid-cols-1 sm:grid-cols-3 gap-4">
      <div class="card p-5 flex items-center gap-4">
        <div class="w-12 h-12 rounded-xl flex items-center justify-center" style="background: var(--primary-bg)">
          <Wallet class="w-6 h-6" style="color: var(--primary)" />
        </div>
        <div>
          <p class="text-sm" style="color: var(--text-secondary)">账户权益</p>
          <p class="text-xl font-bold font-mono" style="color: var(--text-primary)">${{ totalEquity }}</p>
        </div>
      </div>
      <div class="card p-5 flex items-center gap-4">
        <div class="w-12 h-12 rounded-xl flex items-center justify-center" :style="{ background: totalUpl >= 0 ? 'var(--profit-light)' : 'var(--loss-light)' }">
          <TrendingUp v-if="totalUpl >= 0" class="w-6 h-6" style="color: var(--profit)" />
          <TrendingDown v-else class="w-6 h-6" style="color: var(--loss)" />
        </div>
        <div>
          <p class="text-sm" style="color: var(--text-secondary)">未实现盈亏</p>
          <p class="text-xl font-bold font-mono" :style="{ color: totalUpl >= 0 ? 'var(--profit)' : 'var(--loss)' }">
            {{ formatPnl(totalUpl) }} USDT
          </p>
        </div>
      </div>
      <div class="card p-5 flex items-center gap-4">
        <div class="w-12 h-12 rounded-xl flex items-center justify-center" style="background: var(--surface-tertiary)">
          <ArrowLeftRight class="w-6 h-6" style="color: var(--text-secondary)" />
        </div>
        <div>
          <p class="text-sm" style="color: var(--text-secondary)">持仓数量</p>
          <p class="text-xl font-bold font-mono" style="color: var(--text-primary)">{{ positions.length }}</p>
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

      <!-- Positions & Orders -->
      <div class="lg:col-span-2 space-y-6">
        <!-- Positions -->
        <div class="card">
          <div class="p-5" style="border-bottom: 1px solid var(--border)">
            <h2 class="text-lg font-semibold" style="color: var(--text-primary)">当前持仓</h2>
          </div>
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
                  <th class="text-right">杠杆</th>
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
                  <td class="text-right font-mono font-semibold" style="color: var(--primary)">{{ p.lever }}x</td>
                  <td class="text-right font-mono font-semibold" :style="{ color: parseFloat(p.upl) >= 0 ? 'var(--profit)' : 'var(--loss)' }">
                    {{ formatPnl(p.upl) }}
                    <span class="text-xs ml-1">{{ formatPnlPercent(p.upl_ratio) }}</span>
                  </td>
                  <td class="text-right">
                    <button @click="closePosition(p.inst_id, p.side, p.size)" class="btn btn-danger btn-sm">
                      平仓
                    </button>
                  </td>
                </tr>
              </tbody>
            </table>
          </div>
        </div>

        <!-- Pending Orders -->
        <div class="card">
          <div class="p-5" style="border-bottom: 1px solid var(--border)">
            <h2 class="text-lg font-semibold" style="color: var(--text-primary)">当前委托</h2>
          </div>
          <div v-if="orders.length === 0" class="p-12 text-center">
            <p class="text-sm" style="color: var(--text-muted)">暂无委托订单</p>
          </div>
          <div v-else class="table-container border-0 rounded-none">
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
  </div>
</template>

<script setup lang="ts">
import { ref, onMounted, computed } from 'vue'
import api from '@/api'
import { ArrowLeftRight, Plus, X, RefreshCw, TrendingUp, TrendingDown, Wallet } from 'lucide-vue-next'

const positions = ref<any[]>([])
const orders = ref<any[]>([])
const balance = ref<any>(null)
const loading = ref(true)
const submitting = ref(false)
const error = ref('')

const form = ref({
  symbol: 'BTC-USDT-SWAP', side: 'long' as 'long' | 'short',
  type: 'market' as 'market' | 'limit', quantity: 0.01,
  price: 0, leverage: 10, stop_loss: 0, take_profit: 0,
})

const popularSymbols = [
  'BTC-USDT-SWAP', 'ETH-USDT-SWAP', 'SOL-USDT-SWAP',
  'DOGE-USDT-SWAP', 'XRP-USDT-SWAP', 'ADA-USDT-SWAP',
  'AVAX-USDT-SWAP', 'DOT-USDT-SWAP', 'LINK-USDT-SWAP',
  'LTC-USDT-SWAP', 'UNI-USDT-SWAP', 'ATOM-USDT-SWAP',
  'ARB-USDT-SWAP', 'OP-USDT-SWAP', 'NEAR-USDT-SWAP',
  'SUI-USDT-SWAP', 'PEPE-USDT-SWAP', 'FIL-USDT-SWAP',
  'APT-USDT-SWAP',
]

function formatSymbol(symbol: string): string {
  return symbol
    .replace('-USDT-SWAP', '/USDT')
    .replace('-USDT', '/USDT')
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
  // balance is an array of OkxAccount
  if (Array.isArray(balance.value)) {
    const eq = balance.value[0]?.total_eq || balance.value[0]?.eq || '0'
    return parseFloat(eq).toLocaleString('en-US', { minimumFractionDigits: 2, maximumFractionDigits: 2 })
  }
  const eq = balance.value.total_eq || balance.value.eq || balance.value.total_equity || '0'
  return parseFloat(eq).toLocaleString('en-US', { minimumFractionDigits: 2, maximumFractionDigits: 2 })
})

const totalUpl = computed(() => {
  const sum = positions.value.reduce((acc, p) => acc + parseFloat(p.upl || '0'), 0)
  return sum
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
      if (posRes.value.data.error) {
        error.value = posRes.value.data.error
      }
    } else {
      positions.value = []
    }

    if (ordRes.status === 'fulfilled') {
      // OKX returns { orders: { code, msg, data: [...] } }
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
      if (ordRes.value.data.error && !error.value) {
        error.value = ordRes.value.data.error
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
    // Close position by placing opposite order
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

onMounted(() => { loadData() })
</script>

<template>
  <div class="space-y-6">
    <div class="flex items-center justify-between">
      <div class="flex items-center gap-3">
        <ArrowLeftRight class="w-6 h-6" style="color: var(--gold)" />
        <h1 class="font-display text-2xl font-bold" style="color: var(--text-primary)">交易中心</h1>
      </div>
      <button @click="loadData()" class="btn-secondary flex items-center gap-2 text-sm px-4 py-2">
        <RefreshCw class="w-4 h-4" />
        刷新
      </button>
    </div>

    <!-- Error message -->
    <div v-if="error" class="card border-l-4" style="border-color: var(--loss); background: rgba(255,23,68,0.08)">
      <p class="text-sm" style="color: var(--loss)">{{ error }}</p>
    </div>

    <!-- Account Summary -->
    <div class="grid grid-cols-3 gap-4">
      <div class="card flex items-center gap-4">
        <div class="w-10 h-10 rounded-lg flex items-center justify-center" style="background: rgba(212,168,67,0.15)">
          <Wallet class="w-5 h-5" style="color: var(--gold)" />
        </div>
        <div>
          <p class="text-xs" style="color: var(--text-muted)">账户权益</p>
          <p class="text-lg font-bold font-mono" style="color: var(--text-primary)">${{ totalEquity }}</p>
        </div>
      </div>
      <div class="card flex items-center gap-4">
        <div class="w-10 h-10 rounded-lg flex items-center justify-center" style="background: rgba(0,200,83,0.1)">
          <TrendingUp class="w-5 h-5" style="color: var(--profit)" />
        </div>
        <div>
          <p class="text-xs" style="color: var(--text-muted)">未实现盈亏</p>
          <p class="text-lg font-bold font-mono" :style="{ color: totalUpl >= 0 ? 'var(--profit)' : 'var(--loss)' }">
            {{ formatPnl(totalUpl) }}
          </p>
        </div>
      </div>
      <div class="card flex items-center gap-4">
        <div class="w-10 h-10 rounded-lg flex items-center justify-center" style="background: rgba(212,168,67,0.15)">
          <ArrowLeftRight class="w-5 h-5" style="color: var(--gold)" />
        </div>
        <div>
          <p class="text-xs" style="color: var(--text-muted)">持仓数量</p>
          <p class="text-lg font-bold font-mono" style="color: var(--text-primary)">{{ positions.length }}</p>
        </div>
      </div>
    </div>

    <div v-if="loading" class="grid grid-cols-3 gap-6">
      <div v-for="i in 3" :key="i" class="card animate-pulse h-64"></div>
    </div>

    <div v-else class="grid grid-cols-3 gap-6">
      <!-- Order Form -->
      <div class="card space-y-4">
        <h2 class="text-lg font-semibold" style="color: var(--text-primary)">下单</h2>

        <div>
          <label class="text-sm mb-1 block" style="color: var(--text-secondary)">交易对</label>
          <select v-model="form.symbol" class="input-field">
            <option v-for="s in popularSymbols" :key="s" :value="s">{{ formatSymbol(s) }}</option>
          </select>
        </div>

        <div class="grid grid-cols-2 gap-2">
          <button @click="form.side = 'long'" class="py-2 rounded-lg text-sm font-medium transition-colors"
            :style="form.side === 'long' ? 'background: var(--profit); color: #000' : 'background: var(--border); color: var(--text-secondary)'">做多</button>
          <button @click="form.side = 'short'" class="py-2 rounded-lg text-sm font-medium transition-colors"
            :style="form.side === 'short' ? 'background: var(--loss); color: #fff' : 'background: var(--border); color: var(--text-secondary)'">做空</button>
        </div>

        <div class="grid grid-cols-2 gap-2">
          <button @click="form.type = 'market'" class="py-2 rounded-lg text-sm font-medium transition-colors"
            :style="form.type === 'market' ? 'background: var(--gold); color: var(--bg-primary)' : 'background: var(--border); color: var(--text-secondary)'">市价</button>
          <button @click="form.type = 'limit'" class="py-2 rounded-lg text-sm font-medium transition-colors"
            :style="form.type === 'limit' ? 'background: var(--gold); color: var(--bg-primary)' : 'background: var(--border); color: var(--text-secondary)'">限价</button>
        </div>

        <div v-if="form.type === 'limit'">
          <label class="text-sm mb-1 block" style="color: var(--text-secondary)">价格</label>
          <input v-model.number="form.price" type="number" class="input-field" placeholder="输入限价" />
        </div>

        <div>
          <label class="text-sm mb-1 block" style="color: var(--text-secondary)">数量（张）</label>
          <input v-model.number="form.quantity" type="number" step="1" min="1" class="input-field" />
        </div>

        <div>
          <label class="text-sm mb-1 flex justify-between" style="color: var(--text-secondary)">
            <span>杠杆</span><span class="font-mono" style="color: var(--gold)">{{ form.leverage }}x</span>
          </label>
          <input v-model.number="form.leverage" type="range" min="1" max="125" class="w-full accent-[#D4A843]" />
        </div>

        <div class="grid grid-cols-2 gap-2">
          <div>
            <label class="text-xs mb-1 block" style="color: var(--text-muted)">止损</label>
            <input v-model.number="form.stop_loss" type="number" class="input-field text-sm" placeholder="选填" />
          </div>
          <div>
            <label class="text-xs mb-1 block" style="color: var(--text-muted)">止盈</label>
            <input v-model.number="form.take_profit" type="number" class="input-field text-sm" placeholder="选填" />
          </div>
        </div>

        <button @click="submitOrder" class="btn-primary w-full flex items-center justify-center gap-2" :disabled="submitting"
          :style="form.side === 'long' ? 'background: var(--profit); color: #000' : 'background: var(--loss); color: #fff'">
          <Plus class="w-4 h-4" />
          {{ submitting ? '提交中...' : (form.side === 'long' ? '做多' : '做空') }}
        </button>
      </div>

      <!-- Positions & Orders -->
      <div class="col-span-2 space-y-6">
        <!-- Positions -->
        <div class="card">
          <h2 class="text-lg font-semibold mb-4" style="color: var(--text-primary)">当前持仓</h2>
          <div v-if="positions.length === 0" class="py-8 text-center" style="color: var(--text-muted)">暂无持仓</div>
          <div v-else class="overflow-x-auto">
            <table class="w-full text-sm">
              <thead>
                <tr class="text-xs uppercase" style="color: var(--text-muted)">
                  <th class="text-left py-2 px-2 font-medium">交易对</th>
                  <th class="text-left py-2 px-2 font-medium">方向</th>
                  <th class="text-left py-2 px-2 font-medium">模式</th>
                  <th class="text-right py-2 px-2 font-medium">数量</th>
                  <th class="text-right py-2 px-2 font-medium">均价</th>
                  <th class="text-right py-2 px-2 font-medium">标记价</th>
                  <th class="text-right py-2 px-2 font-medium">杠杆</th>
                  <th class="text-right py-2 px-2 font-medium">未实现盈亏</th>
                  <th class="text-right py-2 px-2 font-medium">收益率</th>
                  <th class="text-right py-2 px-2 font-medium">强平价</th>
                  <th class="text-right py-2 px-2 font-medium">操作</th>
                </tr>
              </thead>
              <tbody>
                <tr v-for="p in positions" :key="p.inst_id" class="border-t" style="border-color: var(--border)">
                  <td class="py-2 px-2 font-medium" style="color: var(--text-primary)">{{ formatSymbol(p.inst_id || '') }}</td>
                  <td class="py-2 px-2">
                    <span class="text-xs px-2 py-0.5 rounded-full font-medium"
                      :style="p.side === 'long' ? 'background: rgba(0,200,83,0.15); color: var(--profit)' : 'background: rgba(255,23,68,0.15); color: var(--loss)'">
                      {{ p.side === 'long' ? '多' : '空' }}
                    </span>
                  </td>
                  <td class="py-2 px-2 text-xs" style="color: var(--text-secondary)">{{ p.mgn_mode === 'cross' ? '全仓' : '逐仓' }}</td>
                  <td class="py-2 px-2 text-right font-mono" style="color: var(--text-secondary)">{{ p.size }}</td>
                  <td class="py-2 px-2 text-right font-mono" style="color: var(--text-secondary)">{{ formatPrice(p.avg_px) }}</td>
                  <td class="py-2 px-2 text-right font-mono" style="color: var(--text-secondary)">{{ formatPrice(p.mark_px) }}</td>
                  <td class="py-2 px-2 text-right font-mono" style="color: var(--gold)">{{ p.lever }}x</td>
                  <td class="py-2 px-2 text-right font-mono" :style="{ color: parseFloat(p.upl) >= 0 ? 'var(--profit)' : 'var(--loss)' }">
                    {{ formatPnl(p.upl) }}
                  </td>
                  <td class="py-2 px-2 text-right font-mono text-xs" :style="{ color: parseFloat(p.upl_ratio) >= 0 ? 'var(--profit)' : 'var(--loss)' }">
                    {{ formatPnlPercent(p.upl_ratio) }}
                  </td>
                  <td class="py-2 px-2 text-right font-mono text-xs" style="color: var(--text-secondary)">{{ p.liq_px ? formatPrice(p.liq_px) : '-' }}</td>
                  <td class="py-2 px-2 text-right">
                    <button @click="closePosition(p.inst_id, p.side, p.size)"
                      class="text-xs px-2 py-1 rounded transition-colors"
                      style="background: rgba(255,23,68,0.15); color: var(--loss)">
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
          <h2 class="text-lg font-semibold mb-4" style="color: var(--text-primary)">当前委托</h2>
          <div v-if="orders.length === 0" class="py-8 text-center" style="color: var(--text-muted)">暂无委托</div>
          <div v-else class="overflow-x-auto">
            <table class="w-full text-sm">
              <thead>
                <tr class="text-xs uppercase" style="color: var(--text-muted)">
                  <th class="text-left py-2 px-2 font-medium">交易对</th>
                  <th class="text-left py-2 px-2 font-medium">方向</th>
                  <th class="text-left py-2 px-2 font-medium">类型</th>
                  <th class="text-right py-2 px-2 font-medium">数量</th>
                  <th class="text-right py-2 px-2 font-medium">价格</th>
                  <th class="text-right py-2 px-2 font-medium">操作</th>
                </tr>
              </thead>
              <tbody>
                <tr v-for="o in orders" :key="o.ordId || o.id" class="border-t" style="border-color: var(--border)">
                  <td class="py-2 px-2 font-medium" style="color: var(--text-primary)">{{ formatSymbol(o.instId || '') }}</td>
                  <td class="py-2 px-2">
                    <span class="text-xs px-2 py-0.5 rounded-full font-medium"
                      :style="o.side === 'buy' ? 'background: rgba(0,200,83,0.15); color: var(--profit)' : 'background: rgba(255,23,68,0.15); color: var(--loss)'">
                      {{ o.side === 'buy' ? '买入' : '卖出' }}
                    </span>
                  </td>
                  <td class="py-2 px-2 text-xs" style="color: var(--text-secondary)">{{ o.ordType === 'limit' ? '限价' : '市价' }}</td>
                  <td class="py-2 px-2 text-right font-mono" style="color: var(--text-secondary)">{{ o.sz }}</td>
                  <td class="py-2 px-2 text-right font-mono" style="color: var(--text-secondary)">{{ o.px ? formatPrice(o.px) : '-' }}</td>
                  <td class="py-2 px-2 text-right">
                    <button @click="cancelOrder(o.instId, o.ordId)" class="p-1 rounded hover:bg-[#222839]" style="color: var(--loss)">
                      <X class="w-4 h-4" />
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

<script setup lang="ts">
import { ref, onMounted } from 'vue'
import api from '@/api'
import { BookOpen, DollarSign } from 'lucide-vue-next'

const account = ref({ balance: 0, initial_capital: 0, total_pnl: 0 })
const positions = ref<any[]>([])
const trades = ref<any[]>([])
const loading = ref(true)
const submitting = ref(false)

const form = ref({ symbol: 'BTCUSDT', side: 'long' as 'long' | 'short', quantity: 0.01, price: 0, leverage: 10 })

async function loadData() {
  try {
    const [accRes, posRes, tradeRes] = await Promise.all([
      api.get('/papers/account'), api.get('/papers/positions'), api.get('/papers/trades'),
    ])
    account.value = accRes.data.account || accRes.data || account.value
    positions.value = posRes.data.items || posRes.data.positions || posRes.data || []
    trades.value = tradeRes.data.items || tradeRes.data.trades || tradeRes.data || []
  } catch (e) {
    console.error('Failed to load paper trading data', e)
  } finally {
    loading.value = false
  }
}

async function submitOrder() {
  submitting.value = true
  try {
    await api.post('/papers/orders', form.value)
    await loadData()
  } catch (e) {
    console.error('Paper order failed', e)
  } finally {
    submitting.value = false
  }
}

onMounted(() => { loadData() })
</script>

<template>
  <div class="space-y-6">
    <div class="flex items-center gap-3">
      <BookOpen class="w-6 h-6" style="color: var(--gold)" />
      <h1 class="font-display text-2xl font-bold" style="color: var(--text-primary)">模拟交易</h1>
    </div>

    <div v-if="loading" class="grid grid-cols-3 gap-4">
      <div v-for="i in 3" :key="i" class="card animate-pulse h-24"></div>
    </div>

    <template v-else>
      <div class="grid grid-cols-3 gap-4">
        <div class="card">
          <div class="flex items-center gap-2 mb-2"><DollarSign class="w-4 h-4" style="color: var(--gold)" /><span class="text-sm" style="color: var(--text-secondary)">余额</span></div>
          <div class="stat-value" style="color: var(--text-primary)">${{ account.balance?.toLocaleString('en-US', { minimumFractionDigits: 2 }) }}</div>
        </div>
        <div class="card">
          <div class="text-sm mb-2" style="color: var(--text-secondary)">初始资金</div>
          <div class="stat-value" style="color: var(--text-secondary)">${{ account.initial_capital?.toLocaleString('en-US', { minimumFractionDigits: 2 }) }}</div>
        </div>
        <div class="card">
          <div class="text-sm mb-2" style="color: var(--text-secondary)">总盈亏</div>
          <div class="stat-value" :style="{ color: account.total_pnl >= 0 ? 'var(--profit)' : 'var(--loss)' }">
            ${{ account.total_pnl?.toLocaleString('en-US', { minimumFractionDigits: 2 }) }}
          </div>
        </div>
      </div>

      <div class="grid grid-cols-3 gap-6">
        <div class="card space-y-3">
          <h2 class="text-lg font-semibold" style="color: var(--text-primary)">模拟下单</h2>
          <div>
            <label class="text-sm mb-1 block" style="color: var(--text-secondary)">交易对</label>
            <input v-model="form.symbol" class="input-field" />
          </div>
          <div class="grid grid-cols-2 gap-2">
            <button @click="form.side = 'long'" class="py-2 rounded-lg text-sm font-medium"
              :style="form.side === 'long' ? 'background: var(--profit); color: #000' : 'background: var(--border); color: var(--text-secondary)'">做多</button>
            <button @click="form.side = 'short'" class="py-2 rounded-lg text-sm font-medium"
              :style="form.side === 'short' ? 'background: var(--loss); color: #fff' : 'background: var(--border); color: var(--text-secondary)'">做空</button>
          </div>
          <div class="grid grid-cols-2 gap-2">
            <div>
              <label class="text-sm mb-1 block" style="color: var(--text-secondary)">数量</label>
              <input v-model.number="form.quantity" type="number" step="0.001" class="input-field" />
            </div>
            <div>
              <label class="text-sm mb-1 block" style="color: var(--text-secondary)">价格</label>
              <input v-model.number="form.price" type="number" class="input-field" />
            </div>
          </div>
          <div>
            <label class="text-sm mb-1 flex justify-between" style="color: var(--text-secondary)">
              <span>杠杆</span><span class="font-mono" style="color: var(--gold)">{{ form.leverage }}x</span>
            </label>
            <input v-model.number="form.leverage" type="range" min="1" max="125" class="w-full accent-[#D4A843]" />
          </div>
          <button @click="submitOrder" class="btn-primary w-full" :disabled="submitting">{{ submitting ? '提交中...' : '下单' }}</button>
        </div>

        <div class="col-span-2 space-y-6">
          <div class="card">
            <h2 class="text-lg font-semibold mb-4" style="color: var(--text-primary)">持仓</h2>
            <div v-if="positions.length === 0" class="py-8 text-center" style="color: var(--text-muted)">暂无持仓</div>
            <table v-else class="w-full">
              <thead>
                <tr class="text-xs uppercase" style="color: var(--text-muted)">
                  <th class="text-left py-2 font-medium">交易对</th>
                  <th class="text-left py-2 font-medium">方向</th>
                  <th class="text-right py-2 font-medium">数量</th>
                  <th class="text-right py-2 font-medium">盈亏</th>
                </tr>
              </thead>
              <tbody>
                <tr v-for="p in positions" :key="p.symbol" class="border-t" style="border-color: var(--border)">
                  <td class="py-2 font-medium" style="color: var(--text-primary)">{{ p.symbol }}</td>
                  <td><span class="badge" :class="p.side === 'long' ? 'badge-profit' : 'badge-loss'">{{ p.side === 'long' ? '多' : '空' }}</span></td>
                  <td class="py-2 text-right font-mono" style="color: var(--text-secondary)">{{ p.size }}</td>
                  <td class="py-2 text-right font-mono" :style="{ color: p.pnl >= 0 ? 'var(--profit)' : 'var(--loss)' }">{{ p.pnl }}</td>
                </tr>
              </tbody>
            </table>
          </div>

          <div class="card">
            <h2 class="text-lg font-semibold mb-4" style="color: var(--text-primary)">交易历史</h2>
            <div v-if="trades.length === 0" class="py-8 text-center" style="color: var(--text-muted)">暂无交易</div>
            <div v-else class="space-y-2">
              <div v-for="t in trades" :key="t.id" class="flex items-center justify-between p-2 rounded" style="background: var(--bg-primary)">
                <div class="flex items-center gap-2">
                  <span class="font-medium text-sm" style="color: var(--text-primary)">{{ t.symbol }}</span>
                  <span class="badge" :class="t.side === 'long' ? 'badge-profit' : 'badge-loss'">{{ t.side === 'long' ? '多' : '空' }}</span>
                </div>
                <span class="font-mono text-sm" :style="{ color: t.pnl >= 0 ? 'var(--profit)' : 'var(--loss)' }">{{ t.pnl }}</span>
              </div>
            </div>
          </div>
        </div>
      </div>
    </template>
  </div>
</template>

<script setup lang="ts">
import { computed, onBeforeUnmount, onMounted, ref } from 'vue'
import { Activity, AlertTriangle, RefreshCw } from 'lucide-vue-next'
import { getLlfStatus, type LlfStatus } from '@/api/llf'

const report = ref<LlfStatus | null>(null)
const loading = ref(false)
const error = ref('')
const selectedAsset = ref('')
const selected = computed(() => report.value?.assets.find(row => row.asset === selectedAsset.value))
const paper = computed(() => report.value?.paper)
const selectedPosition = ref('')
const tradeDetail = computed(() => paper.value?.trades.find(trade => trade.position_id === selectedPosition.value))
const qualityLabels = { waiting: '等待采集', ready: '数据就绪', incomplete: '数据不全', stale: '数据过期' }
const inputLabels = { fresh: '新鲜', stale: '过期', invalid: '无效', missing: '缺失' }
const decisions: Record<string, string> = {
  pending: '等待成交', skipped: '未执行', partial: '部分成交', filled: '全部成交',
  p90: '分位达到 P90', below_p70: '分位低于 P70', time_limit: '持仓满 15 分钟',
  observe_only: '观察模式', data_unready: '数据未就绪', already_open: '已有持仓',
  configuration_missing: '缺少交易参数', signal_pending: '已有待处理信号', insufficient_margin: '可用保证金不足',
  invalid_book: '盘口无效', below_minimum: '低于最小成交量', no_depth: '无可成交深度', book_timeout: '等待有效盘口超时',
}
const components = [
  ['oi_load', 'OI 负载'], ['carry', '持有成本'], ['flow', '成交流'], ['liquidity', '流动性'],
] as const
const abort = new AbortController()
let timer: ReturnType<typeof setInterval>

async function refresh() {
  if (loading.value) return
  loading.value = true
  try {
    report.value = await getLlfStatus(abort.signal)
    error.value = ''
  } catch (cause) {
    if (!abort.signal.aborted) error.value = cause instanceof Error ? cause.message : '请求失败'
  } finally {
    loading.value = false
  }
}

onMounted(() => {
  refresh()
  timer = setInterval(refresh, 5000)
})
onBeforeUnmount(() => {
  clearInterval(timer)
  abort.abort()
})

function value(n: number | null | undefined, places = 2) {
  if (n == null || !Number.isFinite(Number(n))) return '—'
  return Number(n).toLocaleString('zh-CN', { maximumFractionDigits: places })
}

function percent(n: number | null) {
  return n == null ? '—' : value(n) + '%'
}

function percentile(n: number | null) {
  return n == null ? '—' : 'P' + value(n, 1)
}

function time(iso: string | null | undefined) {
  if (!iso) return '—'
  return new Date(iso).toLocaleString('zh-CN', { hour12: false })
}

function holding(seconds: number | null) {
  if (seconds == null) return '—'
  const rounded = Math.round(seconds)
  return Math.floor(rounded / 60) + ' 分 ' + (rounded % 60) + ' 秒'
}

const totals = computed(() => {
  if (!paper.value) return []
  const { metrics } = paper.value
  return [
    ['账户净值 · USDT', value(paper.value.equity)],
    ['净收益 · USDT', value(metrics.net_pnl)],
    ['累计开仓', value(metrics.entries)],
    ['已平仓胜率', percent(metrics.win_rate)],
    ['最大回撤', percent(metrics.max_drawdown_pct)],
    ['平均持仓', holding(metrics.avg_holding_seconds)],
  ]
})

function select(asset: string) {
  selectedAsset.value = selectedAsset.value === asset ? '' : asset
}

function tradingState(asset: string) {
  if (paper.value?.positions.some(position => position.asset === asset)) return '空头持仓'
  return paper.value?.enabled && report.value?.formula_ready ? '空仓' : '观察'
}
</script>

<template>
  <main class="space-y-5 animate-fade-in">
    <header class="flex flex-wrap items-start justify-between gap-3">
      <div>
        <div class="flex flex-wrap items-center gap-3">
          <h1 class="text-2xl font-bold">LLF 实时研究</h1>
          <span class="badge badge-primary">Paper</span>
        </div>
        <p class="muted text-sm mt-1">OKX · BTC、ETH、SOL · 3 个现货 / 3 个 USDT 永续</p>
      </div>
      <div class="flex flex-wrap items-center gap-3">
        <span v-if="report" class="muted text-xs">更新于 {{ time(report.as_of) }}</span>
        <button class="btn btn-secondary" :disabled="loading" @click="refresh">
          <RefreshCw class="w-4 h-4" :class="{ 'animate-spin': loading }" />刷新
        </button>
      </div>
    </header>

    <div v-if="error" role="alert" class="notice">
      <AlertTriangle class="w-4 h-4 shrink-0" />
      <span>研究数据获取失败。{{ error }}<template v-if="report"> · 当前显示上次快照</template></span>
    </div>
    <p v-if="!report && !error" role="status" class="card p-6 muted">正在读取研究快照…</p>

    <template v-if="report">
      <div v-if="!report.collector_enabled || !report.formula_ready" class="notice">
        <AlertTriangle class="w-4 h-4 shrink-0" />
        <div>
          <p v-if="!report.collector_enabled">采集开关已关闭，行情快照不会更新。</p>
          <p v-if="!report.formula_ready">{{ report.factor_version }} {{ report.formula_status === 'unconfirmed' ? '公式待确认' : '公式尚未就绪' }}。四项分量、分数和历史分位均待计算。</p>
        </div>
      </div>

      <section class="card overflow-hidden">
        <div class="section-heading">
          <h2><Activity class="w-4 h-4" />市场脆弱度</h2>
          <span class="badge" :class="report.collector_enabled ? 'badge-info' : 'badge-neutral'">
            {{ report.collector_enabled ? '采集已启用' : '采集已关闭' }}
          </span>
        </div>
        <div class="overflow-x-auto">
          <table class="table">
            <thead><tr><th>资产</th><th>LLF 分数</th><th>历史分位</th><th>数据质量</th><th>OI 有样本天数</th><th>模拟状态</th><th>快照时间</th><th>分量与输入</th></tr></thead>
            <tbody>
              <tr v-for="row in report.assets" :key="row.asset">
                <td class="font-semibold">{{ row.asset }}</td>
                <td>{{ value(row.score, 4) }}</td><td>{{ percentile(row.percentile) }}</td>
                <td>
                  <span class="badge" :class="row.quality === 'ready' ? 'badge-profit' : 'badge-warning'">{{ qualityLabels[row.quality] || row.quality }}</span>
                  <div v-if="row.reason" class="muted text-xs mt-1">{{ row.reason }}</div>
                </td>
                <td>{{ value(row.oi_history.sample_days) }} / 30</td><td>{{ tradingState(row.asset) }}</td><td>{{ time(row.observed_at) }}</td>
                <td><button class="detail-btn" :aria-expanded="selectedAsset === row.asset" @click="select(row.asset)">{{ selectedAsset === row.asset ? '收起' : '查看' }}</button></td>
              </tr>
              <tr v-if="!report.assets.length"><td colspan="8" class="empty">尚无资产快照</td></tr>
            </tbody>
          </table>
        </div>
        <div v-if="selected" class="input-detail">
          <h3 class="font-semibold text-sm">{{ selected.asset }} · {{ report.factor_version }} 输入快照</h3>
          <dl class="component-grid">
            <div v-for="[key, label] in components" :key="key">
              <dt>{{ label }} <code>{{ key }}</code></dt><dd>{{ value(selected.components[key], 4) }}</dd>
            </div>
          </dl>
          <p class="muted text-xs my-2">OI 最早样本 {{ time(selected.oi_history.first_at) }}　最近样本 {{ time(selected.oi_history.last_at) }}</p>
          <div v-for="(channels, symbol) in selected.inputs" :key="symbol" class="overflow-x-auto mt-3">
            <table class="table">
              <caption class="text-left font-semibold text-sm">{{ symbol }}</caption>
              <thead><tr><th>行情频道</th><th>新鲜度</th><th>快照时数据年龄</th><th>交易所时间</th><th>本地接收时间</th><th>原始记录</th></tr></thead>
              <tbody><tr v-for="(input, channel) in channels" :key="channel">
                <td>{{ channel }}</td><td>{{ inputLabels[input.status] || input.status }}</td>
                <td>{{ input.age_ms == null ? '—' : value(input.age_ms) + ' ms' }}</td>
                <td>{{ time(input.exchange_ts) }}</td><td>{{ time(input.received_at) }}</td>
                <td><details v-if="input.raw_id != null"><summary>#{{ input.raw_id }}</summary><pre>{{ JSON.stringify(input.values, null, 2) }}</pre></details><span v-else>—</span></td>
              </tr></tbody>
            </table>
          </div>
          <p v-if="!Object.keys(selected.inputs).length" class="muted text-sm">等待行情输入</p>
        </div>
      </section>

      <template v-if="paper">
        <section class="card overflow-hidden">
          <div class="section-heading">
            <h2>模拟账户 <span class="muted text-sm font-normal">{{ paper.account_id }}</span></h2>
            <span class="badge" :class="paper.enabled ? 'badge-info' : 'badge-neutral'">{{ paper.enabled ? '模拟规则已启用' : '候选规则关闭 · 观察模式' }}</span>
          </div>
          <p class="muted text-xs px-5 pb-4">LLF-Paper-001 · P90 开空，低于 P70 平仓，最长持仓 15 分钟 · 1 倍保证金</p>
          <dl class="metric-grid">
            <div v-for="[label, amount] in totals" :key="label"><dt>{{ label }}</dt><dd>{{ amount }}</dd></div>
          </dl>
          <dl class="details-grid">
            <div><dt>初始权益 · USDT</dt><dd>{{ value(paper.initial_equity) }}</dd></div>
            <div><dt>账户现金 · USDT</dt><dd>{{ value(paper.cash) }}</dd></div>
            <div><dt>占用保证金 · USDT</dt><dd>{{ value(paper.margin_used) }}</dd></div>
            <div><dt>单笔模拟金额 · USDT</dt><dd>{{ value(paper.order_usdt) }}</dd></div>
            <div><dt>Taker 费率 · bps</dt><dd>{{ value(paper.fee_bps, 4) }}</dd></div>
            <div><dt>累计手续费 · USDT</dt><dd>{{ value(paper.metrics.total_fees, 4) }}</dd></div>
            <div><dt>成交滑点 · USDT</dt><dd>{{ value(paper.metrics.slippage_cost, 4) }}</dd></div>
            <div><dt>资金费净额 · USDT</dt><dd>{{ value(paper.metrics.funding, 4) }}</dd></div>
            <div><dt>最大不利变动 · USDT</dt><dd>{{ value(paper.metrics.max_adverse_usdt, 4) }}</dd></div>
            <div><dt>命中清算事件的持仓</dt><dd>{{ value(paper.metrics.liquidation_hits) }}</dd></div>
            <div><dt>已平仓交易</dt><dd>{{ value(paper.metrics.closed_trades) }}</dd></div>
          </dl>
        </section>

        <section class="card overflow-hidden">
          <div class="section-heading"><h2>最近模拟交易</h2><span class="muted text-xs">金额单位 USDT</span></div>
          <div v-if="paper.trades.length" class="overflow-x-auto">
            <table class="table">
              <thead><tr><th>资产</th><th>开仓 / 平仓时间</th><th>数量</th><th>开仓均价</th><th>净收益</th><th>手续费</th><th>滑点</th><th>资金费</th><th>最大不利变动</th><th>清算命中</th><th>归因与凭证</th></tr></thead>
              <tbody><tr v-for="trade in paper.trades" :key="trade.position_id">
                <td class="font-semibold">{{ trade.asset }}</td>
                <td>{{ time(trade.opened_at) }}<div class="muted text-xs">{{ trade.closed_at ? time(trade.closed_at) : '持仓中' }}</div></td>
                <td>{{ value(trade.quantity, 8) }}</td><td>{{ value(trade.entry_price, 4) }}</td>
                <td>{{ value(trade.net_pnl, 4) }}</td><td>{{ value(trade.fees, 4) }}</td><td>{{ value(trade.slippage_cost, 4) }}</td>
                <td>{{ value(trade.funding, 4) }}</td><td>{{ value(trade.mae_usdt, 4) }}</td>
                <td>{{ trade.liquidation_hit == null ? '—' : trade.liquidation_hit ? '是' : '否' }}</td>
                <td><button class="detail-btn" :aria-expanded="selectedPosition === trade.position_id" @click="selectedPosition = selectedPosition === trade.position_id ? '' : trade.position_id">{{ selectedPosition === trade.position_id ? '收起' : '查看' }}</button></td>
              </tr></tbody>
            </table>
          </div>
          <p v-else class="empty">此账户尚无模拟交易</p>
          <div v-if="tradeDetail" class="input-detail">
            <h3 class="font-semibold text-sm">{{ tradeDetail.asset }} · {{ tradeDetail.position_id }}</h3>
            <dl class="component-grid">
              <div><dt>首次观察到清算</dt><dd>{{ time(tradeDetail.liquidation_at) }}</dd></div>
              <div><dt>清算后 15 分钟价格变化</dt><dd>{{ percent(tradeDetail.post_liquidation_return_pct) }}</dd></div>
              <div><dt>清算后 15 分钟最大回撤</dt><dd>{{ percent(tradeDetail.post_liquidation_drawdown_pct) }}</dd></div>
              <div><dt>待入账资金费笔数</dt><dd>{{ value(tradeDetail.pending_funding_settlements) }}</dd></div>
            </dl>
            <details v-if="tradeDetail.fills?.length"><summary>成交与账本凭证 · {{ report.factor_version }}</summary><pre>{{ JSON.stringify(tradeDetail.fills, null, 2) }}</pre></details>
          </div>
        </section>

        <section class="card overflow-hidden">
          <div class="section-heading"><h2>高 LLF 事件</h2><span class="muted text-xs">含未开仓信号</span></div>
          <div v-if="paper.events.length" class="overflow-x-auto">
            <table class="table">
              <thead><tr><th>信号时间</th><th>资产</th><th>LLF 分位</th><th>处理结果</th><th>决策原因</th><th>标记价 · USDT</th><th>15 分钟价格变化</th></tr></thead>
              <tbody><tr v-for="event in paper.events" :key="event.signal_id">
                <td>{{ time(event.observed_at) }}</td><td class="font-semibold">{{ event.asset }}</td><td>{{ percentile(event.percentile) }}</td>
                <td>{{ decisions[event.status] || event.status }}</td><td>{{ decisions[event.reason] || event.reason }}</td><td>{{ value(event.entry_mark, 4) }}</td><td>{{ percent(event.return_15m_pct) }}</td>
              </tr></tbody>
            </table>
          </div>
          <p v-else class="empty">尚无达到 P90 的独立事件</p>
        </section>
      </template>
      <p v-else class="card p-6 muted">模拟账户数据暂不可用</p>
    </template>
  </main>
</template>

<style scoped>
.muted, dt, .empty { color: var(--text-secondary); }
.notice { display: flex; align-items: center; gap: .75rem; background: var(--warning-light); color: var(--text-primary); border-radius: var(--radius-md); padding: .75rem 1rem; font-size: .875rem; }
.section-heading { display: flex; align-items: center; justify-content: space-between; flex-wrap: wrap; gap: .5rem; padding: 1rem 1.25rem; }
.section-heading h2 { display: flex; align-items: center; gap: .5rem; font-size: 1rem; font-weight: 600; }
.table th, .table td { white-space: nowrap; }
.empty { padding: 1.25rem; text-align: center; font-size: .875rem; }
.metric-grid, .details-grid { display: grid; gap: 1.25rem; grid-template-columns: repeat(auto-fit, minmax(135px, 1fr)); padding: 1rem 1.25rem; border-top: 1px solid var(--border); }
.metric-grid dt, .details-grid dt { font-size: .75rem; }
.metric-grid dd, .details-grid dd { font-variant-numeric: tabular-nums; font-weight: 600; }
.metric-grid dd { font-size: 1.35rem; }
.details-grid dd { font-size: .9rem; }
.detail-btn { color: var(--primary); }
summary { cursor: pointer; color: var(--primary); font-size: .875rem; }
.input-detail { background: var(--surface-secondary); padding: 1rem 1.25rem; border-top: 1px solid var(--border); }
.input-detail pre { max-height: 20rem; overflow: auto; font-family: var(--font-mono); font-size: .75rem; }
.component-grid { display: grid; gap: .75rem; grid-template-columns: repeat(auto-fit, minmax(120px, 1fr)); margin: .75rem 0; font-size: .875rem; }
.component-grid code { font-size: .75rem; }
</style>

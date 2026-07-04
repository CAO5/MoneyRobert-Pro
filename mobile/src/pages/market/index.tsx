import React, { useCallback, useEffect, useMemo, useRef, useState } from 'react';
import { View, Text, ScrollView } from '@tarojs/components';
import Taro, { usePullDownRefresh, useRouter } from '@tarojs/taro';
import { Empty, Skeleton, Tag } from '@nutui/nutui-react-taro';
import { marketService } from '@/services/market';
import { tradingService } from '@/services/trading';
import { useAuthStore } from '@/store/auth';
import { useI18n, useLocalizedTitle } from '@/store/language';
import type { MarketRegime, MarketSnapshot } from '@/types/market';
import type {
  AccountBalance,
  Order,
  OrderSide,
  OrderStatus,
  OrderType,
  Position,
  PositionSide,
  ProfitCurvePoint,
  Trade,
} from '@/types/trading';
import styles from './index.module.scss';

type TabKey = 'market' | 'position' | 'order' | 'trade';
type TradeMode = 'paper' | 'live';
type ProfitRange = 'week' | 'month';

// 纯文案映射（统一经过 t() 以支持多语言）
const SIDE_LABELS: Record<PositionSide, { label: string; type: 'primary' | 'danger' | 'default' }> = {
  long: { label: '多', type: 'primary' },
  short: { label: '空', type: 'danger' },
  net: { label: '净', type: 'default' },
};

const ORDER_SIDE_LABELS: Record<OrderSide, { label: string; type: 'primary' | 'danger' }> = {
  buy: { label: '买', type: 'primary' },
  sell: { label: '卖', type: 'danger' },
};

const ORDER_TYPE_LABELS: Record<OrderType, string> = {
  market: '市价',
  limit: '限价',
  stop: '止损',
  stop_limit: '止损限价',
};

const ORDER_STATUS_META: Record<
  OrderStatus,
  { label: string; type: 'default' | 'primary' | 'success' | 'warning' | 'danger' }
> = {
  pending: { label: '待成交', type: 'warning' },
  open: { label: '挂单中', type: 'primary' },
  partial: { label: '部分成交', type: 'primary' },
  filled: { label: '已成交', type: 'success' },
  cancelled: { label: '已撤单', type: 'default' },
  rejected: { label: '已拒绝', type: 'danger' },
};

// 市场状态 → 情绪分值（0=极度恐惧，100=极度贪婪）
const REGIME_VALUE: Record<string, number> = {
  trending_up: 82,
  ranging: 50,
  volatile: 38,
  trending_down: 18,
};

function computeSentiment(regimes: MarketRegime[]) {
  if (!regimes.length) return { score: 50, label: '中性', tone: 'neutral' as 'neutral' | 'fear' | 'greed' };
  let sum = 0;
  let weight = 0;
  regimes.forEach((r) => {
    const v = REGIME_VALUE[r.regime] ?? 50;
    const w = Math.max(0.1, r.confidence ?? 0.5);
    sum += v * w;
    weight += w;
  });
  const score = Math.round(weight ? sum / weight : 50);
  let label = '中性';
  let tone: 'neutral' | 'fear' | 'greed' = 'neutral';
  if (score < 25) {
    label = '极度恐惧';
    tone = 'fear';
  } else if (score < 45) {
    label = '恐惧';
    tone = 'fear';
  } else if (score < 55) {
    label = '中性';
    tone = 'neutral';
  } else if (score < 75) {
    label = '贪婪';
    tone = 'greed';
  } else {
    label = '极度贪婪';
    tone = 'greed';
  }
  return { score, label, tone };
}

// 数字格式化：价格 4 位小数，金额 toLocaleString
const fmtPrice = (n: number | undefined | null): string => {
  if (n === undefined || n === null || !Number.isFinite(n)) return '--';
  return n.toFixed(4);
};
const fmtMoney = (n: number | undefined | null): string => {
  if (n === undefined || n === null || !Number.isFinite(n)) return '--';
  return n.toLocaleString();
};
const fmtPct = (n: number | undefined | null): string => {
  if (n === undefined || n === null || !Number.isFinite(n)) return '--';
  return n.toFixed(2);
};
const fmtTime = (s?: string): string => {
  if (!s) return '--';
  const d = new Date(s);
  if (Number.isNaN(d.getTime())) return s;
  const pad = (v: number) => String(v).padStart(2, '0');
  return `${d.getMonth() + 1}/${pad(d.getDate())} ${pad(d.getHours())}:${pad(d.getMinutes())}`;
};

/**
 * 行情 / 持仓页（tabBar：pages/market/index）
 * 内部 Tab：行情 / 持仓 / 订单 / 成交
 * 数据来自 tradingService 与 marketService 真实后端接口
 */
const MarketPage: React.FC = () => {
  const { t } = useI18n();
  useLocalizedTitle('行情');
  const isAuthenticated = useAuthStore((s) => s.isAuthenticated);
  const isLoadingAuth = useAuthStore((s) => s.isLoading);
  const router = useRouter();

  const [activeTab, setActiveTab] = useState<TabKey>('market');
  const [tradeMode, setTradeMode] = useState<TradeMode>('paper');
  const [profitRange, setProfitRange] = useState<ProfitRange>('week');

  const [markets, setMarkets] = useState<MarketSnapshot[]>([]);
  const [regimes, setRegimes] = useState<MarketRegime[]>([]);
  const [positions, setPositions] = useState<Position[]>([]);
  const [balance, setBalance] = useState<AccountBalance | null>(null);
  const [orders, setOrders] = useState<Order[]>([]);
  const [trades, setTrades] = useState<Trade[]>([]);
  const [profitCurve, setProfitCurve] = useState<ProfitCurvePoint[]>([]);

  const [loading, setLoading] = useState(false);
  const loadedRef = useRef<Set<TabKey>>(new Set());
  const profitRangeRef = useRef<ProfitRange>('week');
  useEffect(() => {
    profitRangeRef.current = profitRange;
  }, [profitRange]);

  // 读取 ?tab= 入参
  useEffect(() => {
    const tab = router.params.tab as TabKey;
    if (tab && ['market', 'position', 'order', 'trade'].includes(tab)) {
      setActiveTab(tab);
    }
  }, [router.params.tab]);

  const loadTab = useCallback(
    async (tab: TabKey, force = false) => {
      if (!force && loadedRef.current.has(tab)) return;
      setLoading(true);
      try {
        if (tab === 'market') {
          const [m, r] = await Promise.all([
            marketService.getSnapshots(),
            marketService.getRegimes(),
          ]);
          setMarkets(m || []);
          setRegimes(r || []);
        } else if (tab === 'position') {
          const [p, b, c] = await Promise.all([
            tradingService.getPositions(),
            tradingService.getBalance(),
            tradingService.getProfitTrend(profitRangeRef.current),
          ]);
          setPositions(p || []);
          setBalance(b);
          setProfitCurve(c || []);
        } else if (tab === 'order') {
          const res = await tradingService.getOrders({ limit: 20 });
          setOrders(res?.orders || []);
        } else if (tab === 'trade') {
          const tr = await tradingService.getTrades({ limit: 20 });
          setTrades(tr || []);
        }
        loadedRef.current.add(tab);
      } catch {
        Taro.showToast({ title: t('数据开小差了，请稍后重试'), icon: 'none' });
      } finally {
        setLoading(false);
      }
    },
    [t],
  );

  // 鉴权通过后按需加载当前 Tab
  useEffect(() => {
    if (!isAuthenticated) return;
    loadTab(activeTab);
  }, [isAuthenticated, activeTab, loadTab]);

  // 切换收益曲线区间（仅持仓 Tab 已加载时刷新曲线）
  useEffect(() => {
    if (!isAuthenticated || activeTab !== 'position' || !loadedRef.current.has('position')) {
      return;
    }
    tradingService
      .getProfitTrend(profitRange)
      .then((c) => setProfitCurve(c || []))
      .catch(() => undefined);
  }, [profitRange, isAuthenticated, activeTab]);

  usePullDownRefresh(async () => {
    if (!isAuthenticated) {
      Taro.stopPullDownRefresh();
      return;
    }
    loadedRef.current = new Set();
    await loadTab(activeTab, true);
    Taro.stopPullDownRefresh();
  });

  // 跳转
  const goSymbol = (symbol: string) => {
    Taro.navigateTo({
      url: `/pages/symbol-detail/index?symbol=${encodeURIComponent(symbol)}`,
    }).catch(() => undefined);
  };
  const goPosition = (symbol: string) => {
    Taro.navigateTo({
      url: `/pages/position-detail/index?symbol=${encodeURIComponent(symbol)}`,
    }).catch(() => undefined);
  };
  const goOrder = (orderId: string) => {
    Taro.navigateTo({
      url: `/pages/order-detail/index?id=${encodeURIComponent(orderId)}`,
    }).catch(() => undefined);
  };

  const sentiment = useMemo(() => computeSentiment(regimes), [regimes]);

  // 收益曲线高度（基于 equity 归一化，0~100）
  const curveBars = useMemo(() => {
    if (profitCurve.length < 2) return [];
    const vals = profitCurve.map((p) => p.equity);
    const min = Math.min(...vals);
    const max = Math.max(...vals);
    const range = max - min || 1;
    return vals.map((v) => Math.round(((v - min) / range) * 100));
  }, [profitCurve]);

  const totalUnrealizedPnl = balance?.unrealized_pnl ?? 0;
  const pnlClass =
    totalUnrealizedPnl > 0 ? styles.up : totalUnrealizedPnl < 0 ? styles.down : styles.flat;

  const tabs: Array<{ key: TabKey; label: string }> = [
    { key: 'market', label: t('行情') },
    { key: 'position', label: t('持仓') },
    { key: 'order', label: t('订单') },
    { key: 'trade', label: t('成交') },
  ];

  const isTabEmpty =
    (activeTab === 'market' && markets.length === 0) ||
    (activeTab === 'position' && positions.length === 0 && !balance) ||
    (activeTab === 'order' && orders.length === 0) ||
    (activeTab === 'trade' && trades.length === 0);

  const toggleMode = () => setTradeMode((m) => (m === 'paper' ? 'live' : 'paper'));

  // 未登录：引导登录
  if (!isAuthenticated && !isLoadingAuth) {
    return (
      <View className={styles.page}>
        <View className={styles.loginEmpty}>
          <Empty
            title={t('未登录')}
            description={t('未登录用户可访问"我的"查看本地信息，其他操作需登录后进行')}
            actions={[
              {
                text: t('去登录'),
                type: 'primary',
                onClick: () => Taro.reLaunch({ url: '/pages/login/index' }),
              },
            ]}
          />
        </View>
      </View>
    );
  }

  const renderMarket = () => {
    if (loading && markets.length === 0) return null;
    if (markets.length === 0) {
      return <Empty title={t('暂无行情数据')} />;
    }
    return (
      <React.Fragment>
        <View className={styles.gaugeCard}>
          <View className={styles.gaugeHeader}>
            <Text className={styles.gaugeTitle}>{t('市场情绪')}</Text>
            <Tag
              type={sentiment.tone === 'greed' ? 'danger' : sentiment.tone === 'fear' ? 'primary' : 'default'}
              round
            >
              {t(sentiment.label)}
            </Tag>
          </View>
          <View className={styles.gaugeRow}>
            <Text className={styles.gaugeValue}>{sentiment.score}</Text>
            <View className={styles.gaugeTrack}>
              <View className={styles.gaugeFill} />
              <View
                className={styles.gaugeMarker}
                style={{ left: `${sentiment.score}%` }}
              />
            </View>
          </View>
          <View className={styles.gaugeLabels}>
            <Text>{t('恐惧')}</Text>
            <Text>{t('贪婪')}</Text>
          </View>
        </View>

        {markets.map((m) => {
          const change = m.price_change_percent_24h;
          const changeClass = change > 0 ? styles.up : change < 0 ? styles.down : styles.flat;
          return (
            <View
              key={m.symbol}
              className={styles.marketItem}
              onClick={() => goSymbol(m.symbol)}
            >
              <View className={styles.symbolInfo}>
                <Text className={styles.symbolName}>{m.symbol}</Text>
                <Text className={styles.symbolMeta}>
                  {t('24h 量 {value}', { value: fmtMoney(m.volume_24h) })}
                </Text>
              </View>
              <View className={styles.priceInfo}>
                <Text className={styles.price}>{fmtPrice(m.current_price)}</Text>
                <Text className={`${styles.changePercent} ${changeClass}`}>
                  {change > 0 ? '+' : ''}
                  {fmtPct(change)}%
                </Text>
              </View>
            </View>
          );
        })}
      </React.Fragment>
    );
  };

  const renderPosition = () => {
    if (loading && positions.length === 0 && !balance) return null;
    if (positions.length === 0 && !balance) {
      return <Empty title={t('暂无持仓')} />;
    }
    return (
      <React.Fragment>
        <View className={styles.summaryCard}>
          <View className={styles.summaryTop}>
            <View>
              <Text className={styles.summaryLabel}>{t('总权益')}</Text>
              <Text className={styles.summaryValue}>
                {balance ? fmtMoney(balance.total_equity) : '--'}
              </Text>
            </View>
            <View className={styles.summaryPnl}>
              <Text className={styles.summaryLabel}>{t('未实现盈亏')}</Text>
              <Text className={`${styles.summaryValue} ${pnlClass}`}>
                {totalUnrealizedPnl > 0 ? '+' : ''}
                {fmtMoney(totalUnrealizedPnl)}
              </Text>
            </View>
          </View>
          <View className={styles.curveWrap}>
            <View className={styles.miniChart}>
              {curveBars.length > 0 ? (
                curveBars.map((h, i) => (
                  <View
                    key={i}
                    className={styles.chartBar}
                    style={{ height: `${Math.max(8, h)}%` }}
                  />
                ))
              ) : (
                <Text className={styles.curveEmpty}>{t('暂无收益曲线')}</Text>
              )}
            </View>
          </View>
          <View className={styles.rangeTabs}>
            {(['week', 'month'] as ProfitRange[]).map((r) => (
              <View
                key={r}
                className={`${styles.rangeTab} ${profitRange === r ? styles.rangeActive : ''}`}
                onClick={() => setProfitRange(r)}
              >
                <Text>{t(r === 'week' ? '周' : '月')}</Text>
              </View>
            ))}
          </View>
        </View>

        {positions.length === 0 ? (
          <Empty title={t('暂无持仓')} />
        ) : (
          positions.map((p) => {
            const side = SIDE_LABELS[p.side] || SIDE_LABELS.net;
            const pnl = p.unrealized_pnl;
            const posPnlClass = pnl > 0 ? styles.up : pnl < 0 ? styles.down : styles.flat;
            return (
              <View
                key={`${p.symbol}-${p.side}`}
                className={styles.positionItem}
                onClick={() => goPosition(p.symbol)}
              >
                <View className={styles.posHeader}>
                  <View className={styles.posSymbolWrap}>
                    <Text className={styles.posSymbol}>{p.symbol}</Text>
                    <Tag type={side.type} round>
                      {t(side.label)}
                    </Tag>
                  </View>
                  <Tag type="default" round>
                    {p.leverage}x
                  </Tag>
                </View>
                <View className={styles.posStats}>
                  <View className={styles.posCell}>
                    <Text className={styles.cellLabel}>{t('持仓数量')}</Text>
                    <Text className={styles.cellValue}>{fmtMoney(p.size)}</Text>
                  </View>
                  <View className={styles.posCell}>
                    <Text className={styles.cellLabel}>{t('开仓均价')}</Text>
                    <Text className={styles.cellValue}>{fmtPrice(p.entry_price)}</Text>
                  </View>
                  <View className={styles.posCell}>
                    <Text className={styles.cellLabel}>{t('标记价格')}</Text>
                    <Text className={styles.cellValue}>{fmtPrice(p.mark_price)}</Text>
                  </View>
                  <View className={styles.posCell}>
                    <Text className={styles.cellLabel}>{t('未实现盈亏')}</Text>
                    <Text className={`${styles.cellValue} ${posPnlClass}`}>
                      {pnl > 0 ? '+' : ''}
                      {fmtMoney(pnl)} ({fmtPct(p.unrealized_pnl_pct)}%)
                    </Text>
                  </View>
                </View>
              </View>
            );
          })
        )}
      </React.Fragment>
    );
  };

  const renderOrder = () => {
    if (loading && orders.length === 0) return null;
    if (orders.length === 0) {
      return <Empty title={t('暂无订单')} />;
    }
    return orders.map((o) => {
      const side = ORDER_SIDE_LABELS[o.side];
      const status = ORDER_STATUS_META[o.status] || ORDER_STATUS_META.pending;
      return (
        <View
          key={o.order_id}
          className={styles.orderItem}
          onClick={() => goOrder(o.order_id)}
        >
          <View className={styles.orderHeader}>
            <View className={styles.posSymbolWrap}>
              <Text className={styles.orderSymbol}>{o.symbol}</Text>
              <Tag type={side.type} round>
                {t(side.label)}
              </Tag>
            </View>
            <Tag type={status.type} round>
              {t(status.label)}
            </Tag>
          </View>
          <View className={styles.posStats}>
            <View className={styles.posCell}>
              <Text className={styles.cellLabel}>{t('类型')}</Text>
              <Text className={styles.cellValue}>{t(ORDER_TYPE_LABELS[o.type])}</Text>
            </View>
            <View className={styles.posCell}>
              <Text className={styles.cellLabel}>{t('数量')}</Text>
              <Text className={styles.cellValue}>{fmtMoney(o.quantity)}</Text>
            </View>
            {o.price !== undefined && o.price !== null && (
              <View className={styles.posCell}>
                <Text className={styles.cellLabel}>{t('价格')}</Text>
                <Text className={styles.cellValue}>{fmtPrice(o.price)}</Text>
              </View>
            )}
          </View>
          <View className={styles.orderFooter}>
            <Text className={styles.orderTime}>{fmtTime(o.created_at)}</Text>
            {o.filled_quantity !== undefined && o.filled_quantity !== null && (
              <Text className={styles.orderFilled}>
                {t('已成交 {value}', { value: fmtMoney(o.filled_quantity) })}
              </Text>
            )}
          </View>
        </View>
      );
    });
  };

  const renderTrade = () => {
    if (loading && trades.length === 0) return null;
    if (trades.length === 0) {
      return <Empty title={t('暂无成交')} />;
    }
    return trades.map((tr) => {
      const side = ORDER_SIDE_LABELS[tr.side];
      return (
        <View key={tr.trade_id} className={styles.tradeItem}>
          <View className={styles.tradeHeader}>
            <View className={styles.posSymbolWrap}>
              <Text className={styles.orderSymbol}>{tr.symbol}</Text>
              <Tag type={side.type} round>
                {t(side.label)}
              </Tag>
            </View>
            <Text className={styles.orderTime}>{fmtTime(tr.timestamp)}</Text>
          </View>
          <View className={styles.posStats}>
            <View className={styles.posCell}>
              <Text className={styles.cellLabel}>{t('成交数量')}</Text>
              <Text className={styles.cellValue}>{fmtMoney(tr.size)}</Text>
            </View>
            <View className={styles.posCell}>
              <Text className={styles.cellLabel}>{t('成交价格')}</Text>
              <Text className={styles.cellValue}>{fmtPrice(tr.price)}</Text>
            </View>
            <View className={styles.posCell}>
              <Text className={styles.cellLabel}>{t('手续费')}</Text>
              <Text className={styles.cellValue}>{fmtPrice(tr.fee)}</Text>
            </View>
          </View>
        </View>
      );
    });
  };

  const renderTab = () => {
    if (activeTab === 'market') return renderMarket();
    if (activeTab === 'position') return renderPosition();
    if (activeTab === 'order') return renderOrder();
    return renderTrade();
  };

  return (
    <View className={styles.page}>
      <View className={styles.header}>
        <View className={styles.headerRow}>
          <Text className={styles.modeLabel}>{t('交易模式')}</Text>
          <View className={styles.modeToggle} onClick={toggleMode}>
            <Tag type={tradeMode === 'paper' ? 'warning' : 'primary'} round>
              {tradeMode === 'paper' ? t('模拟交易') : t('真实交易')}
            </Tag>
            <Text className={styles.modeSwitch}>⇄</Text>
          </View>
        </View>
        <View className={styles.tabRow}>
          {tabs.map((tab) => (
            <View
              key={tab.key}
              className={`${styles.tabItem} ${activeTab === tab.key ? styles.active : ''}`}
              onClick={() => setActiveTab(tab.key)}
            >
              <Text>{tab.label}</Text>
              {activeTab === tab.key && <View className={styles.tabIndicator} />}
            </View>
          ))}
        </View>
      </View>

      <ScrollView scrollY className={styles.content}>
        {loading && isTabEmpty ? <Skeleton rows={6} animated /> : renderTab()}
      </ScrollView>
    </View>
  );
};

export default MarketPage;

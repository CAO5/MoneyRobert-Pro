import React, { useEffect, useState } from 'react';
import { View, Text, ScrollView } from '@tarojs/components';
import Taro, { useRouter } from '@tarojs/taro';
import { marketService } from '@/services/market';
import { signalService } from '@/services/signal';
import { backtestService } from '@/services/backtest';
import type { MarketSnapshot } from '@/types/market';
import type { DecisionCardListItem } from '@/types/signal';
import type { BacktestJobSummary } from '@/types/backtest';
import { DECISION_ACTION_LABELS } from '@/types/signal';
import { BACKTEST_STATUS_LABELS } from '@/types/backtest';
import Tag from '@/components/Tag';
import EmptyState from '@/components/EmptyState';
import styles from './index.module.scss';

type TabKey = 'market' | 'decision' | 'backtest' | 'report';

/**
 * 业务页（tabBar）
 * 内部 Tab 切换：行情/决策卡/回测/报告
 * 数据均来自现有后端 API
 */
const BusinessPage: React.FC = () => {
  const router = useRouter();
  const [activeTab, setActiveTab] = useState<TabKey>('market');

  // 各 Tab 数据
  const [markets, setMarkets] = useState<MarketSnapshot[]>([]);
  const [decisions, setDecisions] = useState<DecisionCardListItem[]>([]);
  const [backtests, setBacktests] = useState<BacktestJobSummary[]>([]);
  const [loading, setLoading] = useState(false);

  // 从 URL 参数恢复 Tab（工作台快捷入口跳转用）
  useEffect(() => {
    const tab = router.params.tab as TabKey;
    if (tab && ['market', 'decision', 'backtest', 'report'].includes(tab)) {
      setActiveTab(tab);
    }
  }, [router.params.tab]);

  // 按需加载数据
  useEffect(() => {
    const loadData = async () => {
      setLoading(true);
      try {
        if (activeTab === 'market') {
          const data = await marketService.getSnapshots();
          setMarkets(data);
        } else if (activeTab === 'decision') {
          const data = await signalService.listCards(20);
          setDecisions(data);
        } else if (activeTab === 'backtest') {
          const data = await backtestService.listJobs();
          setBacktests(data);
        }
      } catch (err) {
        console.error(`[Business] load ${activeTab} failed:`, err);
      } finally {
        setLoading(false);
      }
    };
    loadData();
  }, [activeTab]);

  // 跳转标的详情
  const handleMarketClick = (symbol: string) => {
    Taro.navigateTo({ url: `/pages/symbol-detail/index?symbol=${encodeURIComponent(symbol)}` });
  };

  // 跳转决策卡详情
  const handleDecisionClick = (cardId: string) => {
    Taro.navigateTo({ url: `/pages/decision-detail/index?id=${cardId}` });
  };

  // 跳转回测详情
  const handleBacktestClick = (jobId: string) => {
    Taro.navigateTo({ url: `/pages/backtest-detail/index?id=${jobId}` });
  };

  // 跳转报告详情
  const handleReportClick = (id: string) => {
    Taro.navigateTo({ url: `/pages/report-detail/index?id=${id}` });
  };

  const tabs: Array<{ key: TabKey; label: string }> = [
    { key: 'market', label: '行情' },
    { key: 'decision', label: '决策卡' },
    { key: 'backtest', label: '回测' },
    { key: 'report', label: '报告' },
  ];

  return (
    <View className={styles.businessPage}>
      <View className={styles.tabBar}>
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

      <ScrollView scrollY className={styles.tabContent}>
        {loading && <EmptyState title="加载中..." description="正在获取数据" />}

        {!loading && activeTab === 'market' && (
          markets.length === 0 ? (
            <EmptyState title="暂无行情数据" />
          ) : (
            markets.map((m) => {
              const change = m.price_change_percent_24h;
              const changeClass = change > 0 ? styles.up : change < 0 ? styles.down : styles.flat;
              return (
                <View
                  key={m.symbol}
                  className={styles.marketItem}
                  onClick={() => handleMarketClick(m.symbol)}
                >
                  <View className={styles.symbolInfo}>
                    <Text className={styles.symbolName}>{m.symbol}</Text>
                    <Text className={styles.symbolMeta}>24h 量 {m.volume_24h.toLocaleString()}</Text>
                  </View>
                  <View className={styles.priceInfo}>
                    <Text className={styles.price}>{m.current_price.toLocaleString()}</Text>
                    <Text className={`${styles.changePercent} ${changeClass}`}>
                      {change > 0 ? '+' : ''}{change.toFixed(2)}%
                    </Text>
                  </View>
                </View>
              );
            })
          )
        )}

        {!loading && activeTab === 'decision' && (
          decisions.length === 0 ? (
            <EmptyState title="暂无决策卡" />
          ) : (
            decisions.map((d) => {
              const actionLabel = DECISION_ACTION_LABELS[d.suggested_action];
              const variant =
                d.suggested_action === 'open_long' || d.suggested_action === 'open_short'
                  ? 'primary'
                  : d.suggested_action === 'reduce' || d.suggested_action === 'close'
                  ? 'warning'
                  : 'default';
              const evClass = d.expected_value > 0 ? styles.positive : styles.negative;
              return (
                <View
                  key={d.card_id}
                  className={styles.decisionCard}
                  onClick={() => handleDecisionClick(d.card_id)}
                >
                  <View className={styles.decisionCardHeader}>
                    <Text className={styles.decisionCardSymbol}>{d.symbol}</Text>
                    <Tag variant={variant}>{actionLabel}</Tag>
                  </View>
                  <View className={styles.decisionCardContent}>
                    <View className={styles.probBar}>
                      <View className={styles.probUp} style={{ width: `${d.p_up * 100}%` }} />
                      <View className={styles.probFlat} style={{ width: `${d.p_flat * 100}%` }} />
                      <View className={styles.probDown} style={{ width: `${d.p_down * 100}%` }} />
                    </View>
                    {d.trust_level && <Tag variant={d.trust_level === 'A' ? 'success' : d.trust_level === 'B' ? 'primary' : 'warning'}>{d.trust_level}</Tag>}
                  </View>
                  <View className={styles.decisionCardFooter}>
                    <Text>EV {d.expected_value > 0 ? '+' : ''}{d.expected_value.toFixed(2)}</Text>
                    <Text className={`${styles.decisionCardEv} ${evClass}`}>
                      ↑ {(d.p_up * 100).toFixed(0)}% ↓ {(d.p_down * 100).toFixed(0)}%
                    </Text>
                  </View>
                </View>
              );
            })
          )
        )}

        {!loading && activeTab === 'backtest' && (
          backtests.length === 0 ? (
            <EmptyState title="暂无回测任务" />
          ) : (
            backtests.map((b) => {
              const statusLabel = BACKTEST_STATUS_LABELS[b.status] || b.status;
              const returnClass = (b.total_return_pct || 0) > 0 ? styles.up : styles.down;
              return (
                <View
                  key={b.job_id}
                  className={styles.backtestItem}
                  onClick={() => handleBacktestClick(b.job_id)}
                >
                  <View className={styles.backtestHeader}>
                    <Text className={styles.backtestName}>{b.job_name}</Text>
                    <Tag variant={b.status === 'completed' ? 'success' : b.status === 'running' ? 'primary' : 'default'}>
                      {statusLabel}
                    </Tag>
                  </View>
                  {b.status === 'running' && (
                    <View className={styles.progressWrap}>
                      <View className={styles.progressLabel}>
                        <Text>进度</Text>
                        <Text>{b.progress}%</Text>
                      </View>
                      <View className={styles.progressTrack}>
                        <View className={styles.progressFill} style={{ width: `${b.progress}%` }} />
                      </View>
                    </View>
                  )}
                  <View className={styles.backtestStats}>
                    {b.total_return_pct !== undefined && (
                      <Text>
                        收益 <Text className={`${styles.backtestStatValue} ${returnClass}`}>
                          {b.total_return_pct > 0 ? '+' : ''}{b.total_return_pct.toFixed(2)}%
                        </Text>
                      </Text>
                    )}
                    {b.sharpe_ratio !== undefined && (
                      <Text>
                        夏普 <Text className={styles.backtestStatValue}>{b.sharpe_ratio.toFixed(2)}</Text>
                      </Text>
                    )}
                    {b.max_drawdown_pct !== undefined && (
                      <Text>
                        回撤 <Text className={`${styles.backtestStatValue} ${styles.down}`}>-{b.max_drawdown_pct.toFixed(2)}%</Text>
                      </Text>
                    )}
                  </View>
                </View>
              );
            })
          )
        )}

        {!loading && activeTab === 'report' && (
          <EmptyState
            title="报告中心"
            description="完整报告请前往桌面端查看"
            actionText="查看示例报告"
            onAction={() => handleReportClick('report-2026q2')}
          />
        )}
      </ScrollView>
    </View>
  );
};

export default BusinessPage;

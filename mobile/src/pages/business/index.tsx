import React, { useEffect, useState } from 'react';
import { View, Text, ScrollView } from '@tarojs/components';
import Taro, { useRouter } from '@tarojs/taro';
import { Empty, Skeleton, Tag } from '@nutui/nutui-react-taro';
import { marketService } from '@/services/market';
import { signalService } from '@/services/signal';
import { backtestService } from '@/services/backtest';
import type { MarketSnapshot } from '@/types/market';
import type { DecisionCardListItem } from '@/types/signal';
import type { BacktestJobSummary } from '@/types/backtest';
import { DECISION_ACTION_LABELS } from '@/types/signal';
import { BACKTEST_STATUS_LABELS } from '@/types/backtest';
import { useI18n, useLocalizedTitle } from '@/store/language';
import styles from './index.module.scss';

type TabKey = 'market' | 'decision' | 'backtest' | 'report';

/** 自定义 Tag variant → NutUI Tag type 映射 */
const variantToType = (variant: string): 'default' | 'primary' | 'success' | 'warning' | 'danger' => {
  if (variant === 'primary' || variant === 'premium') return 'primary';
  if (variant === 'success') return 'success';
  if (variant === 'warning') return 'warning';
  if (variant === 'error' || variant === 'risk') return 'danger';
  return 'default';
};

/**
 * 业务页（tabBar）
 * 内部 Tab 切换：行情/决策卡/回测/报告
 * 数据均来自现有后端 API
 */
const BusinessPage: React.FC = () => {
  const router = useRouter();
  const [activeTab, setActiveTab] = useState<TabKey>('market');

  const [markets, setMarkets] = useState<MarketSnapshot[]>([]);
  const [decisions, setDecisions] = useState<DecisionCardListItem[]>([]);
  const [backtests, setBacktests] = useState<BacktestJobSummary[]>([]);
  const [loading, setLoading] = useState(false);
  const { t } = useI18n();
  useLocalizedTitle('行情');

  useEffect(() => {
    const tab = router.params.tab as TabKey;
    if (tab && ['market', 'decision', 'backtest', 'report'].includes(tab)) {
      setActiveTab(tab);
    }
  }, [router.params.tab]);

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
      } catch {
      } finally {
        setLoading(false);
      }
    };
    loadData();
  }, [activeTab]);

  const handleMarketClick = (symbol: string) => {
    Taro.navigateTo({ url: `/pages/symbol-detail/index?symbol=${encodeURIComponent(symbol)}` });
  };

  const handleDecisionClick = (cardId: string) => {
    Taro.navigateTo({ url: `/pages/decision-detail/index?id=${cardId}` });
  };

  const handleBacktestClick = (jobId: string) => {
    Taro.navigateTo({ url: `/pages/backtest-detail/index?id=${jobId}` });
  };

  const handleReportClick = (id: string) => {
    Taro.navigateTo({ url: `/pages/report-detail/index?id=${id}` });
  };

  const tabs: Array<{ key: TabKey; label: string }> = [
    { key: 'market', label: t('行情') },
    { key: 'decision', label: t('决策卡') },
    { key: 'backtest', label: t('回测') },
    { key: 'report', label: t('报告') },
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
        {loading && <Skeleton rows={5} animated />}

        {!loading && activeTab === 'market' && (
          markets.length === 0 ? (
            <Empty title={t('暂无行情数据')} />
          ) : (
            markets.map((m) => {
              const change = m.price_change_percent_24h ?? 0;
              const volume = m.volume_24h ?? 0;
              const price = m.current_price ?? 0;
              const changeClass = change > 0 ? styles.up : change < 0 ? styles.down : styles.flat;
              return (
                <View
                  key={m.symbol}
                  className={styles.marketItem}
                  onClick={() => handleMarketClick(m.symbol)}
                >
                  <View className={styles.symbolInfo}>
                    <Text className={styles.symbolName}>{m.symbol}</Text>
                    <Text className={styles.symbolMeta}>{t('24h 量 {value}', { value: volume.toLocaleString() })}</Text>
                  </View>
                  <View className={styles.priceInfo}>
                    <Text className={styles.price}>{price.toLocaleString()}</Text>
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
            <Empty title={t('暂无决策卡')} />
          ) : (
            decisions.map((d) => {
              const actionLabel = DECISION_ACTION_LABELS[d.suggested_action];
              const variant =
                d.suggested_action === 'open_long' || d.suggested_action === 'open_short'
                  ? 'primary'
                  : d.suggested_action === 'reduce' || d.suggested_action === 'close'
                  ? 'warning'
                  : 'default';
              const ev = d.expected_value ?? 0;
              const pUp = d.p_up ?? 0;
              const pFlat = d.p_flat ?? 0;
              const pDown = d.p_down ?? 0;
              const evClass = ev > 0 ? styles.positive : styles.negative;
              return (
                <View
                  key={d.card_id}
                  className={styles.decisionCard}
                  onClick={() => handleDecisionClick(d.card_id)}
                >
                  <View className={styles.decisionCardHeader}>
                    <Text className={styles.decisionCardSymbol}>{d.symbol}</Text>
                    <Tag type={variantToType(variant)}>{t(actionLabel)}</Tag>
                  </View>
                  <View className={styles.decisionCardContent}>
                    <View className={styles.probBar}>
                      <View className={styles.probUp} style={{ width: `${pUp * 100}%` }} />
                      <View className={styles.probFlat} style={{ width: `${pFlat * 100}%` }} />
                      <View className={styles.probDown} style={{ width: `${pDown * 100}%` }} />
                    </View>
                    {d.trust_level && (
                      <Tag
                        type={variantToType(
                          d.trust_level === 'A' ? 'success' : d.trust_level === 'B' ? 'primary' : 'warning',
                        )}
                      >
                        {d.trust_level}
                      </Tag>
                    )}
                  </View>
                  <View className={styles.decisionCardFooter}>
                    <Text>EV {ev > 0 ? '+' : ''}{ev.toFixed(2)}</Text>
                    <Text className={`${styles.decisionCardEv} ${evClass}`}>
                      ↑ {(pUp * 100).toFixed(0)}% ↓ {(pDown * 100).toFixed(0)}%
                    </Text>
                  </View>
                </View>
              );
            })
          )
        )}

        {!loading && activeTab === 'backtest' && (
          backtests.length === 0 ? (
            <Empty title={t('暂无回测任务')} />
          ) : (
            backtests.map((b) => {
              const statusLabel = BACKTEST_STATUS_LABELS[b.status] || b.status;
              const returnPct = b.total_return_pct ?? 0;
              const sharpe = b.sharpe_ratio ?? 0;
              const drawdown = b.max_drawdown_pct ?? 0;
              const returnClass = returnPct > 0 ? styles.up : styles.down;
              const showReturn = b.total_return_pct !== undefined;
              const showSharpe = b.sharpe_ratio !== undefined;
              const showDrawdown = b.max_drawdown_pct !== undefined;
              return (
                <View
                  key={b.job_id}
                  className={styles.backtestItem}
                  onClick={() => handleBacktestClick(b.job_id)}
                >
                  <View className={styles.backtestHeader}>
                    <Text className={styles.backtestName}>{b.job_name}</Text>
                    <Tag
                      type={variantToType(
                        b.status === 'completed' ? 'success' : b.status === 'running' ? 'primary' : 'default',
                      )}
                    >
                      {t(statusLabel)}
                    </Tag>
                  </View>
                  {b.status === 'running' && (
                    <View className={styles.progressWrap}>
                      <View className={styles.progressLabel}>
                        <Text>{t('进度')}</Text>
                        <Text>{b.progress}%</Text>
                      </View>
                      <View className={styles.progressTrack}>
                        <View className={styles.progressFill} style={{ width: `${b.progress}%` }} />
                      </View>
                    </View>
                  )}
                  <View className={styles.backtestStats}>
                    {showReturn && (
                      <Text>
                        {t('收益')} <Text className={`${styles.backtestStatValue} ${returnClass}`}>
                          {returnPct > 0 ? '+' : ''}{returnPct.toFixed(2)}%
                        </Text>
                      </Text>
                    )}
                    {showSharpe && (
                      <Text>
                        {t('夏普')} <Text className={styles.backtestStatValue}>{sharpe.toFixed(2)}</Text>
                      </Text>
                    )}
                    {showDrawdown && (
                      <Text>
                        {t('回撤')} <Text className={`${styles.backtestStatValue} ${styles.down}`}>-{drawdown.toFixed(2)}%</Text>
                      </Text>
                    )}
                  </View>
                </View>
              );
            })
          )
        )}

        {!loading && activeTab === 'report' && (
          <Empty
            title={t('报告中心')}
            description={t('完整报告请前往桌面端查看')}
            actions={[
              {
                text: t('查看示例报告'),
                type: 'primary',
                onClick: () => handleReportClick('report-2026q2'),
              },
            ]}
          />
        )}
      </ScrollView>
    </View>
  );
};

export default BusinessPage;

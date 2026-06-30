import React, { useEffect, useMemo, useState } from 'react';
import { ScrollView, Text, View } from '@tarojs/components';
import Taro, { usePullDownRefresh } from '@tarojs/taro';
import EmptyState from '@/components/EmptyState';
import { workbenchService } from '@/services/workbench';
import { useAuthStore } from '@/store/auth';
import type { DailyInsight, WorkbenchData } from '@/types/workbench';
import { useI18n } from '@/store/language';
import { buildSafeNavigationQuery } from '@/security/transport';
import styles from './index.module.scss';

const WorkbenchPage: React.FC = () => {
  const user = useAuthStore((state) => state.user);
  const isAuthenticated = useAuthStore((state) => state.isAuthenticated);
  const [data, setData] = useState<WorkbenchData | null>(null);
  const [loading, setLoading] = useState(true);
  const [assetVisible, setAssetVisible] = useState(true);
  const [assetRange, setAssetRange] = useState<'今日' | '近7日' | '近30日'>('今日');
  const { t } = useI18n();

  const loadData = async () => {
    setLoading(true);
    try {
      setData(await workbenchService.getWorkbench());
    } catch {
      Taro.showToast({ title: t('数据开小差了，请稍后重试'), icon: 'none' });
    } finally {
      setLoading(false);
    }
  };

  useEffect(() => {
    if (!isAuthenticated) {
      const timer = setTimeout(() => Taro.reLaunch({ url: '/pages/login/index' }), 100);
      return () => clearTimeout(timer);
    }
    loadData();
    return undefined;
  }, [isAuthenticated]);

  usePullDownRefresh(async () => {
    await loadData();
    Taro.stopPullDownRefresh();
  });

  const firstName = useMemo(() => {
    const name = user?.username || t('投资者');
    return name.length > 8 ? name.slice(0, 8) : name;
  }, [user?.username]);

  const navigate = (route: string, params?: Record<string, string>) => {
    try {
      const query = buildSafeNavigationQuery(params);
      Taro.navigateTo({ url: `${route}${query}` }).catch(() => Taro.switchTab({ url: route }));
    } catch {
      Taro.showToast({ title: t('操作失败'), icon: 'none' });
    }
  };

  const openInsight = (insight: DailyInsight) => navigate(insight.route, insight.params);

  if (!isAuthenticated) return <View className={styles.page} />;

  if (loading && !data) {
    return (
      <View className={styles.page}>
        <View className={styles.loadingHero} />
        <View className={styles.loadingCard} />
        <View className={styles.loadingCard} />
      </View>
    );
  }

  if (!data) {
    return (
      <View className={styles.page}>
        <EmptyState title={t('暂时没有拿到分析数据')} description={t('下拉刷新，重新连接市场')} actionText={t('重新加载')} onAction={loadData} />
      </View>
    );
  }

  return (
    <View className={styles.page}>
      <View className={styles.topBar}>
        <View>
          <Text className={styles.brand}>MoneyRobert</Text>
          <Text className={styles.hello}>{t('你好，{name}', { name: firstName })}</Text>
        </View>
        <View className={styles.topActions}>
          <View className={styles.searchButton} onClick={() => Taro.switchTab({ url: '/pages/business/index' })}>
            <Text className={styles.searchGlyph}>⌕</Text>
          </View>
          <View className={styles.bellButton} onClick={() => Taro.switchTab({ url: '/pages/message/index' })}>
            <Text>{t('提醒')}</Text>
            {data.unread_message_count > 0 && <View className={styles.badge}>{data.unread_message_count}</View>}
          </View>
        </View>
      </View>

      <ScrollView scrollY className={styles.scroll}>
        <View className={styles.marketPulse}>
          <View className={`${styles.pulseDot} ${styles[data.market_tone]}`} />
          <View className={styles.marketCopy}>
            <Text className={styles.marketSession}>{t(data.market_session)}</Text>
            <Text className={styles.marketSummary}>{t(data.market_summary)}</Text>
          </View>
          <Text className={styles.chevron}>›</Text>
        </View>

        <View className={styles.assetCard}>
          <View className={styles.assetTop}>
            <View>
              <Text className={styles.eyebrow}>{t('总资产估值')}</Text>
              <View className={styles.assetValueRow}>
                <Text className={styles.assetValue}>{assetVisible ? data.net_asset : '¥ ••••••'}</Text>
                <Text className={styles.eyeButton} onClick={() => setAssetVisible((value) => !value)}>
                  {t(assetVisible ? '隐藏' : '显示')}
                </Text>
              </View>
            </View>
            <View className={styles.syncPill}><Text>{t('已同步')}</Text></View>
          </View>

          <View className={styles.pnlRow}>
            <View>
              <Text className={styles.pnlLabel}>{t(`${assetRange}盈亏`)}</Text>
              <Text className={styles.pnlValue}>
                {assetVisible ? `${data.today_pnl}  +${data.today_pnl_percent.toFixed(2)}%` : '••••'}
              </Text>
            </View>
            <View className={styles.miniChart} aria-label="资产走势">
              {[24, 31, 27, 42, 38, 53, 48, 65, 61, 76, 71, 88].map((height, index) => (
                <View key={index} className={styles.chartBar} style={{ height: `${height}%` }} />
              ))}
            </View>
          </View>

          <View className={styles.rangeTabs}>
            {(['今日', '近7日', '近30日'] as const).map((range) => (
              <View
                key={range}
                className={`${styles.rangeTab} ${assetRange === range ? styles.rangeActive : ''}`}
                onClick={() => setAssetRange(range)}
              >
                <Text>{t(range)}</Text>
              </View>
            ))}
            <View className={styles.assetDetail} onClick={() => Taro.switchTab({ url: '/pages/business/index' })}>
              <Text>{t('资产分析 ›')}</Text>
            </View>
          </View>
        </View>

        <View className={styles.sectionHeader}>
          <View>
            <Text className={styles.sectionTitle}>{t('今日决策')}</Text>
            <Text className={styles.sectionHint}>{t('只给你最值得关注的一件事')}</Text>
          </View>
          <View className={styles.aiTag}><Text>{t('AI 分析')}</Text></View>
        </View>

        <View className={styles.insightCard} onClick={() => openInsight(data.daily_insight)}>
          <View className={styles.insightHeader}>
            <View className={styles.symbolAvatar}><Text>{data.daily_insight.symbol_name.slice(0, 1)}</Text></View>
            <View className={styles.symbolCopy}>
              <Text className={styles.symbolName}>{data.daily_insight.symbol}</Text>
              <Text className={styles.expire}>{t('有效至 {time}', { time: data.daily_insight.expires_at })}</Text>
            </View>
            <View className={styles.actionTag}><Text>{t(data.daily_insight.action)}</Text></View>
          </View>
          <Text className={styles.insightTitle}>{t(data.daily_insight.title)}</Text>
          <Text className={styles.insightReason}>{t(data.daily_insight.reason)}</Text>
          <View className={styles.confidenceRow}>
            <View className={styles.confidenceMeta}>
              <Text>{t('置信度 {value}%', { value: data.daily_insight.confidence })}</Text>
              <Text>{t(data.daily_insight.expected_move)}</Text>
            </View>
            <View className={styles.confidenceTrack}>
              <View className={styles.confidenceFill} style={{ width: `${data.daily_insight.confidence}%` }} />
            </View>
          </View>
          <View className={styles.insightAction}><Text>{t('查看完整依据与风控位')}</Text><Text>›</Text></View>
        </View>

        {data.risk_alerts.length > 0 && (
          <View className={styles.riskStrip} onClick={() => Taro.switchTab({ url: '/pages/todo/index' })}>
            <View className={styles.riskIcon}><Text>!</Text></View>
            <View className={styles.riskCopy}>
              <Text className={styles.riskLabel}>{t('需要你确认')}</Text>
              <Text className={styles.riskTitle}>{t(data.risk_alerts[0].title)}</Text>
            </View>
            <Text className={styles.riskAction}>{t('现在处理 ›')}</Text>
          </View>
        )}

        <View className={styles.sectionHeader}>
          <View>
            <Text className={styles.sectionTitle}>{t('我的关注')}</Text>
            <Text className={styles.sectionHint}>{t('信号变化比价格变化更重要')}</Text>
          </View>
          <Text className={styles.sectionLink} onClick={() => Taro.switchTab({ url: '/pages/business/index' })}>{t('全部')}</Text>
        </View>

        <ScrollView scrollX className={styles.watchScroll}>
          <View className={styles.watchRow}>
            {data.watchlist.map((quote) => (
              <View
                key={quote.symbol}
                className={styles.quoteCard}
                onClick={() => navigate('/pages/symbol-detail/index', { symbol: quote.symbol })}
              >
                <View className={styles.quoteTop}>
                  <Text className={styles.quoteName}>{quote.display_name}</Text>
                  <Text className={quote.change_percent >= 0 ? styles.quoteUp : styles.quoteDown}>
                    {quote.change_percent >= 0 ? '+' : ''}{quote.change_percent.toFixed(2)}%
                  </Text>
                </View>
                <Text className={styles.quotePrice}>{quote.price}</Text>
                <Text className={styles.quoteSignal}>{t(quote.signal)}</Text>
              </View>
            ))}
            <View className={styles.addQuote} onClick={() => Taro.switchTab({ url: '/pages/business/index' })}>
              <Text className={styles.addGlyph}>＋</Text>
              <Text>{t('添加关注')}</Text>
            </View>
          </View>
        </ScrollView>

        <View className={styles.valueCard}>
          <View className={styles.valueHeader}>
            <View>
              <Text className={styles.valueEyebrow}>{t('本周价值回顾')}</Text>
              <Text className={styles.valueTitle}>{t('分析正在变成你的交易优势')}</Text>
            </View>
            <View className={styles.proBadge}><Text>{t('创作者')}</Text></View>
          </View>
          <View className={styles.valueStats}>
            <View><Text className={styles.valueNumber}>{data.value_proof.risks_avoided}</Text><Text className={styles.valueLabel}>{t('次风险预警')}</Text></View>
            <View><Text className={styles.valueNumber}>{data.value_proof.opportunities_found}</Text><Text className={styles.valueLabel}>{t('个有效机会')}</Text></View>
            <View><Text className={styles.valueNumber}>{data.value_proof.estimated_value}</Text><Text className={styles.valueLabel}>{t('预估保护价值')}</Text></View>
          </View>
          <View className={styles.trialRow}>
            <Text>{t('无强制费用 · 盈利后可自愿赞助')}</Text>
            <Text className={styles.proLink} onClick={() => navigate('/pages/membership/index')}>{t('支持作者 ›')}</Text>
          </View>
        </View>

        <View className={styles.disclaimer}>
          <Text>{t('数据与分析仅供参考，不构成投资建议。市场有风险，决策需独立判断。')}</Text>
        </View>
      </ScrollView>
    </View>
  );
};

export default WorkbenchPage;

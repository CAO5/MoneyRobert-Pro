import React, { useCallback, useEffect, useMemo, useRef, useState } from 'react';
import { ScrollView, Text, View } from '@tarojs/components';
import Taro, { usePullDownRefresh } from '@tarojs/taro';
import { Badge, Button, Empty, Skeleton, Tag } from '@nutui/nutui-react-taro';
import { tradingService } from '@/services/trading';
import { aiService } from '@/services/ai';
import { newsService } from '@/services/news';
import { settingsService } from '@/services/settings';
import { messageService } from '@/services/message';
import { useAuthStore } from '@/store/auth';
import { useI18n, useLocalizedTitle } from '@/store/language';
import { buildSafeNavigationQuery } from '@/security/transport';
import type { AnalysisHistoryItem, AiPrediction } from '@/types/ai';
import type { NewsItem } from '@/types/news';
import type { Position, ProfitCurvePoint } from '@/types/trading';
import type { SystemHealth } from '@/types/settings';
import styles from './index.module.scss';

type AssetRange = '今日' | '近7日' | '近30日';

interface DashboardMetrics {
  total_equity: number;
  available_balance: number;
  unrealized_pnl: number;
  realized_pnl: number;
  margin_used: number;
  margin_ratio: number;
  leverage: number;
  open_positions: number;
  today_pnl: number;
  today_pnl_pct: number;
}

interface PositionNewsGroup {
  symbol: string;
  news: NewsItem[];
}

interface ChartBar {
  height: number;
  positive: boolean;
}

const WorkbenchPage: React.FC = () => {
  const user = useAuthStore((state) => state.user);
  const isAuthenticated = useAuthStore((state) => state.isAuthenticated);
  const { t } = useI18n();
  useLocalizedTitle('首页');

  const [metrics, setMetrics] = useState<DashboardMetrics | null>(null);
  const [profitTrendWeek, setProfitTrendWeek] = useState<ProfitCurvePoint[]>([]);
  const [profitTrendMonth, setProfitTrendMonth] = useState<ProfitCurvePoint[]>([]);
  const [aiAnalysis, setAiAnalysis] = useState<AnalysisHistoryItem | null>(null);
  const [aiPrediction, setAiPrediction] = useState<AiPrediction | null>(null);
  const [marketNews, setMarketNews] = useState<NewsItem[]>([]);
  const [positionNews, setPositionNews] = useState<PositionNewsGroup[]>([]);
  const [health, setHealth] = useState<SystemHealth | null>(null);
  const [unreadCount, setUnreadCount] = useState(0);
  const [loading, setLoading] = useState(true);
  const [assetVisible, setAssetVisible] = useState(true);
  const [assetRange, setAssetRange] = useState<AssetRange>('今日');

  // 懒加载标记：避免重复请求
  const loadedRef = useRef({
    ai: false,
    news: false,
    positionNews: false,
    health: false,
    unread: false,
    trendMonth: false,
  });
  // 板块可见性（用于触发懒加载）
  const [aiVisible, setAiVisible] = useState(false);
  const [newsVisible, setNewsVisible] = useState(false);
  const [healthVisible, setHealthVisible] = useState(false);

  // 用 ref 持有 t，避免 useCallback 依赖 t 导致每次渲染重建 → useEffect 死循环
  const tRef = useRef(t);
  tRef.current = t;

  /**
   * 首屏核心数据：仅 metrics + positions + trend(week)
   * 不并发打 9 个请求，避免后端连接被重置（ECONNRESET）
   */
  const loadCoreData = useCallback(async () => {
    setLoading(true);
    try {
      const [metricsR, positionsR, trendWeekR] = await Promise.allSettled([
        tradingService.getDashboardMetrics(),
        tradingService.getPositions(),
        tradingService.getProfitTrend('week'),
      ]);
      if (metricsR.status === 'fulfilled') setMetrics(metricsR.value as DashboardMetrics);
      if (trendWeekR.status === 'fulfilled') setProfitTrendWeek(trendWeekR.value);
      // 持仓加载后异步拉取相关新闻（不阻塞首屏）
      if (positionsR.status === 'fulfilled') {
        loadPositionNews(positionsR.value as Position[]);
      }
    } catch {
      // 核心数据失败才提示
      Taro.showToast({ title: tRef.current('数据开小差了，请稍后重试'), icon: 'none' });
    } finally {
      setLoading(false);
    }
    // 依赖空数组：t 通过 tRef 读取，不会因 t 变化而重建
  }, []);

  /** 持仓相关新闻：依赖持仓列表，串行拉取前 3 个品种 */
  const loadPositionNews = useCallback(async (positions: Position[]) => {
    if (loadedRef.current.positionNews) return;
    loadedRef.current.positionNews = true;
    const topPositions = positions.slice(0, 3);
    if (topPositions.length === 0) {
      setPositionNews([]);
      return;
    }
    const newsResults = await Promise.allSettled(
      topPositions.map((p) => newsService.getSymbolNews(p.symbol, 3)),
    );
    const groups: PositionNewsGroup[] = topPositions
      .map((p, i) => {
        const r = newsResults[i];
        return { symbol: p.symbol, news: r.status === 'fulfilled' ? r.value : [] };
      })
      .filter((g) => g.news.length > 0);
    setPositionNews(groups);
  }, []);

  /** AI 板块懒加载：综合分析 + 预测 */
  const loadAiData = useCallback(async () => {
    if (loadedRef.current.ai) return;
    loadedRef.current.ai = true;
    const [aiR, predR] = await Promise.allSettled([
      aiService.getAnalysisHistory('comprehensive', { limit: 1 }),
      aiService.getPredictions({ limit: 1 }),
    ]);
    if (aiR.status === 'fulfilled') {
      const arr = aiR.value;
      setAiAnalysis(arr && arr.length > 0 ? arr[0] : null);
    }
    if (predR.status === 'fulfilled') {
      const arr = predR.value;
      setAiPrediction(arr && arr.length > 0 ? arr[0] : null);
    }
  }, []);

  /** 市场新闻懒加载 */
  const loadMarketNews = useCallback(async () => {
    if (loadedRef.current.news) return;
    loadedRef.current.news = true;
    try {
      const news = await newsService.getRecent(5);
      setMarketNews(news);
    } catch {
      setMarketNews([]);
    }
  }, []);

  /** 系统健康懒加载 */
  const loadHealth = useCallback(async () => {
    if (loadedRef.current.health) return;
    loadedRef.current.health = true;
    try {
      const h = await settingsService.getHealth();
      setHealth(h);
    } catch {
      setHealth(null);
    }
  }, []);

  /** 未读消息数懒加载 */
  const loadUnreadCount = useCallback(async () => {
    if (loadedRef.current.unread) return;
    loadedRef.current.unread = true;
    try {
      const list = await messageService.listMessages({ status: 'unread' });
      setUnreadCount((list as unknown[]).length);
    } catch {
      setUnreadCount(0);
    }
  }, []);

  /** 月度趋势：切换到"近30日"时才加载 */
  const loadTrendMonth = useCallback(async () => {
    if (loadedRef.current.trendMonth) return;
    loadedRef.current.trendMonth = true;
    try {
      const trend = await tradingService.getProfitTrend('month');
      setProfitTrendMonth(trend);
    } catch {
      setProfitTrendMonth([]);
    }
  }, []);

  // 首屏加载：仅依赖 isAuthenticated，loadCoreData/loadUnreadCount 已用空依赖稳定
  useEffect(() => {
    if (!isAuthenticated) {
      // 未认证：跳转到 login
      // store 初始化时已同步恢复 isAuthenticated，此处为 false 说明真的没 token
      // 用 nextTick 确保 Taro 路由系统初始化完成，避免 connect.ts mount/unmount 时序问题
      Taro.nextTick(() => {
        Taro.reLaunch({ url: '/pages/login/index' });
      });
      return;
    }
    // 重置懒加载标记
    loadedRef.current = {
      ai: false,
      news: false,
      positionNews: false,
      health: false,
      unread: false,
      trendMonth: false,
    };
    loadCoreData();
    // 未读消息延迟 2s 加载（tabBar 已有消息入口，非首屏必需）
    const unreadTimer = setTimeout(() => loadUnreadCount(), 2000);
    return () => clearTimeout(unreadTimer);
  }, [isAuthenticated, loadCoreData, loadUnreadCount]);

  // 板块可见性变化触发懒加载（loadAiData 等已用空依赖稳定，不会死循环）
  useEffect(() => {
    if (aiVisible) loadAiData();
  }, [aiVisible, loadAiData]);
  useEffect(() => {
    if (newsVisible) loadMarketNews();
  }, [newsVisible, loadMarketNews]);
  useEffect(() => {
    if (healthVisible) loadHealth();
  }, [healthVisible, loadHealth]);

  usePullDownRefresh(async () => {
    // 下拉刷新：重置标记后只重新拉核心数据
    loadedRef.current = {
      ai: false,
      news: false,
      positionNews: false,
      health: false,
      unread: false,
      trendMonth: false,
    };
    await loadCoreData();
    Taro.stopPullDownRefresh();
  });

  const firstName = useMemo(() => {
    const name = user?.username || t('投资者');
    return name.length > 8 ? name.slice(0, 8) : name;
  }, [user?.username, t]);

  // 数字格式化
  const formatCurrency = (value?: number) => {
    if (!assetVisible) return '¥ ••••••';
    const v = value ?? 0;
    return `¥${v.toLocaleString('zh-CN', {
      minimumFractionDigits: 2,
      maximumFractionDigits: 2,
    })}`;
  };

  const formatPnl = (value?: number) => {
    if (!assetVisible) return '••••';
    const v = value ?? 0;
    const sign = v >= 0 ? '+' : '';
    return `${sign}${v.toLocaleString('zh-CN', {
      minimumFractionDigits: 2,
      maximumFractionDigits: 2,
    })}`;
  };

  const formatPercent = (value?: number) => {
    if (!assetVisible) return '•••';
    const v = value ?? 0;
    const sign = v >= 0 ? '+' : '';
    return `${sign}${v.toFixed(2)}%`;
  };

  const formatTime = (iso: string) => {
    if (!iso) return '';
    try {
      const d = new Date(iso);
      const now = new Date();
      const diff = (now.getTime() - d.getTime()) / 1000;
      if (diff < 60) return t('刚刚');
      if (diff < 3600) return `${Math.floor(diff / 60)}${t('分钟前')}`;
      if (diff < 86400) return `${Math.floor(diff / 3600)}${t('小时前')}`;
      return `${d.getMonth() + 1}-${String(d.getDate()).padStart(2, '0')}`;
    } catch {
      return iso;
    }
  };

  // 当前区间的盈亏
  const rangePnl = useMemo(() => {
    if (!metrics) return { pnl: 0, pct: 0 };
    if (assetRange === '今日') {
      return { pnl: metrics.today_pnl ?? 0, pct: metrics.today_pnl_pct ?? 0 };
    }
    const trend = assetRange === '近7日' ? profitTrendWeek : profitTrendMonth;
    const sum = trend.reduce((acc, p) => acc + (p.pnl ?? 0), 0);
    const base = trend[0]?.equity || metrics.total_equity || 1;
    const pct = base !== 0 ? (sum / base) * 100 : 0;
    return { pnl: sum, pct };
  }, [assetRange, metrics, profitTrendWeek, profitTrendMonth]);

  // 迷你收益曲线柱状条（12 条）
  const chartBars = useMemo<ChartBar[]>(() => {
    const trend =
      assetRange === '近30日' ? profitTrendMonth : profitTrendWeek;
    const points = trend.slice(-12);
    if (points.length === 0) {
      return Array.from({ length: 12 }, () => ({ height: 30, positive: true }));
    }
    const maxAbs = Math.max(...points.map((p) => Math.abs(p.pnl)), 1);
    const bars = points.map((p) => ({
      height: Math.max(15, Math.min(100, (Math.abs(p.pnl) / maxAbs) * 100)),
      positive: p.pnl >= 0,
    }));
    while (bars.length < 12) {
      bars.unshift({ height: 10, positive: true });
    }
    return bars;
  }, [assetRange, profitTrendWeek, profitTrendMonth]);

  const navigate = (route: string, params?: Record<string, string>) => {
    try {
      const query = buildSafeNavigationQuery(params);
      Taro.navigateTo({ url: `${route}${query}` }).catch(() => {
        Taro.switchTab({ url: route }).catch(() => {
          Taro.showToast({ title: t('页面跳转失败'), icon: 'none' });
        });
      });
    } catch {
      Taro.showToast({ title: t('操作失败'), icon: 'none' });
    }
  };

  const sentimentTagType = (sentiment?: string) => {
    if (sentiment === 'positive') return 'success';
    if (sentiment === 'negative') return 'danger';
    return 'default';
  };

  const sentimentLabel = (sentiment?: string) => {
    if (sentiment === 'positive') return t('利好');
    if (sentiment === 'negative') return t('利空');
    return t('中性');
  };

  const healthOk = (status?: string) => status === 'ok' || status === 'healthy';

  const pUpPct = (prediction?: AiPrediction | null) => {
    if (!prediction || typeof prediction.p_up !== 'number') return null;
    const v = prediction.p_up;
    return v > 1 ? v : v * 100;
  };

  // 未登录或加载中：Skeleton 占位
  if (!isAuthenticated || loading) {
    return (
      <View className={styles.page}>
        <View className={styles.topBar}>
          <View style={{ flex: 1 }}>
            <Skeleton rows={1} animated style={{ width: '180rpx', height: '40rpx' }} />
            <Skeleton rows={1} animated style={{ width: '120rpx', height: '24rpx', marginTop: '12rpx' }} />
          </View>
        </View>
        <View className={styles.scroll}>
          <View className={styles.loadingHero} />
          <View className={styles.loadingCard} />
          <View className={styles.loadingCard} />
        </View>
      </View>
    );
  }

  return (
    <View className={styles.page}>
      {/* 1. 顶部品牌栏 */}
      <View className={styles.topBar}>
        <View className={styles.brandBlock}>
          <Text className={styles.brand}>MoneyRobert</Text>
          <Text className={styles.hello}>{t('你好，{name}', { name: firstName })}</Text>
        </View>
        <View className={styles.topActions}>
          <Button
            shape="round"
            fill="none"
            size="mini"
            className={styles.searchButton}
            onClick={() => Taro.switchTab({ url: '/pages/business/index' })}
          >
            <Text className={styles.searchGlyph}>⌕</Text>
          </Button>
          <View
            className={styles.bellButton}
            onClick={() => Taro.switchTab({ url: '/pages/message/index' })}
          >
            <Text className={styles.bellGlyph}>🔔</Text>
            {unreadCount > 0 && (
              <Badge value={unreadCount} max={99} className={styles.badge} />
            )}
          </View>
        </View>
      </View>

      <ScrollView scrollY className={styles.scroll}>
        {/* 2. 资产总览卡片 */}
        <View className={styles.assetCard}>
          <View className={styles.assetTop}>
            <View>
              <Text className={styles.eyebrow}>{t('总资产估值')}</Text>
              <View className={styles.assetValueRow}>
                <Text className={styles.assetValue}>
                  {formatCurrency(metrics?.total_equity ?? 0)}
                </Text>
                <Button
                  fill="none"
                  size="mini"
                  className={styles.eyeButton}
                  onClick={() => setAssetVisible((v) => !v)}
                >
                  {t(assetVisible ? '隐藏' : '显示')}
                </Button>
              </View>
            </View>
            <View className={styles.pnlBlock}>
              <Text className={styles.pnlLabel}>{t(`${assetRange}盈亏`)}</Text>
              <Text
                className={`${styles.pnlValue} ${rangePnl.pnl >= 0 ? styles.up : styles.down}`}
              >
                {formatPnl(rangePnl.pnl)}  {formatPercent(rangePnl.pct)}
              </Text>
            </View>
          </View>

          <View className={styles.miniChart} aria-label={t('资产走势')}>
            {chartBars.map((bar, idx) => (
              <View
                key={idx}
                className={`${styles.chartBar} ${bar.positive ? styles.barUp : styles.barDown}`}
                style={{ height: `${bar.height}%` }}
              />
            ))}
          </View>

          <View className={styles.rangeTabs}>
            {(['今日', '近7日', '近30日'] as const).map((range) => (
              <View
                key={range}
                className={`${styles.rangeTab} ${assetRange === range ? styles.rangeActive : ''}`}
                onClick={() => {
                  setAssetRange(range);
                  if (range === '近30日') loadTrendMonth();
                }}
              >
                <Text>{t(range)}</Text>
              </View>
            ))}
          </View>
        </View>

        {/* 3. AI 当前观点卡片（触摸触发懒加载） */}
        <View onTouchStart={() => setAiVisible(true)}>
        <View className={styles.sectionHeader}>
          <View>
            <Text className={styles.sectionTitle}>{t('AI 当前观点')}</Text>
            <Text className={styles.sectionHint}>{t('综合分析市场判断')}</Text>
          </View>
          <Tag type="primary" className={styles.aiTag}>
            <Text>{t('AI 分析')}</Text>
          </Tag>
        </View>

        {aiAnalysis ? (
          <View
            className={styles.aiCard}
            onClick={() =>
              navigate('/pages/decision-detail/index', { id: aiAnalysis.analysis_id })
            }
          >
            <View className={styles.aiHeader}>
              <View className={styles.symbolAvatar}>
                <Text>{(aiAnalysis.symbol || '?').slice(0, 1)}</Text>
              </View>
              <View className={styles.symbolCopy}>
                <Text className={styles.symbolName}>{aiAnalysis.symbol || '-'}</Text>
                <Text className={styles.expire}>{formatTime(aiAnalysis.created_at)}</Text>
              </View>
              {pUpPct(aiPrediction) !== null && (
                <Tag type="primary" round className={styles.actionTag}>
                  <Text>
                    {t('看涨')} {pUpPct(aiPrediction)!.toFixed(0)}%
                  </Text>
                </Tag>
              )}
            </View>
            <Text className={styles.insightTitle}>{aiAnalysis.summary}</Text>
            <View className={styles.confidenceRow}>
              <View className={styles.confidenceMeta}>
                <Text>{t('置信度 {value}%', { value: aiAnalysis.confidence })}</Text>
              </View>
              <View className={styles.confidenceTrack}>
                <View
                  className={styles.confidenceFill}
                  style={{ width: `${aiAnalysis.confidence ?? 0}%` }}
                />
              </View>
            </View>
            <View className={styles.insightAction}>
              <Text>{t('查看完整分析 ›')}</Text>
            </View>
          </View>
        ) : (
          <View className={styles.emptyCard}>
            <Empty description={t('暂无 AI 分析')} />
          </View>
        )}
        </View>

        {/* 4. 市场新闻板块（触摸触发懒加载） */}
        <View onTouchStart={() => setNewsVisible(true)}>
        <View className={styles.sectionHeader}>
          <View>
            <Text className={styles.sectionTitle}>{t('市场新闻')}</Text>
            <Text className={styles.sectionHint}>{t('最新市场动态')}</Text>
          </View>
        </View>

        {marketNews.length > 0 ? (
          <ScrollView scrollX className={styles.newsScroll}>
            <View className={styles.newsRow}>
              {marketNews.map((news) => (
                <View
                  key={news.news_id}
                  className={styles.newsCard}
                  onClick={() => navigate('/pages/news-detail/index', { id: news.news_id })}
                >
                  <View className={styles.newsTagRow}>
                    <Tag type={sentimentTagType(news.sentiment)} className={styles.newsSentiment}>
                      <Text>{sentimentLabel(news.sentiment)}</Text>
                    </Tag>
                  </View>
                  <Text className={styles.newsTitle}>{news.title}</Text>
                  <View className={styles.newsMeta}>
                    <Text className={styles.newsSource}>{news.source}</Text>
                    <Text className={styles.newsTime}>{formatTime(news.published_at)}</Text>
                  </View>
                </View>
              ))}
            </View>
          </ScrollView>
        ) : (
          <View className={styles.emptyCard}>
            <Empty description={t('暂无新闻')} />
          </View>
        )}
        </View>

        {/* 5. 重大事件板块 */}
        <View className={styles.sectionHeader}>
          <View>
            <Text className={styles.sectionTitle}>{t('重大事件')}</Text>
            <Text className={styles.sectionHint}>{t('即将发生的重要事件')}</Text>
          </View>
        </View>
        <View className={styles.emptyCard}>
          <Empty description={t('暂无重大事件')} />
        </View>

        {/* 6. 持仓相关新闻 */}
        {positionNews.length > 0 && (
          <>
            <View className={styles.sectionHeader}>
              <View>
                <Text className={styles.sectionTitle}>{t('持仓相关新闻')}</Text>
                <Text className={styles.sectionHint}>{t('当前持仓品种的相关资讯')}</Text>
              </View>
            </View>
            <View className={styles.positionNewsList}>
              {positionNews.map((group) =>
                group.news.map((news) => (
                  <View
                    key={`${group.symbol}-${news.news_id}`}
                    className={styles.positionNewsItem}
                    onClick={() => navigate('/pages/news-detail/index', { id: news.news_id })}
                  >
                    <Tag type="primary" className={styles.positionSymbol}>
                      <Text>{group.symbol}</Text>
                    </Tag>
                    <View className={styles.positionNewsBody}>
                      <Text className={styles.positionNewsTitle}>{news.title}</Text>
                      <View className={styles.positionNewsMeta}>
                        <Tag
                          type={sentimentTagType(news.sentiment)}
                          className={styles.newsSentiment}
                        >
                          <Text>{sentimentLabel(news.sentiment)}</Text>
                        </Tag>
                        <Text className={styles.newsTime}>
                          {formatTime(news.published_at)}
                        </Text>
                      </View>
                    </View>
                  </View>
                )),
              )}
            </View>
          </>
        )}

        {/* 7. 系统运行状态（触摸触发懒加载） */}
        <View onTouchStart={() => setHealthVisible(true)}>
        <View className={styles.sectionHeader}>
          <View>
            <Text className={styles.sectionTitle}>{t('系统运行状态')}</Text>
            <Text className={styles.sectionHint}>{t('后端服务连接状态')}</Text>
          </View>
        </View>
        <View className={styles.healthCard}>
          <View className={styles.healthRow}>
            <View className={styles.healthItem}>
              <View
                className={`${styles.healthDot} ${healthOk(health?.database) ? styles.dotOk : styles.dotFail}`}
              />
              <Text className={styles.healthLabel}>{t('数据库')}</Text>
            </View>
            <View className={styles.healthItem}>
              <View
                className={`${styles.healthDot} ${healthOk(health?.redis) ? styles.dotOk : styles.dotFail}`}
              />
              <Text className={styles.healthLabel}>{t('Redis')}</Text>
            </View>
            <View className={styles.healthItem}>
              <View
                className={`${styles.healthDot} ${healthOk(health?.status) ? styles.dotOk : styles.dotFail}`}
              />
              <Text className={styles.healthLabel}>{t('服务')}</Text>
            </View>
          </View>
          <View className={styles.healthMeta}>
            <Text className={styles.healthVersion}>
              {t('版本')} {health?.version ?? '-'}
            </Text>
            <Text className={styles.healthEnv}>{health?.environment ?? '-'}</Text>
          </View>
        </View>
        </View>

        <View className={styles.disclaimer}>
          <Text>
            {t('数据与分析仅供参考，不构成投资建议。市场有风险，决策需独立判断。')}
          </Text>
        </View>
      </ScrollView>
    </View>
  );
};

export default WorkbenchPage;

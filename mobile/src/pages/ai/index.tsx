import React, { useCallback, useEffect, useRef, useState } from 'react';
import { ScrollView, Text, View } from '@tarojs/components';
import Taro from '@tarojs/taro';
import { Button, Empty, Input, Skeleton, Tag } from '@nutui/nutui-react-taro';
import { aiService } from '@/services/ai';
import { tradingService } from '@/services/trading';
import { useAuthStore } from '@/store/auth';
import { useI18n, useLocalizedTitle } from '@/store/language';
import { buildSafeNavigationQuery } from '@/security/transport';
import type {
  AnalysisHistoryItem,
  AnalysisResult,
  AnalysisType,
  DebateEvent,
  DebateSession,
} from '@/types/ai';
import type { Position } from '@/types/trading';
import styles from './index.module.scss';

type TabKey = 'manual' | 'market' | 'positions' | 'debate' | 'history';
type Stance = 'bullish' | 'bearish' | 'neutral';

const TABS: Array<{ key: TabKey; label: string }> = [
  { key: 'manual', label: '手动分析' },
  { key: 'market', label: '市场分析' },
  { key: 'positions', label: '持仓分析' },
  { key: 'debate', label: '辩论' },
  { key: 'history', label: '历史' },
];

const INTERVALS: Array<{ key: string; label: string }> = [
  { key: '15m', label: '15m' },
  { key: '1h', label: '1h' },
  { key: '4h', label: '4h' },
  { key: '1d', label: '1d' },
];

const ANALYSIS_TYPES: Array<{ key: AnalysisType; label: string }> = [
  { key: 'technical', label: '技术' },
  { key: 'funding', label: '资金费率' },
  { key: 'sentiment', label: '情绪' },
  { key: 'comprehensive', label: '综合' },
];

const ANALYSIS_TYPE_LABELS: Record<AnalysisType, string> = {
  technical: '技术',
  funding: '资金费率',
  sentiment: '情绪',
  comprehensive: '综合',
};

const TYPE_TAG_COLOR: Record<AnalysisType, 'primary' | 'warning' | 'default' | 'success'> = {
  technical: 'primary',
  funding: 'warning',
  sentiment: 'default',
  comprehensive: 'success',
};

const DEBATE_TAB = 'debate' as TabKey;

/** 置信度归一到 0-100（兼容 0-1 与 0-100 两种返回） */
function normConf(value?: number): number {
  if (value == null || Number.isNaN(value)) return 0;
  const n = value <= 1 ? value * 100 : value;
  return Math.max(0, Math.min(100, Math.round(n)));
}

/** 格式化时间 */
function formatTime(iso: string, locale: string): string {
  const d = new Date(iso);
  if (Number.isNaN(d.getTime())) return iso;
  return d.toLocaleString(locale);
}

// ============ 纯展示子组件 ============

const ChipRow: React.FC<{
  options: Array<{ key: string; label: string }>;
  value: string;
  onChange: (v: string) => void;
}> = ({ options, value, onChange }) => (
  <View className={styles.chipRow}>
    {options.map((o) => (
      <View
        key={o.key}
        className={`${styles.chip} ${value === o.key ? styles.chipActive : ''}`}
        onClick={() => onChange(o.key)}
      >
        <Text>{o.label}</Text>
      </View>
    ))}
  </View>
);

const ConfidenceBar: React.FC<{ value?: number; showLabel?: boolean }> = ({
  value,
  showLabel = true,
}) => {
  const { t } = useI18n();
  const pct = normConf(value);
  return (
    <View className={styles.confidence}>
      {showLabel && (
        <View className={styles.confidenceMeta}>
          <Text className={styles.confidenceLabel}>{t('置信度')}</Text>
          <Text className={styles.confidenceValue}>{pct}%</Text>
        </View>
      )}
      <View className={styles.confidenceTrack}>
        <View className={styles.confidenceFill} style={{ width: `${pct}%` }} />
      </View>
    </View>
  );
};

const AnalysisTypeTag: React.FC<{ type: AnalysisType }> = ({ type }) => {
  const { t } = useI18n();
  return <Tag type={TYPE_TAG_COLOR[type]}>{t(ANALYSIS_TYPE_LABELS[type])}</Tag>;
};

/** 立场标签：bullish→红（涨），bearish→绿（跌），neutral→灰（A股惯例） */
const StanceTag: React.FC<{ stance?: Stance }> = ({ stance }) => {
  const { t } = useI18n();
  if (!stance) return null;
  const cfg: Record<Stance, { label: string; cls: string }> = {
    bullish: { label: t('看多'), cls: styles.stanceBullish },
    bearish: { label: t('看空'), cls: styles.stanceBearish },
    neutral: { label: t('中性'), cls: styles.stanceNeutral },
  };
  const item = cfg[stance];
  return (
    <View className={`${styles.stanceTag} ${item.cls}`}>
      <Text>{item.label}</Text>
    </View>
  );
};

/**
 * AI 分析页（tabBar）
 * 五个内部 Tab：手动分析 / 市场分析 / 持仓分析 / 辩论 / 历史
 * 对接后端 aiService / tradingService 真实接口
 */
const AiPage: React.FC = () => {
  const isAuthenticated = useAuthStore((s) => s.isAuthenticated);
  const { t, locale } = useI18n();
  useLocalizedTitle('AI 分析');

  const [activeTab, setActiveTab] = useState<TabKey>('manual');
  const [refreshing, setRefreshing] = useState(false);
  const loadedRef = useRef<Set<TabKey>>(new Set<TabKey>(['manual']));

  // 手动分析
  const [symbol, setSymbol] = useState('BTC/USDT');
  const [interval, setIntervalVal] = useState('1h');
  const [analysisType, setAnalysisType] = useState<AnalysisType>('comprehensive');
  const [symbolOptions, setSymbolOptions] = useState<string[]>([]);
  const [analyzing, setAnalyzing] = useState(false);
  const [analysisResult, setAnalysisResult] = useState<AnalysisResult | null>(null);
  const [analysisError, setAnalysisError] = useState('');

  // 市场分析
  const [marketLoading, setMarketLoading] = useState(false);
  const [marketItems, setMarketItems] = useState<AnalysisHistoryItem[]>([]);

  // 持仓分析
  const [posLoading, setPosLoading] = useState(false);
  const [positions, setPositions] = useState<Position[]>([]);
  const [posAnalyses, setPosAnalyses] = useState<Record<string, AnalysisResult | null>>({});

  // 辩论
  const [debateLoading, setDebateLoading] = useState(false);
  const [debates, setDebates] = useState<DebateSession[]>([]);
  const [currentDebateId, setCurrentDebateId] = useState('');
  const [debateStarting, setDebateStarting] = useState(false);
  const [debateSymbol, setDebateSymbol] = useState('BTC/USDT');

  // 历史
  const [historyLoading, setHistoryLoading] = useState(false);
  const [historyItems, setHistoryItems] = useState<AnalysisHistoryItem[]>([]);

  // ============ 数据加载 ============

  const loadSymbols = useCallback(async () => {
    try {
      setSymbolOptions(await aiService.getSymbols());
    } catch {
      // 静默：品种列表加载失败时仍可手动输入
    }
  }, []);

  const loadMarket = useCallback(async () => {
    setMarketLoading(true);
    try {
      setMarketItems(await aiService.getAnalysisHistory('comprehensive', { limit: 10 }));
    } catch {
      Taro.showToast({ title: t('加载失败'), icon: 'none' });
    } finally {
      setMarketLoading(false);
    }
  }, [t]);

  const loadPositions = useCallback(async () => {
    setPosLoading(true);
    try {
      const pos = await tradingService.getPositions();
      setPositions(pos);
      const results = await Promise.allSettled(
        pos.map((p) =>
          aiService.analyze('comprehensive', { symbol: p.symbol, interval: '1h' }),
        ),
      );
      const map: Record<string, AnalysisResult | null> = {};
      pos.forEach((p, idx) => {
        const r = results[idx];
        map[p.symbol] = r.status === 'fulfilled' ? r.value : null;
      });
      setPosAnalyses(map);
    } catch {
      Taro.showToast({ title: t('加载失败'), icon: 'none' });
    } finally {
      setPosLoading(false);
    }
  }, [t]);

  const loadDebates = useCallback(async () => {
    setDebateLoading(true);
    try {
      const list = await aiService.listDebates({ limit: 10 });
      setDebates(list);
      setCurrentDebateId((prev) => prev || list[0]?.debate_id || '');
    } catch {
      Taro.showToast({ title: t('加载失败'), icon: 'none' });
    } finally {
      setDebateLoading(false);
    }
  }, [t]);

  const loadHistory = useCallback(async () => {
    setHistoryLoading(true);
    try {
      const [tech, comp] = await Promise.all([
        aiService.getAnalysisHistory('technical', { limit: 20 }),
        aiService.getAnalysisHistory('comprehensive', { limit: 20 }),
      ]);
      const merged = [...tech, ...comp].sort(
        (a, b) => new Date(b.created_at).getTime() - new Date(a.created_at).getTime(),
      );
      setHistoryItems(merged);
    } catch {
      Taro.showToast({ title: t('加载失败'), icon: 'none' });
    } finally {
      setHistoryLoading(false);
    }
  }, [t]);

  useEffect(() => {
    void loadSymbols();
  }, [loadSymbols]);

  // ============ 交互 ============

  const switchTab = (tab: TabKey) => {
    setActiveTab(tab);
    if (loadedRef.current.has(tab)) return;
    loadedRef.current.add(tab);
    if (tab === 'market') void loadMarket();
    else if (tab === 'positions') void loadPositions();
    else if (tab === DEBATE_TAB) void loadDebates();
    else if (tab === 'history') void loadHistory();
  };

  const handleRefresh = async () => {
    setRefreshing(true);
    try {
      if (activeTab === 'market') await loadMarket();
      else if (activeTab === 'positions') await loadPositions();
      else if (activeTab === DEBATE_TAB) await loadDebates();
      else if (activeTab === 'history') await loadHistory();
    } finally {
      setRefreshing(false);
    }
  };

  const handleAnalyze = async () => {
    setAnalyzing(true);
    setAnalysisError('');
    setAnalysisResult(null);
    try {
      setAnalysisResult(
        await aiService.analyze(analysisType, { symbol, interval }),
      );
    } catch (err) {
      setAnalysisError(err instanceof Error ? err.message : t('分析失败'));
    } finally {
      setAnalyzing(false);
    }
  };

  const handleStartDebate = async () => {
    setDebateStarting(true);
    try {
      const { debate_id } = await aiService.startDebate({ symbol: debateSymbol });
      let session: DebateSession | null = null;
      try {
        session = await aiService.getDebate(debate_id);
      } catch {
        // 辩论可能仍在生成中，详情拉取失败时保留 debate_id
      }
      setDebates((prev) => {
        const filtered = prev.filter((d) => d.debate_id !== debate_id);
        return session ? [session as DebateSession, ...filtered] : filtered;
      });
      setCurrentDebateId(debate_id);
      Taro.showToast({ title: t('辩论已启动'), icon: 'success' });
    } catch (err) {
      Taro.showToast({
        title: err instanceof Error ? err.message : t('启动失败'),
        icon: 'none',
      });
    } finally {
      setDebateStarting(false);
    }
  };

  const openAnalysisDetail = (analysisId: string) => {
    try {
      const query = buildSafeNavigationQuery({ id: analysisId });
      Taro.navigateTo({
        url: `/pages/ai-analysis-detail/index${query}`,
      }).catch(() => {
        Taro.showToast({ title: t('页面不存在'), icon: 'none' });
      });
    } catch {
      Taro.showToast({ title: t('操作失败'), icon: 'none' });
    }
  };

  // ============ 未登录引导 ============

  if (!isAuthenticated) {
    return (
      <View className={styles.aiPage}>
        <View className={styles.loginWrap}>
          <Empty
            title={t('未登录')}
            description={t('登录后可使用 AI 分析能力')}
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

  // ============ 各 Tab 渲染 ============

  const renderManual = () => {
    return (
      <View className={styles.tabPane}>
        <View className={styles.card}>
          <View className={styles.formGroup}>
            <Text className={styles.formLabel}>{t('品种')}</Text>
            <Input
              className={styles.nutInput}
              type="text"
              placeholder={t('请输入品种，如 BTC/USDT')}
              value={symbol}
              maxLength={32}
              clearable
              onChange={(val) => setSymbol(val)}
            />
            {symbolOptions.length > 0 && (
              <ScrollView scrollX className={styles.symbolScroll}>
                <View className={styles.symbolChips}>
                  {symbolOptions.slice(0, 8).map((s) => (
                    <View
                      key={s}
                      className={`${styles.symbolChip} ${symbol === s ? styles.symbolChipActive : ''}`}
                      onClick={() => setSymbol(s)}
                    >
                      <Text>{s}</Text>
                    </View>
                  ))}
                </View>
              </ScrollView>
            )}
          </View>

          <View className={styles.formGroup}>
            <Text className={styles.formLabel}>{t('时间周期')}</Text>
            <ChipRow
              options={INTERVALS}
              value={interval}
              onChange={(v) => setIntervalVal(v)}
            />
          </View>

          <View className={styles.formGroup}>
            <Text className={styles.formLabel}>{t('分析类型')}</Text>
            <ChipRow
              options={ANALYSIS_TYPES}
              value={analysisType}
              onChange={(v) => setAnalysisType(v as AnalysisType)}
            />
          </View>

          <Button
            type="primary"
            block
            loading={analyzing}
            className={styles.actionButton}
            onClick={handleAnalyze}
          >
            {t('开始分析')}
          </Button>
        </View>

        <View className={styles.card}>
          {analyzing ? (
            <Skeleton rows={6} animated />
          ) : analysisError ? (
            <View className={styles.errorText}>
              <Text>{analysisError}</Text>
            </View>
          ) : analysisResult ? (
            <View className={styles.result}>
              <View className={styles.resultHeader}>
                <Text className={styles.resultSymbol}>{analysisResult.symbol}</Text>
                <AnalysisTypeTag type={analysisResult.analysis_type} />
              </View>

              {analysisResult.summary && (
                <View className={styles.resultBlock}>
                  <Text className={styles.blockTitle}>{t('分析摘要')}</Text>
                  <Text className={styles.blockText}>{analysisResult.summary}</Text>
                </View>
              )}

              <ConfidenceBar value={analysisResult.confidence} />

              {analysisResult.key_points && analysisResult.key_points.length > 0 && (
                <View className={styles.resultBlock}>
                  <Text className={styles.blockTitle}>{t('关键点')}</Text>
                  <View className={styles.bulletList}>
                    {analysisResult.key_points.map((kp, idx) => (
                      <View key={idx} className={styles.bulletItem}>
                        <Text className={styles.bulletDot}>·</Text>
                        <Text className={styles.bulletText}>{kp}</Text>
                      </View>
                    ))}
                  </View>
                </View>
              )}

              {analysisResult.recommendations &&
                analysisResult.recommendations.length > 0 && (
                  <View className={styles.resultBlock}>
                    <Text className={styles.blockTitle}>{t('建议')}</Text>
                    <View className={styles.bulletList}>
                      {analysisResult.recommendations.map((r, idx) => (
                        <View key={idx} className={styles.bulletItem}>
                          <Text className={styles.bulletDot}>·</Text>
                          <Text className={styles.bulletText}>{r}</Text>
                        </View>
                      ))}
                    </View>
                  </View>
                )}

              {analysisResult.result && (
                <View className={styles.resultBlock}>
                  <Text className={styles.blockTitle}>{t('详细分析')}</Text>
                  <Text className={styles.blockText}>{analysisResult.result}</Text>
                </View>
              )}
            </View>
          ) : (
            <Empty
              title={t('暂无分析结果')}
              description={t('点击“开始分析”生成结果')}
            />
          )}
        </View>
      </View>
    );
  };

  const renderMarket = () => {
    const avgConf = marketItems.length
      ? Math.round(
          marketItems.reduce((s, i) => s + normConf(i.confidence), 0) / marketItems.length,
        )
      : 0;
    const level: Stance = avgConf >= 66 ? 'bullish' : avgConf >= 40 ? 'neutral' : 'bearish';
    const sentimentLabel =
      level === 'bullish' ? t('乐观') : level === 'neutral' ? t('中性') : t('谨慎');
    const gaugeCls =
      level === 'bullish'
        ? styles.gaugeBullish
        : level === 'bearish'
          ? styles.gaugeBearish
          : styles.gaugeNeutral;

    return (
      <View className={styles.tabPane}>
        <View className={`${styles.card} ${styles.gaugeCard}`}>
          <View className={styles.gaugeHeader}>
            <View>
              <Text className={styles.gaugeEyebrow}>{t('市场综合情绪')}</Text>
              <Text className={styles.gaugeValue}>{sentimentLabel}</Text>
            </View>
            <Text className={styles.gaugeScore}>{avgConf}</Text>
          </View>
          <View className={styles.gaugeTrack}>
            <View
              className={`${styles.gaugeFill} ${gaugeCls}`}
              style={{ width: `${avgConf}%` }}
            />
          </View>
          <Text className={styles.gaugeHint}>
            {t('综合分析品种 {n}', { n: marketItems.length })}
          </Text>
        </View>

        <View className={styles.sectionHeader}>
          <Text className={styles.sectionTitle}>{t('主要品种分析')}</Text>
        </View>

        {marketLoading ? (
          <View className={styles.card}>
            <Skeleton rows={4} animated />
          </View>
        ) : marketItems.length === 0 ? (
          <View className={styles.card}>
            <Empty title={t('暂无市场分析')} />
          </View>
        ) : (
          marketItems.map((item) => (
            <View
              key={item.analysis_id}
              className={`${styles.card} ${styles.clickable}`}
              onClick={() => openAnalysisDetail(item.analysis_id)}
            >
              <View className={styles.listHeader}>
                <Text className={styles.listTitle}>{item.symbol}</Text>
                <AnalysisTypeTag type={item.analysis_type} />
              </View>
              <Text className={styles.listSummary}>{item.summary}</Text>
              <View className={styles.listFooter}>
                <ConfidenceBar value={item.confidence} showLabel={false} />
                <Text className={styles.listTime}>
                  {formatTime(item.created_at, locale)}
                </Text>
              </View>
            </View>
          ))
        )}
      </View>
    );
  };

  const renderPositions = () => {
    return (
      <View className={styles.tabPane}>
        {posLoading ? (
          <View className={styles.card}>
            <Skeleton rows={5} animated />
          </View>
        ) : positions.length === 0 ? (
          <View className={styles.card}>
            <Empty
              title={t('当前无持仓')}
              description={t('持有仓位后可分析')}
            />
          </View>
        ) : (
          positions.map((p) => {
            const analysis = posAnalyses[p.symbol];
            return (
              <View
                key={p.symbol}
                className={`${styles.card} ${styles.clickable}`}
                onClick={() => analysis && openAnalysisDetail(analysis.analysis_id)}
              >
                <View className={styles.listHeader}>
                  <View className={styles.symbolRow}>
                    <Text className={styles.listTitle}>{p.symbol}</Text>
                    <Tag type={p.side === 'long' ? 'danger' : p.side === 'short' ? 'success' : 'default'}>
                      {t(p.side === 'long' ? '多' : p.side === 'short' ? '空' : '净')}
                    </Tag>
                  </View>
                  <Text
                    className={
                      (p.unrealized_pnl ?? 0) >= 0 ? styles.pnlUp : styles.pnlDown
                    }
                  >
                    {(p.unrealized_pnl ?? 0) >= 0 ? '+' : ''}
                    {(p.unrealized_pnl ?? 0).toFixed(2)}
                  </Text>
                </View>
                {analysis ? (
                  <>
                    <Text className={styles.listSummary}>{analysis.summary || analysis.result}</Text>
                    <View className={styles.listFooter}>
                      <ConfidenceBar value={analysis.confidence} showLabel={false} />
                      <Text className={styles.listLink}>{t('查看详情 ›')}</Text>
                    </View>
                  </>
                ) : (
                  <Text className={styles.errorText}>{t('分析失败')}</Text>
                )}
              </View>
            );
          })
        )}
      </View>
    );
  };

  const renderDebate = () => {
    const current =
      debates.find((d) => d.debate_id === currentDebateId) || debates[0] || null;

    return (
      <View className={styles.tabPane}>
        <View className={styles.card}>
          <View className={styles.formGroup}>
            <Text className={styles.formLabel}>{t('辩论品种')}</Text>
            <Input
              className={styles.nutInput}
              type="text"
              placeholder={t('请输入品种，如 BTC/USDT')}
              value={debateSymbol}
              maxLength={32}
              clearable
              onChange={(val) => setDebateSymbol(val)}
            />
          </View>
          <Button
            type="primary"
            block
            loading={debateStarting}
            className={styles.actionButton}
            onClick={handleStartDebate}
          >
            {t('启动新辩论')}
          </Button>
        </View>

        {debateLoading ? (
          <View className={styles.card}>
            <Skeleton rows={5} animated />
          </View>
        ) : !current ? (
          <View className={styles.card}>
            <Empty
              title={t('暂无辩论记录')}
              description={t('启动辩论后查看多空观点')}
            />
          </View>
        ) : (
          <>
            <View className={styles.card}>
              <View className={styles.resultHeader}>
                <Text className={styles.resultSymbol}>{current.symbol}</Text>
                <Tag type={current.status === 'completed' ? 'success' : 'primary'}>
                  {t(current.status === 'completed' ? '已完成' : '进行中')}
                </Tag>
              </View>

              {current.events && current.events.length > 0 ? (
                <View className={styles.debateEvents}>
                  {current.events.map((ev: DebateEvent, idx) => (
                    <View key={idx} className={styles.debateEvent}>
                      <View className={styles.debateEventHeader}>
                        <Text className={styles.debateAgent}>{ev.agent}</Text>
                        <StanceTag stance={ev.stance} />
                      </View>
                      <Text className={styles.debateMessage}>{ev.message}</Text>
                    </View>
                  ))}
                </View>
              ) : (
                <Text className={styles.errorText}>{t('辩论生成中，请稍后刷新')}</Text>
              )}

              {current.final_recommendation && (
                <View className={styles.recommendCard}>
                  <Text className={styles.recommendTitle}>{t('最终建议')}</Text>
                  <Text className={styles.recommendText}>
                    {current.final_recommendation}
                  </Text>
                </View>
              )}
            </View>

            {debates.length > 1 && (
              <ScrollView scrollX className={styles.symbolScroll}>
                <View className={styles.symbolChips}>
                  {debates.map((d) => (
                    <View
                      key={d.debate_id}
                      className={`${styles.symbolChip} ${d.debate_id === current.debate_id ? styles.symbolChipActive : ''}`}
                      onClick={() => setCurrentDebateId(d.debate_id)}
                    >
                      <Text>{d.symbol}</Text>
                    </View>
                  ))}
                </View>
              </ScrollView>
            )}
          </>
        )}
      </View>
    );
  };

  const renderHistory = () => {
    return (
      <View className={styles.tabPane}>
        <View className={styles.sectionHeader}>
          <Text className={styles.sectionTitle}>{t('分析历史')}</Text>
          <Text className={styles.refreshLink} onClick={() => void loadHistory()}>
            {t('刷新')}
          </Text>
        </View>
        {historyLoading ? (
          <View className={styles.card}>
            <Skeleton rows={5} animated />
          </View>
        ) : historyItems.length === 0 ? (
          <View className={styles.card}>
            <Empty title={t('暂无分析历史')} />
          </View>
        ) : (
          historyItems.map((item) => (
            <View
              key={item.analysis_id}
              className={`${styles.card} ${styles.clickable}`}
              onClick={() => openAnalysisDetail(item.analysis_id)}
            >
              <View className={styles.listHeader}>
                <Text className={styles.listTitle}>{item.symbol}</Text>
                <AnalysisTypeTag type={item.analysis_type} />
              </View>
              <Text className={styles.listSummary}>{item.summary}</Text>
              <View className={styles.listFooter}>
                <ConfidenceBar value={item.confidence} showLabel={false} />
                <Text className={styles.listTime}>
                  {formatTime(item.created_at, locale)}
                </Text>
              </View>
            </View>
          ))
        )}
      </View>
    );
  };

  return (
    <View className={styles.aiPage}>
      <View className={styles.tabBar}>
        <ScrollView scrollX className={styles.tabScroll}>
          <View className={styles.tabList}>
            {TABS.map((tab) => (
              <View
                key={tab.key}
                className={`${styles.tabItem} ${activeTab === tab.key ? styles.tabActive : ''}`}
                onClick={() => switchTab(tab.key)}
              >
                <Text>{t(tab.label)}</Text>
              </View>
            ))}
          </View>
        </ScrollView>
      </View>

      <ScrollView
        scrollY
        className={styles.content}
        refresherEnabled={activeTab === 'history'}
        refresherTriggered={refreshing}
        onRefresherRefresh={() => void handleRefresh()}
      >
        {activeTab === 'manual' && renderManual()}
        {activeTab === 'market' && renderMarket()}
        {activeTab === 'positions' && renderPositions()}
        {activeTab === 'debate' && renderDebate()}
        {activeTab === 'history' && renderHistory()}
        <View className={styles.bottomSafe} />
      </ScrollView>
    </View>
  );
};

export default AiPage;

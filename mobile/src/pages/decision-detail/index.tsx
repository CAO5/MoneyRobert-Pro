import React, { useEffect, useState } from 'react';
import { View, Text, Button } from '@tarojs/components';
import Taro, { useRouter } from '@tarojs/taro';
import { signalService } from '@/services/signal';
import type { DecisionCard } from '@/types/signal';
import { DECISION_ACTION_LABELS } from '@/types/signal';
import EmptyState from '@/components/EmptyState';
import { useI18n, useLocalizedTitle } from '@/store/language';
import styles from './index.module.scss';

/**
 * 决策卡详情页（二级页面）
 * 展示：建议动作、概率分布、关键指标、失效条件、数据血缘
 * 对应深度研究报告"决策卡详情页示意"原型
 */
const DecisionDetailPage: React.FC = () => {
  const router = useRouter();
  const cardId = router.params.id;
  const { t, locale } = useI18n();
  useLocalizedTitle('决策卡');

  const [card, setCard] = useState<DecisionCard | null>(null);
  const [loading, setLoading] = useState(true);

  useEffect(() => {
    const load = async () => {
      if (!cardId) {
        setLoading(false);
        return;
      }
      try {
        const data = await signalService.getCard(cardId);
        setCard(data);
      } catch {
        Taro.showToast({ title: t('加载失败'), icon: 'none' });
      } finally {
        setLoading(false);
      }
    };
    load();
  }, [cardId]);

  const handleBack = () => {
    Taro.navigateBack({ delta: 1 }).catch(() => {
      Taro.switchTab({ url: '/pages/business/index' });
    });
  };

  const handleAction = (action: string) => {
    Taro.showToast({
      title: t('已{action}', { action: t(action) }),
      icon: 'success',
    });
  };

  if (loading) {
    return (
      <View className={styles.detailPage}>
        <EmptyState title={t('加载中...')} />
      </View>
    );
  }

  if (!card) {
    return (
      <View className={styles.detailPage}>
        <EmptyState title={t('决策卡不存在')} description={t('可能已被删除或链接错误')} actionText={t('返回')} onAction={handleBack} />
      </View>
    );
  }

  const actionLabel = DECISION_ACTION_LABELS[card.suggested_action] || card.suggested_action;
  const evClass = card.expected_value > 0 ? styles.positive : styles.negative;
  const horizonMin = Math.round(card.target_horizon_sec / 60);

  return (
    <View className={styles.detailPage}>
      {/* 顶部建议卡 */}
      <View className={styles.suggestionCard}>
        <View className={styles.suggestionHeader}>
          <View className={styles.backButton} onClick={handleBack}>
            <Text className={styles.backIcon}>‹</Text>
          </View>
          <Text className={styles.shareButton}>{t('分享')}</Text>
        </View>
        <View className={styles.symbolRow}>
          <Text className={styles.symbolText}>{card.symbol}</Text>
          <Text className={styles.actionTag}>{t(actionLabel)}</Text>
        </View>
        <View className={styles.metaRow}>
          <Text>{t('模型 {value}', { value: card.model_version })}</Text>
          <Text>{t('周期 {value}min', { value: horizonMin })}</Text>
        </View>
      </View>

      {/* 内容区 */}
      <View className={styles.content}>
        {/* 概率分布 */}
        <View className={styles.contentCard}>
          <Text className={styles.cardTitle}>{t('概率分布')}</Text>
          <View className={styles.probSection}>
            <View className={styles.probRow}>
              <Text className={styles.probLabel}>{t('上涨')}</Text>
              <View className={styles.probBarWrap}>
                <View
                  className={`${styles.probFill} ${styles.probFillUp}`}
                  style={{ width: `${card.p_up * 100}%` }}
                />
              </View>
              <Text className={styles.probValue}>{(card.p_up * 100).toFixed(1)}%</Text>
            </View>
            <View className={styles.probRow}>
              <Text className={styles.probLabel}>{t('持平')}</Text>
              <View className={styles.probBarWrap}>
                <View
                  className={`${styles.probFill} ${styles.probFillFlat}`}
                  style={{ width: `${card.p_flat * 100}%` }}
                />
              </View>
              <Text className={styles.probValue}>{(card.p_flat * 100).toFixed(1)}%</Text>
            </View>
            <View className={styles.probRow}>
              <Text className={styles.probLabel}>{t('下跌')}</Text>
              <View className={styles.probBarWrap}>
                <View
                  className={`${styles.probFill} ${styles.probFillDown}`}
                  style={{ width: `${card.p_down * 100}%` }}
                />
              </View>
              <Text className={styles.probValue}>{(card.p_down * 100).toFixed(1)}%</Text>
            </View>
          </View>
          <View className={styles.metricsGrid}>
            <View className={styles.metricItem}>
              <Text className={styles.metricLabel}>{t('净期望 EV')}</Text>
              <Text className={`${styles.metricValue} ${evClass}`}>
                {card.expected_value > 0 ? '+' : ''}{(card.expected_value * 100).toFixed(2)}bps
              </Text>
            </View>
            {card.worst_case !== undefined && (
              <View className={styles.metricItem}>
                <Text className={styles.metricLabel}>{t('最坏情形 CVaR')}</Text>
                <Text className={`${styles.metricValue} ${styles.negative}`}>
                  {(card.worst_case * 100).toFixed(2)}bps
                </Text>
              </View>
            )}
            <View className={styles.metricItem}>
              <Text className={styles.metricLabel}>{t('建议仓位')}</Text>
              <Text className={styles.metricValue}>{(card.position_suggestion * 100).toFixed(1)}%</Text>
            </View>
            {card.risk_budget_used !== undefined && (
              <View className={styles.metricItem}>
                <Text className={styles.metricLabel}>{t('已用风险预算')}</Text>
                <Text className={styles.metricValue}>{(card.risk_budget_used * 100).toFixed(1)}%</Text>
              </View>
            )}
          </View>
        </View>

        {/* 适用场景 */}
        {card.applicable_regime && (
          <View className={styles.contentCard}>
            <Text className={styles.cardTitle}>{t('适用场景')}</Text>
            <Text className={styles.textItem}>{t('市场状态：{value}', { value: card.applicable_regime })}</Text>
            {card.data_freshness_sec !== undefined && (
              <Text className={styles.textItem}>{t('数据新鲜度：{value} 秒前更新', { value: card.data_freshness_sec })}</Text>
            )}
          </View>
        )}

        {/* 失效条件 */}
        {card.invalidation_conditions && (
          <View className={styles.contentCard}>
            <Text className={styles.cardTitle}>{t('失效条件')}</Text>
            {Object.entries(card.invalidation_conditions).map(([k, v]) => (
              <Text key={k} className={styles.textItem}>
                {k}：{String(v)}
              </Text>
            ))}
          </View>
        )}

        {/* 数据血缘 */}
        <View className={styles.contentCard}>
          <Text className={styles.cardTitle}>{t('数据血缘')}</Text>
          <Text className={styles.textItem}>{t('模型版本：{value}', { value: card.model_version })}</Text>
          <Text className={styles.textItem}>{t('生成时间：{value}', { value: new Date(card.generated_at).toLocaleString(locale) })}</Text>
          <Text className={styles.textItem}>{t('决策卡 ID：{value}', { value: card.card_id })}</Text>
        </View>
      </View>

      {/* 底部操作区 */}
      <View className={styles.actionBar}>
        <Button
          className={`${styles.actionButton} ${styles.secondary}`}
          onClick={() => handleAction('加入关注')}
        >
          {t('加入关注')}
        </Button>
        <Button
          className={`${styles.actionButton} ${styles.primary}`}
          onClick={() => handleAction('提交待办')}
        >
          {t('提交待办')}
        </Button>
      </View>
    </View>
  );
};

export default DecisionDetailPage;

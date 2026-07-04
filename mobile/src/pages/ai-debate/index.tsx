import React, { useCallback, useEffect, useState } from 'react';
import { View, Text, ScrollView } from '@tarojs/components';
import Taro, { useRouter, usePullDownRefresh } from '@tarojs/taro';
import { Button, Empty, Skeleton, Tag } from '@nutui/nutui-react-taro';
import { aiService } from '@/services/ai';
import type { DebateSession, DebateEvent } from '@/types/ai';
import { useI18n, useLocalizedTitle } from '@/store/language';
import styles from './index.module.scss';

/**
 * AI 辩论页（二级页面）
 * 接收参数：id
 * 调用 aiService.getDebate(id)
 * 展示：辩论过程、多空观点、最终建议
 */
const AiDebatePage: React.FC = () => {
  const router = useRouter();
  const debateId = router.params.id || '';
  const { t, locale } = useI18n();
  useLocalizedTitle('AI 辩论');

  const [debate, setDebate] = useState<DebateSession | null>(null);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState(false);

  const load = useCallback(async () => {
    if (!debateId) {
      setLoading(false);
      setError(true);
      return;
    }
    setLoading(true);
    setError(false);
    try {
      const data = await aiService.getDebate(debateId);
      setDebate(data);
    } catch {
      setError(true);
    } finally {
      setLoading(false);
    }
  }, [debateId]);

  useEffect(() => {
    load();
  }, [load]);

  usePullDownRefresh(async () => {
    await load();
    Taro.stopPullDownRefresh();
  });

  const handleBack = () => {
    Taro.navigateBack({ delta: 1 }).catch(() => {
      Taro.switchTab({ url: '/pages/ai/index' });
    });
  };

  const stanceTag = (stance?: DebateEvent['stance']): { type: 'success' | 'danger' | 'default'; label: string } => {
    if (stance === 'bullish') return { type: 'success', label: t('看多') };
    if (stance === 'bearish') return { type: 'danger', label: t('看空') };
    return { type: 'default', label: t('中性') };
  };

  if (loading) {
    return (
      <View className={styles.detailPage}>
        <Skeleton rows={5} animated />
      </View>
    );
  }

  if (error) {
    return (
      <View className={styles.detailPage}>
        <Empty
          title={t('加载失败')}
          description={t('分析失败')}
          actions={[{ text: t('返回'), type: 'primary', onClick: handleBack }]}
        />
      </View>
    );
  }

  if (!debate) {
    return (
      <View className={styles.detailPage}>
        <Empty
          title={t('暂无辩论记录')}
          description={t('可能已被删除或链接错误')}
          actions={[{ text: t('返回'), type: 'primary', onClick: handleBack }]}
        />
      </View>
    );
  }

  const isRunning = debate.status === 'running' || debate.status === 'pending';

  return (
    <View className={styles.detailPage}>
      <View className={styles.header}>
        <View className={styles.headerTop}>
          <View className={styles.backButton} onClick={handleBack}>
            <Text className={styles.backIcon}>‹</Text>
          </View>
        </View>
        <Text className={styles.title}>{t('辩论品种')}：{debate.symbol}</Text>
        <View className={styles.metaRow}>
          <Tag type={isRunning ? 'warning' : 'success'}>
            {isRunning ? t('进行中') : t('已完成')}
          </Tag>
        </View>
      </View>

      <View className={styles.content}>
        {isRunning && (
          <View className={styles.contentCard}>
            <Text className={styles.hint}>{t('辩论生成中，请稍后刷新')}</Text>
          </View>
        )}

        <View className={styles.contentCard}>
          <Text className={styles.cardTitle}>{t('辩论过程')}</Text>
          {debate.events.length === 0 ? (
            <Text className={styles.hint}>{t('暂无辩论记录')}</Text>
          ) : (
            <ScrollView scrollY className={styles.eventList}>
              {debate.events.map((evt, idx) => {
                const tag = stanceTag(evt.stance);
                return (
                  <View key={idx} className={styles.eventItem}>
                    <View className={styles.eventHeader}>
                      <Text className={styles.agentName}>{evt.agent}</Text>
                      <Tag type={tag.type}>{tag.label}</Tag>
                    </View>
                    <Text className={styles.eventMessage}>{evt.message}</Text>
                    {evt.timestamp && (
                      <Text className={styles.eventTime}>
                        {new Date(evt.timestamp).toLocaleString(locale)}
                      </Text>
                    )}
                  </View>
                );
              })}
            </ScrollView>
          )}
        </View>

        {debate.final_recommendation && (
          <View className={styles.contentCard}>
            <Text className={styles.cardTitle}>{t('最终建议')}</Text>
            <Text className={styles.recommendation}>{debate.final_recommendation}</Text>
          </View>
        )}

        <Button className={styles.actionButton} type="primary" onClick={handleBack}>
          {t('返回')}
        </Button>
      </View>
    </View>
  );
};

export default AiDebatePage;

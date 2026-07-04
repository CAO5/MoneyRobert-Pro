import React, { useCallback, useEffect, useState } from 'react';
import { View, Text } from '@tarojs/components';
import Taro, { useRouter, usePullDownRefresh } from '@tarojs/taro';
import { Button, Empty, Skeleton, Tag } from '@nutui/nutui-react-taro';
import { aiService } from '@/services/ai';
import type { AnalysisHistoryItem } from '@/types/ai';
import { useI18n, useLocalizedTitle } from '@/store/language';
import styles from './index.module.scss';

/**
 * AI 分析详情页（二级页面）
 * 接收参数：id
 * 调用 aiService.getAnalysisHistory()
 * 展示：分析结果详情
 */
const AiAnalysisDetailPage: React.FC = () => {
  const router = useRouter();
  const analysisId = router.params.id || '';
  const { t, locale } = useI18n();
  useLocalizedTitle('AI 分析详情');

  const [analysis, setAnalysis] = useState<AnalysisHistoryItem | null>(null);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState(false);

  const load = useCallback(async () => {
    if (!analysisId) {
      setLoading(false);
      setError(true);
      return;
    }
    setLoading(true);
    setError(false);
    try {
      const list = await aiService.getAnalysisHistory('comprehensive', { limit: 100 });
      const target = list.find((a) => a.analysis_id === analysisId) || null;
      setAnalysis(target);
    } catch {
      setError(true);
    } finally {
      setLoading(false);
    }
  }, [analysisId]);

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

  if (!analysis) {
    return (
      <View className={styles.detailPage}>
        <Empty
          title={t('暂无分析结果')}
          description={t('可能已被删除或链接错误')}
          actions={[{ text: t('返回'), type: 'primary', onClick: handleBack }]}
        />
      </View>
    );
  }

  return (
    <View className={styles.detailPage}>
      <View className={styles.header}>
        <View className={styles.headerTop}>
          <View className={styles.backButton} onClick={handleBack}>
            <Text className={styles.backIcon}>‹</Text>
          </View>
        </View>
        <Text className={styles.title}>{analysis.symbol}</Text>
        <View className={styles.metaRow}>
          <Tag type="primary">{analysis.analysis_type}</Tag>
          <Tag type="default">{t('周期 {value}min', { value: analysis.interval })}</Tag>
          {analysis.confidence !== undefined && (
            <Tag type="success">{t('置信度 {value}%', { value: analysis.confidence })}</Tag>
          )}
        </View>
      </View>

      <View className={styles.content}>
        <View className={styles.contentCard}>
          <Text className={styles.cardTitle}>{t('分析摘要')}</Text>
          <Text className={styles.summary}>{analysis.summary}</Text>
        </View>

        <View className={styles.contentCard}>
          <Text className={styles.cardTitle}>{t('生成时间')}</Text>
          <Text className={styles.timeValue}>
            {new Date(analysis.created_at).toLocaleString(locale)}
          </Text>
        </View>

        <Button className={styles.actionButton} type="primary" onClick={handleBack}>
          {t('返回')}
        </Button>
      </View>
    </View>
  );
};

export default AiAnalysisDetailPage;

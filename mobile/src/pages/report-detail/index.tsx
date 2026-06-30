import React, { useCallback, useEffect, useMemo, useRef, useState } from 'react';
import { View, Text } from '@tarojs/components';
import Taro, { useRouter } from '@tarojs/taro';
import PageHeader from '@/components/PageHeader';
import { languageOptions } from '@/i18n';
import { reportService } from '@/services/report';
import type { AnalysisReport } from '@/types/report';
import { useI18n, useLocalizedTitle } from '@/store/language';
import styles from './index.module.scss';

function reportContentToText(content: unknown): string {
  if (typeof content === 'string') return content;
  if (content === null || content === undefined) return '';
  if (Array.isArray(content)) return content.map(reportContentToText).filter(Boolean).join('\n\n');
  if (typeof content === 'object') {
    return Object.entries(content as Record<string, unknown>)
      .map(([key, value]) => `${key}\n${reportContentToText(value)}`)
      .join('\n\n');
  }
  return String(content);
}

const ReportDetailPage: React.FC = () => {
  const router = useRouter();
  const { locale, t } = useI18n();
  const reportId = router.params.id || '';
  const [report, setReport] = useState<AnalysisReport | null>(null);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState(false);
  const requestSequence = useRef(0);
  useLocalizedTitle('报告详情');

  const languageLabel = useMemo(() => {
    const option = languageOptions.find((item) => item.value === locale);
    return t(option?.labelKey || 'English');
  }, [locale, t]);

  const loadReport = useCallback(async () => {
    const requestId = ++requestSequence.current;
    if (!reportId) {
      setError(true);
      setLoading(false);
      return;
    }
    setLoading(true);
    setError(false);
    try {
      const nextReport = await reportService.getReport(reportId, locale);
      if (requestId === requestSequence.current) setReport(nextReport);
    } catch {
      if (requestId === requestSequence.current) {
        setReport(null);
        setError(true);
      }
    } finally {
      if (requestId === requestSequence.current) setLoading(false);
    }
  }, [locale, reportId]);

  useEffect(() => {
    loadReport();
  }, [loadReport]);

  const handleBack = () => {
    Taro.navigateBack({ delta: 1 }).catch(() => {
      Taro.switchTab({ url: '/pages/business/index' });
    });
  };

  const content = reportContentToText(report?.content);

  return (
    <View className={styles.page}>
      <PageHeader title={t('报告详情')} onBack={handleBack} />

      {loading && (
        <View className={styles.state}>
          <Text className={styles.stateTitle}>{t('正在加载报告...')}</Text>
        </View>
      )}

      {!loading && error && (
        <View className={styles.state}>
          <Text className={styles.stateTitle}>{t('报告加载失败')}</Text>
          <Text className={styles.retry} onClick={loadReport}>{t('重新加载')}</Text>
        </View>
      )}

      {!loading && report && !report.language_match && (
        <View className={styles.state}>
          <Text className={styles.stateTitle}>
            {t('该报告暂时没有{language}版本', { language: languageLabel })}
          </Text>
          <Text className={styles.stateDesc}>
            {t('为避免展示错误语言，系统不会使用其他语言正文替代。请重新生成该语言版本。')}
          </Text>
        </View>
      )}

      {!loading && report?.language_match && (
        <View className={styles.report}>
          <Text className={styles.title}>{report.title}</Text>
          <Text className={styles.meta}>
            {t('生成语言：{language}', { language: languageLabel })}
          </Text>
          <View className={styles.content}>
            <Text userSelect>{content}</Text>
          </View>
          <Text className={styles.disclaimer}>
            {t('分析报告仅供参考，不构成投资建议。')}
          </Text>
        </View>
      )}
    </View>
  );
};

export default ReportDetailPage;

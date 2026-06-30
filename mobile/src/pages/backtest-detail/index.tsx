import React, { useEffect, useState } from 'react';
import { View, Text, Button } from '@tarojs/components';
import Taro, { useRouter } from '@tarojs/taro';
import { backtestService } from '@/services/backtest';
import type { BacktestJobDetail, BacktestReport, TrustLevelResponse } from '@/types/backtest';
import { BACKTEST_STATUS_LABELS, TRUST_LEVEL_LABELS } from '@/types/backtest';
import EmptyState from '@/components/EmptyState';
import { useI18n, useLocalizedTitle } from '@/store/language';
import styles from './index.module.scss';

type TabKey = 'overview' | 'performance' | 'trust';

/**
 * 回测详情页（二级页面）
 * 展示：进度、绩效指标、可信等级门禁
 * 对应深度研究报告"回测详情页示意"原型
 */
const BacktestDetailPage: React.FC = () => {
  const router = useRouter();
  const jobId = router.params.id;
  const { t } = useI18n();
  useLocalizedTitle('回测');

  const [job, setJob] = useState<BacktestJobDetail | null>(null);
  const [report, setReport] = useState<BacktestReport | null>(null);
  const [trust, setTrust] = useState<TrustLevelResponse | null>(null);
  const [activeTab, setActiveTab] = useState<TabKey>('overview');
  const [loading, setLoading] = useState(true);

  useEffect(() => {
    const load = async () => {
      if (!jobId) {
        setLoading(false);
        return;
      }
      try {
        // 并行加载详情、绩效、可信等级
        const [jobData, reportData, trustData] = await Promise.all([
          backtestService.getJob(jobId),
          backtestService.getReport(jobId),
          backtestService.getTrustLevel(jobId),
        ]);
        setJob(jobData);
        setReport(reportData);
        setTrust(trustData);
      } catch {
        Taro.showToast({ title: t('加载失败'), icon: 'none' });
      } finally {
        setLoading(false);
      }
    };
    load();
  }, [jobId]);

  const handleBack = () => {
    Taro.navigateBack({ delta: 1 }).catch(() => {
      Taro.switchTab({ url: '/pages/business/index' });
    });
  };

  const handleAction = (action: string) => {
    Taro.showToast({ title: t('已{action}', { action: t(action) }), icon: 'success' });
  };

  if (loading) {
    return (
      <View className={styles.detailPage}>
        <EmptyState title={t('加载中...')} />
      </View>
    );
  }

  if (!job) {
    return (
      <View className={styles.detailPage}>
        <EmptyState title={t('任务不存在')} actionText={t('返回')} onAction={handleBack} />
      </View>
    );
  }

  const statusLabel = BACKTEST_STATUS_LABELS[job.status] || job.status;
  const trustInfo = trust ? TRUST_LEVEL_LABELS[trust.trust_level] : null;

  return (
    <View className={styles.detailPage}>
      {/* 顶部头部 */}
      <View className={styles.header}>
        <View className={styles.headerTop}>
          <View className={styles.backButton} onClick={handleBack}>
            <Text className={styles.backIcon}>‹</Text>
          </View>
          <Text className={styles.moreButton}>⋯</Text>
        </View>
        <Text className={styles.jobId}>{job.job_id}</Text>
        <Text className={styles.jobName}>{job.job_name}</Text>
        <View className={styles.statusRow}>
          <Text className={styles.statusTag}>{t(statusLabel)}</Text>
          {trustInfo && (
            <Text className={styles.statusTag} style={{ color: trustInfo.color, background: 'rgba(255,255,255,0.95)' }}>
              {t('可信 {value}', { value: trustInfo.label })}
            </Text>
          )}
        </View>
      </View>

      <View className={styles.content}>
        {/* 运行中显示进度 */}
        {job.status === 'running' && (
          <View className={styles.contentCard}>
            <View className={styles.progressSection}>
              <View className={styles.progressLabel}>
                <Text>{t('运行进度')}</Text>
                <Text>{job.progress}%</Text>
              </View>
              <View className={styles.progressTrack}>
                <View className={styles.progressFill} style={{ width: `${job.progress}%` }} />
              </View>
            </View>
          </View>
        )}

        {/* Tab 切换 */}
        <View className={styles.tabBar}>
          {[
            { key: 'overview' as const, label: t('概览') },
            { key: 'performance' as const, label: t('绩效') },
            { key: 'trust' as const, label: t('可信门禁') },
          ].map((tab) => (
            <View
              key={tab.key}
              className={`${styles.tabItem} ${activeTab === tab.key ? styles.active : ''}`}
              onClick={() => setActiveTab(tab.key)}
            >
              <Text>{tab.label}</Text>
            </View>
          ))}
        </View>

        {/* 概览 Tab */}
        {activeTab === 'overview' && (
          <View className={styles.contentCard}>
            <View className={styles.metricsGrid}>
              <View className={styles.metricItem}>
                <Text className={styles.metricLabel}>{t('初始资金')}</Text>
                <Text className={styles.metricValue}>
                  {job.initial_equity.toLocaleString()}
                </Text>
              </View>
              <View className={styles.metricItem}>
                <Text className={styles.metricLabel}>{t('总交易数')}</Text>
                <Text className={styles.metricValue}>
                  {job.total_trades || 0}
                </Text>
              </View>
              <View className={styles.metricItem}>
                <Text className={styles.metricLabel}>{t('数据频率')}</Text>
                <Text className={styles.metricValue}>{job.data_frequency}</Text>
              </View>
              <View className={styles.metricItem}>
                <Text className={styles.metricLabel}>{t('交易标的')}</Text>
                <Text className={styles.metricValue} style={{ fontSize: '24rpx' }}>
                  {job.assets.join(', ')}
                </Text>
              </View>
            </View>
          </View>
        )}

        {/* 绩效 Tab */}
        {activeTab === 'performance' && report && (
          <View className={styles.contentCard}>
            <View className={styles.metricsGrid}>
              <View className={styles.metricItem}>
                <Text className={styles.metricLabel}>{t('总收益')}</Text>
                <Text className={`${styles.metricValue} ${(report.total_return || 0) > 0 ? styles.positive : styles.negative}`}>
                  {((report.total_return || 0) * 100).toFixed(2)}%
                </Text>
              </View>
              <View className={styles.metricItem}>
                <Text className={styles.metricLabel}>{t('年化收益')}</Text>
                <Text className={`${styles.metricValue} ${(report.annualized_return || 0) > 0 ? styles.positive : styles.negative}`}>
                  {((report.annualized_return || 0) * 100).toFixed(2)}%
                </Text>
              </View>
              <View className={styles.metricItem}>
                <Text className={styles.metricLabel}>{t('夏普比率')}</Text>
                <Text className={styles.metricValue}>
                  {(report.sharpe_ratio || 0).toFixed(2)}
                </Text>
              </View>
              <View className={styles.metricItem}>
                <Text className={styles.metricLabel}>{t('最大回撤')}</Text>
                <Text className={`${styles.metricValue} ${styles.negative}`}>
                  {((report.max_drawdown || 0) * 100).toFixed(2)}%
                </Text>
              </View>
              <View className={styles.metricItem}>
                <Text className={styles.metricLabel}>{t('胜率')}</Text>
                <Text className={styles.metricValue}>
                  {((report.win_rate || 0) * 100).toFixed(1)}%
                </Text>
              </View>
              <View className={styles.metricItem}>
                <Text className={styles.metricLabel}>{t('盈亏比')}</Text>
                <Text className={styles.metricValue}>
                  {(report.payoff_ratio || 0).toFixed(2)}
                </Text>
              </View>
            </View>
          </View>
        )}

        {/* 可信门禁 Tab */}
        {activeTab === 'trust' && trust && (
          <View className={styles.contentCard}>
            <View className={styles.checkItem}>
              <View className={`${styles.checkIcon} ${trust.test_coverage_passed ? styles.pass : styles.fail}`}>
                <Text>{trust.test_coverage_passed ? '✓' : '✗'}</Text>
              </View>
              <View className={styles.checkContent}>
                <Text className={styles.checkLabel}>{t('测试覆盖')}</Text>
                <Text className={styles.checkValue}>{t('通过率 {value}%', { value: (trust.test_pass_rate * 100).toFixed(1) })}</Text>
              </View>
            </View>
            <View className={styles.checkItem}>
              <View className={`${styles.checkIcon} ${trust.capital_conservation_passed ? styles.pass : styles.fail}`}>
                <Text>{trust.capital_conservation_passed ? '✓' : '✗'}</Text>
              </View>
              <View className={styles.checkContent}>
                <Text className={styles.checkLabel}>{t('资金保全')}</Text>
                <Text className={styles.checkValue}>{t(trust.capital_conservation_passed ? '通过' : '未通过')}</Text>
              </View>
            </View>
            <View className={styles.checkItem}>
              <View className={`${styles.checkIcon} ${trust.slippage_accounted ? styles.pass : styles.fail}`}>
                <Text>{trust.slippage_accounted ? '✓' : '✗'}</Text>
              </View>
              <View className={styles.checkContent}>
                <Text className={styles.checkLabel}>{t('滑点核算')}</Text>
                <Text className={styles.checkValue}>{t(trust.slippage_accounted ? '已计入' : '未计入')}</Text>
              </View>
            </View>
            <View className={styles.checkItem}>
              <View className={`${styles.checkIcon} ${trust.sample_size_sufficient ? styles.pass : styles.fail}`}>
                <Text>{trust.sample_size_sufficient ? '✓' : '✗'}</Text>
              </View>
              <View className={styles.checkContent}>
                <Text className={styles.checkLabel}>{t('样本量充足')}</Text>
                <Text className={styles.checkValue}>{t('共 {value} 笔交易', { value: trust.total_trades })}</Text>
              </View>
            </View>
            <View className={styles.checkItem}>
              <View className={`${styles.checkIcon} ${trust.walk_forward_validated ? styles.pass : styles.fail}`}>
                <Text>{trust.walk_forward_validated ? '✓' : '✗'}</Text>
              </View>
              <View className={styles.checkContent}>
                <Text className={styles.checkLabel}>{t('Walk-forward 验证')}</Text>
                <Text className={styles.checkValue}>{t(trust.walk_forward_validated ? '已通过' : '未通过')}</Text>
              </View>
            </View>
            <View className={styles.checkItem}>
              <View className={`${styles.checkIcon} ${trust.calibration_healthy ? styles.pass : styles.fail}`}>
                <Text>{trust.calibration_healthy ? '✓' : '✗'}</Text>
              </View>
              <View className={styles.checkContent}>
                <Text className={styles.checkLabel}>{t('校准健康')}</Text>
                <Text className={styles.checkValue}>{t(trust.calibration_healthy ? '正常' : '异常')}</Text>
              </View>
            </View>
          </View>
        )}
      </View>

      {/* 底部操作区 */}
      <View className={styles.actionBar}>
        <Button
          className={`${styles.actionButton} ${styles.secondary}`}
          onClick={() => handleAction('分享')}
        >
          {t('分享')}
        </Button>
        <Button
          className={`${styles.actionButton} ${styles.primary}`}
          onClick={() => handleAction('发起复评')}
        >
          {t('发起复评')}
        </Button>
      </View>
    </View>
  );
};

export default BacktestDetailPage;

import React, { useEffect, useState } from 'react';
import { View, Text, ScrollView } from '@tarojs/components';
import Taro, { usePullDownRefresh } from '@tarojs/taro';
import { workbenchService } from '@/services/workbench';
import type { WorkbenchData } from '@/types/workbench';
import StatCard from '@/components/StatCard';
import EmptyState from '@/components/EmptyState';
import { useAuthStore } from '@/store/auth';
import styles from './index.module.scss';

/**
 * 工作台页（tabBar 入口）
 * 按深度研究报告建议：聚合首屏数据，避免多接口并发
 * 展示：问候、待办/告警/消息汇总、关键指标、风险提醒、快捷入口、最近访问
 */
const WorkbenchPage: React.FC = () => {
  const user = useAuthStore((s) => s.user);
  const isAuthenticated = useAuthStore((s) => s.isAuthenticated);
  const [data, setData] = useState<WorkbenchData | null>(null);
  const [loading, setLoading] = useState(true);

  const loadData = async () => {
    setLoading(true);
    try {
      const result = await workbenchService.getWorkbench();
      setData(result);
    } catch (err) {
      console.error('[Workbench] load failed:', err);
      Taro.showToast({ title: '加载失败', icon: 'none' });
    } finally {
      setLoading(false);
    }
  };

  useEffect(() => {
    // 未登录时跳登录页
    if (!isAuthenticated) {
      const timer = setTimeout(() => {
        Taro.reLaunch({ url: '/pages/login/index' });
      }, 100);
      return () => clearTimeout(timer);
    }
    loadData();
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [isAuthenticated]);

  // 下拉刷新
  usePullDownRefresh(async () => {
    await loadData();
    Taro.stopPullDownRefresh();
  });

  if (!isAuthenticated) {
    return <View className={styles.workbenchPage} />;
  }

  if (loading && !data) {
    return (
      <View className={styles.workbenchPage}>
        <View className={styles.header}>
          <Text className={styles.greeting}>加载中...</Text>
        </View>
      </View>
    );
  }

  if (!data) {
    return (
      <View className={styles.workbenchPage}>
        <EmptyState title="加载失败" description="请下拉刷新重试" actionText="重新加载" onAction={loadData} />
      </View>
    );
  }

  // 跳转快捷入口
  const handleQuickEntry = (route?: string) => {
    if (!route) return;
    if (route.startsWith('/pages/todo') || route.startsWith('/pages/business')) {
      // tabBar 页面使用 switchTab，但带参数的需要 navigateTo
      if (route.includes('?')) {
        Taro.navigateTo({ url: route });
      } else {
        Taro.switchTab({ url: route.split('?')[0] });
      }
    } else {
      Taro.navigateTo({ url: route });
    }
  };

  // 跳转最近访问
  const handleRecent = (route: string, params?: Record<string, string>) => {
    const query = params ? `?${Object.entries(params).map(([k, v]) => `${k}=${encodeURIComponent(v)}`).join('&')}` : '';
    Taro.navigateTo({ url: `${route}${query}` }).catch(() => {
      Taro.switchTab({ url: route });
    });
  };

  // 跳转消息/待办
  const handleHeaderAction = (type: 'message' | 'todo') => {
    Taro.switchTab({ url: type === 'message' ? '/pages/message/index' : '/pages/todo/index' });
  };

  return (
    <View className={styles.workbenchPage}>
      {/* 顶部头部区域 */}
      <View className={styles.header}>
        <View className={styles.headerTop}>
          <View className={styles.orgSwitch}>
            <Text>MoneyRobert Pro</Text>
            <Text className={styles.orgArrow}>▼</Text>
          </View>
          <View className={styles.headerActions}>
            <View className={styles.headerAction} onClick={() => handleHeaderAction('message')}>
              <Text>消息</Text>
              {data.unread_message_count > 0 && (
                <View className={styles.actionBadge}>{data.unread_message_count}</View>
              )}
            </View>
          </View>
        </View>
        <Text className={styles.greeting}>{data.greeting}，{user?.username || '访客'}</Text>
        <View className={styles.summaryRow}>
          <View className={styles.summaryItem}>
            <Text className={styles.summaryNumber}>{data.todo_count}</Text>
            <Text>待办</Text>
          </View>
          <View className={styles.summaryItem}>
            <Text className={styles.summaryNumber}>{data.risk_alert_count}</Text>
            <Text>风险告警</Text>
          </View>
          <View className={styles.summaryItem}>
            <Text className={styles.summaryNumber}>{data.unread_message_count}</Text>
            <Text>未读消息</Text>
          </View>
        </View>
      </View>

      <ScrollView scrollY className={styles.content}>
        {/* 关键指标卡 */}
        <View className={styles.metricsGrid}>
          {data.metrics.map((metric, idx) => (
            <StatCard
              key={metric.key}
              label={metric.label}
              value={metric.value}
              unit={metric.unit}
              trend={metric.trend}
              changePercent={metric.change_percent}
              highlight={idx === 0}
            />
          ))}
        </View>

        {/* 风险提醒 */}
        {data.risk_alerts.length > 0 && (
          <View className={styles.section}>
            <View className={styles.sectionHeader}>
              <Text className={styles.sectionTitle}>风险提醒</Text>
              <Text className={styles.sectionMore}>查看全部 ›</Text>
            </View>
            {data.risk_alerts.map((alert) => (
              <View key={alert.id} className={styles.riskItem}>
                <View className={`${styles.riskDot} ${styles[alert.level]}`} />
                <View className={styles.riskContent}>
                  <Text className={styles.riskTitle}>{alert.title}</Text>
                  <Text className={styles.riskDesc}>{alert.description}</Text>
                </View>
              </View>
            ))}
          </View>
        )}

        {/* 快捷入口 */}
        <View className={styles.section}>
          <View className={styles.sectionHeader}>
            <Text className={styles.sectionTitle}>快捷入口</Text>
          </View>
          <View className={styles.quickEntries}>
            {data.quick_entries.map((entry) => (
              <View
                key={entry.key}
                className={styles.quickEntry}
                onClick={() => handleQuickEntry(entry.route)}
              >
                {entry.badge ? <View className={styles.quickBadge}>{entry.badge}</View> : null}
                <View className={styles.quickIcon}>
                  <Text>{entry.label.slice(0, 1)}</Text>
                </View>
                <Text className={styles.quickLabel}>{entry.label}</Text>
              </View>
            ))}
          </View>
        </View>

        {/* 最近访问 */}
        <View className={styles.section}>
          <View className={styles.sectionHeader}>
            <Text className={styles.sectionTitle}>最近访问</Text>
          </View>
          {data.recent_items.map((item) => (
            <View
              key={item.id}
              className={styles.recentItem}
              onClick={() => handleRecent(item.route, item.params)}
            >
              <View className={styles.recentIcon}>
                <Text>{item.type === 'symbol' ? 'S' : item.type === 'decision' ? 'D' : item.type === 'backtest' ? 'B' : 'R'}</Text>
              </View>
              <View className={styles.recentContent}>
                <Text className={styles.recentTitle}>{item.title}</Text>
                {item.subtitle && <Text className={styles.recentSubtitle}>{item.subtitle}</Text>}
              </View>
              <Text className={styles.recentArrow}>›</Text>
            </View>
          ))}
        </View>
      </ScrollView>
    </View>
  );
};

export default WorkbenchPage;

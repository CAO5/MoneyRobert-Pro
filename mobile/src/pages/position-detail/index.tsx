import React, { useCallback, useEffect, useState } from 'react';
import { View, Text } from '@tarojs/components';
import Taro, { useRouter, usePullDownRefresh } from '@tarojs/taro';
import { Button, Empty, Skeleton, Tag } from '@nutui/nutui-react-taro';
import { tradingService } from '@/services/trading';
import type { Position } from '@/types/trading';
import { useI18n, useLocalizedTitle } from '@/store/language';
import styles from './index.module.scss';

/**
 * 持仓详情页（二级页面）
 * 接收参数：symbol
 * 调用 tradingService.getPositions() 筛选对应 symbol
 * 展示：持仓详情、未实现盈亏、止损止盈设置
 */
const PositionDetailPage: React.FC = () => {
  const router = useRouter();
  const symbol = router.params.symbol || '';
  const { t, locale } = useI18n();
  useLocalizedTitle('持仓详情');

  const [position, setPosition] = useState<Position | null>(null);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState(false);

  const load = useCallback(async () => {
    if (!symbol) {
      setLoading(false);
      setError(true);
      return;
    }
    setLoading(true);
    setError(false);
    try {
      const list = await tradingService.getPositions();
      const target = list.find((p) => p.symbol === symbol) || null;
      setPosition(target);
    } catch {
      setError(true);
    } finally {
      setLoading(false);
    }
  }, [symbol]);

  useEffect(() => {
    load();
  }, [load]);

  usePullDownRefresh(async () => {
    await load();
    Taro.stopPullDownRefresh();
  });

  const handleBack = () => {
    Taro.navigateBack({ delta: 1 }).catch(() => {
      Taro.switchTab({ url: '/pages/market/index' });
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
          description={t('数据开小差了，请稍后重试')}
          actions={[{ text: t('返回'), type: 'primary', onClick: handleBack }]}
        />
      </View>
    );
  }

  if (!position) {
    return (
      <View className={styles.detailPage}>
        <Empty
          title={t('暂无数据')}
          description={t('当前无持仓')}
          actions={[{ text: t('返回'), type: 'primary', onClick: handleBack }]}
        />
      </View>
    );
  }

  const pnlPositive = position.unrealized_pnl >= 0;
  const sideLabel = position.side === 'long' ? t('多') : position.side === 'short' ? t('空') : t('净');

  return (
    <View className={styles.detailPage}>
      <View className={styles.header}>
        <View className={styles.headerTop}>
          <View className={styles.backButton} onClick={handleBack}>
            <Text className={styles.backIcon}>‹</Text>
          </View>
        </View>
        <Text className={styles.title}>{position.symbol}</Text>
        <View className={styles.metaRow}>
          <Tag type="primary">{sideLabel}</Tag>
          <Tag type="default">{t('杠杆')} {position.leverage}x</Tag>
        </View>
      </View>

      <View className={styles.content}>
        <View className={styles.contentCard}>
          <Text className={styles.cardTitle}>{t('持仓信息')}</Text>
          <View className={styles.contextItem}>
            <Text className={styles.contextLabel}>{t('数量')}</Text>
            <Text className={styles.contextValue}>{position.size}</Text>
          </View>
          <View className={styles.contextItem}>
            <Text className={styles.contextLabel}>{t('开仓均价')}</Text>
            <Text className={styles.contextValue}>{position.entry_price}</Text>
          </View>
          <View className={styles.contextItem}>
            <Text className={styles.contextLabel}>{t('标记价格')}</Text>
            <Text className={styles.contextValue}>{position.mark_price}</Text>
          </View>
          <View className={styles.contextItem}>
            <Text className={styles.contextLabel}>{t('保证金')}</Text>
            <Text className={styles.contextValue}>{position.margin}</Text>
          </View>
          {position.liquidation_price !== undefined && (
            <View className={styles.contextItem}>
              <Text className={styles.contextLabel}>{t('强平价格')}</Text>
              <Text className={styles.contextValue}>{position.liquidation_price}</Text>
            </View>
          )}
          {position.funding_rate !== undefined && (
            <View className={styles.contextItem}>
              <Text className={styles.contextLabel}>{t('资金费率')}</Text>
              <Text className={styles.contextValue}>{position.funding_rate}</Text>
            </View>
          )}
          {position.updated_at && (
            <View className={styles.contextItem}>
              <Text className={styles.contextLabel}>{t('更新时间')}</Text>
              <Text className={styles.contextValue}>
                {new Date(position.updated_at).toLocaleString(locale)}
              </Text>
            </View>
          )}
        </View>

        <View className={styles.contentCard}>
          <Text className={styles.cardTitle}>{t('未实现盈亏')}</Text>
          <View className={styles.pnlRow}>
            <Text className={`${styles.pnlValue} ${pnlPositive ? styles.positive : styles.negative}`}>
              {pnlPositive ? '+' : ''}{position.unrealized_pnl}
            </Text>
            <Text className={`${styles.pnlPct} ${pnlPositive ? styles.positive : styles.negative}`}>
              {pnlPositive ? '+' : ''}{position.unrealized_pnl_pct}%
            </Text>
          </View>
        </View>

        <View className={styles.contentCard}>
          <Text className={styles.cardTitle}>{t('止损止盈设置')}</Text>
          <Text className={styles.hint}>{t('止损止盈设置功能正在开发中')}</Text>
          <Button className={styles.actionButton} type="primary" onClick={handleBack}>
            {t('返回')}
          </Button>
        </View>
      </View>
    </View>
  );
};

export default PositionDetailPage;

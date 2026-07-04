import React, { useCallback, useEffect, useState } from 'react';
import { View, Text, ScrollView } from '@tarojs/components';
import Taro, { usePullDownRefresh } from '@tarojs/taro';
import { Empty, Skeleton, Tag } from '@nutui/nutui-react-taro';
import { tradingService } from '@/services/trading';
import type { Trade } from '@/types/trading';
import { useI18n, useLocalizedTitle } from '@/store/language';
import styles from './index.module.scss';

/**
 * 交易记录页（二级页面）
 * 调用 tradingService.getTrades({ limit: 50 })
 * 展示：成交记录列表
 */
const TradeHistoryPage: React.FC = () => {
  const { t, locale } = useI18n();
  useLocalizedTitle('交易记录');

  const [trades, setTrades] = useState<Trade[]>([]);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState(false);

  const load = useCallback(async () => {
    setLoading(true);
    setError(false);
    try {
      const list = await tradingService.getTrades({ limit: 50 });
      setTrades(list || []);
    } catch {
      setError(true);
    } finally {
      setLoading(false);
    }
  }, []);

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
      <View className={styles.page}>
        <View className={styles.skeletonWrap}>
          <Skeleton rows={6} animated />
        </View>
      </View>
    );
  }

  if (error) {
    return (
      <View className={styles.page}>
        <Empty
          title={t('加载失败')}
          description={t('数据开小差了，请稍后重试')}
          actions={[{ text: t('返回'), type: 'primary', onClick: handleBack }]}
        />
      </View>
    );
  }

  if (trades.length === 0) {
    return (
      <View className={styles.page}>
        <Empty
          title={t('暂无数据')}
          description={t('暂无交易记录')}
          actions={[{ text: t('返回'), type: 'primary', onClick: handleBack }]}
        />
      </View>
    );
  }

  return (
    <View className={styles.page}>
      <ScrollView scrollY className={styles.list}>
        {trades.map((trade) => (
          <View key={trade.trade_id} className={styles.tradeItem}>
            <View className={styles.itemHeader}>
              <Text className={styles.symbol}>{trade.symbol}</Text>
              <Tag type={trade.side === 'buy' ? 'success' : 'danger'}>
                {trade.side === 'buy' ? t('开多') : t('开空')}
              </Tag>
            </View>
            <View className={styles.itemBody}>
              <View className={styles.bodyItem}>
                <Text className={styles.bodyLabel}>{t('价格')}</Text>
                <Text className={styles.bodyValue}>{trade.price}</Text>
              </View>
              <View className={styles.bodyItem}>
                <Text className={styles.bodyLabel}>{t('数量')}</Text>
                <Text className={styles.bodyValue}>{trade.size}</Text>
              </View>
              <View className={styles.bodyItem}>
                <Text className={styles.bodyLabel}>{t('手续费')}</Text>
                <Text className={styles.bodyValue}>{trade.fee}</Text>
              </View>
              <View className={styles.bodyItem}>
                <Text className={styles.bodyLabel}>{t('时间')}</Text>
                <Text className={styles.bodyValue}>
                  {new Date(trade.timestamp).toLocaleString(locale)}
                </Text>
              </View>
            </View>
          </View>
        ))}
      </ScrollView>
    </View>
  );
};

export default TradeHistoryPage;

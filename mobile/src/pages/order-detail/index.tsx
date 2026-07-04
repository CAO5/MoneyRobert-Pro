import React, { useCallback, useEffect, useState } from 'react';
import { View, Text } from '@tarojs/components';
import Taro, { useRouter, usePullDownRefresh } from '@tarojs/taro';
import { Button, Empty, Skeleton, Tag } from '@nutui/nutui-react-taro';
import { tradingService } from '@/services/trading';
import type { Order } from '@/types/trading';
import { useI18n, useLocalizedTitle } from '@/store/language';
import styles from './index.module.scss';

/**
 * 订单详情页（二级页面）
 * 接收参数：id
 * 调用 tradingService.getOrders() 筛选对应 id
 * 展示：订单详情、状态、成交信息
 */
const OrderDetailPage: React.FC = () => {
  const router = useRouter();
  const orderId = router.params.id || '';
  const { t, locale } = useI18n();
  useLocalizedTitle('订单详情');

  const [order, setOrder] = useState<Order | null>(null);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState(false);

  const load = useCallback(async () => {
    if (!orderId) {
      setLoading(false);
      setError(true);
      return;
    }
    setLoading(true);
    setError(false);
    try {
      const { orders } = await tradingService.getOrders({ limit: 100 });
      const target = orders.find((o) => o.order_id === orderId) || null;
      setOrder(target);
    } catch {
      setError(true);
    } finally {
      setLoading(false);
    }
  }, [orderId]);

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

  const statusTypeMap: Record<string, 'default' | 'primary' | 'success' | 'warning' | 'danger'> = {
    pending: 'warning',
    open: 'primary',
    partial: 'warning',
    filled: 'success',
    cancelled: 'default',
    rejected: 'danger',
  };
  const statusLabelMap: Record<string, string> = {
    pending: '待成交',
    open: '挂单中',
    partial: '部分成交',
    filled: '已完成',
    cancelled: '已撤销',
    rejected: '已拒绝',
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

  if (!order) {
    return (
      <View className={styles.detailPage}>
        <Empty
          title={t('暂无数据')}
          description={t('订单不存在')}
          actions={[{ text: t('返回'), type: 'primary', onClick: handleBack }]}
        />
      </View>
    );
  }

  const sideLabel = order.side === 'buy' ? t('开多') : t('开空');
  const statusType = statusTypeMap[order.status] || 'default';

  return (
    <View className={styles.detailPage}>
      <View className={styles.header}>
        <View className={styles.headerTop}>
          <View className={styles.backButton} onClick={handleBack}>
            <Text className={styles.backIcon}>‹</Text>
          </View>
        </View>
        <Text className={styles.title}>{order.symbol}</Text>
        <View className={styles.metaRow}>
          <Tag type={order.side === 'buy' ? 'success' : 'danger'}>{sideLabel}</Tag>
          <Tag type={statusType}>{t(statusLabelMap[order.status] || order.status)}</Tag>
        </View>
      </View>

      <View className={styles.content}>
        <View className={styles.contentCard}>
          <Text className={styles.cardTitle}>{t('订单信息')}</Text>
          <View className={styles.contextItem}>
            <Text className={styles.contextLabel}>{t('订单 ID')}</Text>
            <Text className={styles.contextValue}>{order.order_id}</Text>
          </View>
          <View className={styles.contextItem}>
            <Text className={styles.contextLabel}>{t('订单类型')}</Text>
            <Text className={styles.contextValue}>{order.type}</Text>
          </View>
          <View className={styles.contextItem}>
            <Text className={styles.contextLabel}>{t('数量')}</Text>
            <Text className={styles.contextValue}>{order.quantity}</Text>
          </View>
          {order.price !== undefined && (
            <View className={styles.contextItem}>
              <Text className={styles.contextLabel}>{t('价格')}</Text>
              <Text className={styles.contextValue}>{order.price}</Text>
            </View>
          )}
          {order.leverage !== undefined && (
            <View className={styles.contextItem}>
              <Text className={styles.contextLabel}>{t('杠杆')}</Text>
              <Text className={styles.contextValue}>{order.leverage}x</Text>
            </View>
          )}
          {order.reduce_only !== undefined && (
            <View className={styles.contextItem}>
              <Text className={styles.contextLabel}>{t('仅减仓')}</Text>
              <Text className={styles.contextValue}>{order.reduce_only ? t('是') : t('否')}</Text>
            </View>
          )}
          <View className={styles.contextItem}>
            <Text className={styles.contextLabel}>{t('创建时间')}</Text>
            <Text className={styles.contextValue}>
              {new Date(order.created_at).toLocaleString(locale)}
            </Text>
          </View>
        </View>

        <View className={styles.contentCard}>
          <Text className={styles.cardTitle}>{t('成交信息')}</Text>
          {order.filled_quantity !== undefined && order.avg_price !== undefined ? (
            <>
              <View className={styles.contextItem}>
                <Text className={styles.contextLabel}>{t('已成交')}</Text>
                <Text className={styles.contextValue}>{order.filled_quantity}</Text>
              </View>
              <View className={styles.contextItem}>
                <Text className={styles.contextLabel}>{t('成交均价')}</Text>
                <Text className={styles.contextValue}>{order.avg_price}</Text>
              </View>
            </>
          ) : (
            <Text className={styles.hint}>{t('暂无成交信息')}</Text>
          )}
        </View>

        {(order.stop_loss !== undefined || order.take_profit !== undefined) && (
          <View className={styles.contentCard}>
            <Text className={styles.cardTitle}>{t('止损止盈')}</Text>
            {order.stop_loss !== undefined && (
              <View className={styles.contextItem}>
                <Text className={styles.contextLabel}>{t('止损价')}</Text>
                <Text className={styles.contextValue}>{order.stop_loss}</Text>
              </View>
            )}
            {order.take_profit !== undefined && (
              <View className={styles.contextItem}>
                <Text className={styles.contextLabel}>{t('止盈价')}</Text>
                <Text className={styles.contextValue}>{order.take_profit}</Text>
              </View>
            )}
          </View>
        )}

        <Button className={styles.actionButton} type="primary" onClick={handleBack}>
          {t('返回')}
        </Button>
      </View>
    </View>
  );
};

export default OrderDetailPage;

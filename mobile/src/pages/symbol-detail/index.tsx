import React from 'react';
import { View, Text } from '@tarojs/components';
import Taro, { useRouter } from '@tarojs/taro';
import PageHeader from '@/components/PageHeader';
import styles from './index.module.scss';

/**
 * 标的详情页（二级占位页）
 * 占位实现，避免编译报错
 * 后续可对接 /market/snapshots/{symbol}、/signals/decision-cards?symbol={symbol}
 */
const SymbolDetailPage: React.FC = () => {
  const router = useRouter();
  const symbol = router.params.symbol || '未知标的';

  const handleBack = () => {
    Taro.navigateBack({ delta: 1 }).catch(() => {
      Taro.switchTab({ url: '/pages/business/index' });
    });
  };

  return (
    <View className={styles.placeholderPage}>
      <PageHeader title={symbol} onBack={handleBack} />
      <View className={styles.placeholderContent}>
        <View className={styles.placeholderIcon}>
          <Text>{symbol.slice(0, 1)}</Text>
        </View>
        <Text className={styles.placeholderTitle}>{symbol} 详情</Text>
        <Text className={styles.placeholderDesc}>
          标的详情功能正在开发中{'\n'}当前可查看决策卡、回测详情中的相关数据
        </Text>
      </View>
    </View>
  );
};

export default SymbolDetailPage;

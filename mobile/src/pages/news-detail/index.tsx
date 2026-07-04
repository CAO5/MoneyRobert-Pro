import React, { useEffect } from 'react';
import { View, Text } from '@tarojs/components';
import Taro from '@tarojs/taro';
import { Empty } from '@nutui/nutui-react-taro';
import { useI18n } from '@/store/language';
import { useAuthStore } from '@/store/auth';
import styles from './index.module.scss';

/**
 * 新闻详情
 */
const NewsDetailPage: React.FC = () => {
  const { t } = useI18n();
  const isAuthenticated = useAuthStore((s) => s.isAuthenticated);

  useEffect(() => {
    if (!isAuthenticated) return;
    // 加载数据
  }, [isAuthenticated]);

  if (!isAuthenticated) {
    return (
      <View className={styles.page}>
        <Empty
          title={t('未登录')}
          description={t('请先登录')}
          actions={[
            {
              text: t('去登录'),
              type: 'primary',
              onClick: () => Taro.reLaunch({ url: '/pages/login/index' }),
            },
          ]}
        />
      </View>
    );
  }

  return (
    <View className={styles.page}>
      <View className={styles.header}>
        <Text className={styles.title}>{t('新闻详情')}</Text>
      </View>
      <View className={styles.content}>
        <View className={styles.placeholderCard}>
          <Text className={styles.placeholderIcon}>🚧</Text>
          <Text className={styles.placeholderTitle}>{t('功能开发中')}</Text>
          <Text className={styles.placeholderDesc}>{t('此功能正在开发中，敬请期待')}</Text>
          <Text className={styles.placeholderHint}>{t('可前往桌面端使用完整功能')}</Text>
        </View>
      </View>
    </View>
  );
};

export default NewsDetailPage;

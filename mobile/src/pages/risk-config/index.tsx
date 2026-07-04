import React, { useEffect, useState } from 'react';
import { View, Text } from '@tarojs/components';
import Taro, { useRouter } from '@tarojs/taro';
import { Empty, Skeleton, Button } from '@nutui/nutui-react-taro';
import { useAuthStore } from '@/store/auth';
import { useI18n, useLocalizedTitle } from '@/store/language';
import styles from './index.module.scss';

const RiskConfigPage: React.FC = () => {
  const router = useRouter();
  const { id, symbol } = router.params;
  const { isAuthenticated } = useAuthStore();
  const { t } = useI18n();
  useLocalizedTitle('风控配置');

  const [loading, setLoading] = useState(true);
  const [data, setData] = useState<unknown>(null);

  useEffect(() => {
    if (!isAuthenticated) {
      setTimeout(() => Taro.reLaunch({ url: '/pages/login/index' }), 100);
      return;
    }
    // 模拟加载
    setLoading(true);
    setTimeout(() => {
      setData({ id, symbol });
      setLoading(false);
    }, 500);
  }, [isAuthenticated, id, symbol]);

  if (!isAuthenticated) {
    return (
      <View className={styles.page}>
        <Empty
          title={t('未登录')}
          actions={[{ text: t('去登录'), type: 'primary', onClick: () => Taro.reLaunch({ url: '/pages/login/index' }) }]}
        />
      </View>
    );
  }

  return (
    <View className={styles.page}>
      {loading ? <Skeleton rows={5} animated /> : (
        <View className={styles.card}>
          <Text className={styles.title}>{t('风控配置')}</Text>
          {id && <Text className={styles.meta}>ID: {id}</Text>}
          {symbol && <Text className={styles.meta}>Symbol: {symbol}</Text>}
          <Text className={styles.desc}>{t('该功能正在建设中，请前往桌面端查看完整内容。')}</Text>
          <Button type="primary" className={styles.btn} onClick={() => Taro.navigateBack()}>
            {t('返回')}
          </Button>
        </View>
      )}
    </View>
  );
};

export default RiskConfigPage;

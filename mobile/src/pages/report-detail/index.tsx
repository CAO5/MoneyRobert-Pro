import React from 'react';
import { View, Text } from '@tarojs/components';
import Taro, { useRouter } from '@tarojs/taro';
import PageHeader from '@/components/PageHeader';
import styles from './index.module.scss';

/**
 * 报告详情页（二级占位页）
 * 完整报告功能建议在桌面端查看
 * 对接后端 /reports/* 接口的能力可在此扩展
 */
const ReportDetailPage: React.FC = () => {
  const router = useRouter();
  const reportId = router.params.id || '';

  const handleBack = () => {
    Taro.navigateBack({ delta: 1 }).catch(() => {
      Taro.switchTab({ url: '/pages/business/index' });
    });
  };

  return (
    <View className={styles.placeholderPage}>
      <PageHeader title="报告详情" onBack={handleBack} />
      <View className={styles.placeholderContent}>
        <View className={styles.placeholderIcon}>
          <Text>报</Text>
        </View>
        <Text className={styles.placeholderTitle}>报告功能开发中</Text>
        <Text className={styles.placeholderDesc}>
          完整报告预览、PDF 导出、分享功能建议前往桌面端查看{'\n'}
          {reportId ? `报告 ID：${reportId}` : ''}
        </Text>
      </View>
    </View>
  );
};

export default ReportDetailPage;

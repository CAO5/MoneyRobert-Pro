import React from 'react';
import { View, Text } from '@tarojs/components';
import Taro from '@tarojs/taro';
import classnames from 'classnames';
import styles from './index.module.scss';

/**
 * 页面头部组件
 * 用于二级页面的自定义导航栏（带返回箭头）
 */
export interface PageHeaderProps {
  title: string;
  /** 右侧附加内容 */
  right?: React.ReactNode;
  /** 是否显示返回箭头（默认 true） */
  showBack?: boolean;
  /** 自定义返回动作（默认 navigateBack） */
  onBack?: () => void;
  className?: string;
}

const PageHeader: React.FC<PageHeaderProps> = ({
  title,
  right,
  showBack = true,
  onBack,
  className,
}) => {
  const handleBack = () => {
    if (onBack) {
      onBack();
      return;
    }
    Taro.navigateBack({ delta: 1 }).catch(() => {
      // 无历史记录时回退到工作台
      Taro.switchTab({ url: '/pages/workbench/index' });
    });
  };

  return (
    <View className={classnames(styles.header, className)}>
      <View className={styles.left}>
        {showBack && (
          <View className={styles.back} onClick={handleBack}>
            <Text className={styles.backIcon}>‹</Text>
          </View>
        )}
        <Text className={styles.title}>{title}</Text>
      </View>
      {right && <View className={styles.right}>{right}</View>}
    </View>
  );
};

export default PageHeader;

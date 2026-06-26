import React from 'react';
import { View, Text } from '@tarojs/components';
import classnames from 'classnames';
import styles from './index.module.scss';

/**
 * 指标卡片组件
 * 用于工作台关键指标展示，支持趋势标识
 */
export interface StatCardProps {
  label: string;
  value: string | number;
  unit?: string;
  trend?: 'up' | 'down' | 'flat';
  changePercent?: number;
  /** 是否使用渐变背景（突出主指标） */
  highlight?: boolean;
  className?: string;
}

const trendIcon = {
  up: '↑',
  down: '↓',
  flat: '—',
};

const StatCard: React.FC<StatCardProps> = ({
  label,
  value,
  unit,
  trend,
  changePercent,
  highlight = false,
  className,
}) => {
  const rootClass = classnames(styles.statCard, highlight && styles.highlight, className);
  const trendClass = classnames(
    styles.trend,
    trend === 'up' && styles.up,
    trend === 'down' && styles.down,
    trend === 'flat' && styles.flat,
  );

  return (
    <View className={rootClass}>
      <Text className={styles.label}>{label}</Text>
      <View className={styles.valueRow}>
        <Text className={styles.value}>{value}</Text>
        {unit && <Text className={styles.unit}>{unit}</Text>}
      </View>
      {trend && (
        <View className={trendClass}>
          <Text className={styles.trendIcon}>{trendIcon[trend]}</Text>
          {changePercent !== undefined && (
            <Text className={styles.trendValue}>{Math.abs(changePercent).toFixed(2)}%</Text>
          )}
        </View>
      )}
    </View>
  );
};

export default StatCard;

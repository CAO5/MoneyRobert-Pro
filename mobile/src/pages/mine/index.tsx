import React from 'react';
import { View, Text, Button } from '@tarojs/components';
import Taro from '@tarojs/taro';
import { useAuthStore } from '@/store/auth';
import EmptyState from '@/components/EmptyState';
import { useI18n, useLocalizedTitle } from '@/store/language';
import styles from './index.module.scss';

/**
 * 我的页（tabBar）
 * 展示用户信息、统计、设置入口、退出登录
 */
const MinePage: React.FC = () => {
  const { user, isAuthenticated, logout } = useAuthStore();
  const { t } = useI18n();
  useLocalizedTitle('我的');

  // 未登录显示登录引导
  if (!isAuthenticated) {
    return (
      <View className={styles.minePage}>
        <EmptyState
          title={t('未登录')}
          description={t('登录后可查看个人信息与设置')}
          actionText={t('去登录')}
          onAction={() => Taro.reLaunch({ url: '/pages/login/index' })}
        />
      </View>
    );
  }

  // 跳转设置
  const handleNavigate = (url: string) => {
    Taro.navigateTo({ url }).catch(() => {
      Taro.showToast({ title: t('页面不存在'), icon: 'none' });
    });
  };

  // 退出登录二次确认
  const handleLogout = () => {
    Taro.showModal({
      title: t('退出登录'),
      content: t('确定退出当前账号吗？'),
      confirmText: t('退出'),
      confirmColor: '#f53f3f',
      success: (res) => {
        if (res.confirm) {
          logout();
        }
      },
    });
  };

  // 用户首字头像
  const avatarChar = (user?.username || 'U').slice(0, 1).toUpperCase();

  const menuGroups: Array<{
    items: Array<{ icon: string; label: string; route?: string; onClick?: () => void }>;
  }> = [
    {
      items: [
        { icon: 'S', label: t('设置'), route: '/pages/settings/index' },
        { icon: 'D', label: t('设备与安全'), route: '/pages/settings/index?type=security' },
      ],
    },
    {
      items: [
        { icon: 'N', label: t('通知设置'), route: '/pages/settings/index?type=notification' },
        { icon: 'A', label: t('显示设置'), route: '/pages/settings/index?type=display' },
      ],
    },
    {
      items: [
        { icon: '?', label: t('帮助与反馈'), onClick: () => Taro.showToast({ title: t('请前往桌面端查看'), icon: 'none' }) },
        { icon: 'i', label: t('关于 MoneyRobert'), onClick: () => Taro.showToast({ title: 'v1.0.0', icon: 'none' }) },
      ],
    },
  ];

  return (
    <View className={styles.minePage}>
      {/* 用户信息 */}
      <View className={styles.userHeader}>
        <View className={styles.userInfo}>
          <View className={styles.avatar}>
            <Text>{avatarChar}</Text>
          </View>
          <View className={styles.userDetail}>
            <Text className={styles.username}>{user?.username || '未登录'}</Text>
            {user?.email && <Text className={styles.userEmail}>{user.email}</Text>}
            {user?.role && <Text className={styles.roleTag}>{user.role}</Text>}
          </View>
        </View>
      </View>

      {/* 统计卡 */}
      <View className={styles.memberCard} onClick={() => handleNavigate('/pages/membership/index')}>
        <View>
          <Text className={styles.memberEyebrow}>{t('支持独立创作')}</Text>
          <Text className={styles.memberTitle}>{t('产品免费，赞助完全自愿')}</Text>
          <Text className={styles.memberDesc}>{t('赞助与盈利、策略权限和功能无关，可永久关闭提示')}</Text>
        </View>
        <Text className={styles.memberAction}>{t('支持作者 ›')}</Text>
      </View>

      {/* 统计卡 */}
      <View className={styles.statsRow}>
        <View className={styles.statItem}>
          <Text className={styles.statValue}>12</Text>
          <Text className={styles.statLabel}>{t('深度分析')}</Text>
        </View>
        <View className={styles.statItem}>
          <Text className={styles.statValue}>3</Text>
          <Text className={styles.statLabel}>{t('有效机会')}</Text>
        </View>
        <View className={styles.statItem}>
          <Text className={styles.statValue}>5</Text>
          <Text className={styles.statLabel}>{t('风险预警')}</Text>
        </View>
      </View>

      {/* 菜单分组 */}
      {menuGroups.map((group, gIdx) => (
        <View key={gIdx} className={styles.menuGroup}>
          {group.items.map((item, idx) => (
            <View
              key={idx}
              className={styles.menuItem}
              onClick={() => (item.onClick ? item.onClick() : handleNavigate(item.route || ''))}
            >
              <View className={styles.menuIcon}>
                <Text>{item.icon}</Text>
              </View>
              <Text className={styles.menuLabel}>{item.label}</Text>
              <Text className={styles.menuArrow}>›</Text>
            </View>
          ))}
        </View>
      ))}

      <Button className={styles.logoutButton} onClick={handleLogout}>
        {t('退出登录')}
      </Button>
    </View>
  );
};

export default MinePage;

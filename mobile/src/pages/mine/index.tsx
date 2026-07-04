import React, { useEffect, useState } from 'react';
import { View, Text } from '@tarojs/components';
import Taro from '@tarojs/taro';
import { Button, Empty, Tag } from '@nutui/nutui-react-taro';
import { useAuthStore } from '@/store/auth';
import { useI18n, useLocalizedTitle } from '@/store/language';
import { settingsService } from '@/services/settings';
import { workbenchService } from '@/services/workbench';
import { CLIENT_VERSION } from '@/types/common';
import type { SystemHealth } from '@/types/settings';
import styles from './index.module.scss';

/** 菜单图标配色（圆形彩色背景 + 白色字母） */
type IconColor =
  | 'blue'
  | 'purple'
  | 'green'
  | 'red'
  | 'yellow'
  | 'cyan'
  | 'gray'
  | 'darkBlue';

interface MenuItem {
  icon: string;
  color: IconColor;
  label: string;
  route?: string;
  /** 右侧附加文本（如版本号、配置状态） */
  value?: string;
  onClick?: () => void;
}

interface MenuGroup {
  title: string;
  items: MenuItem[];
}

/** 图标配色 → CSS Modules 类名映射（静态引用保证打包包含） */
const iconColorClass: Record<IconColor, string> = {
  blue: styles.iconBlue,
  purple: styles.iconPurple,
  green: styles.iconGreen,
  red: styles.iconRed,
  yellow: styles.iconYellow,
  cyan: styles.iconCyan,
  gray: styles.iconGray,
  darkBlue: styles.iconDarkBlue,
};

/**
 * 我的页（tabBar）
 * 用户信息 + 会员赞助 + 统计 + 设置菜单分组 + 退出登录
 * 基于后端真实接口：useAuthStore / workbenchService / settingsService
 */
const MinePage: React.FC = () => {
  const { user, isAuthenticated, logout } = useAuthStore();
  const { t } = useI18n();
  useLocalizedTitle('我的');

  const [stats, setStats] = useState({ analysis: 0, opportunities: 0, risks: 0 });
  const [health, setHealth] = useState<SystemHealth | null>(null);
  const [notifEnabled, setNotifEnabled] = useState<boolean | null>(null);

  useEffect(() => {
    if (!isAuthenticated) return;
    let cancelled = false;

    // 统计：深度分析次数 / 有效机会数 / 风险预警数（工作台聚合接口）
    workbenchService
      .getWorkbench()
      .then((data) => {
        if (cancelled) return;
        setStats({
          analysis: data?.value_proof?.analysis_count ?? 0,
          opportunities: data?.value_proof?.opportunities_found ?? 0,
          risks: data?.risk_alert_count ?? 0,
        });
      })
      .catch(() => undefined);

    // 系统健康（用于"关于"展示服务端版本/环境）
    settingsService
      .getHealth()
      .then((data) => {
        if (!cancelled) setHealth(data);
      })
      .catch(() => undefined);

    // 通知配置状态（用于"通知配置"菜单项右侧状态）
    settingsService
      .getNotificationConfig()
      .then((data) => {
        if (!cancelled) setNotifEnabled(!!data?.enable_push);
      })
      .catch(() => undefined);

    return () => {
      cancelled = true;
    };
  }, [isAuthenticated]);

  if (!isAuthenticated) {
    return (
      <View className={styles.minePage}>
        <Empty
          title={t('未登录')}
          description={t('登录后可查看个人信息与设置')}
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

  const handleNavigate = (url: string) => {
    Taro.navigateTo({ url }).catch(() => {
      Taro.showToast({ title: t('页面不存在'), icon: 'none' });
    });
  };

  const handleLogout = () => {
    Taro.showModal({
      title: t('退出登录'),
      content: t('确定退出当前账号吗？'),
      confirmText: t('退出'),
      confirmColor: '#f53f3f',
      success: (res) => {
        if (res.confirm) logout();
      },
    });
  };

  const handleAbout = () => {
    const lines = [`${t('客户端版本')}: v${CLIENT_VERSION}`];
    if (health?.version) lines.push(`${t('服务端版本')}: v${health.version}`);
    if (health?.environment) lines.push(`${t('环境')}: ${health.environment}`);
    Taro.showModal({
      title: t('关于 MoneyRobert'),
      content: lines.join('\n'),
      showCancel: false,
      confirmText: t('知道了'),
    });
  };

  const avatarChar = (user?.username || 'U').slice(0, 1).toUpperCase();
  const isAdmin = user?.role?.toLowerCase() === 'admin';
  const roleTagType = isAdmin ? 'danger' : 'primary';
  const roleTagText = isAdmin ? t('admin') : t('user');
  const notifStatusText =
    notifEnabled === null ? '' : notifEnabled ? t('已开启') : t('未开启');
  const appVersionText = `v${CLIENT_VERSION}`;

  const menuGroups: MenuGroup[] = [
    {
      title: t('API Key 配置'),
      items: [
        {
          icon: 'E',
          color: 'blue',
          label: t('交易所 API Key 配置'),
          route: '/pages/exchange-api-key/index',
        },
        {
          icon: 'M',
          color: 'purple',
          label: t('模型 API Key 配置'),
          route: '/pages/model-api-key/index',
        },
      ],
    },
    {
      title: t('交易与风控'),
      items: [
        {
          icon: 'T',
          color: 'green',
          label: t('交易参数配置'),
          route: '/pages/trading-config/index',
        },
        {
          icon: 'R',
          color: 'red',
          label: t('风控配置'),
          route: '/pages/risk-config/index',
        },
      ],
    },
    {
      title: t('通知与显示'),
      items: [
        {
          icon: 'N',
          color: 'yellow',
          label: t('通知配置'),
          route: '/pages/notification-config/index',
          value: notifStatusText,
        },
        {
          icon: 'D',
          color: 'cyan',
          label: t('显示设置'),
          route: '/pages/settings/index?type=display',
        },
      ],
    },
    {
      title: t('系统'),
      items: [
        {
          icon: 'L',
          color: 'gray',
          label: t('系统日志'),
          route: '/pages/system-log/index',
        },
        {
          icon: 'S',
          color: 'darkBlue',
          label: t('设备与安全'),
          route: '/pages/settings/index?type=security',
        },
        {
          icon: 'i',
          color: 'gray',
          label: t('关于 MoneyRobert'),
          value: appVersionText,
          onClick: handleAbout,
        },
      ],
    },
  ];

  return (
    <View className={styles.minePage}>
      {/* 用户信息卡片（点击跳转账户详情） */}
      <View
        className={styles.userHeader}
        onClick={() => handleNavigate('/pages/account/index')}
      >
        <View className={styles.userInfo}>
          <View className={styles.avatar}>
            <Text>{avatarChar}</Text>
          </View>
          <View className={styles.userDetail}>
            <Text className={styles.username}>{user?.username || '-'}</Text>
            {user?.email ? <Text className={styles.userEmail}>{user.email}</Text> : null}
            {user?.role ? (
              <Tag type={roleTagType} className={styles.roleTag}>
                {roleTagText}
              </Tag>
            ) : null}
          </View>
        </View>
        <Text className={styles.headerArrow}>›</Text>
      </View>

      {/* 会员赞助卡 */}
      <View
        className={styles.memberCard}
        onClick={() => handleNavigate('/pages/membership/index')}
      >
        <View className={styles.memberText}>
          <Text className={styles.memberEyebrow}>{t('支持独立创作')}</Text>
          <Text className={styles.memberTitle}>{t('产品免费，赞助完全自愿')}</Text>
          <Text className={styles.memberDesc}>
            {t('赞助与盈利、策略权限和功能无关，可永久关闭提示')}
          </Text>
        </View>
        <Text className={styles.memberAction}>{t('支持作者 ›')}</Text>
      </View>

      {/* 统计卡 */}
      <View className={styles.statsRow}>
        <View className={styles.statItem}>
          <Text className={styles.statValue}>{stats.analysis}</Text>
          <Text className={styles.statLabel}>{t('深度分析次数')}</Text>
        </View>
        <View className={styles.statDivider} />
        <View className={styles.statItem}>
          <Text className={styles.statValue}>{stats.opportunities}</Text>
          <Text className={styles.statLabel}>{t('有效机会数')}</Text>
        </View>
        <View className={styles.statDivider} />
        <View className={styles.statItem}>
          <Text className={styles.statValue}>{stats.risks}</Text>
          <Text className={styles.statLabel}>{t('风险预警数')}</Text>
        </View>
      </View>

      {/* 设置菜单分组 */}
      {menuGroups.map((group, gIdx) => (
        <View key={gIdx} className={styles.menuGroup}>
          <Text className={styles.groupTitle}>{group.title}</Text>
          <View className={styles.menuList}>
            {group.items.map((item, idx) => (
              <View
                key={idx}
                className={styles.menuItem}
                onClick={() =>
                  item.onClick ? item.onClick() : handleNavigate(item.route || '')
                }
              >
                <View className={`${styles.menuIcon} ${iconColorClass[item.color]}`}>
                  <Text>{item.icon}</Text>
                </View>
                <Text className={styles.menuLabel}>{item.label}</Text>
                {item.value ? <Text className={styles.menuValue}>{item.value}</Text> : null}
                <Text className={styles.menuArrow}>›</Text>
              </View>
            ))}
          </View>
        </View>
      ))}

      {/* 退出登录 */}
      <Button
        type="default"
        fill="outline"
        block
        className={styles.logoutButton}
        onClick={handleLogout}
      >
        {t('退出登录')}
      </Button>
    </View>
  );
};

export default MinePage;

import React from 'react';
import { View, Text, Switch } from '@tarojs/components';
import Taro, { useRouter } from '@tarojs/taro';
import { getClientPlatform, CLIENT_VERSION } from '@/types/common';
import styles from './index.module.scss';

/**
 * 设置页（二级页面）
 * 通知设置、显示设置、安全设置
 * 对接桌面端已有的 SettingsPage 能力
 */
const SettingsPage: React.FC = () => {
  const router = useRouter();
  const initialType = (router.params.type as string) || 'all';

  // 平台中文名
  const platformNameMap: Record<string, string> = {
    weapp: '微信小程序',
    alipay: '支付宝小程序',
    tt: '抖音小程序',
    h5: 'H5 / PWA',
    rn: 'React Native（Android/iOS）',
    harmony: '鸿蒙',
    qq: 'QQ 小程序',
    jd: '京东小程序',
  };
  const currentPlatform = getClientPlatform();
  const platformName = platformNameMap[currentPlatform] || currentPlatform;

  const handleBack = () => {
    Taro.navigateBack({ delta: 1 }).catch(() => {
      Taro.switchTab({ url: '/pages/mine/index' });
    });
  };

  // 通知设置组
  const notificationSettings = [
    { key: 'push', label: '推送通知', icon: '推', value: true },
    { key: 'risk', label: '风险告警', icon: '险', value: true },
    { key: 'approval', label: '审批通知', icon: '审', value: true },
    { key: 'system', label: '系统通知', icon: '系', value: false },
  ];

  // 显示设置组
  const displaySettings = [
    { key: 'theme', label: '主题模式', icon: '主', value: '跟随系统', arrow: true },
    { key: 'font', label: '字号', icon: '字', value: '标准', arrow: true },
    { key: 'cache', label: '清除缓存', icon: '清', value: '12.5 MB', arrow: true },
  ];

  // 安全设置组
  const securitySettings = [
    { key: 'biometric', label: '生物识别', icon: '生', value: false },
    { key: 'screenshot', label: '截图防护', icon: '截', value: true },
    { key: 'devices', label: '设备管理', icon: '设', value: '2 台设备', arrow: true },
  ];

  // 仅显示指定分组
  const showNotification = initialType === 'all' || initialType === 'notification';
  const showDisplay = initialType === 'all' || initialType === 'display';
  const showSecurity = initialType === 'all' || initialType === 'security';

  // Switch 切换处理
  const handleSwitch = (key: string, value: boolean) => {
    Taro.showToast({ title: `${value ? '已开启' : '已关闭'}`, icon: 'none' });
  };

  return (
    <View className={styles.settingsPage}>
      <View className={styles.header}>
        <View className={styles.backButton} onClick={handleBack}>
          <Text className={styles.backIcon}>‹</Text>
        </View>
        <Text className={styles.headerTitle}>设置</Text>
      </View>

      {/* 平台信息卡 */}
      <View className={styles.platformCard}>
        <Text className={styles.platformTitle}>MoneyRobert 移动工作台</Text>
        <Text className={styles.platformDesc}>
          当前平台：{platformName}
          {'\n'}对接统一后台 API，与桌面端共享业务能力
        </Text>
        <Text className={styles.versionText}>版本 {CLIENT_VERSION}</Text>
      </View>

      <View className={styles.content}>
        {showNotification && (
          <>
            <Text className={styles.groupTitle}>通知设置</Text>
            <View className={styles.groupCard}>
              {notificationSettings.map((item) => (
                <View key={item.key} className={styles.settingItem}>
                  <View className={styles.settingLeft}>
                    <View className={styles.settingIcon}>
                      <Text>{item.icon}</Text>
                    </View>
                    <Text className={styles.settingLabel}>{item.label}</Text>
                  </View>
                  <Switch
                    checked={item.value}
                    onChange={(v) => handleSwitch(item.key, v)}
                  />
                </View>
              ))}
            </View>
          </>
        )}

        {showDisplay && (
          <>
            <Text className={styles.groupTitle}>显示设置</Text>
            <View className={styles.groupCard}>
              {displaySettings.map((item) => (
                <View key={item.key} className={styles.settingItem}>
                  <View className={styles.settingLeft}>
                    <View className={styles.settingIcon}>
                      <Text>{item.icon}</Text>
                    </View>
                    <Text className={styles.settingLabel}>{item.label}</Text>
                  </View>
                  <View className={styles.settingLeft}>
                    <Text className={styles.settingValue}>{item.value}</Text>
                    {item.arrow && <Text className={styles.settingArrow}>›</Text>}
                  </View>
                </View>
              ))}
            </View>
          </>
        )}

        {showSecurity && (
          <>
            <Text className={styles.groupTitle}>安全设置</Text>
            <View className={styles.groupCard}>
              {securitySettings.map((item) => (
                <View key={item.key} className={styles.settingItem}>
                  <View className={styles.settingLeft}>
                    <View className={styles.settingIcon}>
                      <Text>{item.icon}</Text>
                    </View>
                    <Text className={styles.settingLabel}>{item.label}</Text>
                  </View>
                  {typeof item.value === 'boolean' ? (
                    <Switch
                      checked={item.value}
                      onChange={(v) => handleSwitch(item.key, v)}
                    />
                  ) : (
                    <View className={styles.settingLeft}>
                      <Text className={styles.settingValue}>{item.value}</Text>
                      {item.arrow && <Text className={styles.settingArrow}>›</Text>}
                    </View>
                  )}
                </View>
              ))}
            </View>
          </>
        )}
      </View>
    </View>
  );
};

export default SettingsPage;

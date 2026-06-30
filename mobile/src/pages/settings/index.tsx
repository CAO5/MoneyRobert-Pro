import React, { useState } from 'react';
import { View, Text, Switch, Picker } from '@tarojs/components';
import Taro, { useRouter } from '@tarojs/taro';
import { getClientPlatform, CLIENT_VERSION } from '@/types/common';
import { languageOptions } from '@/i18n';
import { useI18n, useLocalizedTitle } from '@/store/language';
import {
  applyScreenshotProtection,
  getScreenshotProtectionPreference,
  supportsScreenshotProtection,
} from '@/security/privacy';
import styles from './index.module.scss';

/**
 * 设置页（二级页面）
 * 通知设置、显示设置、安全设置
 * 对接桌面端已有的 SettingsPage 能力
 */
const SettingsPage: React.FC = () => {
  const router = useRouter();
  const { t, preference, setPreference } = useI18n();
  useLocalizedTitle('设置');
  const initialType = (router.params.type as string) || 'all';

  // 平台中文名
  const platformNameMap: Record<string, string> = {
    weapp: 'WeChat Mini Program',
    alipay: 'Alipay Mini Program',
    tt: 'Douyin Mini Program',
    h5: 'H5 / PWA',
    rn: 'React Native（Android/iOS）',
    harmony: 'HarmonyOS',
    qq: 'QQ Mini Program',
    jd: 'JD Mini Program',
  };
  const currentPlatform = getClientPlatform();
  const platformName = platformNameMap[currentPlatform] || currentPlatform;
  const screenshotSupported = supportsScreenshotProtection();
  const [screenshotProtection, setScreenshotProtection] = useState(
    getScreenshotProtectionPreference,
  );

  const handleBack = () => {
    Taro.navigateBack({ delta: 1 }).catch(() => {
      Taro.switchTab({ url: '/pages/mine/index' });
    });
  };

  // 通知设置组
  const notificationSettings = [
    { key: 'push', label: t('推送通知'), icon: 'P', value: true },
    { key: 'risk', label: t('风险预警'), icon: 'R', value: true },
    { key: 'approval', label: t('审批通知'), icon: 'A', value: true },
    { key: 'system', label: t('系统通知'), icon: 'S', value: false },
  ];

  // 显示设置组
  const displaySettings = [
    { key: 'theme', label: t('主题模式'), icon: 'T', value: t('跟随系统'), arrow: true },
    { key: 'font', label: t('字号'), icon: 'A', value: t('标准'), arrow: true },
    { key: 'cache', label: t('清除缓存'), icon: 'C', value: '12.5 MB', arrow: true },
  ];

  // 安全设置组
  const securitySettings = [
    { key: 'biometric', label: t('生物识别'), icon: 'B', value: false, disabled: true },
    {
      key: 'screenshot',
      label: t('截图防护'),
      icon: 'S',
      value: screenshotProtection,
      disabled: !screenshotSupported,
    },
    {
      key: 'devices',
      label: t('设备管理'),
      icon: 'D',
      value: t('2 台设备'),
      arrow: true,
      disabled: true,
    },
  ];

  // 仅显示指定分组
  const showNotification = initialType === 'all' || initialType === 'notification';
  const showDisplay = initialType === 'all' || initialType === 'display';
  const showSecurity = initialType === 'all' || initialType === 'security';

  // Switch 切换处理
  const handleSwitch = (value: boolean) => {
    Taro.showToast({ title: t(value ? '已开启' : '已关闭'), icon: 'none' });
  };

  const handleSecuritySwitch = async (key: string, value: boolean) => {
    if (key !== 'screenshot') {
      Taro.showToast({ title: t('该功能尚未接入'), icon: 'none' });
      return;
    }
    const applied = await applyScreenshotProtection(value);
    if (!applied) {
      Taro.showToast({ title: t('当前平台不支持截图防护'), icon: 'none' });
      return;
    }
    setScreenshotProtection(value);
    handleSwitch(value);
  };

  const languageIndex = Math.max(0, languageOptions.findIndex((option) => option.value === preference));
  const languageLabels = languageOptions.map((option) => t(option.labelKey));

  return (
    <View className={styles.settingsPage}>
      <View className={styles.header}>
        <View className={styles.backButton} onClick={handleBack}>
          <Text className={styles.backIcon}>‹</Text>
        </View>
        <Text className={styles.headerTitle}>{t('设置')}</Text>
      </View>

      {/* 平台信息卡 */}
      <View className={styles.platformCard}>
        <Text className={styles.platformTitle}>{t('MoneyRobert 移动工作台')}</Text>
        <Text className={styles.platformDesc}>
          {t('当前平台：{platform}', { platform: platformName })}
          {'\n'}{t('对接统一后台 API，与桌面端共享业务能力')}
        </Text>
        <Text className={styles.versionText}>{t('版本 {version}', { version: CLIENT_VERSION })}</Text>
      </View>

      <View className={styles.content}>
        <Text className={styles.groupTitle}>{t('语言')}</Text>
        <View className={styles.groupCard}>
          <Picker
            mode="selector"
            range={languageLabels}
            value={languageIndex}
            onChange={(event) => setPreference(languageOptions[Number(event.detail.value)].value)}
          >
            <View className={styles.settingItem}>
              <View className={styles.settingLeft}>
                <View className={styles.settingIcon}><Text>文</Text></View>
                <Text className={styles.settingLabel}>{t('语言')}</Text>
              </View>
              <View className={styles.settingLeft}>
                <Text className={styles.settingValue}>{languageLabels[languageIndex]}</Text>
                <Text className={styles.settingArrow}>›</Text>
              </View>
            </View>
          </Picker>
        </View>

        {showNotification && (
          <>
            <Text className={styles.groupTitle}>{t('通知设置')}</Text>
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
                    onChange={(event) => handleSwitch(event.detail.value)}
                  />
                </View>
              ))}
            </View>
          </>
        )}

        {showDisplay && (
          <>
            <Text className={styles.groupTitle}>{t('显示设置')}</Text>
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
            <Text className={styles.groupTitle}>{t('安全设置')}</Text>
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
                      disabled={item.disabled}
                      onChange={(event) => handleSecuritySwitch(item.key, event.detail.value)}
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

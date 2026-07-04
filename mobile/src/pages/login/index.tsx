import React, { useEffect, useState } from 'react';
import { View, Text, ScrollView } from '@tarojs/components';
import Taro from '@tarojs/taro';
import { Button, Checkbox, Input } from '@nutui/nutui-react-taro';
import { useAuthStore } from '@/store/auth';
import { useI18n, useLocalizedTitle } from '@/store/language';
import { AppError } from '@/types/common';
import {
  applyScreenshotProtection,
  getScreenshotProtectionPreference,
} from '@/security/privacy';
import styles from './index.module.scss';

/**
 * 登录页（基于 NutUI 框架组件重设计）
 * 使用 NutUI 的 Button / Input / Checkbox 替代原生 Taro 组件
 * 视觉参考后台管理系统 LoginPage.vue：品牌头部 + 光晕装饰 + 表单卡片 + 安全徽章
 */
const LoginPage: React.FC = () => {
  const login = useAuthStore((s) => s.login);
  const isLoading = useAuthStore((s) => s.isLoading);
  const { t } = useI18n();
  useLocalizedTitle('登录');

  const [username, setUsername] = useState('');
  const [password, setPassword] = useState('');
  const [showPassword, setShowPassword] = useState(false);
  const [rememberMe, setRememberMe] = useState(true);
  const [errorMsg, setErrorMsg] = useState('');

  useEffect(() => {
    void applyScreenshotProtection(true, false);
    try {
      const saved = Taro.getStorageSync('remembered_username');
      if (saved) setUsername(saved);
    } catch {
      // ignore
    }
    return () => {
      setPassword('');
      void applyScreenshotProtection(getScreenshotProtectionPreference());
    };
  }, []);

  /** 提交登录 */
  const handleSubmit = async () => {
    setErrorMsg('');
    if (!username.trim() || !password) {
      setErrorMsg(t('请输入用户名和密码'));
      return;
    }
    const credentials = { username: username.trim(), password };
    setPassword('');
    setShowPassword(false);
    try {
      if (rememberMe) {
        Taro.setStorageSync('remembered_username', username.trim());
      } else {
        Taro.removeStorageSync('remembered_username');
      }
      await login(credentials);
      Taro.showToast({ title: t('登录成功'), icon: 'none', duration: 1000 });
      // 用 reLaunch 替代 switchTab + setTimeout：
      // switchTab 在 custom tabBar 下行为不稳定，reLaunch 会重置路由栈避免状态混乱
      Taro.reLaunch({ url: '/pages/workbench/index' });
    } catch (err) {
      const message =
        err instanceof AppError && err.code === 'INSECURE_TRANSPORT'
          ? t('安全连接不可用，已阻止发送账号密码')
          : err instanceof Error
            ? err.message
            : t('登录失败，请检查用户名和密码');
      setErrorMsg(message);
    }
  };

  /** 注册引导 */
  const handleRegister = () => {
    Taro.showModal({
      title: t('注册账号'),
      content: t('移动端暂未开放注册，请前往桌面端注册账号'),
      showCancel: false,
      confirmText: t('知道了'),
    });
  };

  /** 忘记密码：跳转重置密码页 */
  const handleForgotPassword = () => {
    Taro.navigateTo({ url: '/pages/forgot-password/index' });
  };

  return (
    <ScrollView scrollY className={styles.loginPage}>
      {/* 背景装饰光晕 */}
      <View className={styles.bgEffects}>
        <View className={`${styles.decoCircle} ${styles.decoCircle1}`} />
        <View className={`${styles.decoCircle} ${styles.decoCircle2}`} />
      </View>

      {/* 品牌头部 */}
      <View className={styles.brand}>
        <View className={styles.logoIcon}>
          <Text className={styles.logoGlyph}>M</Text>
        </View>
        <Text className={styles.brandTitle}>MoneyRobert</Text>
        <Text className={styles.brandSubtitle}>{t('Professional Trading Platform')}</Text>
      </View>

      {/* 登录卡片 */}
      <View className={styles.loginCard}>
        <Text className={styles.formTitle}>{t('账号登录')}</Text>

        {/* 用户名 - NutUI Input */}
        <View className={styles.formGroup}>
          <Text className={styles.formLabel}>{t('用户名')}</Text>
          <Input
            className={styles.nutInput}
            type="text"
            placeholder={t('请输入用户名或邮箱')}
            value={username}
            maxLength={128}
            clearable
            onChange={(val) => setUsername(val)}
          />
        </View>

        {/* 密码 - NutUI Input */}
        <View className={styles.formGroup}>
          <Text className={styles.formLabel}>{t('密码')}</Text>
          <Input
            className={styles.nutInput}
            type={showPassword ? 'text' : 'password'}
            placeholder={t('请输入密码')}
            value={password}
            maxLength={128}
            clearable
            onChange={(val) => setPassword(val)}
          />
          <View className={styles.eyeBtn} onClick={() => setShowPassword((s) => !s)}>
            <Text>{showPassword ? '👁' : '👁‍🗨'}</Text>
          </View>
        </View>

        {/* 记住我 + 忘记密码 */}
        <View className={styles.formOptions}>
          <Checkbox
            checked={rememberMe}
            onChange={(checked) => setRememberMe(checked as boolean)}
            className={styles.nutCheckbox}
          >
            {t('记住我')}
          </Checkbox>
          <Text className={styles.forgotLink} onClick={handleForgotPassword}>
            {t('忘记密码？')}
          </Text>
        </View>

        {/* 错误提示 */}
        {errorMsg ? (
          <View className={styles.errorBox}>
            <Text className={styles.errorIcon}>!</Text>
            <Text className={styles.errorText}>{errorMsg}</Text>
          </View>
        ) : null}

        {/* 登录按钮 - NutUI Button */}
        <Button
          type="primary"
          shape="round"
          block
          loading={isLoading}
          disabled={isLoading}
          className={styles.submitButton}
          onClick={handleSubmit}
        >
          {isLoading ? t('登录中...') : t('登 录')}
        </Button>

        <Text className={styles.hint}>
          {t('未登录可访问"我的"查看本地信息，其他操作需登录')}
        </Text>
      </View>

      {/* 注册引导 */}
      <View className={styles.registerLink}>
        <Text className={styles.registerText}>{t('还没有账户？')}</Text>
        <Text className={styles.registerAction} onClick={handleRegister}>
          {t('立即注册')}
        </Text>
      </View>

      {/* 底部安全徽章 */}
      <View className={styles.footerInfo}>
        <View className={styles.securityBadge}>
          <Text className={styles.badgeIcon}>🔒</Text>
          <Text>{t('安全加密')}</Text>
        </View>
        <View className={styles.securityBadge}>
          <Text className={styles.badgeIcon}>⚡</Text>
          <Text>{t('高性能')}</Text>
        </View>
        <View className={styles.securityBadge}>
          <Text className={styles.badgeIcon}>🛡️</Text>
          <Text>{t('企业级')}</Text>
        </View>
      </View>

      <View className={styles.copyright}>
        <Text>© 2026 MoneyRobert Pro. {t('保留所有权利')}</Text>
      </View>
    </ScrollView>
  );
};

export default LoginPage;

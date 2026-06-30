import React, { useEffect, useState } from 'react';
import { View, Text, Input, Button } from '@tarojs/components';
import Taro from '@tarojs/taro';
import { useAuthStore } from '@/store/auth';
import { useI18n, useLocalizedTitle } from '@/store/language';
import { AppError } from '@/types/common';
import {
  applyScreenshotProtection,
  getScreenshotProtectionPreference,
} from '@/security/privacy';
import styles from './index.module.scss';

/**
 * 登录页
 * 对接后端 /auth/login 接口
 * 登录成功后跳转工作台
 */
const LoginPage: React.FC = () => {
  const login = useAuthStore((s) => s.login);
  const isLoading = useAuthStore((s) => s.isLoading);
  const { t } = useI18n();
  useLocalizedTitle('登录');

  const [username, setUsername] = useState('');
  const [password, setPassword] = useState('');
  const [showPassword, setShowPassword] = useState(false);

  useEffect(() => {
    // 登录页始终尝试隐藏截屏/录屏内容，不受普通页面偏好影响。
    void applyScreenshotProtection(true, false);
    return () => {
      setPassword('');
      void applyScreenshotProtection(getScreenshotProtectionPreference());
    };
  }, []);

  /** 提交登录 */
  const handleSubmit = async () => {
    if (!username.trim() || !password) {
      Taro.showToast({ title: t('请输入账号和密码'), icon: 'none' });
      return;
    }
    const credentials = { username: username.trim(), password };
    // 提交后立即从可见表单状态移除密码；凭证只在本次异步调用内短暂存在。
    setPassword('');
    setShowPassword(false);
    try {
      await login(credentials);
      Taro.showToast({ title: t('登录成功'), icon: 'success' });
      // 登录成功后切换到工作台 tabBar
      setTimeout(() => {
        Taro.switchTab({ url: '/pages/workbench/index' });
      }, 500);
    } catch (err) {
      const message =
        err instanceof AppError && err.code === 'INSECURE_TRANSPORT'
          ? t('安全连接不可用，已阻止发送账号密码')
          : err instanceof Error
            ? err.message
            : t('登录失败');
      Taro.showToast({ title: message, icon: 'none' });
    }
  };

  return (
    <View className={styles.loginPage}>
      <View className={styles.brand}>
        <Text className={styles.brandTitle}>MoneyRobert</Text>
        <Text className={styles.brandSubtitle}>{t('量化决策移动工作台')}</Text>
      </View>

      <View className={styles.formCard}>
        <Text className={styles.formTitle}>{t('账号登录')}</Text>

        <View className={styles.formItem}>
          <Text className={styles.formLabel}>{t('用户名')}</Text>
          <View className={styles.inputWrap}>
            <Input
              className={styles.input}
              type="text"
              placeholder={t('请输入用户名')}
              value={username}
              maxlength={128}
              onInput={(e) => setUsername(e.detail.value)}
            />
          </View>
        </View>

        <View className={styles.formItem}>
          <Text className={styles.formLabel}>{t('密码')}</Text>
          <View className={styles.inputWrap}>
            <Input
              className={styles.input}
              password={!showPassword}
              placeholder={t('请输入密码')}
              value={password}
              maxlength={128}
              onInput={(e) => setPassword(e.detail.value)}
            />
            <View className={styles.eye} onClick={() => setShowPassword((s) => !s)}>
              <Text>{showPassword ? '隐' : '显'}</Text>
            </View>
          </View>
        </View>

        <Button
          className={styles.submitButton}
          loading={isLoading}
          disabled={isLoading}
          onClick={handleSubmit}
        >
          {t(isLoading ? '登录中...' : '登录')}
        </Button>

        <Text className={styles.hint}>
          {t('未登录用户可访问"我的"查看本地信息，其他操作需登录后进行')}
        </Text>
      </View>
    </View>
  );
};

export default LoginPage;

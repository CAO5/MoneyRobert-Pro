import React, { useState } from 'react';
import { View, Text, Input, Button } from '@tarojs/components';
import Taro from '@tarojs/taro';
import { useAuthStore } from '@/store/auth';
import styles from './index.module.scss';

/**
 * 登录页
 * 对接后端 /auth/login 接口
 * 登录成功后跳转工作台
 */
const LoginPage: React.FC = () => {
  const login = useAuthStore((s) => s.login);
  const isLoading = useAuthStore((s) => s.isLoading);

  const [username, setUsername] = useState('');
  const [password, setPassword] = useState('');
  const [showPassword, setShowPassword] = useState(false);

  /** 提交登录 */
  const handleSubmit = async () => {
    if (!username.trim() || !password) {
      Taro.showToast({ title: '请输入账号和密码', icon: 'none' });
      return;
    }
    try {
      await login({ username: username.trim(), password });
      Taro.showToast({ title: '登录成功', icon: 'success' });
      // 登录成功后切换到工作台 tabBar
      setTimeout(() => {
        Taro.switchTab({ url: '/pages/workbench/index' });
      }, 500);
    } catch (err) {
      const message = err instanceof Error ? err.message : '登录失败';
      Taro.showToast({ title: message, icon: 'none' });
      console.error('[Login] failed:', err);
    }
  };

  return (
    <View className={styles.loginPage}>
      <View className={styles.brand}>
        <Text className={styles.brandTitle}>MoneyRobert</Text>
        <Text className={styles.brandSubtitle}>量化决策移动工作台</Text>
      </View>

      <View className={styles.formCard}>
        <Text className={styles.formTitle}>账号登录</Text>

        <View className={styles.formItem}>
          <Text className={styles.formLabel}>用户名</Text>
          <View className={styles.inputWrap}>
            <Input
              className={styles.input}
              type="text"
              placeholder="请输入用户名"
              value={username}
              onInput={(e) => setUsername(e.detail.value)}
            />
          </View>
        </View>

        <View className={styles.formItem}>
          <Text className={styles.formLabel}>密码</Text>
          <View className={styles.inputWrap}>
            <Input
              className={styles.input}
              password={!showPassword}
              placeholder="请输入密码"
              value={password}
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
          {isLoading ? '登录中...' : '登录'}
        </Button>

        <Text className={styles.hint}>
          未登录用户可访问"我的"查看本地信息，其他操作需登录后进行
        </Text>
      </View>
    </View>
  );
};

export default LoginPage;

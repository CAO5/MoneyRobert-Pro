import React, { useState } from 'react';
import { View, Text, ScrollView } from '@tarojs/components';
import Taro from '@tarojs/taro';
import { Button, Input, Cell } from '@nutui/nutui-react-taro';
import { http } from '@/services/request';
import { encryptPassword } from '@/utils/passwordCrypto';
import { useI18n, useLocalizedTitle } from '@/store/language';
import { AppError } from '@/types/common';
import styles from './index.module.scss';

/**
 * 重置密码页（两步流程）
 * 步骤 1：输入邮箱 → 发送验证码
 * 步骤 2：输入邮箱、验证码、新密码 → 重置密码
 * 视觉风格与登录页保持一致（品牌蓝 + 白色表单卡片 + 浮动光晕）
 */
const ForgotPasswordPage: React.FC = () => {
  const { t } = useI18n();
  useLocalizedTitle('重置密码');

  const [step, setStep] = useState<1 | 2>(1);
  const [email, setEmail] = useState('');
  const [code, setCode] = useState('');
  const [newPassword, setNewPassword] = useState('');
  const [confirmPassword, setConfirmPassword] = useState('');
  const [showPassword, setShowPassword] = useState(false);
  const [submitting, setSubmitting] = useState(false);

  const passwordMismatch =
    newPassword.length > 0 && confirmPassword.length > 0 && newPassword !== confirmPassword;

  const canReset =
    !!email.trim() &&
    code.trim().length === 6 &&
    newPassword.length > 0 &&
    confirmPassword.length > 0 &&
    newPassword === confirmPassword;

  /** 从异常中提取可读消息 */
  const extractError = (err: unknown, fallback: string): string => {
    if (err instanceof AppError) return err.message;
    if (err instanceof Error) return err.message;
    return t(fallback);
  };

  /** 步骤 1：发送验证码 */
  const handleSendCode = async () => {
    if (!email.trim()) {
      Taro.showToast({ title: t('请输入邮箱'), icon: 'none' });
      return;
    }
    setSubmitting(true);
    try {
      await http.post('/auth/password-reset/request', { email: email.trim() }, { auth: false });
      Taro.showToast({ title: t('验证码已发送'), icon: 'success' });
      setStep(2);
    } catch (err) {
      Taro.showToast({ title: extractError(err, '验证码发送失败，请稍后重试'), icon: 'none' });
    } finally {
      setSubmitting(false);
    }
  };

  /** 步骤 2：重置密码 */
  const handleResetPassword = async () => {
    if (!canReset) {
      if (passwordMismatch) {
        Taro.showToast({ title: t('两次输入的密码不一致'), icon: 'none' });
      } else {
        Taro.showToast({ title: t('请填写完整信息'), icon: 'none' });
      }
      return;
    }
    setSubmitting(true);
    try {
      const encryptedPassword = encryptPassword(newPassword);
      await http.post(
        '/auth/password-reset/confirm',
        { email: email.trim(), code: code.trim(), new_password: encryptedPassword },
        { auth: false },
      );
      Taro.showToast({ title: t('密码重置成功'), icon: 'success' });
      setTimeout(() => {
        Taro.reLaunch({ url: '/pages/login/index' });
      }, 1000);
    } catch (err) {
      Taro.showToast({ title: extractError(err, '密码重置失败，请稍后重试'), icon: 'none' });
    } finally {
      setSubmitting(false);
    }
  };

  /** 返回登录 */
  const handleBackToLogin = () => {
    Taro.reLaunch({ url: '/pages/login/index' });
  };

  /** 回到步骤 1（重新发送验证码） */
  const handleBackToEmail = () => {
    setStep(1);
    setCode('');
    setNewPassword('');
    setConfirmPassword('');
  };

  const renderEyeToggle = () => (
    <View className={styles.eyeBtn} onClick={() => setShowPassword((s) => !s)}>
      <Text>{showPassword ? '👁' : '👁‍🗨'}</Text>
    </View>
  );

  return (
    <ScrollView scrollY className={styles.forgotPage}>
      {/* 背景装饰光晕 */}
      <View className={styles.bgEffects}>
        <View className={`${styles.decoCircle} ${styles.decoCircle1}`} />
        <View className={`${styles.decoCircle} ${styles.decoCircle2}`} />
      </View>

      <View className={styles.card}>
        <Text className={styles.formTitle}>{t('重置密码')}</Text>
        <Text className={styles.subtitle}>
          {step === 1 ? t('输入注册邮箱以接收验证码') : t('设置新的登录密码')}
        </Text>

        <View className={styles.stepBadgeWrap}>
          <Text className={styles.stepBadge}>
            {t('步骤 {n}/2', { n: step })}
          </Text>
        </View>

        {/* 步骤 1：邮箱 */}
        {step === 1 && (
          <>
            <Cell className={styles.cellField} title={t('邮箱')}>
              <Input
                type="text"
                placeholder={t('请输入邮箱')}
                value={email}
                maxLength={128}
                clearable
                onChange={(val) => setEmail(val)}
              />
            </Cell>

            <Button
              type="primary"
              shape="round"
              block
              loading={submitting}
              disabled={submitting}
              className={styles.submitButton}
              onClick={handleSendCode}
            >
              {submitting ? t('发送中...') : t('发送验证码')}
            </Button>
          </>
        )}

        {/* 步骤 2：验证码 + 新密码 */}
        {step === 2 && (
          <>
            <Cell className={styles.cellField} title={t('邮箱')}>
              <Input
                type="text"
                placeholder={t('请输入邮箱')}
                value={email}
                maxLength={128}
                clearable
                onChange={(val) => setEmail(val)}
              />
            </Cell>

            <Cell className={styles.cellField} title={t('验证码')}>
              <Input
                type="number"
                placeholder={t('请输入6位验证码')}
                value={code}
                maxLength={6}
                onChange={(val) => setCode(val)}
              />
            </Cell>

            <Cell className={styles.cellField} title={t('新密码')} extra={renderEyeToggle()}>
              <Input
                type={showPassword ? 'text' : 'password'}
                placeholder={t('请输入新密码')}
                value={newPassword}
                maxLength={128}
                onChange={(val) => setNewPassword(val)}
              />
            </Cell>

            <Cell className={styles.cellField} title={t('确认密码')}>
              <Input
                type={showPassword ? 'text' : 'password'}
                placeholder={t('请再次输入新密码')}
                value={confirmPassword}
                maxLength={128}
                onChange={(val) => setConfirmPassword(val)}
              />
            </Cell>

            {passwordMismatch && (
              <Text className={styles.hint}>{t('两次输入的密码不一致')}</Text>
            )}

            <Button
              type="primary"
              shape="round"
              block
              loading={submitting}
              disabled={!canReset || submitting}
              className={styles.submitButton}
              onClick={handleResetPassword}
            >
              {submitting ? t('提交中...') : t('重置密码')}
            </Button>

            <Button
              className={styles.secondaryButton}
              onClick={handleBackToEmail}
              disabled={submitting}
            >
              {t('重新发送验证码')}
            </Button>
          </>
        )}

        <Text className={styles.hint}>
          {t('验证码有效期内仅可使用一次，重置成功后需重新登录')}
        </Text>
      </View>

      <View className={styles.backLink}>
        <Text className={styles.backText}>{t('已想起密码？')}</Text>
        <Text className={styles.backAction} onClick={handleBackToLogin}>
          {t('返回登录')}
        </Text>
      </View>

      <View className={styles.copyright}>
        <Text>© 2026 MoneyRobert Pro. {t('保留所有权利')}</Text>
      </View>
    </ScrollView>
  );
};

export default ForgotPasswordPage;

import React, { useState } from 'react';
import { Button, Text, View } from '@tarojs/components';
import Taro from '@tarojs/taro';
import { useI18n, useLocalizedTitle } from '@/store/language';
import styles from './index.module.scss';

const principles = [
  { title: '完全自愿', description: '不赞助不会影响策略、分析、提醒或任何现有功能。' },
  { title: '与盈利无关', description: '赞助金额不按收益计算，也不代表投资管理费或盈利分成。' },
  { title: '随时忽略', description: '可以选择不再提示；系统不会催促、自动扣款或追缴。' },
  { title: '不购买权益', description: '赞助只是对开源与产品创作的支持，不获得更高收益或优先信号。' },
];

const sponsorOptions = [
  { amount: 18, label: '请喝咖啡' },
  { amount: 68, label: '支持一次迭代' },
  { amount: 188, label: '支持长期创作' },
];

const SponsorPage: React.FC = () => {
  const [selectedAmount, setSelectedAmount] = useState(18);
  const { t } = useI18n();
  useLocalizedTitle('支持作者');

  const handleSponsor = () => {
    Taro.showModal({
      title: t('自愿赞助 ¥{amount}', { amount: selectedAmount }),
      content: t('赞助支付能力尚未接入。赞助与交易收益、策略权限及服务质量无关，不构成投资管理费。'),
      confirmText: t('我知道了'),
      showCancel: false,
    });
  };

  const disableReminder = () => {
    Taro.setStorageSync('sponsor_prompt_disabled', true);
    Taro.showToast({ title: t('已关闭赞助提示'), icon: 'none' });
  };

  return (
    <View className={styles.page}>
      <View className={styles.hero}>
        <Text className={styles.eyebrow}>SUPPORT THE CREATOR</Text>
        <Text className={styles.title}>{t('如果 MoneyRobert 帮到了你，可以自愿支持作者')}</Text>
        <Text className={styles.subtitle}>{t('系统不会收取订阅费或盈利分成。赞助不是使用条件，也不会改变任何交易、策略或产品权益。')}</Text>

        <View className={styles.proofRow}>
          <View><Text className={styles.proofValue}>¥0</Text><Text className={styles.proofLabel}>{t('强制费用')}</Text></View>
          <View><Text className={styles.proofValue}>0%</Text><Text className={styles.proofLabel}>{t('盈利分成')}</Text></View>
          <View><Text className={styles.proofValue}>{t('自愿')}</Text><Text className={styles.proofLabel}>{t('支持方式')}</Text></View>
        </View>
      </View>

      <View className={styles.body}>
        <Text className={styles.sectionTitle}>{t('赞助原则')}</Text>
        <View className={styles.benefitList}>
          {principles.map((principle) => (
            <View key={principle.title} className={styles.benefitItem}>
              <View className={styles.check}><Text>✓</Text></View>
              <View>
                <Text className={styles.benefitTitle}>{t(principle.title)}</Text>
                <Text className={styles.benefitDesc}>{t(principle.description)}</Text>
              </View>
            </View>
          ))}
        </View>

        <Text className={styles.sectionTitle}>{t('选择固定金额')}</Text>
        <View className={styles.sponsorGrid}>
          {sponsorOptions.map((option) => (
            <View
              key={option.amount}
              className={`${styles.sponsorOption} ${selectedAmount === option.amount ? styles.selected : ''}`}
              onClick={() => setSelectedAmount(option.amount)}
            >
              <Text className={styles.sponsorAmount}>¥{option.amount}</Text>
              <Text className={styles.sponsorLabel}>{t(option.label)}</Text>
            </View>
          ))}
        </View>
        <Text className={styles.amountHint}>{t('固定金额与你的账户规模、盈利金额和交易次数无关。')}</Text>

        <Button className={styles.cta} onClick={handleSponsor}>{t('自愿赞助 ¥{amount}', { amount: selectedAmount })}</Button>
        <View className={styles.laterButton} onClick={disableReminder}><Text>{t('不需要，并关闭以后提示')}</Text></View>

        <View className={styles.freePlan}>
          <Text className={styles.freeTitle}>{t('监管说明')}</Text>
          <Text className={styles.freeDesc}>{t('将费用改为自愿赞助，不会自动消除系统执行交易、投资建议或资产管理可能产生的许可义务。MoneyRobert 仍需按服务所在地和客户所在地完成相应合规评估。')}</Text>
        </View>
      </View>
    </View>
  );
};

export default SponsorPage;

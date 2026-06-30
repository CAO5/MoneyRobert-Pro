import { create } from 'zustand';
import { useEffect } from 'react';
import Taro from '@tarojs/taro';
import {
  AppLocale,
  LanguagePreference,
  normalizeLocale,
  translate,
} from '@/i18n';

const STORAGE_KEY = 'mr_language_preference';

interface LanguageState {
  locale: AppLocale;
  preference: LanguagePreference;
  initialized: boolean;
  initialize: () => void;
  setPreference: (preference: LanguagePreference) => void;
}

function getSystemLocale(): AppLocale {
  try {
    return normalizeLocale(Taro.getSystemInfoSync().language);
  } catch {
    return 'en-US';
  }
}

function updateTabBar(locale: AppLocale) {
  ['首页', '行情', '策略', '消息', '我的'].forEach((label, index) => {
    Taro.setTabBarItem({ index, text: translate(label, locale) }).catch(() => undefined);
  });
}

export const useLanguageStore = create<LanguageState>((set) => ({
  locale: 'en-US',
  preference: 'system',
  initialized: false,
  initialize() {
    let preference: LanguagePreference = 'system';
    try {
      const stored = Taro.getStorageSync(STORAGE_KEY);
      if (stored === 'system' || stored === 'zh-CN' || stored === 'zh-TW' || stored === 'en-US') {
        preference = stored;
      }
    } catch {
      // Storage can be unavailable during very early application startup.
    }
    const locale = preference === 'system' ? getSystemLocale() : preference;
    set({ locale, preference, initialized: true });
    updateTabBar(locale);
  },
  setPreference(preference) {
    const locale = preference === 'system' ? getSystemLocale() : preference;
    Taro.setStorageSync(STORAGE_KEY, preference);
    set({ locale, preference, initialized: true });
    updateTabBar(locale);
  },
}));

/** 当前实际生效的语言，供非 React 请求层读取。 */
export function getActiveLocale(): AppLocale {
  return useLanguageStore.getState().locale;
}

export function useI18n() {
  const locale = useLanguageStore((state) => state.locale);
  const preference = useLanguageStore((state) => state.preference);
  const setPreference = useLanguageStore((state) => state.setPreference);
  return {
    locale,
    preference,
    setPreference,
    t: (key: string, params?: Record<string, string | number>) => translate(key, locale, params),
  };
}

export function useLocalizedTitle(titleKey: string) {
  const { locale } = useI18n();
  useEffect(() => {
    Taro.setNavigationBarTitle({ title: translate(titleKey, locale) }).catch(() => undefined);
  }, [locale, titleKey]);
}

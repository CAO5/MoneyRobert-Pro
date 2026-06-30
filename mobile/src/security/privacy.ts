import Taro from '@tarojs/taro';
import { getClientPlatform } from '@/types/common';

const SCREENSHOT_PROTECTION_KEY = 'mr_screenshot_protection';

export function supportsScreenshotProtection(): boolean {
  const platform = getClientPlatform();
  return platform === 'weapp' || platform === 'alipay';
}

export function getScreenshotProtectionPreference(): boolean {
  if (!supportsScreenshotProtection()) return false;
  try {
    return Taro.getStorageSync(SCREENSHOT_PROTECTION_KEY) !== false;
  } catch {
    return true;
  }
}

export async function applyScreenshotProtection(
  enabled: boolean,
  persistPreference = true,
): Promise<boolean> {
  if (!supportsScreenshotProtection()) return false;
  try {
    await Taro.setVisualEffectOnCapture({ visualEffect: enabled ? 'hidden' : 'none' });
    if (persistPreference) Taro.setStorageSync(SCREENSHOT_PROTECTION_KEY, enabled);
    return true;
  } catch {
    return false;
  }
}

export function initializePrivacyProtection(): void {
  void applyScreenshotProtection(getScreenshotProtectionPreference());
}

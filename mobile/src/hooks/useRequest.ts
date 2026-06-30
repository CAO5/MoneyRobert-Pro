import { useCallback, useEffect, useState } from 'react';
import type { AppError } from '@/types/common';

/**
 * 通用请求 Hook
 * 封装数据加载、loading、error 状态
 */
export function useRequest<T, P extends unknown[] = unknown[]>(
  fetcher: (...args: P) => Promise<T>,
  options?: {
    manual?: boolean; // 是否手动触发（默认 false，自动触发）
    initialData?: T;
    onSuccess?: (data: T) => void;
    onError?: (err: AppError) => void;
  },
) {
  const [data, setData] = useState<T | undefined>(options?.initialData);
  const [loading, setLoading] = useState<boolean>(!options?.manual);
  const [error, setError] = useState<AppError | null>(null);

  const run = useCallback(
    async (...args: P) => {
      setLoading(true);
      setError(null);
      try {
        const result = await fetcher(...args);
        setData(result);
        options?.onSuccess?.(result);
        return result;
      } catch (err) {
        const appErr = err as AppError;
        setError(appErr);
        options?.onError?.(appErr);
        throw appErr;
      } finally {
        setLoading(false);
      }
    },
    [fetcher, options],
  );

  // 自动触发（如果非 manual 模式）
  useEffect(() => {
    if (!options?.manual) {
      // 注意：使用 void 处理 Promise rejection，避免未处理的 rejection 警告
      void run(...([] as unknown as P));
    }
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);

  return {
    data,
    loading,
    error,
    refresh: run,
    setData,
  };
}

export default useRequest;

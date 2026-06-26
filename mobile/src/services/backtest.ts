import { http, MOCK_ENABLED } from './request';
import type { BacktestJobSummary, BacktestJobDetail, BacktestReport, TrustLevelResponse } from '@/types/backtest';
import { mockBacktestJobs, mockBacktestJobDetail, mockBacktestReport, mockTrustLevel } from '@/data/backtest';

/**
 * 回测服务
 * 对接后端 /backtest/* 接口
 */
export const backtestService = {
  /** 查询回测任务列表 */
  async listJobs(): Promise<BacktestJobSummary[]> {
    if (MOCK_ENABLED) {
      return mockBacktestJobs();
    }
    return http.get<{ jobs: BacktestJobSummary[] }>('/backtest/jobs').then((res) => res.jobs || []);
  },

  /** 查询任务详情 */
  async getJob(jobId: string): Promise<BacktestJobDetail> {
    if (MOCK_ENABLED) {
      return mockBacktestJobDetail(jobId);
    }
    return http.get<BacktestJobDetail>(`/backtest/jobs/${jobId}`);
  },

  /** 查询绩效报告 */
  async getReport(jobId: string): Promise<BacktestReport> {
    if (MOCK_ENABLED) {
      return mockBacktestReport(jobId);
    }
    return http.get<BacktestReport>(`/backtest/jobs/${jobId}/report`);
  },

  /** 查询可信等级 */
  async getTrustLevel(jobId: string): Promise<TrustLevelResponse> {
    if (MOCK_ENABLED) {
      return mockTrustLevel(jobId);
    }
    return http.get<TrustLevelResponse>(`/backtest/jobs/${jobId}/trust-level`);
  },
};

export default backtestService;

import { http } from './request';
import type { AppLocale } from '@/i18n';
import type { AnalysisReport, ReportDetailResponse } from '@/types/report';

export const reportService = {
  async getReport(reportId: string, locale: AppLocale): Promise<AnalysisReport> {
    const response = await http.get<ReportDetailResponse>(`/reports/${reportId}`, { locale });
    return response.data;
  },
};

export default reportService;

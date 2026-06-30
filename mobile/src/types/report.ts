import type { AppLocale } from '@/i18n';

export interface AnalysisReport {
  id: string;
  title: string;
  content: unknown;
  report_type: string;
  status: string;
  locale: AppLocale;
  requested_locale: AppLocale;
  language_match: boolean;
  created_at: string;
  updated_at?: string;
}

export interface ReportDetailResponse {
  data: AnalysisReport;
}

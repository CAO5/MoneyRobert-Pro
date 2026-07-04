/**
 * 新闻相关类型
 * 对接后端 /news/* 接口
 */

/** 新闻条目 */
export interface NewsItem {
  news_id: string;
  title: string;
  summary: string;
  url: string;
  source: string;
  published_at: string;
  sentiment?: 'positive' | 'negative' | 'neutral';
  sentiment_score?: number;
  related_symbols?: string[];
  category?: string;
}

/** 新闻情绪 */
export interface NewsSentiment {
  symbol: string;
  sentiment: 'positive' | 'negative' | 'neutral';
  score: number;
  news_count: number;
}

/** 重大事件 */
export interface MajorEvent {
  event_id: string;
  title: string;
  description: string;
  event_type: 'economic' | 'regulatory' | 'exchange' | 'protocol' | 'market';
  impact_level: 'low' | 'medium' | 'high' | 'critical';
  scheduled_at: string;
  related_symbols?: string[];
  source?: string;
}

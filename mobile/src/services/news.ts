import { http } from './request';
import type { NewsItem, NewsSentiment } from '@/types/news';

/**
 * 新闻服务
 * 对接后端 /news/* 接口
 *
 * 后端实际路由：
 * - GET  /news                     新闻列表
 * - POST /news/fetch               抓取新闻
 * - GET  /news/recent/{symbol}     品种相关最近新闻
 * - POST /news/sentiment/analyze   情绪分析
 * - GET  /news/sentiment/{symbol}  品种情绪
 * - GET  /news/sentiment/{symbol}/aggregated  聚合情绪
 * - GET  /news/sentiment/{symbol}/history     情绪历史
 * - GET  /news/{news_id}           新闻详情
 */
export const newsService = {
  /** 获取新闻列表 */
  async getNews(params?: {
    symbol?: string;
    category?: string;
    limit?: number;
    offset?: number;
  }): Promise<{ news: NewsItem[]; total: number }> {
    return http.get('/news', params);
  },

  /** 获取最近新闻（后端 /news 返回 {items:[...], page, page_size}） */
  async getRecent(limit = 10): Promise<NewsItem[]> {
    const res = await http.get<{ items: NewsItem[] } | { news: NewsItem[] } | NewsItem[]>(
      '/news',
      { limit },
    );
    if (Array.isArray(res)) return res;
    return res.items || res.news || [];
  },

  /** 获取品种相关新闻（后端 /news/recent/{symbol} 返回 {items:[...]}） */
  async getSymbolNews(symbol: string, limit?: number): Promise<NewsItem[]> {
    const res = await http.get<{ items: NewsItem[] } | { news: NewsItem[] } | NewsItem[]>(
      `/news/recent/${encodeURIComponent(symbol)}`,
      { limit },
    );
    if (Array.isArray(res)) return res;
    return res.items || res.news || [];
  },

  /** 获取新闻详情 */
  async getNewsDetail(newsId: string): Promise<NewsItem> {
    const res = await http.get<{ news: NewsItem } | NewsItem>(
      `/news/${encodeURIComponent(newsId)}`,
    );
    return Array.isArray(res) ? res[0] : ((res as { news?: NewsItem }).news || res as NewsItem);
  },

  /** 获取品种情绪 */
  async getSentiment(symbol: string): Promise<NewsSentiment> {
    return http.get<NewsSentiment>(`/news/sentiment/${encodeURIComponent(symbol)}`);
  },

  /** 获取聚合情绪 */
  async getAggregatedSentiment(symbol: string): Promise<NewsSentiment & { aggregated_sentiment: string }> {
    return http.get(`/news/sentiment/${encodeURIComponent(symbol)}/aggregated`);
  },

  /** 抓取最新新闻 */
  async fetchNews(): Promise<{ status: string; fetched: number; inserted: number }> {
    return http.post('/news/fetch');
  },
};

export default newsService;

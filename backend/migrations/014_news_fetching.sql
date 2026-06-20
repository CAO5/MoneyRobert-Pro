-- News feeds use the canonical article URL as their idempotency key.
DELETE FROM news duplicate
USING news original
WHERE duplicate.url = original.url
  AND (duplicate.created_at, duplicate.id::text) > (original.created_at, original.id::text);

CREATE UNIQUE INDEX IF NOT EXISTS idx_news_url_unique ON news(url);
CREATE INDEX IF NOT EXISTS idx_news_related_symbols ON news USING GIN(related_symbols);

-- $1/$2 are the spot/perpetual instruments, $3 is the UTC cutoff, $4 is the active stream.
WITH oi_points AS (
    SELECT boundary, x.id, x.payload
    FROM (VALUES ($3::timestamptz), ($3 - interval '5 minutes')) AS limits(boundary)
    LEFT JOIN LATERAL (
        SELECT id, payload, quality FROM llf_market_data
        WHERE symbol=$2 AND channel='open-interest' AND session_id=$4
          AND exchange_ts BETWEEN boundary - interval '15 seconds' AND boundary
          AND received_at <= boundary
        ORDER BY exchange_ts DESC, id DESC LIMIT 1
    ) x ON x.quality='ok'
), funding AS (
    SELECT id, quality, (payload->>'fundingRate')::float8 AS rate,
        ((payload->>'nextFundingTime')::bigint - (payload->>'fundingTime')::bigint) AS interval_ms
    FROM (
        SELECT id, quality, CASE WHEN quality='ok' THEN payload END AS payload
        FROM llf_market_data WHERE symbol=$2 AND channel='funding-rate' AND session_id=$4
          AND exchange_ts BETWEEN $3 - interval '180 seconds' AND $3 AND received_at <= $3
        ORDER BY exchange_ts DESC, id DESC LIMIT 1
    ) x
), contract AS (
    SELECT id, CASE WHEN quality='ok' THEN (payload->>'ctVal')::float8 END AS multiplier
    FROM llf_market_data
    WHERE symbol=$2 AND channel='instruments'
      AND exchange_ts BETWEEN $3 - interval '25 minutes' AND $3 - interval '5 minutes'
    ORDER BY exchange_ts DESC, id DESC LIMIT 1
), change_in_contract AS (
    SELECT 1 FROM llf_market_data m CROSS JOIN contract c
    WHERE m.symbol=$2 AND m.channel='instruments'
      AND m.exchange_ts > $3 - interval '5 minutes' AND m.exchange_ts <= $3
      AND CASE WHEN m.quality='ok'
               THEN (m.payload->>'ctVal')::float8 IS DISTINCT FROM c.multiplier ELSE true END
    LIMIT 1
), basis_pairs AS (
    SELECT (p.payload#>>'{bids,0,0}')::float8 + (p.payload#>>'{asks,0,0}')::float8 AS perp,
           (s.payload#>>'{bids,0,0}')::float8 + (s.payload#>>'{asks,0,0}')::float8 AS spot
    FROM llf_market_data p
    CROSS JOIN LATERAL (
        SELECT payload, exchange_ts FROM llf_market_data m
        WHERE m.symbol=$1 AND m.channel='books5' AND m.session_id=$4 AND m.quality='ok'
          AND m.received_at <= $3 AND m.exchange_ts <= $3
          AND m.exchange_ts BETWEEN GREATEST($3 - interval '60 seconds',p.exchange_ts-interval '250 milliseconds')
                                AND p.exchange_ts + interval '250 milliseconds'
        ORDER BY abs(extract(epoch FROM m.exchange_ts-p.exchange_ts)), m.exchange_ts, m.id LIMIT 1
    ) s
    WHERE p.symbol=$2 AND p.channel='books5' AND p.session_id=$4 AND p.quality='ok'
      AND p.exchange_ts > $3 - interval '60 seconds' AND p.exchange_ts <= $3 AND p.received_at <= $3
), basis AS (
    SELECT percentile_cont(0.5) WITHIN GROUP (ORDER BY perp/spot-1) AS value, count(*) AS n
    FROM basis_pairs
), volumes AS (
    SELECT symbol,
           sum(CASE WHEN quality='ok' THEN (payload->>'px')::float8*(payload->>'sz')::float8 END)
               FILTER (WHERE payload->>'side'='buy') AS buy,
           sum(CASE WHEN quality='ok' THEN (payload->>'px')::float8*(payload->>'sz')::float8 END)
               FILTER (WHERE payload->>'side'='sell') AS sell,
           bool_and(quality='ok') AS complete
    FROM llf_market_data
    WHERE symbol IN ($1,$2) AND channel='trades' AND session_id=$4 AND received_at <= $3
      AND exchange_ts > $3 - interval '5 minutes' AND exchange_ts <= $3
    GROUP BY symbol
), flow AS (
    SELECT ((coalesce(p.buy,0)-coalesce(p.sell,0))*c.multiplier)
               / ((coalesce(p.buy,0)+coalesce(p.sell,0))*c.multiplier+2.220446049250313e-16)
         - (coalesce(s.buy,0)-coalesce(s.sell,0))
               / (coalesce(s.buy,0)+coalesce(s.sell,0)+2.220446049250313e-16) AS value
    FROM volumes p CROSS JOIN volumes s CROSS JOIN contract c
    WHERE p.symbol=$2 AND s.symbol=$1 AND p.complete AND s.complete
      AND NOT EXISTS (SELECT 1 FROM change_in_contract)
      AND NOT EXISTS (
          SELECT 1 FROM llf_market_gaps g WHERE g.symbol IN ($1,$2) AND g.channel='trades'
            AND g.session_id=$4 AND g.detected_at > $3 - interval '5 minutes' AND g.detected_at <= $3
      )
), depth_events AS (
    (SELECT id, exchange_ts, payload FROM llf_market_data
     WHERE symbol=$2 AND channel='depth-25bp' AND session_id=$4 AND received_at <= $3
       AND exchange_ts BETWEEN $3 - interval '35 seconds' AND $3 - interval '30 seconds'
     ORDER BY exchange_ts DESC, id DESC LIMIT 1)
    UNION ALL
    SELECT id, exchange_ts, payload FROM llf_market_data
    WHERE symbol=$2 AND channel='depth-25bp' AND session_id=$4 AND received_at <= $3
      AND exchange_ts > $3 - interval '30 seconds' AND exchange_ts <= $3
), depth_intervals AS (
    SELECT CASE WHEN payload->>'event_depth'='true' AND payload->>'bid_covered'='true'
                THEN (payload->>'bid_price_size')::float8 END AS depth,
           extract(epoch FROM lead(exchange_ts,1,$3) OVER(ORDER BY exchange_ts,id)
               - greatest(exchange_ts,$3 - interval '30 seconds')) AS seconds
    FROM depth_events
), depth_rank AS (
    SELECT depth, sum(seconds) OVER(ORDER BY depth) AS duration
    FROM depth_intervals
), liquidity AS (
    SELECT -ln(d.depth*c.multiplier) AS value
    FROM depth_rank d CROSS JOIN contract c
    WHERE d.duration >= 15 AND d.depth > 0 AND c.multiplier > 0
      AND NOT EXISTS (SELECT 1 FROM change_in_contract)
      AND (SELECT min(exchange_ts) FROM depth_events) <= $3 - interval '30 seconds'
      AND (SELECT max(exchange_ts) FROM depth_events) >= $3 - interval '5 seconds'
      AND NOT EXISTS (SELECT 1 FROM depth_intervals WHERE depth IS NULL OR depth <= 0)
      AND NOT EXISTS (
          SELECT 1 FROM llf_market_gaps g WHERE g.symbol=$2 AND g.channel='books' AND g.session_id=$4
            AND g.detected_at > $3 - interval '35 seconds' AND g.detected_at <= $3
      )
    ORDER BY d.depth LIMIT 1
)
SELECT
    (SELECT ln((payload->>'oiUsd')::float8) FROM oi_points WHERE boundary=$3) AS oi_level,
    (SELECT ln((curr.payload->>'oiCcy')::float8 / (prev.payload->>'oiCcy')::float8)
     FROM oi_points curr CROSS JOIN oi_points prev WHERE curr.boundary=$3 AND prev.boundary<$3) AS oi_growth,
    (SELECT rate*28800000/interval_ms FROM funding WHERE quality='ok' AND interval_ms>0) AS funding,
    (SELECT value FROM basis) AS basis,
    (SELECT value FROM flow) AS flow,
    (SELECT value FROM liquidity) AS liquidity,
    jsonb_build_object('oi_ids',(SELECT jsonb_agg(id ORDER BY boundary) FROM oi_points),
        'funding_id',(SELECT id FROM funding),'instrument_id',(SELECT id FROM contract),
        'basis_pairs',(SELECT n FROM basis),'session_id',$4::uuid,'cutoff',$3) AS evidence

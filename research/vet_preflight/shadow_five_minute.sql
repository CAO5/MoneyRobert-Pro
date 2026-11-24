COPY (
    SELECT s.asset,s.available_at,s.origin_ts,l.target_ts,l.completed_at,s.quality_status,
           l.status AS label_status,s.past_1m_rv,s.past_5m_rv,s.past_30m_rv,s.a,s.l_depth,s.vet,
           l.realized_variance AS future_5m_variance
    FROM vet_shadow_samples s
    JOIN vet_shadow_labels l USING(run_id,asset,exchange_ts)
    WHERE s.factor_version='vet-v0.2-shadow' AND s.quality_status='OK'
      AND l.minutes=5 AND l.status='COMPLETE'
    ORDER BY s.asset,s.origin_ts
) TO STDOUT WITH CSV HEADER;

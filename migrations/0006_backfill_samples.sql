-- Copy the one-row-per-frame data from before 0005 into the per-sample tables, then drop the frame tables.
-- One hour at a time, compressing each finished hour, so only about an hour of new rows is ever uncompressed.
SET LOCAL search_path = vitals_data, public;

DO $$
DECLARE
  h  timestamptz;
  hi timestamptz;
  c  regclass;
BEGIN
  SELECT date_trunc('hour', min(lo)), max(up) INTO h, hi
  FROM (SELECT min(time) AS lo, max(time) AS up FROM ecg_frame
        UNION ALL SELECT min(time), max(time) FROM acc_frame) m;
  WHILE h <= hi LOOP
    -- Same time formula as ingest; ON CONFLICT skips frames the phone resent after 0005.
    INSERT INTO ecg (time, user_id, source, device_id, uv)
    SELECT f.time + (s.i - 1) * interval '1 second' / f.sample_rate, f.user_id, f.source, f.device_id, s.uv
    FROM ecg_frame f CROSS JOIN LATERAL unnest(f.samples) WITH ORDINALITY AS s(uv, i)
    WHERE f.time >= h AND f.time < h + interval '1 hour'
    ON CONFLICT DO NOTHING;
    INSERT INTO acc (time, user_id, source, device_id, x, y, z)
    SELECT f.time + (s.i - 1) * interval '1 second' / f.sample_rate, f.user_id, f.source, f.device_id, s.x, s.y, s.z
    FROM acc_frame f CROSS JOIN LATERAL unnest(f.x, f.y, f.z) WITH ORDINALITY AS s(x, y, z, i)
    WHERE f.time >= h AND f.time < h + interval '1 hour'
    ON CONFLICT DO NOTHING;
    -- A frame's last samples can spill into the next hour, so only chunks ending by h are complete.
    FOR c IN SELECT format('%I.%I', chunk_schema, chunk_name)::regclass FROM timescaledb_information.chunks
             WHERE hypertable_schema = 'vitals_data' AND hypertable_name IN ('ecg', 'acc')
               AND NOT is_compressed AND range_end <= h LOOP
      PERFORM compress_chunk(c);
    END LOOP;
    h := h + interval '1 hour';
  END LOOP;
END $$;

DROP VIEW vitals.ecg_frame;
DROP VIEW vitals.acc_frame;
DROP TABLE ecg_frame;
DROP TABLE acc_frame;

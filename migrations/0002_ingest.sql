-- Raw Polar H10 frames, written by POST /api/v1/ingest as vitals_app.
-- The raw tables have no RLS: TimescaleDB refuses compression on tables with row security, and uncompressed raw
-- ECG/ACC is ~0.5 GB per day. The server sets user_id from the token; readers still only see their own rows via the views.
SET LOCAL search_path = vitals_data, public;

-- Polar HR frames land in heart_rate (source 'polar_h10') so the MCP tools see them; rr_ms carries the RR intervals.
ALTER TABLE heart_rate ADD COLUMN rr_ms smallint[];

CREATE TABLE ecg (
  time        timestamptz NOT NULL,
  user_id     text        NOT NULL,
  source      text        NOT NULL,
  device_id   text        NOT NULL,
  sample_rate smallint    NOT NULL CHECK (sample_rate > 0),
  samples     int[]       NOT NULL,  -- µV
  UNIQUE (user_id, source, device_id, time)
);

CREATE TABLE acc (
  time        timestamptz NOT NULL,
  user_id     text        NOT NULL,
  source      text        NOT NULL,
  device_id   text        NOT NULL,
  sample_rate smallint    NOT NULL CHECK (sample_rate > 0),
  x           smallint[]  NOT NULL,  -- milli-g
  y           smallint[]  NOT NULL,
  z           smallint[]  NOT NULL,
  CHECK (cardinality(x) = cardinality(y) AND cardinality(y) = cardinality(z)),
  UNIQUE (user_id, source, device_id, time)
);

CREATE TABLE device_battery (
  time      timestamptz NOT NULL,
  user_id   text        NOT NULL,
  source    text        NOT NULL,
  device_id text        NOT NULL,
  level     smallint    NOT NULL CHECK (level BETWEEN 0 AND 100),
  UNIQUE (user_id, source, device_id, time)
);

CREATE TABLE device_event (
  time      timestamptz NOT NULL,
  user_id   text        NOT NULL,
  source    text        NOT NULL,
  device_id text        NOT NULL,
  event     text        NOT NULL,
  UNIQUE (user_id, source, device_id, time)
);

SELECT create_hypertable('vitals_data.ecg', by_range('time', interval '1 day'));
SELECT create_hypertable('vitals_data.acc', by_range('time', interval '1 day'));
SELECT create_hypertable('vitals_data.device_battery', by_range('time'));
SELECT create_hypertable('vitals_data.device_event', by_range('time'));

DO $$
DECLARE t text;
BEGIN
  FOREACH t IN ARRAY ARRAY['ecg', 'acc', 'device_battery', 'device_event'] LOOP
    EXECUTE format('GRANT INSERT ON vitals_data.%I TO vitals_app', t);
    EXECUTE format('CREATE VIEW vitals.%I WITH (security_barrier) AS
                    SELECT * FROM vitals_data.%I WHERE user_id = (SELECT vitals.current_user_id())', t, t);
    EXECUTE format('GRANT SELECT ON vitals.%I TO vitals_reader', t);
  END LOOP;
END $$;

-- heart_rate keeps RLS (the MCP write tools use it): ingest may only add its own source for the signed user.
CREATE POLICY ingest_insert ON vitals_data.heart_rate FOR INSERT TO vitals_app
  WITH CHECK (user_id = (SELECT vitals.current_user_id()) AND source = 'polar_h10');

-- Compress chunks once the phone's outbox (7 days) can no longer resend into them.
ALTER TABLE vitals_data.ecg SET (timescaledb.compress, timescaledb.compress_segmentby = 'user_id, device_id',
                                 timescaledb.compress_orderby = 'time');
ALTER TABLE vitals_data.acc SET (timescaledb.compress, timescaledb.compress_segmentby = 'user_id, device_id',
                                 timescaledb.compress_orderby = 'time');
SELECT add_compression_policy('vitals_data.ecg', interval '8 days');
SELECT add_compression_policy('vitals_data.acc', interval '8 days');

-- SELECT * views freeze their column list at creation; recreate so the reader sees rr_ms.
CREATE OR REPLACE VIEW vitals.heart_rate WITH (security_barrier) AS
  SELECT * FROM vitals_data.heart_rate WHERE user_id = (SELECT vitals.current_user_id());

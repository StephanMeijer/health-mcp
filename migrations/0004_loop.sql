-- Polar Loop data via the ingest API: live skin temperature, nightly sleep, and Nightly Recharge.
SET LOCAL search_path = vitals_data, public;

-- Raw like acc: no RLS, the server sets user_id and checks device_id. Loose range: one bad sample rejects the whole batch.
CREATE TABLE skin_temp (
  time        timestamptz   NOT NULL,
  user_id     text          NOT NULL,
  source      vitals.source NOT NULL,
  device_id   integer       NOT NULL REFERENCES devices (id),
  sample_rate smallint      NOT NULL CHECK (sample_rate > 0),
  celsius     real[]        NOT NULL CHECK (0 < ALL (celsius) AND 50 > ALL (celsius)),
  UNIQUE (user_id, source, device_id, time)
);
SELECT create_hypertable('vitals_data.skin_temp', by_range('time', interval '1 day'));

-- One row per night; plain table, it stays tiny.
CREATE TABLE recharge (
  night          date          NOT NULL,
  user_id        text          NOT NULL,
  source         vitals.source NOT NULL,
  device_id      integer       NOT NULL REFERENCES devices (id),
  status         smallint      NOT NULL CHECK (status BETWEEN 1 AND 6),        -- 1 very poor … 6 very good
  ans_charge     real          NOT NULL CHECK (ans_charge BETWEEN -10 AND 10),
  hrv_ms         real          CHECK (hrv_ms > 0),
  breathing_rate real          CHECK (breathing_rate > 0),                     -- breaths per minute
  hr_bpm         smallint      CHECK (hr_bpm BETWEEN 20 AND 250),
  UNIQUE (user_id, source, device_id, night)
);

DO $$
DECLARE t text;
BEGIN
  FOREACH t IN ARRAY ARRAY['skin_temp', 'recharge'] LOOP
    EXECUTE format('GRANT INSERT ON vitals_data.%I TO vitals_app', t);
    EXECUTE format('CREATE VIEW vitals.%I WITH (security_barrier) AS
                    SELECT * FROM vitals_data.%I WHERE user_id = (SELECT vitals.current_user_id())', t, t);
    EXECUTE format('GRANT SELECT ON vitals.%I TO vitals_reader', t);
  END LOOP;
END $$;

-- App sleep records its device, like heart_rate in 0003; manual and imported sessions keep deduplicating on start_at.
ALTER TABLE sleep_session ADD COLUMN device_id integer REFERENCES devices (id);
ALTER TABLE sleep_session DROP CONSTRAINT sleep_session_user_id_source_start_at_key;
ALTER TABLE sleep_session ADD UNIQUE NULLS NOT DISTINCT (user_id, source, device_id, start_at);
CREATE OR REPLACE VIEW vitals.sleep_session WITH (security_barrier) AS
  SELECT * FROM sleep_session WHERE user_id = (SELECT vitals.current_user_id());

-- Ingest may only add app sleep for the signed user; manual_write still keeps the MCP tools off it.
CREATE POLICY ingest_insert ON sleep_session FOR INSERT TO vitals_app
  WITH CHECK (user_id = (SELECT vitals.current_user_id()) AND source = 'app'
              AND EXISTS (SELECT FROM devices d WHERE d.id = device_id));
CREATE POLICY ingest_insert ON sleep_stage FOR INSERT TO vitals_app
  WITH CHECK (user_id = (SELECT vitals.current_user_id())
              AND EXISTS (SELECT FROM sleep_session s WHERE s.id = session_id AND s.source = 'app'));

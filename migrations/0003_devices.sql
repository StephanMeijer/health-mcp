-- Per-user devices with a numeric id that every app measurement carries, and `source` as a 4-byte enum.
-- App data moves from 'polar_h10' to 'app'; any other source (e.g. 'withings') aborts the migration instead of being lost.
SET LOCAL search_path = vitals_data, public;

-- 'import' is for future importers, which describe what they read with a devices row, so no ALTER TYPE per importer.
CREATE TYPE vitals.source AS ENUM ('manual', 'app', 'import');

CREATE TABLE devices (
  id         integer     GENERATED ALWAYS AS IDENTITY PRIMARY KEY,
  user_id    text        NOT NULL,
  model      text        NOT NULL CHECK (length(model) BETWEEN 1 AND 100),   -- e.g. 'Polar H10'
  serial     text        NOT NULL CHECK (length(serial) BETWEEN 1 AND 100),  -- e.g. '244E7234'
  created_at timestamptz NOT NULL DEFAULT now(),
  UNIQUE (user_id, model, serial)
);
-- UPDATE (model) only so registration can be an idempotent ON CONFLICT DO UPDATE … RETURNING id.
GRANT SELECT, INSERT, UPDATE (model) ON devices TO vitals_app;
ALTER TABLE devices ENABLE ROW LEVEL SECURITY;
CREATE POLICY own_rows ON devices FOR ALL TO vitals_app
  USING (user_id = (SELECT vitals.current_user_id())) WITH CHECK (user_id = (SELECT vitals.current_user_id()));

-- Every serial seen so far came from the H10 via the app.
INSERT INTO devices (user_id, model, serial)
SELECT DISTINCT user_id, 'Polar H10', device_id FROM (
  SELECT user_id, device_id FROM ecg UNION SELECT user_id, device_id FROM acc
  UNION SELECT user_id, device_id FROM device_battery UNION SELECT user_id, device_id FROM device_event
) s;

-- Everything that depends on source or device_id has to go before their types can change; recreated below.
DROP VIEW vitals.heart_rate, vitals.hrv, vitals.blood_pressure, vitals.body, vitals.sleep_session,
          vitals.ecg, vitals.acc, vitals.device_battery, vitals.device_event;
DROP POLICY manual_write ON heart_rate;
DROP POLICY manual_write ON hrv;
DROP POLICY manual_write ON blood_pressure;
DROP POLICY manual_write ON body;
DROP POLICY manual_write ON sleep_session;
DROP POLICY manual_write ON sleep_stage;
DROP POLICY ingest_insert ON heart_rate;

-- Column types can't change on compressed hypertables, and device_id is a segmentby column.
SELECT remove_compression_policy('vitals_data.ecg');
SELECT remove_compression_policy('vitals_data.acc');
SELECT decompress_chunk(c, if_compressed => true) FROM show_chunks('vitals_data.ecg') c;
SELECT decompress_chunk(c, if_compressed => true) FROM show_chunks('vitals_data.acc') c;
ALTER TABLE ecg SET (timescaledb.compress = false);
ALTER TABLE acc SET (timescaledb.compress = false);

DO $$
DECLARE t text;
BEGIN
  FOREACH t IN ARRAY ARRAY['ecg', 'acc', 'device_battery', 'device_event'] LOOP
    EXECUTE format('ALTER TABLE %I ADD COLUMN device integer', t);
    EXECUTE format('UPDATE %I r SET device = d.id FROM devices d
                    WHERE d.user_id = r.user_id AND d.model = %L AND d.serial = r.device_id', t, 'Polar H10');
    -- Also drops the old UNIQUE (user_id, source, device_id, time).
    EXECUTE format('ALTER TABLE %I DROP COLUMN device_id', t);
    EXECUTE format('ALTER TABLE %I RENAME COLUMN device TO device_id', t);
    EXECUTE format('ALTER TABLE %I ALTER COLUMN device_id SET NOT NULL', t);
    EXECUTE format('ALTER TABLE %I ADD FOREIGN KEY (device_id) REFERENCES devices (id)', t);
  END LOOP;

  FOREACH t IN ARRAY ARRAY['heart_rate', 'hrv', 'blood_pressure', 'body', 'sleep_session',
                           'ecg', 'acc', 'device_battery', 'device_event'] LOOP
    EXECUTE format('ALTER TABLE %I ALTER COLUMN source TYPE vitals.source
                    USING (CASE source WHEN %L THEN %L ELSE source END)::vitals.source', t, 'polar_h10', 'app');
  END LOOP;

  FOREACH t IN ARRAY ARRAY['ecg', 'acc', 'device_battery', 'device_event'] LOOP
    EXECUTE format('ALTER TABLE %I ADD UNIQUE (user_id, source, device_id, time)', t);
  END LOOP;
END $$;

-- Strap heart rate now records its device, so two trackers at the same instant are two rows.
-- Manual and imported rows have no device; NULLS NOT DISTINCT keeps them deduplicating as before.
ALTER TABLE heart_rate ADD COLUMN device_id integer REFERENCES devices (id);
ALTER TABLE heart_rate DROP CONSTRAINT heart_rate_user_id_source_time_key;
ALTER TABLE heart_rate ADD UNIQUE NULLS NOT DISTINCT (user_id, source, device_id, time);
-- The old frames never stored their serial: only a user with exactly one device can be attributed.
UPDATE heart_rate h SET device_id = d.id
FROM (SELECT user_id, min(id) AS id FROM devices GROUP BY user_id HAVING count(*) = 1) d
WHERE h.user_id = d.user_id AND h.source = 'app';

-- Same policies as 0001/0002, with the app's source.
DO $$
DECLARE t text;
BEGIN
  FOREACH t IN ARRAY ARRAY['heart_rate', 'hrv', 'blood_pressure', 'body', 'sleep_session'] LOOP
    EXECUTE format('CREATE POLICY manual_write ON %I FOR ALL TO vitals_app
                    USING (user_id = (SELECT vitals.current_user_id()) AND source = %L)
                    WITH CHECK (user_id = (SELECT vitals.current_user_id()) AND source = %L)', t, 'manual', 'manual');
  END LOOP;

  FOREACH t IN ARRAY ARRAY['heart_rate', 'hrv', 'blood_pressure', 'body', 'sleep_session',
                           'ecg', 'acc', 'device_battery', 'device_event', 'devices'] LOOP
    EXECUTE format('CREATE VIEW vitals.%I WITH (security_barrier) AS
                    SELECT * FROM vitals_data.%I WHERE user_id = (SELECT vitals.current_user_id())', t, t);
    EXECUTE format('GRANT SELECT ON vitals.%I TO vitals_reader', t);
  END LOOP;
END $$;

CREATE POLICY manual_write ON sleep_stage FOR ALL TO vitals_app
  USING (user_id = (SELECT vitals.current_user_id())
         AND EXISTS (SELECT FROM sleep_session s WHERE s.id = session_id AND s.source = 'manual'))
  WITH CHECK (user_id = (SELECT vitals.current_user_id())
         AND EXISTS (SELECT FROM sleep_session s WHERE s.id = session_id AND s.source = 'manual'));

-- Ingest may only add app rows for the signed user, from one of that user's devices.
CREATE POLICY ingest_insert ON heart_rate FOR INSERT TO vitals_app
  WITH CHECK (user_id = (SELECT vitals.current_user_id()) AND source = 'app'
              AND EXISTS (SELECT FROM devices d WHERE d.id = device_id));

ALTER TABLE ecg SET (timescaledb.compress, timescaledb.compress_segmentby = 'user_id, device_id',
                     timescaledb.compress_orderby = 'time');
ALTER TABLE acc SET (timescaledb.compress, timescaledb.compress_segmentby = 'user_id, device_id',
                     timescaledb.compress_orderby = 'time');
SELECT add_compression_policy('vitals_data.ecg', interval '8 days');
SELECT add_compression_policy('vitals_data.acc', interval '8 days');

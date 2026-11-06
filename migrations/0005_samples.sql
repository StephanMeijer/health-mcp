-- ECG and ACC as one row per sample. TimescaleDB compresses array columns generically but scalar columns with
-- delta-of-delta: on a real H10 recording this stores ~37 % less once compressed and reads 3–20× faster.
-- Rows are ~15× larger until compressed, so chunks are 1 hour and compressed after 2 hours.
-- The old one-row-per-frame tables stay readable as ecg_frame / acc_frame until their data is copied over.
SET LOCAL search_path = vitals_data, public;

ALTER TABLE ecg RENAME TO ecg_frame;
ALTER TABLE acc RENAME TO acc_frame;
-- Free the names the new hypertables' indexes would take.
ALTER INDEX ecg_time_idx RENAME TO ecg_frame_time_idx;
ALTER INDEX acc_time_idx RENAME TO acc_frame_time_idx;
ALTER INDEX ecg_user_id_source_device_id_time_key RENAME TO ecg_frame_user_id_source_device_id_time_key;
ALTER INDEX acc_user_id_source_device_id_time_key RENAME TO acc_frame_user_id_source_device_id_time_key;
-- Views follow the table by OID and keep their grants.
ALTER VIEW vitals.ecg RENAME TO ecg_frame;
ALTER VIEW vitals.acc RENAME TO acc_frame;

-- Raw like before: no RLS (it blocks compression), the server sets user_id and checks device_id.
CREATE TABLE ecg (
  time      timestamptz   NOT NULL,
  user_id   text          NOT NULL,
  source    vitals.source NOT NULL,
  device_id integer       NOT NULL REFERENCES devices (id),
  uv        integer       NOT NULL,  -- µV
  UNIQUE (user_id, source, device_id, time)
);

CREATE TABLE acc (
  time      timestamptz   NOT NULL,
  user_id   text          NOT NULL,
  source    vitals.source NOT NULL,
  device_id integer       NOT NULL REFERENCES devices (id),
  x         smallint      NOT NULL,  -- milli-g
  y         smallint      NOT NULL,
  z         smallint      NOT NULL,
  UNIQUE (user_id, source, device_id, time)
);

DO $$
DECLARE t text;
BEGIN
  FOREACH t IN ARRAY ARRAY['ecg', 'acc'] LOOP
    PERFORM create_hypertable(format('vitals_data.%I', t), by_range('time', interval '1 hour'));
    -- source is in the unique key, so it must be a segmentby column too; without it batches compress far worse.
    EXECUTE format('ALTER TABLE %I SET (timescaledb.compress, timescaledb.compress_segmentby = %L,
                    timescaledb.compress_orderby = %L)', t, 'user_id, source, device_id', 'time');
    -- Late resends from the phone's 7-day outbox land in compressed chunks and still deduplicate.
    PERFORM add_compression_policy(format('vitals_data.%I', t), interval '2 hours');
    EXECUTE format('GRANT INSERT ON vitals_data.%I TO vitals_app', t);
    EXECUTE format('CREATE VIEW vitals.%I WITH (security_barrier) AS
                    SELECT * FROM vitals_data.%I WHERE user_id = (SELECT vitals.current_user_id())', t, t);
    EXECUTE format('GRANT SELECT ON vitals.%I TO vitals_reader', t);
  END LOOP;
END $$;

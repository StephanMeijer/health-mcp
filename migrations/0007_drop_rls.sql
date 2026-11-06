-- No row-level security: TimescaleDB refuses to compress hypertables that have it. The typed tools and ingest
-- filter on user_id in every query instead; the SQL tool keeps reading through the signed vitals.* views.
-- Then compress every hypertable that wasn't yet (ecg and acc are, since 0005).
SET LOCAL search_path = vitals_data, public;

DO $$
DECLARE r record;
BEGIN
  FOR r IN SELECT policyname, tablename FROM pg_policies WHERE schemaname = 'vitals_data' LOOP
    EXECUTE format('DROP POLICY %I ON vitals_data.%I', r.policyname, r.tablename);
  END LOOP;
  FOR r IN SELECT c.relname FROM pg_class c JOIN pg_namespace n ON n.oid = c.relnamespace
           WHERE n.nspname = 'vitals_data' AND c.relrowsecurity LOOP
    EXECUTE format('ALTER TABLE vitals_data.%I DISABLE ROW LEVEL SECURITY', r.relname);
  END LOOP;
END $$;

-- segmentby is each table's unique key minus time, which TimescaleDB needs to check uniqueness in compressed chunks.
-- 7 days: after the phone's outbox can resend; late resends and manual edits still work, just slower.
DO $$
DECLARE
  t   text[];
BEGIN
  FOREACH t SLICE 1 IN ARRAY ARRAY[
    ['heart_rate',     'user_id, source, device_id'],
    ['hrv',            'user_id, source'],
    ['blood_pressure', 'user_id, source'],
    ['body',           'user_id, source'],
    ['sleep_stage',    'user_id, session_id'],
    ['device_battery', 'user_id, source, device_id'],
    ['device_event',   'user_id, source, device_id'],
    ['skin_temp',      'user_id, source, device_id']
  ] LOOP
    EXECUTE format('ALTER TABLE %I SET (timescaledb.compress, timescaledb.compress_segmentby = %L,
                    timescaledb.compress_orderby = %L)', t[1], t[2], 'time');
    PERFORM add_compression_policy(format('vitals_data.%I', t[1]), interval '7 days');
  END LOOP;
END $$;

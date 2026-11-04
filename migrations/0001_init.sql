-- Run as the owner role (needs CREATEROLE when the vitals_* roles don't exist yet).
CREATE EXTENSION IF NOT EXISTS timescaledb;
CREATE EXTENSION IF NOT EXISTS pgcrypto WITH SCHEMA public;

DO $$
BEGIN
  IF NOT EXISTS (SELECT FROM pg_roles WHERE rolname = 'vitals_app') THEN CREATE ROLE vitals_app NOLOGIN; END IF;
  IF NOT EXISTS (SELECT FROM pg_roles WHERE rolname = 'vitals_reader') THEN CREATE ROLE vitals_reader NOLOGIN; END IF;
END $$;

-- Vitals ------------------------------------------------------------------
-- Base tables: written by importers (BYPASSRLS) and vitals_app (RLS). vitals_reader never gets a grant here:
-- TimescaleDB chunks copy the hypertable's grants but not its RLS, so a grant would expose every user's rows.
CREATE SCHEMA vitals_data;
SET LOCAL search_path = vitals_data, public;

CREATE TABLE heart_rate (
  time    timestamptz NOT NULL,
  user_id text        NOT NULL,
  source  text        NOT NULL,
  bpm     smallint    NOT NULL CHECK (bpm BETWEEN 20 AND 250),
  note    text,
  UNIQUE (user_id, source, time)
);

CREATE TABLE hrv (
  time     timestamptz NOT NULL,
  user_id  text        NOT NULL,
  source   text        NOT NULL,
  rmssd_ms real        NOT NULL CHECK (rmssd_ms > 0 AND rmssd_ms <= 500),
  note     text,
  UNIQUE (user_id, source, time)
);

CREATE TABLE blood_pressure (
  time      timestamptz NOT NULL,
  user_id   text        NOT NULL,
  source    text        NOT NULL,
  systolic  smallint    NOT NULL CHECK (systolic BETWEEN 50 AND 260),
  diastolic smallint    NOT NULL CHECK (diastolic BETWEEN 30 AND 180),
  pulse     smallint             CHECK (pulse BETWEEN 20 AND 250),
  note      text,
  CHECK (systolic > diastolic),
  UNIQUE (user_id, source, time)
);

CREATE TABLE body (
  time      timestamptz NOT NULL,
  user_id   text        NOT NULL,
  source    text        NOT NULL,
  weight_kg real CHECK (weight_kg BETWEEN 20 AND 400),
  fat_pct   real CHECK (fat_pct BETWEEN 1 AND 80),
  spo2_pct  real CHECK (spo2_pct BETWEEN 50 AND 100),
  temp_c    real CHECK (temp_c BETWEEN 30 AND 45),
  note      text,
  CHECK (num_nonnulls(weight_kg, fat_pct, spo2_pct, temp_c) > 0),
  UNIQUE (user_id, source, time)
);

CREATE TABLE sleep_session (
  id       uuid        PRIMARY KEY DEFAULT gen_random_uuid(),
  user_id  text        NOT NULL,
  source   text        NOT NULL,
  start_at timestamptz NOT NULL,
  end_at   timestamptz NOT NULL,
  note     text,
  CHECK (end_at > start_at AND end_at - start_at <= interval '24 hours'),
  UNIQUE (user_id, source, start_at)
);

CREATE TABLE sleep_stage (
  time       timestamptz NOT NULL,
  user_id    text        NOT NULL,
  session_id uuid        NOT NULL REFERENCES sleep_session (id) ON DELETE CASCADE,
  stage      text        NOT NULL CHECK (stage IN ('awake', 'light', 'deep', 'rem')),
  duration_s integer     NOT NULL CHECK (duration_s > 0),
  PRIMARY KEY (session_id, time)
);

SELECT create_hypertable(t, by_range('time'))
FROM unnest(ARRAY['vitals_data.heart_rate', 'vitals_data.hrv', 'vitals_data.blood_pressure',
                  'vitals_data.body', 'vitals_data.sleep_stage']::regclass[]) AS t;

-- Signed user id ----------------------------------------------------------
-- The server sets vitals.user_id + vitals.user_sig per transaction. A query can set its own
-- vitals.user_id, but cannot produce a valid signature: the secret is unreadable to login roles.

CREATE SCHEMA vitals_private;
REVOKE ALL ON SCHEMA vitals_private FROM PUBLIC;
CREATE TABLE vitals_private.secret (key bytea NOT NULL);
INSERT INTO vitals_private.secret VALUES (public.gen_random_bytes(32));

CREATE FUNCTION vitals_private.sign(uid text) RETURNS text
LANGUAGE sql STABLE SECURITY DEFINER SET search_path = pg_catalog AS $$
  SELECT encode(public.hmac(convert_to(uid, 'UTF8'), s.key, 'sha256'), 'hex') FROM vitals_private.secret s
$$;

CREATE SCHEMA vitals;
CREATE FUNCTION vitals.current_user_id() RETURNS text
LANGUAGE plpgsql STABLE SECURITY DEFINER SET search_path = pg_catalog AS $$
DECLARE
  uid text := current_setting('vitals.user_id', true);
  sig text := current_setting('vitals.user_sig', true);
BEGIN
  IF coalesce(uid, '') = '' OR sig IS DISTINCT FROM vitals_private.sign(uid) THEN
    RAISE EXCEPTION 'vitals: no authenticated user' USING ERRCODE = 'insufficient_privilege';
  END IF;
  RETURN uid;
END
$$;

REVOKE ALL ON FUNCTION vitals_private.sign(text), vitals.current_user_id() FROM PUBLIC;
GRANT USAGE ON SCHEMA vitals_private TO vitals_app;
GRANT EXECUTE ON FUNCTION vitals_private.sign(text) TO vitals_app;
GRANT USAGE ON SCHEMA vitals TO vitals_app, vitals_reader;
GRANT EXECUTE ON FUNCTION vitals.current_user_id() TO vitals_app, vitals_reader;

-- Grants, RLS (vitals_app) and views (vitals_reader) -------------------------

GRANT USAGE ON SCHEMA vitals_data TO vitals_app;

DO $$
DECLARE t text;
BEGIN
  FOREACH t IN ARRAY ARRAY['heart_rate', 'hrv', 'blood_pressure', 'body', 'sleep_session', 'sleep_stage'] LOOP
    EXECUTE format('GRANT SELECT, INSERT, UPDATE, DELETE ON vitals_data.%I TO vitals_app', t);
    EXECUTE format('ALTER TABLE vitals_data.%I ENABLE ROW LEVEL SECURITY', t);
    EXECUTE format('CREATE POLICY own_read ON vitals_data.%I FOR SELECT TO vitals_app
                    USING (user_id = (SELECT vitals.current_user_id()))', t);
    -- security_barrier: the user filter runs before any function in the caller's query.
    EXECUTE format('CREATE VIEW vitals.%I WITH (security_barrier) AS
                    SELECT * FROM vitals_data.%I WHERE user_id = (SELECT vitals.current_user_id())', t, t);
    EXECUTE format('GRANT SELECT ON vitals.%I TO vitals_reader', t);
  END LOOP;

  -- Write tools only ever touch manually entered rows.
  FOREACH t IN ARRAY ARRAY['heart_rate', 'hrv', 'blood_pressure', 'body', 'sleep_session'] LOOP
    EXECUTE format('CREATE POLICY manual_write ON vitals_data.%I FOR ALL TO vitals_app
                    USING (user_id = (SELECT vitals.current_user_id()) AND source = %L)
                    WITH CHECK (user_id = (SELECT vitals.current_user_id()) AND source = %L)', t, 'manual', 'manual');
  END LOOP;
END $$;

CREATE POLICY manual_write ON vitals_data.sleep_stage FOR ALL TO vitals_app
  USING (user_id = (SELECT vitals.current_user_id())
         AND EXISTS (SELECT FROM vitals_data.sleep_session s WHERE s.id = session_id AND s.source = 'manual'))
  WITH CHECK (user_id = (SELECT vitals.current_user_id())
         AND EXISTS (SELECT FROM vitals_data.sleep_session s WHERE s.id = session_id AND s.source = 'manual'));

ALTER ROLE vitals_reader SET search_path = vitals, public;
ALTER ROLE vitals_reader SET statement_timeout = '5s';

-- Sharing: an owner lets another user read (ro) or read and write (rw) their gym and/or vitals data.
-- Only vitals_app touches these tables; vitals_reader gets nothing (see SPEC.md, Sharing).

CREATE SCHEMA share;

-- A pending invite, by email. Only the sha256 of the emailed code is stored. Accepting or declining deletes it;
-- inviting the same address again replaces it.
CREATE TABLE share.invite (
  id          bigint GENERATED ALWAYS AS IDENTITY PRIMARY KEY,
  code_hash   bytea NOT NULL UNIQUE,
  owner_sub   text NOT NULL,
  owner_email text NOT NULL,
  email       text NOT NULL CHECK (email = lower(email)),
  gym         text NOT NULL CHECK (gym IN ('none', 'ro', 'rw')),
  vitals      text NOT NULL CHECK (vitals IN ('none', 'ro', 'rw')),
  created_at  timestamptz NOT NULL DEFAULT now(),
  expires_at  timestamptz NOT NULL DEFAULT now() + interval '7 days',
  CHECK (gym <> 'none' OR vitals <> 'none'),
  UNIQUE (owner_sub, email)
);
CREATE INDEX ON share.invite (email);

-- An accepted share. The emails are the ones verified at invite and accept time, for display and for `owner`.
CREATE TABLE share.access (
  owner_sub     text NOT NULL,
  grantee_sub   text NOT NULL,
  owner_email   text NOT NULL,
  grantee_email text NOT NULL,
  gym           text NOT NULL CHECK (gym IN ('none', 'ro', 'rw')),
  vitals        text NOT NULL CHECK (vitals IN ('none', 'ro', 'rw')),
  created_at    timestamptz NOT NULL DEFAULT now(),
  updated_at    timestamptz NOT NULL DEFAULT now(),
  PRIMARY KEY (owner_sub, grantee_sub),
  CHECK (owner_sub <> grantee_sub),
  CHECK (gym <> 'none' OR vitals <> 'none')
);
CREATE INDEX ON share.access (grantee_sub);

GRANT USAGE ON SCHEMA share TO vitals_app;
GRANT SELECT, INSERT, UPDATE, DELETE ON ALL TABLES IN SCHEMA share TO vitals_app;
GRANT USAGE ON ALL SEQUENCES IN SCHEMA share TO vitals_app;

-- Signed domains -------------------------------------------------------------------------------------------------
-- sql_query on someone else's data may only see the domains they shared. The server now also sets vitals.domains
-- ('gym,vitals' or a subset) and signs domains || '/' || user_id: domains never contain '/', so the split is
-- unambiguous, and a query can't widen its domains without breaking the signature.

CREATE FUNCTION vitals_private.user_for(domain text) RETURNS text
LANGUAGE plpgsql STABLE SECURITY DEFINER SET search_path = pg_catalog AS $$
DECLARE
  uid  text := current_setting('vitals.user_id', true);
  doms text := coalesce(current_setting('vitals.domains', true), '');
  sig  text := current_setting('vitals.user_sig', true);
BEGIN
  IF coalesce(uid, '') = '' OR doms !~ '^[a-z,]*$' OR sig IS DISTINCT FROM vitals_private.sign(doms || '/' || uid) THEN
    RAISE EXCEPTION 'vitals: no authenticated user' USING ERRCODE = 'insufficient_privilege';
  END IF;
  IF NOT domain = ANY (string_to_array(doms, ',')) THEN
    RAISE EXCEPTION 'no access to the % data of this user', domain USING ERRCODE = 'insufficient_privilege';
  END IF;
  RETURN uid;
END
$$;
REVOKE ALL ON FUNCTION vitals_private.user_for(text) FROM PUBLIC;

-- The vitals views keep calling current_user_id(); it now also requires the vitals domain.
CREATE OR REPLACE FUNCTION vitals.current_user_id() RETURNS text
LANGUAGE sql STABLE SECURITY DEFINER SET search_path = pg_catalog AS $$
  SELECT vitals_private.user_for('vitals')
$$;

CREATE FUNCTION vitals.gym_user_id() RETURNS text
LANGUAGE sql STABLE SECURITY DEFINER SET search_path = pg_catalog AS $$
  SELECT vitals_private.user_for('gym')
$$;
REVOKE ALL ON FUNCTION vitals.gym_user_id() FROM PUBLIC;
GRANT EXECUTE ON FUNCTION vitals.gym_user_id() TO vitals_app, vitals_reader;

-- The gym views (0010) filter on the gym domain instead.
CREATE OR REPLACE VIEW vitals.user_settings WITH (security_barrier) AS
  SELECT owner_sub, timezone, goal, goals_notes, priority_muscle_groups, limitations
  FROM gym.user_settings WHERE owner_sub = (SELECT vitals.gym_user_id());

CREATE OR REPLACE VIEW vitals.gym WITH (security_barrier) AS
  SELECT id, owner_sub, name, aliases, notes, equipment, created_at
  FROM gym.gym WHERE owner_sub = (SELECT vitals.gym_user_id());

CREATE OR REPLACE VIEW vitals.exercise WITH (security_barrier) AS
  SELECT id, owner_sub, name, aliases, kind, muscle_groups, secondary_muscle_groups, equipment, description
  FROM gym.exercise WHERE owner_sub = (SELECT vitals.gym_user_id());

CREATE OR REPLACE VIEW vitals.workout WITH (security_barrier) AS
  SELECT id, owner_sub, gym_id, started_at, finished_at, finish_reason, time_estimated, notes, created_at
  FROM gym.workout WHERE owner_sub = (SELECT vitals.gym_user_id());

CREATE OR REPLACE VIEW vitals.workout_set WITH (security_barrier) AS
  SELECT s.id, s.workout_id, s.exercise_id, s.performed_at, s.time_estimated, s.set_type, s.reps, s.weight_kg,
         s.unilateral, s.load_basis, s.machine, s.variant, s.rpe, s.rest_s, s.duration_s, s.distance_m, s.intensity,
         s.avg_hr, s.max_hr, s.calories, s.avg_incline_pct, s.max_incline_pct, s.avg_speed_kmh, s.max_speed_kmh,
         s.notes, s.logged_at, s.updated_at
  FROM gym.workout_set s JOIN gym.workout w ON w.id = s.workout_id
  WHERE w.owner_sub = (SELECT vitals.gym_user_id());

CREATE OR REPLACE VIEW vitals.body_note WITH (security_barrier) AS
  SELECT id, owner_sub, noted_at, location, severity, notes, workout_id
  FROM gym.body_note WHERE owner_sub = (SELECT vitals.gym_user_id());

-- sql_query reads the gym logbook like the vitals data: through signed views in schema vitals, never the base tables
-- (see SPEC.md, Isolation). The tsvector search columns are left out.
CREATE VIEW vitals.user_settings WITH (security_barrier) AS
  SELECT owner_sub, timezone, goal, goals_notes, priority_muscle_groups, limitations
  FROM gym.user_settings WHERE owner_sub = (SELECT vitals.current_user_id());

CREATE VIEW vitals.gym WITH (security_barrier) AS
  SELECT id, owner_sub, name, aliases, notes, equipment, created_at
  FROM gym.gym WHERE owner_sub = (SELECT vitals.current_user_id());

CREATE VIEW vitals.exercise WITH (security_barrier) AS
  SELECT id, owner_sub, name, aliases, kind, muscle_groups, secondary_muscle_groups, equipment, description
  FROM gym.exercise WHERE owner_sub = (SELECT vitals.current_user_id());

CREATE VIEW vitals.workout WITH (security_barrier) AS
  SELECT id, owner_sub, gym_id, started_at, finished_at, finish_reason, time_estimated, notes, created_at
  FROM gym.workout WHERE owner_sub = (SELECT vitals.current_user_id());

-- workout_set has no owner_sub of its own; it belongs to its workout's owner.
CREATE VIEW vitals.workout_set WITH (security_barrier) AS
  SELECT s.id, s.workout_id, s.exercise_id, s.performed_at, s.time_estimated, s.set_type, s.reps, s.weight_kg,
         s.unilateral, s.load_basis, s.machine, s.variant, s.rpe, s.rest_s, s.duration_s, s.distance_m, s.intensity,
         s.avg_hr, s.max_hr, s.calories, s.avg_incline_pct, s.max_incline_pct, s.avg_speed_kmh, s.max_speed_kmh,
         s.notes, s.logged_at, s.updated_at
  FROM gym.workout_set s JOIN gym.workout w ON w.id = s.workout_id
  WHERE w.owner_sub = (SELECT vitals.current_user_id());

CREATE VIEW vitals.body_note WITH (security_barrier) AS
  SELECT id, owner_sub, noted_at, location, severity, notes, workout_id
  FROM gym.body_note WHERE owner_sub = (SELECT vitals.current_user_id());

GRANT SELECT ON vitals.user_settings, vitals.gym, vitals.exercise, vitals.workout, vitals.workout_set, vitals.body_note
  TO vitals_reader;

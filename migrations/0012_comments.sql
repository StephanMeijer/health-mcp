-- Comment threads on a workout, an exercise or a set. A thread belongs to the owner of the logbook it is about;
-- its comments are written by the owner or by users the owner shares gym data with (see SPEC.md, Comments).

CREATE TABLE gym.comment_thread (
  id              bigint GENERATED ALWAYS AS IDENTITY PRIMARY KEY,
  owner_sub       text NOT NULL,
  target          text NOT NULL CHECK (target IN ('workout', 'exercise', 'set')),
  -- Exactly the column of `target` is set, so a thread is bound to that row and goes when it goes.
  workout_id      bigint REFERENCES gym.workout (id) ON DELETE CASCADE,
  exercise_id     bigint REFERENCES gym.exercise (id) ON DELETE CASCADE,
  set_id          bigint REFERENCES gym.workout_set (id) ON DELETE CASCADE,
  started_by_sub  text NOT NULL,
  resolved_at     timestamptz,
  resolved_by_sub text,
  created_at      timestamptz NOT NULL DEFAULT now(),
  CONSTRAINT comment_thread_one_target CHECK (
    (target = 'workout') = (workout_id IS NOT NULL)
    AND (target = 'exercise') = (exercise_id IS NOT NULL)
    AND (target = 'set') = (set_id IS NOT NULL)),
  CHECK ((resolved_at IS NULL) = (resolved_by_sub IS NULL))
);
CREATE INDEX ON gym.comment_thread (workout_id) WHERE workout_id IS NOT NULL;
CREATE INDEX ON gym.comment_thread (exercise_id) WHERE exercise_id IS NOT NULL;
CREATE INDEX ON gym.comment_thread (set_id) WHERE set_id IS NOT NULL;
CREATE INDEX ON gym.comment_thread (owner_sub, created_at);

-- A deleted comment keeps its place in the thread with an empty body.
CREATE TABLE gym.comment (
  id           bigint GENERATED ALWAYS AS IDENTITY PRIMARY KEY,
  thread_id    bigint NOT NULL REFERENCES gym.comment_thread (id) ON DELETE CASCADE,
  author_sub   text NOT NULL,
  author_email text,
  body         text NOT NULL,
  created_at   timestamptz NOT NULL DEFAULT now(),
  edited_at    timestamptz,
  deleted_at   timestamptz,
  CONSTRAINT comment_body_length CHECK (
    CASE WHEN deleted_at IS NULL THEN char_length(btrim(body)) BETWEEN 1 AND 10000 ELSE body = '' END)
);
CREATE INDEX ON gym.comment (thread_id, created_at);

GRANT SELECT, INSERT, UPDATE, DELETE ON gym.comment_thread, gym.comment TO vitals_app;
GRANT USAGE ON ALL SEQUENCES IN SCHEMA gym TO vitals_app;

-- sql_query: the same gym-domain filter as the other gym views (migration 0011).
CREATE VIEW vitals.comment_thread WITH (security_barrier) AS
  SELECT id, owner_sub, target, workout_id, exercise_id, set_id, started_by_sub, resolved_at, resolved_by_sub, created_at
  FROM gym.comment_thread WHERE owner_sub = (SELECT vitals.gym_user_id());

CREATE VIEW vitals.comment WITH (security_barrier) AS
  SELECT c.id, c.thread_id, c.author_sub, c.author_email, c.body, c.created_at, c.edited_at, c.deleted_at
  FROM gym.comment c JOIN gym.comment_thread t ON t.id = c.thread_id
  WHERE t.owner_sub = (SELECT vitals.gym_user_id());

GRANT SELECT ON vitals.comment_thread, vitals.comment TO vitals_reader;

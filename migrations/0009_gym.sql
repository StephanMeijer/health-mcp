-- The gym logbook (formerly gym-mcp), in its own schema. This is gym-mcp's schema after its migration 0019, generated
-- with pg_dump, minus its measurement table (vitals_data.measurement covers that). Plain tables, not hypertables:
-- they're small. Every row is scoped by owner_sub, the token's `sub` (the same value as user_id in vitals_data).
CREATE SCHEMA gym;

CREATE TABLE gym.body_note (
    id bigint NOT NULL,
    owner_sub text NOT NULL,
    noted_at timestamp with time zone NOT NULL,
    location text NOT NULL,
    severity smallint,
    notes text DEFAULT ''::text NOT NULL,
    workout_id bigint,
    CONSTRAINT body_note_location_check CHECK ((location <> ''::text)),
    CONSTRAINT body_note_severity_check CHECK (((severity >= 1) AND (severity <= 10)))
);

ALTER TABLE gym.body_note ALTER COLUMN id ADD GENERATED ALWAYS AS IDENTITY (
    SEQUENCE NAME gym.body_note_id_seq
    START WITH 1
    INCREMENT BY 1
    NO MINVALUE
    NO MAXVALUE
    CACHE 1
);

CREATE TABLE gym.exercise (
    id bigint NOT NULL,
    owner_sub text NOT NULL,
    name text NOT NULL,
    aliases text[] DEFAULT '{}'::text[] NOT NULL,
    description text DEFAULT ''::text NOT NULL,
    muscle_groups text[] DEFAULT '{}'::text[] NOT NULL,
    description_tsv tsvector GENERATED ALWAYS AS (to_tsvector('simple'::regconfig, description)) STORED,
    equipment text DEFAULT ''::text NOT NULL,
    equipment_tsv tsvector GENERATED ALWAYS AS (to_tsvector('simple'::regconfig, equipment)) STORED,
    kind text DEFAULT 'strength'::text NOT NULL,
    secondary_muscle_groups text[] DEFAULT '{}'::text[] NOT NULL,
    CONSTRAINT exercise_kind_check CHECK ((kind = ANY (ARRAY['strength'::text, 'cardio'::text]))),
    CONSTRAINT exercise_muscle_groups_check CHECK (((muscle_groups <@ ARRAY['chest'::text, 'lats'::text, 'upper_back'::text, 'lower_back'::text, 'front_delts'::text, 'side_delts'::text, 'rear_delts'::text, 'biceps'::text, 'triceps'::text, 'forearms'::text, 'core'::text, 'glutes'::text, 'quads'::text, 'hamstrings'::text, 'adductors'::text, 'abductors'::text, 'calves'::text, 'neck'::text]) AND (secondary_muscle_groups <@ ARRAY['chest'::text, 'lats'::text, 'upper_back'::text, 'lower_back'::text, 'front_delts'::text, 'side_delts'::text, 'rear_delts'::text, 'biceps'::text, 'triceps'::text, 'forearms'::text, 'core'::text, 'glutes'::text, 'quads'::text, 'hamstrings'::text, 'adductors'::text, 'abductors'::text, 'calves'::text, 'neck'::text]) AND (NOT (muscle_groups && secondary_muscle_groups)))),
    CONSTRAINT exercise_name_check CHECK ((btrim(name) <> ''::text))
);

ALTER TABLE gym.exercise ALTER COLUMN id ADD GENERATED ALWAYS AS IDENTITY (
    SEQUENCE NAME gym.exercise_id_seq
    START WITH 1
    INCREMENT BY 1
    NO MINVALUE
    NO MAXVALUE
    CACHE 1
);

CREATE TABLE gym.gym (
    id bigint NOT NULL,
    owner_sub text NOT NULL,
    name text NOT NULL,
    aliases text[] DEFAULT '{}'::text[] NOT NULL,
    notes text DEFAULT ''::text NOT NULL,
    created_at timestamp with time zone DEFAULT now() NOT NULL,
    notes_tsv tsvector GENERATED ALWAYS AS (to_tsvector('simple'::regconfig, notes)) STORED,
    equipment text DEFAULT ''::text NOT NULL,
    equipment_tsv tsvector GENERATED ALWAYS AS (to_tsvector('simple'::regconfig, equipment)) STORED,
    CONSTRAINT gym_name_check CHECK ((btrim(name) <> ''::text))
);

ALTER TABLE gym.gym ALTER COLUMN id ADD GENERATED ALWAYS AS IDENTITY (
    SEQUENCE NAME gym.gym_id_seq
    START WITH 1
    INCREMENT BY 1
    NO MINVALUE
    NO MAXVALUE
    CACHE 1
);

CREATE TABLE gym.user_settings (
    owner_sub text NOT NULL,
    timezone text NOT NULL,
    goal text,
    goals_notes text,
    priority_muscle_groups text[],
    limitations text,
    CONSTRAINT user_settings_goal_check CHECK ((goal = ANY (ARRAY['strength'::text, 'hypertrophy'::text, 'endurance'::text, 'general'::text])))
);

CREATE TABLE gym.workout (
    id bigint NOT NULL,
    owner_sub text NOT NULL,
    gym_id bigint NOT NULL,
    started_at timestamp with time zone NOT NULL,
    finished_at timestamp with time zone,
    finish_reason text,
    notes text DEFAULT ''::text NOT NULL,
    created_at timestamp with time zone DEFAULT now() NOT NULL,
    notes_tsv tsvector GENERATED ALWAYS AS (to_tsvector('simple'::regconfig, notes)) STORED,
    time_estimated boolean DEFAULT false NOT NULL,
    CONSTRAINT workout_check CHECK (((finished_at IS NULL) = (finish_reason IS NULL))),
    CONSTRAINT workout_check1 CHECK (((finished_at IS NULL) OR (finished_at >= started_at))),
    CONSTRAINT workout_finish_reason_check CHECK ((finish_reason = ANY (ARRAY['manual'::text, 'stale'::text, 'superseded'::text])))
);

ALTER TABLE gym.workout ALTER COLUMN id ADD GENERATED ALWAYS AS IDENTITY (
    SEQUENCE NAME gym.workout_id_seq
    START WITH 1
    INCREMENT BY 1
    NO MINVALUE
    NO MAXVALUE
    CACHE 1
);

CREATE TABLE gym.workout_set (
    id bigint NOT NULL,
    workout_id bigint NOT NULL,
    exercise_id bigint NOT NULL,
    performed_at timestamp with time zone NOT NULL,
    reps integer,
    weight_kg double precision,
    duration_s integer,
    notes text DEFAULT ''::text NOT NULL,
    logged_at timestamp with time zone DEFAULT now() NOT NULL,
    updated_at timestamp with time zone DEFAULT now() NOT NULL,
    notes_tsv tsvector GENERATED ALWAYS AS (to_tsvector('simple'::regconfig, notes)) STORED,
    rpe double precision,
    rest_s integer,
    distance_m double precision,
    intensity text DEFAULT ''::text NOT NULL,
    machine text,
    set_type text DEFAULT 'working'::text NOT NULL,
    unilateral boolean DEFAULT false NOT NULL,
    load_basis text,
    variant text,
    avg_hr integer,
    max_hr integer,
    calories integer,
    avg_incline_pct double precision,
    max_incline_pct double precision,
    avg_speed_kmh double precision,
    max_speed_kmh double precision,
    time_estimated boolean DEFAULT false NOT NULL,
    CONSTRAINT workout_set_avg_hr_check CHECK (((avg_hr >= 30) AND (avg_hr <= 250))),
    CONSTRAINT workout_set_avg_incline_pct_check CHECK (((avg_incline_pct >= ('-10'::integer)::double precision) AND (avg_incline_pct <= (40)::double precision))),
    CONSTRAINT workout_set_avg_speed_kmh_check CHECK (((avg_speed_kmh >= (0)::double precision) AND (avg_speed_kmh <= (60)::double precision))),
    CONSTRAINT workout_set_calories_check CHECK (((calories >= 0) AND (calories <= 10000))),
    CONSTRAINT workout_set_check CHECK (((load_basis IS NULL) OR unilateral)),
    CONSTRAINT workout_set_distance_m_check CHECK ((distance_m >= (0)::double precision)),
    CONSTRAINT workout_set_duration_s_check CHECK ((duration_s >= 0)),
    CONSTRAINT workout_set_hr_order CHECK ((max_hr >= avg_hr)),
    CONSTRAINT workout_set_incline_order CHECK ((max_incline_pct >= avg_incline_pct)),
    CONSTRAINT workout_set_load_basis_check CHECK ((load_basis = ANY (ARRAY['per_implement'::text, 'per_side'::text, 'total'::text]))),
    CONSTRAINT workout_set_machine_check CHECK ((btrim(machine) <> ''::text)),
    CONSTRAINT workout_set_max_hr_check CHECK (((max_hr >= 30) AND (max_hr <= 250))),
    CONSTRAINT workout_set_max_incline_pct_check CHECK (((max_incline_pct >= ('-10'::integer)::double precision) AND (max_incline_pct <= (40)::double precision))),
    CONSTRAINT workout_set_max_speed_kmh_check CHECK (((max_speed_kmh >= (0)::double precision) AND (max_speed_kmh <= (60)::double precision))),
    CONSTRAINT workout_set_reps_check CHECK ((reps >= 0)),
    CONSTRAINT workout_set_rest_s_check CHECK ((rest_s >= 0)),
    CONSTRAINT workout_set_rpe_check CHECK ((((rpe >= (1)::double precision) AND (rpe <= (10)::double precision)) AND ((rpe * (2)::double precision) = round((rpe * (2)::double precision))))),
    CONSTRAINT workout_set_set_type_check CHECK ((set_type = ANY (ARRAY['warmup'::text, 'working'::text, 'partial'::text, 'interrupted'::text]))),
    CONSTRAINT workout_set_speed_order CHECK ((max_speed_kmh >= avg_speed_kmh)),
    CONSTRAINT workout_set_variant_check CHECK ((btrim(variant) <> ''::text)),
    CONSTRAINT workout_set_weight_kg_check CHECK ((weight_kg >= (0)::double precision))
);

ALTER TABLE gym.workout_set ALTER COLUMN id ADD GENERATED ALWAYS AS IDENTITY (
    SEQUENCE NAME gym.workout_set_id_seq
    START WITH 1
    INCREMENT BY 1
    NO MINVALUE
    NO MAXVALUE
    CACHE 1
);

ALTER TABLE ONLY gym.body_note
    ADD CONSTRAINT body_note_pkey PRIMARY KEY (id);

ALTER TABLE ONLY gym.exercise
    ADD CONSTRAINT exercise_pkey PRIMARY KEY (id);

ALTER TABLE ONLY gym.gym
    ADD CONSTRAINT gym_pkey PRIMARY KEY (id);

ALTER TABLE ONLY gym.user_settings
    ADD CONSTRAINT user_settings_pkey PRIMARY KEY (owner_sub);

ALTER TABLE ONLY gym.workout
    ADD CONSTRAINT workout_pkey PRIMARY KEY (id);

ALTER TABLE ONLY gym.workout_set
    ADD CONSTRAINT workout_set_pkey PRIMARY KEY (id);

CREATE INDEX body_note_owner_at ON gym.body_note USING btree (owner_sub, noted_at);

CREATE INDEX exercise_description_tsv ON gym.exercise USING gin (description_tsv);

CREATE INDEX exercise_equipment_tsv ON gym.exercise USING gin (equipment_tsv);

CREATE UNIQUE INDEX exercise_owner_name ON gym.exercise USING btree (owner_sub, lower(name));

CREATE INDEX gym_equipment_tsv ON gym.gym USING gin (equipment_tsv);

CREATE INDEX gym_notes_tsv ON gym.gym USING gin (notes_tsv);

CREATE UNIQUE INDEX gym_owner_name ON gym.gym USING btree (owner_sub, lower(name));

CREATE INDEX workout_notes_tsv ON gym.workout USING gin (notes_tsv);

CREATE UNIQUE INDEX workout_one_open_per_owner ON gym.workout USING btree (owner_sub) WHERE (finished_at IS NULL);

CREATE INDEX workout_owner_started ON gym.workout USING btree (owner_sub, started_at);

CREATE INDEX workout_set_exercise ON gym.workout_set USING btree (exercise_id, performed_at);

CREATE INDEX workout_set_machine ON gym.workout_set USING btree (exercise_id, lower(machine), performed_at);

CREATE INDEX workout_set_notes_tsv ON gym.workout_set USING gin (notes_tsv);

CREATE INDEX workout_set_order ON gym.workout_set USING btree (workout_id, exercise_id, performed_at);

ALTER TABLE ONLY gym.body_note
    ADD CONSTRAINT body_note_workout_id_fkey FOREIGN KEY (workout_id) REFERENCES gym.workout(id) ON DELETE SET NULL;

ALTER TABLE ONLY gym.workout
    ADD CONSTRAINT workout_gym_id_fkey FOREIGN KEY (gym_id) REFERENCES gym.gym(id);

ALTER TABLE ONLY gym.workout_set
    ADD CONSTRAINT workout_set_exercise_id_fkey FOREIGN KEY (exercise_id) REFERENCES gym.exercise(id);

ALTER TABLE ONLY gym.workout_set
    ADD CONSTRAINT workout_set_workout_id_fkey FOREIGN KEY (workout_id) REFERENCES gym.workout(id) ON DELETE CASCADE;


-- Typed tools run as vitals_app and filter on owner_sub in every query, like the vitals tools. The reader gets
-- views only (0010).
GRANT USAGE ON SCHEMA gym TO vitals_app;
GRANT SELECT, INSERT, UPDATE, DELETE ON ALL TABLES IN SCHEMA gym TO vitals_app;
GRANT USAGE ON ALL SEQUENCES IN SCHEMA gym TO vitals_app;

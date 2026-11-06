-- One narrow table for spot measurements (weight, waist, blood pressure, glucose, …): one row per value, so a new
-- type is an INSERT into measurement_type instead of a column, and deleting one value leaves the others.
-- body is copied over; blood_pressure is dropped without copying (it held no data worth keeping).
SET LOCAL search_path = vitals_data, public;

CREATE TABLE measurement_type (
  name text PRIMARY KEY,
  unit text NOT NULL,
  min  real NOT NULL,
  max  real NOT NULL,
  CHECK (min < max),
  UNIQUE (name, unit)
);

INSERT INTO measurement_type (name, unit, min, max) VALUES
  ('weight',       'kg',     20,  400),
  ('body_fat',     '%',      1,   80),
  ('height',       'cm',     50,  250),
  ('waist',        'cm',     30,  250),
  ('hip',          'cm',     30,  250),
  ('neck',         'cm',     15,  80),
  ('muscle_mass',  'kg',     5,   200),
  ('bone_mass',    'kg',     0.5, 10),
  ('body_water',   '%',      20,  80),
  ('visceral_fat', 'level',  1,   60),
  ('spo2',         '%',      50,  100),
  ('temperature',  '°C',     30,  45),
  ('glucose',      'mmol/L', 1,   35),
  ('systolic',     'mmHg',   50,  260),
  ('diastolic',    'mmHg',   30,  180),
  ('pulse',        'bpm',    20,  250);

-- The (type, unit) foreign key is what pins each type to its one unit.
CREATE TABLE measurement (
  time      timestamptz   NOT NULL,
  user_id   text          NOT NULL,
  source    vitals.source NOT NULL,
  device_id integer       REFERENCES devices (id),
  type      text          NOT NULL,
  value     real          NOT NULL,
  unit      text          NOT NULL,
  note      text,
  FOREIGN KEY (type, unit) REFERENCES measurement_type (name, unit),
  UNIQUE NULLS NOT DISTINCT (user_id, source, device_id, type, time)
);
SELECT create_hypertable('vitals_data.measurement', by_range('time'));

-- A CHECK can't read measurement_type, so the per-type range is a trigger; importers writing here are checked too.
CREATE FUNCTION measurement_in_range() RETURNS trigger
LANGUAGE plpgsql SET search_path = pg_catalog AS $$
DECLARE r record;
BEGIN
  SELECT min, max INTO r FROM vitals_data.measurement_type WHERE name = NEW.type;
  -- An unknown type is left to the foreign key.
  IF FOUND AND NOT NEW.value BETWEEN r.min AND r.max THEN
    RAISE EXCEPTION '% % is outside % to %', NEW.type, NEW.value, r.min, r.max USING ERRCODE = 'check_violation';
  END IF;
  RETURN NEW;
END
$$;
CREATE TRIGGER in_range BEFORE INSERT OR UPDATE ON measurement
  FOR EACH ROW EXECUTE FUNCTION measurement_in_range();

INSERT INTO measurement (time, user_id, source, type, value, unit, note)
SELECT b.time, b.user_id, b.source, m.type, m.value, m.unit, b.note
FROM body b CROSS JOIN LATERAL (VALUES ('weight', b.weight_kg, 'kg'), ('body_fat', b.fat_pct, '%'),
                                       ('spo2', b.spo2_pct, '%'), ('temperature', b.temp_c, '°C')) m(type, value, unit)
WHERE m.value IS NOT NULL;

DO $$
BEGIN
  IF (SELECT count(*) FROM measurement)
     <> (SELECT coalesce(sum(num_nonnulls(weight_kg, fat_pct, spo2_pct, temp_c)), 0) FROM body) THEN
    RAISE EXCEPTION 'body copy incomplete, keeping body';
  END IF;
END $$;

DROP VIEW vitals.body, vitals.blood_pressure;
DROP TABLE body, blood_pressure;

-- segmentby is the unique key minus time, as in 0007.
ALTER TABLE measurement SET (timescaledb.compress, timescaledb.compress_segmentby = 'user_id, source, device_id, type',
                             timescaledb.compress_orderby = 'time');
SELECT add_compression_policy('vitals_data.measurement', interval '7 days');

GRANT SELECT, INSERT, UPDATE, DELETE ON measurement TO vitals_app;
GRANT SELECT ON measurement_type TO vitals_app;
CREATE VIEW vitals.measurement WITH (security_barrier) AS
  SELECT * FROM measurement WHERE user_id = (SELECT vitals.current_user_id());
CREATE VIEW vitals.measurement_type AS SELECT * FROM measurement_type;
GRANT SELECT ON vitals.measurement, vitals.measurement_type TO vitals_reader;

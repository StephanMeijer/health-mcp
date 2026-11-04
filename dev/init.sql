-- Dev only: login roles the migration expects (it creates them NOLOGIN when missing).
CREATE ROLE vitals_app LOGIN PASSWORD 'app';
CREATE ROLE vitals_reader LOGIN PASSWORD 'reader';

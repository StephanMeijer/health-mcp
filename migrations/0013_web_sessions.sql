-- Browser sessions for the web app at /app (see SPEC.md, Web sessions). No tokens are kept: after the login the server
-- trusts its own session, and only the sha256 of the cookie's random value is stored.
CREATE SCHEMA web;

CREATE TABLE web.session (
  id_hash    bytea PRIMARY KEY,
  sub        text NOT NULL,
  -- Verified email from the login's access token; sharing needs it.
  email      text,
  created_at timestamptz NOT NULL DEFAULT now(),
  last_seen  timestamptz NOT NULL DEFAULT now(),
  -- Absolute end; an idle session also ends 12 h after last_seen.
  expires_at timestamptz NOT NULL
);
CREATE INDEX ON web.session (expires_at);

GRANT USAGE ON SCHEMA web TO vitals_app;
GRANT SELECT, INSERT, UPDATE, DELETE ON web.session TO vitals_app;

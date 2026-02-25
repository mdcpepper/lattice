SET
  LOCAL lock_timeout = '5s';

CREATE TABLE tenants (
  uuid UUID PRIMARY KEY,

  name TEXT NOT NULL,

  created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
  deleted_at TIMESTAMPTZ
);

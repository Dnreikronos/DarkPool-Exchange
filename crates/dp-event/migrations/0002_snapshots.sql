CREATE TABLE snapshots (
    seq        BIGINT       PRIMARY KEY,
    envelope   BYTEA        NOT NULL,
    created_at TIMESTAMPTZ  NOT NULL DEFAULT now()
);

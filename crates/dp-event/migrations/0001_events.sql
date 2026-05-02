CREATE TABLE events (
    seq        BIGSERIAL PRIMARY KEY,
    event_type SMALLINT     NOT NULL,
    timestamp  TIMESTAMPTZ  NOT NULL,
    data       BYTEA        NOT NULL
);

CREATE INDEX events_event_type_idx ON events (event_type);
CREATE INDEX events_timestamp_idx  ON events (timestamp);

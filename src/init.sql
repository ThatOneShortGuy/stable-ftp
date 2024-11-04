PRAGMA foreign_keys = ON;

CREATE TABLE user_tokens (
    id    INTEGER PRIMARY KEY,
    token TEXT NOT NULL UNIQUE,
    notes TEXT
);

CREATE TABLE files (
    id             INTEGER PRIMARY KEY,
    filename       TEXT NOT NULL UNIQUE,
    current_packet INTEGER NOT NULL,
    total_packets  INTEGER NOT NULL,
    packet_size    INTEGER NOT NULL,
    inserted_by_id INTEGER NOT NULL,

    FOREIGN KEY (inserted_by_id)
        REFERENCES user_tokens (id)
);

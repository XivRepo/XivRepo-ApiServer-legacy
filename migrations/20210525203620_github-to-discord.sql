ALTER TABLE users
    RENAME COLUMN github_id TO discord_id;

ALTER TABLE users
    ALTER COLUMN discord_id TYPE VARCHAR

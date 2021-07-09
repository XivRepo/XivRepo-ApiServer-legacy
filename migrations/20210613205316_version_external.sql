ALTER TABLE versions ADD COLUMN external_url VARCHAR DEFAULT NULL;
ALTER TABLE versions ADD COLUMN hosting_location VARCHAR(12) NOT NULL DEFAULT 'hosted';

CREATE TABLE user_follows (
    follower_id BIGINT NOT NULL,
    user_id BIGINT NOT NULL,
    created timestamptz NOT NULL DEFAULT CURRENT_TIMESTAMP
);

CREATE TABLE mod_images (
    mod_id BIGINT NOT NULL,
    image_url VARCHAR NOT NULL
);
ALTER TABLE dependencies
-- drop and add to clear the now obsolete version IDs?
-- previously referenced versions instead of mods
ALTER COLUMN id TYPE bigint,
DROP COLUMN dependent_id,
ADD COLUMN dependent_id bigint REFERENCES mods ON UPDATE CASCADE NOT NULL,
DROP COLUMN dependency_id,
ADD COLUMN dependency_id bigint REFERENCES mods ON UPDATE CASCADE NOT NULL,
ADD COLUMN version_id bigint REFERENCES versions,
ADD COLUMN min_version_num varchar(32)

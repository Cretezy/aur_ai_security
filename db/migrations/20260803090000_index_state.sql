CREATE TABLE index_state (
    id INTEGER PRIMARY KEY CHECK (id = 1),
    current_seen_at INTEGER NOT NULL
);

INSERT INTO index_state (id, current_seen_at)
SELECT 1, COALESCE(MAX(last_seen_at), 0)
FROM package_versions
WHERE is_current = 1;

DROP INDEX package_versions_current;
DROP INDEX package_versions_search;

ALTER TABLE package_versions DROP COLUMN is_current;

CREATE INDEX package_versions_current
    ON package_versions (last_seen_at, package_name);

CREATE INDEX package_versions_search
    ON package_versions (last_seen_at, popularity DESC, package_name);

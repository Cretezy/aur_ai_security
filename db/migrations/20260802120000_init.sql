CREATE TABLE package_versions (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    package_name TEXT NOT NULL,
    version TEXT NOT NULL,
    package_base TEXT NOT NULL,
    aur_package_id INTEGER NOT NULL,
    aur_package_base_id INTEGER NOT NULL,
    submitter TEXT,
    last_modified INTEGER NOT NULL,
    popularity REAL NOT NULL,
    url_path TEXT NOT NULL,
    is_current INTEGER NOT NULL DEFAULT 1,
    first_seen_at INTEGER NOT NULL,
    last_seen_at INTEGER NOT NULL,
    UNIQUE (package_name, version)
);

CREATE INDEX package_versions_current
    ON package_versions (is_current, package_name);

CREATE INDEX package_versions_search
    ON package_versions (is_current, popularity DESC, package_name);

CREATE INDEX package_versions_base
    ON package_versions (package_base);

CREATE TABLE checks (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    package_version_id INTEGER NOT NULL REFERENCES package_versions(id),
    provider TEXT NOT NULL,
    model TEXT NOT NULL,
    pkgbuild_commit TEXT NOT NULL,
    verdict TEXT NOT NULL CHECK (verdict IN ('safe', 'suspicious', 'dangerous')),
    explanation TEXT,
    checked_at INTEGER NOT NULL,
    commit_diff TEXT NOT NULL CHECK (length(commit_diff) > 0),
    pkgbuild TEXT NOT NULL CHECK (length(pkgbuild) > 0),
    UNIQUE (package_version_id, provider, model)
);

CREATE INDEX checks_package_version
    ON checks (package_version_id);

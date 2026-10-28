-- A ledger as schema version 4 wrote it, for the migration tests.
--
-- Made by the Mindflayer of schema 4 (dev at 065e96c): two projects, one a
-- `mind.toml` marks and one marked by its settings naming every agent, so
-- that its installations have a target for each folder; one source; skills
-- installed from the shelf, from the workspace's own and from a loaded
-- workspace, each with its hash, and one uninstalled; and a rule of the
-- workspace's own installed into the second, whose block made its
-- `AGENTS.md`, so that the target has a hash of its own and the file is
-- noted as made. Dumped with `sqlite3 mindflayer.db .dump`, with the
-- temporary paths it ran in replaced by the address below. A dump leaves out
-- the schema version, so the last line puts it back.
PRAGMA foreign_keys=OFF;
BEGIN TRANSACTION;
CREATE TABLE sources (
    id           INTEGER PRIMARY KEY,
    kind         TEXT    NOT NULL,
    url          TEXT    NOT NULL,
    reference    TEXT,
    subdirectory TEXT    NOT NULL,
    revision     TEXT,
    directory    TEXT    NOT NULL UNIQUE,
    first_seen   INTEGER NOT NULL,
    last_seen    INTEGER NOT NULL,
    UNIQUE (kind, url, reference, subdirectory)
);
INSERT INTO sources VALUES(1,'git','https://github.com/acme/skills',NULL,'skills','4fd9c0f2bf0534e75eacacf1d679f1a06e0a0a5f','github.com-acme-skills',1793167672,1793167672);
CREATE TABLE artifacts (
    id          INTEGER PRIMARY KEY,
    kind        TEXT    NOT NULL,
    name        TEXT    NOT NULL,
    source_id   INTEGER NOT NULL REFERENCES sources(id) ON DELETE CASCADE,
    path        TEXT    NOT NULL,
    summary     TEXT,
    revision    TEXT,
    gathered_at INTEGER NOT NULL,
    UNIQUE (kind, name, source_id)
);
INSERT INTO artifacts VALUES(1,'skill','deploy',1,'.mindflayer/skills/github.com-acme-skills/deploy','Ship the service to staging','4fd9c0f2bf0534e75eacacf1d679f1a06e0a0a5f',1793167672);
INSERT INTO artifacts VALUES(2,'skill','review',1,'.mindflayer/skills/github.com-acme-skills/review','Review a change before it merges','4fd9c0f2bf0534e75eacacf1d679f1a06e0a0a5f',1793167672);
CREATE TABLE actions (
    id      INTEGER PRIMARY KEY,
    at      INTEGER NOT NULL,
    action  TEXT    NOT NULL,
    target  TEXT,
    outcome TEXT    NOT NULL,
    detail  TEXT
);
INSERT INTO actions VALUES(1,1793167672,'gather','https://github.com/acme/skills','ok','2 added, 0 updated, 0 unchanged, 0 failed');
INSERT INTO actions VALUES(2,1793167679,'install','collapse','ok','deploy into collapse');
INSERT INTO actions VALUES(3,1793167679,'install','collapse','ok','notes into collapse');
INSERT INTO actions VALUES(4,1793167679,'install','collapse','ok','lint into collapse');
INSERT INTO actions VALUES(5,1793167679,'install','tanukeys','ok','review into tanukeys');
INSERT INTO actions VALUES(6,1793167679,'install','tanukeys','ok','notes into tanukeys');
INSERT INTO actions VALUES(7,1793167679,'install','tanukeys','ok','lint into tanukeys');
INSERT INTO actions VALUES(8,1793167679,'install','tanukeys','ok','style into tanukeys');
INSERT INTO actions VALUES(9,1793167679,'uninstall','tanukeys','ok','lint from tanukeys');
CREATE TABLE installations (
    id           INTEGER PRIMARY KEY,
    project      TEXT    NOT NULL,
    kind         TEXT    NOT NULL,
    name         TEXT    NOT NULL,
    source_id    INTEGER REFERENCES sources(id) ON DELETE SET NULL,
    installed_at INTEGER NOT NULL, origin TEXT, content_hash TEXT,
    UNIQUE (project, kind, name)
);
INSERT INTO installations VALUES(1,'collapse','skill','deploy',1,1793167679,'https://github.com/acme/skills','b6c1c1e92a73d98cf9df1c8495d2d24bf7f8cb56adf3476c7722d92d0f0004fb');
INSERT INTO installations VALUES(2,'collapse','skill','notes',NULL,1793167679,'workspace','2467e3671be22f82a31ddc3dee38619028e88cdf0ac7ae93ba6a2cc8d909e4f8');
INSERT INTO installations VALUES(3,'collapse','skill','lint',NULL,1793167679,'load:team','a3ac2bbcf13d13e06e40434aba78380fbd8805eb70277448357893a747341e07');
INSERT INTO installations VALUES(4,'tanukeys','skill','review',1,1793167679,'https://github.com/acme/skills','0191c2a8094fd2d72cdbc601187471b95718fd11c49ec09f7898a68c097bbc93');
INSERT INTO installations VALUES(5,'tanukeys','skill','notes',NULL,1793167679,'workspace','2467e3671be22f82a31ddc3dee38619028e88cdf0ac7ae93ba6a2cc8d909e4f8');
INSERT INTO installations VALUES(7,'tanukeys','rule','style',NULL,1793167679,'workspace','f52505bd5f1d991840a92134794486acca467cd1538b7861f821a1f070b9b57d');
CREATE TABLE installation_targets (
    installation_id INTEGER NOT NULL REFERENCES installations(id) ON DELETE CASCADE,
    agent           TEXT,
    path            TEXT    NOT NULL, content_hash TEXT,
    UNIQUE (installation_id, path)
);
INSERT INTO installation_targets VALUES(1,NULL,'skills/deploy',NULL);
INSERT INTO installation_targets VALUES(2,NULL,'skills/notes',NULL);
INSERT INTO installation_targets VALUES(3,NULL,'skills/lint',NULL);
INSERT INTO installation_targets VALUES(4,NULL,'.agents/skills/review',NULL);
INSERT INTO installation_targets VALUES(4,'claude','.claude/skills/review',NULL);
INSERT INTO installation_targets VALUES(5,NULL,'.agents/skills/notes',NULL);
INSERT INTO installation_targets VALUES(5,'claude','.claude/skills/notes',NULL);
INSERT INTO installation_targets VALUES(7,NULL,'AGENTS.md','f52505bd5f1d991840a92134794486acca467cd1538b7861f821a1f070b9b57d');
CREATE TABLE created_files (
    project    TEXT    NOT NULL,
    path       TEXT    NOT NULL,
    created_at INTEGER NOT NULL,
    UNIQUE (project, path)
);
INSERT INTO created_files VALUES('tanukeys','AGENTS.md',1793167679);
CREATE INDEX artifacts_by_name ON artifacts (kind, name);
CREATE INDEX actions_by_time ON actions (at);
CREATE INDEX installations_by_project ON installations (project);
COMMIT;
PRAGMA user_version = 4;

-- A ledger as schema version 2 wrote it, for the migration tests.
--
-- Made by the Mindflayer of schema 2 (dev at fd48c17): two projects, a
-- source gathered at a branch, one without, one that failed; installs from
-- the shelf, from the workspace's own and from a loaded workspace, and one
-- uninstalled. Dumped with `sqlite3 mindflayer.db .dump`, with the temporary
-- paths it ran in replaced by the addresses below. A dump leaves out the
-- schema version, so the last line puts it back.
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
INSERT INTO sources VALUES(1,'git','https://github.com/acme/skills','next','skills','504175ab0f1cd39dc937aac04bd1872e191d0529','github.com-acme-skills',1793034315,1793034315);
INSERT INTO sources VALUES(2,'git','https://github.com/acme/tools',NULL,'skills','4075def524820f24724f9cf208462272441be947','github.com-acme-tools',1793034315,1793034315);
INSERT INTO sources VALUES(3,'git','https://github.com/acme/missing',NULL,'skills',NULL,'github.com-acme-missing',1793034315,1793034315);
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
INSERT INTO artifacts VALUES(1,'skill','deploy',1,'.mindflayer/skills/github.com-acme-skills/deploy','Ship the service to staging','504175ab0f1cd39dc937aac04bd1872e191d0529',1793034315);
INSERT INTO artifacts VALUES(2,'skill','review',1,'.mindflayer/skills/github.com-acme-skills/review','Review a change before it merges','504175ab0f1cd39dc937aac04bd1872e191d0529',1793034315);
INSERT INTO artifacts VALUES(3,'skill','commit-style',2,'.mindflayer/skills/github.com-acme-tools/commit-style','How this repo writes commits','4075def524820f24724f9cf208462272441be947',1793034315);
CREATE TABLE actions (
    id      INTEGER PRIMARY KEY,
    at      INTEGER NOT NULL,
    action  TEXT    NOT NULL,
    target  TEXT,
    outcome TEXT    NOT NULL,
    detail  TEXT
);
INSERT INTO actions VALUES(1,1793034315,'gather','https://github.com/acme/skills','ok','2 added, 0 updated, 0 unchanged, 0 failed');
INSERT INTO actions VALUES(2,1793034315,'gather','https://github.com/acme/tools','ok','1 added, 0 updated, 0 unchanged, 0 failed');
INSERT INTO actions VALUES(3,1793034315,'gather','https://github.com/acme/missing','failed','https://github.com/acme/missing: cannot be fetched: the remote said the repository does not exist');
INSERT INTO actions VALUES(4,1793034315,'install','collapse','ok','deploy into collapse');
INSERT INTO actions VALUES(5,1793034315,'install','collapse','ok','commit-style into collapse');
INSERT INTO actions VALUES(6,1793034315,'install','collapse','ok','notes into collapse');
INSERT INTO actions VALUES(7,1793034315,'install','collapse','ok','lint into collapse');
INSERT INTO actions VALUES(8,1793034315,'install','tanukeys','ok','review into tanukeys');
INSERT INTO actions VALUES(9,1793034315,'install','tanukeys','ok','lint into tanukeys');
INSERT INTO actions VALUES(10,1793034315,'uninstall','tanukeys','ok','lint from tanukeys');
INSERT INTO actions VALUES(11,1793034315,'install','tanukeys','ok','notes into tanukeys');
CREATE TABLE installations (
    id           INTEGER PRIMARY KEY,
    project      TEXT    NOT NULL,
    kind         TEXT    NOT NULL,
    name         TEXT    NOT NULL,
    source_id    INTEGER REFERENCES sources(id) ON DELETE SET NULL,
    path         TEXT    NOT NULL,
    installed_at INTEGER NOT NULL,
    UNIQUE (project, kind, name)
);
INSERT INTO installations VALUES(1,'collapse','skill','deploy',1,'skills/deploy',1793034315);
INSERT INTO installations VALUES(2,'collapse','skill','commit-style',2,'skills/commit-style',1793034315);
INSERT INTO installations VALUES(3,'collapse','skill','notes',NULL,'skills/notes',1793034315);
INSERT INTO installations VALUES(4,'collapse','skill','lint',NULL,'skills/lint',1793034315);
INSERT INTO installations VALUES(5,'tanukeys','skill','review',1,'skills/review',1793034315);
INSERT INTO installations VALUES(6,'tanukeys','skill','notes',NULL,'skills/notes',1793034315);
CREATE INDEX artifacts_by_name ON artifacts (kind, name);
CREATE INDEX actions_by_time ON actions (at);
CREATE INDEX installations_by_project ON installations (project);
COMMIT;
PRAGMA user_version = 2;

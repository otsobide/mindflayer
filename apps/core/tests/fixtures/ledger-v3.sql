-- A ledger as schema version 3 wrote it, for the migration tests.
--
-- Made by the Mindflayer of schema 3 (dev at b1d8c17): two projects, one a
-- `mind.toml` marks and one marked by its settings naming every agent, so
-- that its installations have a target for each folder; two sources; installs
-- from the shelf, from the workspace's own and from a loaded workspace, each
-- with its hash, and one uninstalled. Dumped with `sqlite3 mindflayer.db
-- .dump`, with the temporary paths it ran in replaced by the addresses below.
-- A dump leaves out the schema version, so the last line puts it back.
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
INSERT INTO sources VALUES(1,'git','https://github.com/acme/skills',NULL,'skills','9321b749b3a573caa18dcc603a6d29099fca63dd','github.com-acme-skills',1793114959,1793114959);
INSERT INTO sources VALUES(2,'git','https://github.com/acme/tools',NULL,'skills','81bc61fce03402b0f8c0feae37ace8435af7e455','github.com-acme-tools',1793114959,1793114959);
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
INSERT INTO artifacts VALUES(1,'skill','deploy',1,'.mindflayer/skills/github.com-acme-skills/deploy','Ship the service to staging','9321b749b3a573caa18dcc603a6d29099fca63dd',1793114959);
INSERT INTO artifacts VALUES(2,'skill','review',1,'.mindflayer/skills/github.com-acme-skills/review','Review a change before it merges','9321b749b3a573caa18dcc603a6d29099fca63dd',1793114959);
INSERT INTO artifacts VALUES(3,'skill','commit-style',2,'.mindflayer/skills/github.com-acme-tools/commit-style','How this repo writes commits','81bc61fce03402b0f8c0feae37ace8435af7e455',1793114959);
CREATE TABLE actions (
    id      INTEGER PRIMARY KEY,
    at      INTEGER NOT NULL,
    action  TEXT    NOT NULL,
    target  TEXT,
    outcome TEXT    NOT NULL,
    detail  TEXT
);
INSERT INTO actions VALUES(1,1793114959,'gather','https://github.com/acme/skills','ok','2 added, 0 updated, 0 unchanged, 0 failed');
INSERT INTO actions VALUES(2,1793114959,'gather','https://github.com/acme/tools','ok','1 added, 0 updated, 0 unchanged, 0 failed');
INSERT INTO actions VALUES(3,1793114959,'install','collapse','ok','deploy into collapse');
INSERT INTO actions VALUES(4,1793114959,'install','collapse','ok','commit-style into collapse');
INSERT INTO actions VALUES(5,1793114959,'install','collapse','ok','notes into collapse');
INSERT INTO actions VALUES(6,1793114959,'install','collapse','ok','lint into collapse');
INSERT INTO actions VALUES(7,1793114959,'install','tanukeys','ok','review into tanukeys');
INSERT INTO actions VALUES(8,1793114959,'install','tanukeys','ok','lint into tanukeys');
INSERT INTO actions VALUES(9,1793114959,'install','tanukeys','ok','notes into tanukeys');
INSERT INTO actions VALUES(10,1793114959,'uninstall','tanukeys','ok','lint from tanukeys');
CREATE TABLE installations (
    id           INTEGER PRIMARY KEY,
    project      TEXT    NOT NULL,
    kind         TEXT    NOT NULL,
    name         TEXT    NOT NULL,
    source_id    INTEGER REFERENCES sources(id) ON DELETE SET NULL,
    installed_at INTEGER NOT NULL, origin TEXT, content_hash TEXT,
    UNIQUE (project, kind, name)
);
INSERT INTO installations VALUES(1,'collapse','skill','deploy',1,1793114959,'https://github.com/acme/skills','f739b82ff0a9246dc21294934531ea8fe92c35d1fe0d0eee146dc9de1cab9d68');
INSERT INTO installations VALUES(2,'collapse','skill','commit-style',2,1793114959,'https://github.com/acme/tools','30f6ed0bc232ca059a923f9f462891be44d55602d3a4ecc8fd849e5dbeb1c6a4');
INSERT INTO installations VALUES(3,'collapse','skill','notes',NULL,1793114959,'workspace','2467e3671be22f82a31ddc3dee38619028e88cdf0ac7ae93ba6a2cc8d909e4f8');
INSERT INTO installations VALUES(4,'collapse','skill','lint',NULL,1793114959,'load:../team','a3ac2bbcf13d13e06e40434aba78380fbd8805eb70277448357893a747341e07');
INSERT INTO installations VALUES(5,'tanukeys','skill','review',1,1793114959,'https://github.com/acme/skills','0751377f031de19e4e54bfde0b70864343730422988ecf4ee990df7311046d85');
INSERT INTO installations VALUES(7,'tanukeys','skill','notes',NULL,1793114959,'workspace','2467e3671be22f82a31ddc3dee38619028e88cdf0ac7ae93ba6a2cc8d909e4f8');
CREATE TABLE installation_targets (
    installation_id INTEGER NOT NULL REFERENCES installations(id) ON DELETE CASCADE,
    agent           TEXT,
    path            TEXT    NOT NULL,
    UNIQUE (installation_id, path)
);
INSERT INTO installation_targets VALUES(1,NULL,'skills/deploy');
INSERT INTO installation_targets VALUES(2,NULL,'skills/commit-style');
INSERT INTO installation_targets VALUES(3,NULL,'skills/notes');
INSERT INTO installation_targets VALUES(4,NULL,'skills/lint');
INSERT INTO installation_targets VALUES(5,NULL,'.agents/skills/review');
INSERT INTO installation_targets VALUES(5,'claude','.claude/skills/review');
INSERT INTO installation_targets VALUES(7,NULL,'.agents/skills/notes');
INSERT INTO installation_targets VALUES(7,'claude','.claude/skills/notes');
CREATE INDEX artifacts_by_name ON artifacts (kind, name);
CREATE INDEX actions_by_time ON actions (at);
CREATE INDEX installations_by_project ON installations (project);
COMMIT;
PRAGMA user_version = 3;

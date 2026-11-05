-- A ledger as schema version 5 wrote it, for the migration tests.
--
-- Made by the Mindweaver of schema 5 (dev at 80892bb): two projects marked
-- by their settings, one naming Claude alone and one every agent, each
-- known by its id; one source; skills installed from the shelf, from the
-- workspace's own and from a loaded workspace, each with its hash, and one
-- uninstalled; and a rule of the workspace's own installed into the second,
-- whose block made its `AGENTS.md`, so that the target has a hash of its
-- own and the file is noted as made. No MCP server, which no build of
-- schema 5 installs. Dumped with `sqlite3 mindweaver.db .dump`, with the
-- temporary paths it ran in replaced by the address below. A dump leaves
-- out the schema version, so the last line puts it back.
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
INSERT INTO sources VALUES(1,'git','https://github.com/acme/skills',NULL,'skills','66b3868a3e23a56e842c8fcaeb0bb73e5e678654','github.com-acme-skills',1793914367,1793914367);
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
INSERT INTO artifacts VALUES(1,'skill','deploy',1,'.mindweaver/skills/github.com-acme-skills/deploy','Ship the service to staging','66b3868a3e23a56e842c8fcaeb0bb73e5e678654',1793914367);
INSERT INTO artifacts VALUES(2,'skill','review',1,'.mindweaver/skills/github.com-acme-skills/review','Review a change before it merges','66b3868a3e23a56e842c8fcaeb0bb73e5e678654',1793914367);
CREATE TABLE actions (
    id      INTEGER PRIMARY KEY,
    at      INTEGER NOT NULL,
    action  TEXT    NOT NULL,
    target  TEXT,
    outcome TEXT    NOT NULL,
    detail  TEXT
);
INSERT INTO actions VALUES(1,1793914367,'gather','https://github.com/acme/skills','ok','2 added, 0 updated, 0 unchanged, 0 failed');
INSERT INTO actions VALUES(2,1793914367,'install','collapse','ok','deploy into collapse');
INSERT INTO actions VALUES(3,1793914367,'install','collapse','ok','notes into collapse');
INSERT INTO actions VALUES(4,1793914367,'install','collapse','ok','lint into collapse');
INSERT INTO actions VALUES(5,1793914367,'install','tanukeys','ok','review into tanukeys');
INSERT INTO actions VALUES(6,1793914367,'install','tanukeys','ok','notes into tanukeys');
INSERT INTO actions VALUES(7,1793914367,'install','tanukeys','ok','lint into tanukeys');
INSERT INTO actions VALUES(8,1793914367,'install','tanukeys','ok','style into tanukeys');
INSERT INTO actions VALUES(9,1793914367,'uninstall','tanukeys','ok','lint from tanukeys');
CREATE TABLE installations (
    id           INTEGER PRIMARY KEY,
    project      TEXT    NOT NULL,
    kind         TEXT    NOT NULL,
    name         TEXT    NOT NULL,
    source_id    INTEGER REFERENCES sources(id) ON DELETE SET NULL,
    installed_at INTEGER NOT NULL, origin TEXT, content_hash TEXT, project_id TEXT REFERENCES projects(id) ON UPDATE CASCADE ON DELETE SET NULL,
    UNIQUE (project, kind, name)
);
INSERT INTO installations VALUES(1,'collapse','skill','deploy',1,1793914367,'https://github.com/acme/skills','d51c6b91c4bf8a385ee4e6d631f3ada0a5b7aa3a38524dc9dddfd9a59a7e9d3d','50cebf8addc60d295b498e9212c9e848');
INSERT INTO installations VALUES(2,'collapse','skill','notes',NULL,1793914367,'workspace','2467e3671be22f82a31ddc3dee38619028e88cdf0ac7ae93ba6a2cc8d909e4f8','50cebf8addc60d295b498e9212c9e848');
INSERT INTO installations VALUES(3,'collapse','skill','lint',NULL,1793914367,'load:../team','a3ac2bbcf13d13e06e40434aba78380fbd8805eb70277448357893a747341e07','50cebf8addc60d295b498e9212c9e848');
INSERT INTO installations VALUES(4,'tanukeys','skill','review',1,1793914367,'https://github.com/acme/skills','c45e59a85efdc0a8de796b31af2b815163d492fa873b3aa2e6789c0a54de9e39','c00c03a1f3c6676732405a8bf90a2d48');
INSERT INTO installations VALUES(5,'tanukeys','skill','notes',NULL,1793914367,'workspace','2467e3671be22f82a31ddc3dee38619028e88cdf0ac7ae93ba6a2cc8d909e4f8','c00c03a1f3c6676732405a8bf90a2d48');
INSERT INTO installations VALUES(7,'tanukeys','rule','style',NULL,1793914367,'workspace','e090524948a8e8afea4b6c3eb511ec1416deeb8f17e06155f4946f87423b03cf','c00c03a1f3c6676732405a8bf90a2d48');
CREATE TABLE installation_targets (
    installation_id INTEGER NOT NULL REFERENCES installations(id) ON DELETE CASCADE,
    agent           TEXT,
    path            TEXT    NOT NULL, content_hash TEXT,
    UNIQUE (installation_id, path)
);
INSERT INTO installation_targets VALUES(1,'claude','.claude/skills/deploy',NULL);
INSERT INTO installation_targets VALUES(2,'claude','.claude/skills/notes',NULL);
INSERT INTO installation_targets VALUES(3,'claude','.claude/skills/lint',NULL);
INSERT INTO installation_targets VALUES(4,NULL,'.agents/skills/review',NULL);
INSERT INTO installation_targets VALUES(4,'claude','.claude/skills/review',NULL);
INSERT INTO installation_targets VALUES(5,NULL,'.agents/skills/notes',NULL);
INSERT INTO installation_targets VALUES(5,'claude','.claude/skills/notes',NULL);
INSERT INTO installation_targets VALUES(7,NULL,'AGENTS.md','e090524948a8e8afea4b6c3eb511ec1416deeb8f17e06155f4946f87423b03cf');
CREATE TABLE created_files (
    project    TEXT    NOT NULL,
    path       TEXT    NOT NULL,
    created_at INTEGER NOT NULL,
    UNIQUE (project, path)
);
INSERT INTO created_files VALUES('tanukeys','AGENTS.md',1793914367);
CREATE TABLE projects (
    id         TEXT    PRIMARY KEY,
    last_route TEXT    NOT NULL UNIQUE
);
INSERT INTO projects VALUES('50cebf8addc60d295b498e9212c9e848','collapse');
INSERT INTO projects VALUES('c00c03a1f3c6676732405a8bf90a2d48','tanukeys');
CREATE INDEX artifacts_by_name ON artifacts (kind, name);
CREATE INDEX actions_by_time ON actions (at);
CREATE INDEX installations_by_project ON installations (project);
CREATE INDEX installations_by_project_id ON installations (project_id);
COMMIT;
PRAGMA user_version = 5;

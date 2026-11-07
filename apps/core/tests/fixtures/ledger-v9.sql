-- A ledger as schema version 9 wrote it, for the migration tests.
--
-- Made by the Mindweaver of schema 9 (#108's branch at 7b336f1): two
-- projects marked by their settings, one naming Claude alone and one every
-- agent, each known by its id; two repositories gathered from their root,
-- one under the name it is given (`acme-skills`), its skills' licences read
-- from a manifest and a licence file, and one under the name `ops`, pinned
-- to its commit (`--pin`); a Claude Code marketplace, `acme-market`, and
-- the one plugin it lists, gathered into a source of its own under it from
-- the marketplace's own repository (`--plugin lint`); and a skill
-- installed from the first repository into each project, one from the
-- second and one from the plugin. No rule gathered, since no build before
-- schema 10 gathered one, no MCP server and nothing of the workspace's own,
-- which the step to schema 10 does not touch.
--
-- Each repository was a folder on the disk beside the workspace, gathered
-- by its relative path, so every URL, name and folder here is the one that
-- build gives such a path, and nothing was fetched over a network. Dumped
-- with `sqlite3 mindweaver.db .dump`, with the times set a minute apart, a
-- gather a minute, the marketplace and its plugin in one, and the installs
-- after them. A dump leaves out the schema version, so the last line puts
-- it back.
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
    last_seen    INTEGER NOT NULL, name TEXT NOT NULL DEFAULT '', canonical_url TEXT NOT NULL DEFAULT '', reference_key TEXT NOT NULL DEFAULT '', pin TEXT, parent_id INTEGER REFERENCES sources(id) ON DELETE CASCADE, entry TEXT,
    UNIQUE (kind, url, reference, subdirectory)
);
INSERT INTO sources VALUES(1,'git','../acme/skills',NULL,'','ef9b1494ccac876b3ac37c4dbe212ac81ad18a30','..-acme-skills',1794052800,1794052800,'acme-skills','../acme/skills','',NULL,NULL,NULL);
INSERT INTO sources VALUES(2,'git','../acme/ops',NULL,'','97ee7ac56c2a9692d6d1f02d73a14e18527bf2b4','..-acme-ops',1794052860,1794052860,'ops','../acme/ops','','97ee7ac56c2a9692d6d1f02d73a14e18527bf2b4',NULL,NULL);
INSERT INTO sources VALUES(3,'marketplace','../acme/market',NULL,'','a091bc195d85f1c6b715521a79c8bc3cf4416ef7','..-acme-market',1794052920,1794052920,'acme-market','../acme/market','',NULL,NULL,NULL);
INSERT INTO sources VALUES(4,'git','../acme/market',NULL,'plugins/lint','a091bc195d85f1c6b715521a79c8bc3cf4416ef7','..-acme-market-lint',1794052920,1794052920,'acme-market.lint','../acme/market','',NULL,3,'lint');
CREATE TABLE artifacts (
    id          INTEGER PRIMARY KEY,
    kind        TEXT    NOT NULL,
    name        TEXT    NOT NULL,
    source_id   INTEGER NOT NULL REFERENCES sources(id) ON DELETE CASCADE,
    path        TEXT    NOT NULL,
    summary     TEXT,
    revision    TEXT,
    gathered_at INTEGER NOT NULL, upstream_path TEXT NOT NULL DEFAULT '', content_hash TEXT, license TEXT,
    UNIQUE (kind, name, source_id)
);
INSERT INTO artifacts VALUES(1,'skill','deploy',1,'.mindweaver/skills/..-acme-skills/deploy','Ship the service to staging','ef9b1494ccac876b3ac37c4dbe212ac81ad18a30',1794052800,'skills/deploy','004a8c99cec9a72f41b6d1437f64e9f35f910519d82b71e2d5977731416b905a','MIT');
INSERT INTO artifacts VALUES(2,'skill','review',1,'.mindweaver/skills/..-acme-skills/review','Review a change before it merges','ef9b1494ccac876b3ac37c4dbe212ac81ad18a30',1794052800,'skills/review','14e06f6118082e5fbe66f25d96da9af5dc0cee124782355aa0e053b13b9ab361','Apache License');
INSERT INTO artifacts VALUES(3,'skill','audit',2,'.mindweaver/skills/..-acme-ops/audit','Audit what a change lets in','97ee7ac56c2a9692d6d1f02d73a14e18527bf2b4',1794052860,'plugins/ops/skills/audit','e41ae7d1c4545096b57ae2fc92bb77b45291fff62c2dcb8c01e5f68764558e8f',NULL);
INSERT INTO artifacts VALUES(4,'skill','lint',4,'.mindweaver/skills/..-acme-market-lint/lint','Lint a change before review','a091bc195d85f1c6b715521a79c8bc3cf4416ef7',1794052920,'plugins/lint/skills/lint','5f58f35158254330568407bad15fda80090fcf20b526a63d5d1f7111897d42d7',NULL);
CREATE TABLE actions (
    id      INTEGER PRIMARY KEY,
    at      INTEGER NOT NULL,
    action  TEXT    NOT NULL,
    target  TEXT,
    outcome TEXT    NOT NULL,
    detail  TEXT
);
INSERT INTO actions VALUES(1,1794052800,'gather','../acme/skills','ok','2 added, 0 updated, 0 unchanged, 0 failed');
INSERT INTO actions VALUES(2,1794052860,'gather','../acme/ops','ok','1 added, 0 updated, 0 unchanged, 0 failed');
INSERT INTO actions VALUES(3,1794052920,'gather','../acme/market','ok','a marketplace listing 1 plugin');
INSERT INTO actions VALUES(4,1794052920,'gather','../acme/market','ok','1 added, 0 updated, 0 unchanged, 0 failed');
INSERT INTO actions VALUES(5,1794052980,'install','collapse','ok','deploy into collapse');
INSERT INTO actions VALUES(6,1794052980,'install','tanukeys','ok','review into tanukeys');
INSERT INTO actions VALUES(7,1794052980,'install','tanukeys','ok','audit into tanukeys');
INSERT INTO actions VALUES(8,1794052980,'install','collapse','ok','lint into collapse');
CREATE TABLE installations (
    id           INTEGER PRIMARY KEY,
    project      TEXT    NOT NULL,
    kind         TEXT    NOT NULL,
    name         TEXT    NOT NULL,
    source_id    INTEGER REFERENCES sources(id) ON DELETE SET NULL,
    installed_at INTEGER NOT NULL, origin TEXT, content_hash TEXT, project_id TEXT REFERENCES projects(id) ON UPDATE CASCADE ON DELETE SET NULL,
    UNIQUE (project, kind, name)
);
INSERT INTO installations VALUES(1,'collapse','skill','deploy',1,1794052980,'../acme/skills','004a8c99cec9a72f41b6d1437f64e9f35f910519d82b71e2d5977731416b905a','a70100b5f798576fdf66f8bbbfaa4989');
INSERT INTO installations VALUES(2,'tanukeys','skill','review',1,1794052980,'../acme/skills','14e06f6118082e5fbe66f25d96da9af5dc0cee124782355aa0e053b13b9ab361','351309d5965f06473562e9a2ffdf7823');
INSERT INTO installations VALUES(3,'tanukeys','skill','audit',2,1794052980,'../acme/ops','e41ae7d1c4545096b57ae2fc92bb77b45291fff62c2dcb8c01e5f68764558e8f','351309d5965f06473562e9a2ffdf7823');
INSERT INTO installations VALUES(4,'collapse','skill','lint',4,1794052980,'../acme/market','5f58f35158254330568407bad15fda80090fcf20b526a63d5d1f7111897d42d7','a70100b5f798576fdf66f8bbbfaa4989');
CREATE TABLE installation_targets (
    installation_id INTEGER NOT NULL REFERENCES installations(id) ON DELETE CASCADE,
    agent           TEXT,
    path            TEXT    NOT NULL, content_hash TEXT,
    UNIQUE (installation_id, path)
);
INSERT INTO installation_targets VALUES(1,'claude','.claude/skills/deploy',NULL);
INSERT INTO installation_targets VALUES(2,NULL,'.agents/skills/review',NULL);
INSERT INTO installation_targets VALUES(2,'claude','.claude/skills/review',NULL);
INSERT INTO installation_targets VALUES(3,NULL,'.agents/skills/audit',NULL);
INSERT INTO installation_targets VALUES(3,'claude','.claude/skills/audit',NULL);
INSERT INTO installation_targets VALUES(4,'claude','.claude/skills/lint',NULL);
CREATE TABLE created_files (
    project    TEXT    NOT NULL,
    path       TEXT    NOT NULL,
    created_at INTEGER NOT NULL,
    UNIQUE (project, path)
);
CREATE TABLE projects (
    id         TEXT    PRIMARY KEY,
    last_route TEXT    NOT NULL UNIQUE
);
INSERT INTO projects VALUES('a70100b5f798576fdf66f8bbbfaa4989','collapse');
INSERT INTO projects VALUES('351309d5965f06473562e9a2ffdf7823','tanukeys');
CREATE TABLE installation_entries (
    installation_id INTEGER NOT NULL REFERENCES installations(id) ON DELETE CASCADE,
    agent           TEXT    NOT NULL,
    file            TEXT    NOT NULL,
    name            TEXT    NOT NULL,
    fingerprint     TEXT    NOT NULL,
    UNIQUE (installation_id, agent)
);
CREATE TABLE merged_sources (
    id        INTEGER PRIMARY KEY,
    directory TEXT    NOT NULL UNIQUE,
    url       TEXT    NOT NULL,
    reference TEXT,
    into_id   INTEGER NOT NULL REFERENCES sources(id) ON DELETE CASCADE
);
CREATE INDEX artifacts_by_name ON artifacts (kind, name);
CREATE INDEX actions_by_time ON actions (at);
CREATE INDEX installations_by_project ON installations (project);
CREATE INDEX installations_by_project_id ON installations (project_id);
CREATE UNIQUE INDEX sources_by_name ON sources (name);
CREATE UNIQUE INDEX sources_by_identity
    ON sources (kind, canonical_url, reference_key, subdirectory) WHERE parent_id IS NULL;
CREATE UNIQUE INDEX sources_by_entry ON sources (parent_id, entry) WHERE parent_id IS NOT NULL;
COMMIT;
PRAGMA user_version = 9;

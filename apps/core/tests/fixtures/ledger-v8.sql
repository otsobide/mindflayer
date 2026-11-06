-- A ledger as schema version 8 wrote it, for the migration tests.
--
-- Made by the Mindweaver of schema 8 (#101's branch at 27a07aa): two
-- projects marked by their settings, one naming Claude alone and one every
-- agent, each known by its id; two repositories gathered from their root,
-- one under the name it is given (`acme-skills`), its skills' licences read
-- from a manifest and a licence file, and one under the name `ops`, pinned
-- to its commit (`--pin`); and a skill installed from the first into each
-- project and one from the second. No marketplace, no MCP server and
-- nothing of the workspace's own, which the step to schema 9 does not
-- touch.
--
-- Each repository was a folder on the disk, reached by its GitHub address
-- through git config's `url.<base>.insteadOf`, so every URL, name and
-- folder here is the one that build gives the address, and nothing was
-- fetched over a network. Dumped with `sqlite3 mindweaver.db .dump`, with
-- the times set a minute apart, a gather a minute and the installs after
-- them. A dump leaves out the schema version, so the last line puts it
-- back.
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
    last_seen    INTEGER NOT NULL, name TEXT NOT NULL DEFAULT '', canonical_url TEXT NOT NULL DEFAULT '', reference_key TEXT NOT NULL DEFAULT '', pin TEXT, parent_id INTEGER REFERENCES sources(id) ON DELETE CASCADE,
    UNIQUE (kind, url, reference, subdirectory)
);
INSERT INTO sources VALUES(1,'git','https://github.com/acme/skills',NULL,'','04248be5bf20674301576493527bda269a3e5a1f','github.com-acme-skills',1794052800,1794052800,'acme-skills','https://github.com/acme/skills','',NULL,NULL);
INSERT INTO sources VALUES(2,'git','https://github.com/acme/ops',NULL,'','dbf1870c3af27a91d74684fe327cb50b7504b119','github.com-acme-ops',1794052860,1794052860,'ops','https://github.com/acme/ops','','dbf1870c3af27a91d74684fe327cb50b7504b119',NULL);
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
INSERT INTO artifacts VALUES(1,'skill','deploy',1,'.mindweaver/skills/github.com-acme-skills/deploy','Ship the service to staging','04248be5bf20674301576493527bda269a3e5a1f',1794052800,'skills/deploy','e21110f36feef50d0d14305f5fe86e3f17f0f63e2f4a277a3a93663b783995b1','MIT');
INSERT INTO artifacts VALUES(2,'skill','review',1,'.mindweaver/skills/github.com-acme-skills/review','Review a change before it merges','04248be5bf20674301576493527bda269a3e5a1f',1794052800,'skills/review','ebaf8ad66391391dc29014b60fb8511ca1ee8d1ee4098637f86cd8bd0625cb9e','Apache License');
INSERT INTO artifacts VALUES(3,'skill','audit',2,'.mindweaver/skills/github.com-acme-ops/audit','Audit what a change lets in','dbf1870c3af27a91d74684fe327cb50b7504b119',1794052860,'plugins/ops/skills/audit','1c6dbeb3f7de6a5966526b5896a5e0a3b4846a7f1b2708a011870e432250663d',NULL);
CREATE TABLE actions (
    id      INTEGER PRIMARY KEY,
    at      INTEGER NOT NULL,
    action  TEXT    NOT NULL,
    target  TEXT,
    outcome TEXT    NOT NULL,
    detail  TEXT
);
INSERT INTO actions VALUES(1,1794052800,'gather','https://github.com/acme/skills','ok','2 added, 0 updated, 0 unchanged, 0 failed');
INSERT INTO actions VALUES(2,1794052860,'gather','https://github.com/acme/ops','ok','1 added, 0 updated, 0 unchanged, 0 failed');
INSERT INTO actions VALUES(3,1794052920,'install','collapse','ok','deploy into collapse');
INSERT INTO actions VALUES(4,1794052920,'install','tanukeys','ok','review into tanukeys');
INSERT INTO actions VALUES(5,1794052920,'install','tanukeys','ok','audit into tanukeys');
CREATE TABLE installations (
    id           INTEGER PRIMARY KEY,
    project      TEXT    NOT NULL,
    kind         TEXT    NOT NULL,
    name         TEXT    NOT NULL,
    source_id    INTEGER REFERENCES sources(id) ON DELETE SET NULL,
    installed_at INTEGER NOT NULL, origin TEXT, content_hash TEXT, project_id TEXT REFERENCES projects(id) ON UPDATE CASCADE ON DELETE SET NULL,
    UNIQUE (project, kind, name)
);
INSERT INTO installations VALUES(1,'collapse','skill','deploy',1,1794052920,'https://github.com/acme/skills','e21110f36feef50d0d14305f5fe86e3f17f0f63e2f4a277a3a93663b783995b1','37de956e41b64a6465f136493acbc0a8');
INSERT INTO installations VALUES(2,'tanukeys','skill','review',1,1794052920,'https://github.com/acme/skills','ebaf8ad66391391dc29014b60fb8511ca1ee8d1ee4098637f86cd8bd0625cb9e','47878a67c0b3772a159aa177ed5a264f');
INSERT INTO installations VALUES(3,'tanukeys','skill','audit',2,1794052920,'https://github.com/acme/ops','1c6dbeb3f7de6a5966526b5896a5e0a3b4846a7f1b2708a011870e432250663d','47878a67c0b3772a159aa177ed5a264f');
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
INSERT INTO projects VALUES('37de956e41b64a6465f136493acbc0a8','collapse');
INSERT INTO projects VALUES('47878a67c0b3772a159aa177ed5a264f','tanukeys');
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
CREATE UNIQUE INDEX sources_by_identity
    ON sources (kind, canonical_url, reference_key, subdirectory);
CREATE UNIQUE INDEX sources_by_name ON sources (name);
COMMIT;
PRAGMA user_version = 8;

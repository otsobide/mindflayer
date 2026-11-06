-- A ledger as schema version 7 wrote it, for the migration tests.
--
-- Made by the Mindweaver of schema 7 (#98's branch at 4813d1d): two
-- projects marked by their settings, one naming Claude alone and one every
-- agent, each known by its id; three repositories gathered, one from its
-- `skills` folder, one from `plugins/ops/skills` under the name `ops`, and
-- one from its root (`--path .`); and a skill installed from the first into
-- each project and one from the second. No pin, no MCP server and nothing
-- of the workspace's own, which the step to schema 8 does not touch.
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
INSERT INTO sources VALUES(1,'git','https://github.com/acme/skills',NULL,'skills','689a61910ff7a6ed452f1cd3b914a8e934f3ade6','github.com-acme-skills',1793966400,1793966400,'acme-skills','https://github.com/acme/skills','',NULL,NULL);
INSERT INTO sources VALUES(2,'git','https://github.com/acme/ops',NULL,'plugins/ops/skills','a5d4e430c7a36a7335c3f6ebe4953062c9c509ad','github.com-acme-ops',1793966460,1793966460,'ops','https://github.com/acme/ops','',NULL,NULL);
INSERT INTO sources VALUES(3,'git','https://github.com/acme/lint',NULL,'.','398ceeadd65bbbf3457741fa0657156944542976','github.com-acme-lint',1793966520,1793966520,'acme-lint','https://github.com/acme/lint','',NULL,NULL);
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
INSERT INTO artifacts VALUES(1,'skill','deploy',1,'.mindweaver/skills/github.com-acme-skills/deploy','Ship the service to staging','689a61910ff7a6ed452f1cd3b914a8e934f3ade6',1793966400);
INSERT INTO artifacts VALUES(2,'skill','review',1,'.mindweaver/skills/github.com-acme-skills/review','Review a change before it merges','689a61910ff7a6ed452f1cd3b914a8e934f3ade6',1793966400);
INSERT INTO artifacts VALUES(3,'skill','audit',2,'.mindweaver/skills/github.com-acme-ops/audit','Audit what a change lets in','a5d4e430c7a36a7335c3f6ebe4953062c9c509ad',1793966460);
INSERT INTO artifacts VALUES(4,'skill','lint',3,'.mindweaver/skills/github.com-acme-lint/lint','Lint the code the way the team does','398ceeadd65bbbf3457741fa0657156944542976',1793966520);
CREATE TABLE actions (
    id      INTEGER PRIMARY KEY,
    at      INTEGER NOT NULL,
    action  TEXT    NOT NULL,
    target  TEXT,
    outcome TEXT    NOT NULL,
    detail  TEXT
);
INSERT INTO actions VALUES(1,1793966400,'gather','https://github.com/acme/skills','ok','2 added, 0 updated, 0 unchanged, 0 failed');
INSERT INTO actions VALUES(2,1793966460,'gather','https://github.com/acme/ops','ok','1 added, 0 updated, 0 unchanged, 0 failed');
INSERT INTO actions VALUES(3,1793966520,'gather','https://github.com/acme/lint','ok','1 added, 0 updated, 0 unchanged, 0 failed');
INSERT INTO actions VALUES(4,1793966580,'install','collapse','ok','deploy into collapse');
INSERT INTO actions VALUES(5,1793966580,'install','tanukeys','ok','review into tanukeys');
INSERT INTO actions VALUES(6,1793966580,'install','tanukeys','ok','audit into tanukeys');
CREATE TABLE installations (
    id           INTEGER PRIMARY KEY,
    project      TEXT    NOT NULL,
    kind         TEXT    NOT NULL,
    name         TEXT    NOT NULL,
    source_id    INTEGER REFERENCES sources(id) ON DELETE SET NULL,
    installed_at INTEGER NOT NULL, origin TEXT, content_hash TEXT, project_id TEXT REFERENCES projects(id) ON UPDATE CASCADE ON DELETE SET NULL,
    UNIQUE (project, kind, name)
);
INSERT INTO installations VALUES(1,'collapse','skill','deploy',1,1793966580,'https://github.com/acme/skills','d51c6b91c4bf8a385ee4e6d631f3ada0a5b7aa3a38524dc9dddfd9a59a7e9d3d','1f8e71f5d4b23a41fbcbe8a3b8b798e7');
INSERT INTO installations VALUES(2,'tanukeys','skill','review',1,1793966580,'https://github.com/acme/skills','c45e59a85efdc0a8de796b31af2b815163d492fa873b3aa2e6789c0a54de9e39','bd7d366fb3a5e24d42806938772c7f3c');
INSERT INTO installations VALUES(3,'tanukeys','skill','audit',2,1793966580,'https://github.com/acme/ops','1c6dbeb3f7de6a5966526b5896a5e0a3b4846a7f1b2708a011870e432250663d','bd7d366fb3a5e24d42806938772c7f3c');
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
INSERT INTO projects VALUES('1f8e71f5d4b23a41fbcbe8a3b8b798e7','collapse');
INSERT INTO projects VALUES('bd7d366fb3a5e24d42806938772c7f3c','tanukeys');
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
PRAGMA user_version = 7;

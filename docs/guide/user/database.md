---
title: Database tools and SQL console
description: Data sources for MySQL, MariaDB, PostgreSQL, SQLite, SQL Server, ClickHouse, Oracle, Redis and MongoDB with SSH tunnels and SSL, the Database tool window, query consoles with sessions and transactions, read-only and production data sources, the data grid with editing, and export.
order: 170
tags: [database, sql, mysql, postgresql, sqlite, sqlserver, clickhouse, oracle, redis, mongodb, ssh, ssl, console]
---

# Database tools and SQL console

The plugin **Database Tools and SQL** (`forge.database`) is Forge's DataGrip part. Drivers are
built into Forge — there are no JDBC drivers to download:

| Engine | Driver | Tree | Console |
| --- | --- | --- | --- |
| MySQL, MariaDB, PostgreSQL, SQLite | `sqlx` | schemas, tables, views, columns, keys, indexes, routines | SQL, transactions |
| SQL Server, Azure SQL | `tiberius` (pure-Rust TDS over rustls) | schemas, tables, views, columns, keys, indexes, procedures and functions | T-SQL (`GO` separates batches), transactions |
| ClickHouse | HTTP interface (port 8123, 8443 with TLS), JSON results | databases, tables, views, columns, primary key | SQL; every statement on its own |
| Oracle | SQL\*Plus or SQLcl **installed by you** (see [Oracle](#oracle)) | schemas, tables, views, columns, keys, indexes, procedures and packages | SQL and PL/SQL blocks ending with `/`; every statement on its own |
| Redis | `redis-rs` | databases → keys with type and TTL | Redis commands, one per line |
| MongoDB | official MongoDB driver | databases → collections → fields | shell calls: `db.users.find({...})`, `aggregate`, … |

ODBC is not supported: no permissive Rust crate makes generic ODBC drivers work without a
system driver manager, so Forge supports the engines above natively instead.

## Database tool window

The **Database** tool window (right stripe) shows your data sources as a tree:
data source → schema → tables (with comments) → columns, keys and indexes.

![Database tree](../../screenshots/database-tree.png)

Toolbar: **+** (add data source), **Refresh**, **Data Source Properties** (`Shift+Enter`),
**Jump to Query Console…** (`Ctrl+Shift+F10`), **Collapse All**, **Sessions** (see
[Sessions](#sessions)). Context menu of a node: **New Query Console**, **Edit Data** (tables),
**Go to DDL**, **Copy Name**, **Properties**, **Refresh**, **Disconnect**, **Delete**.

Data sources with a colour tint their rows (see [Colours](#read-only-and-production-data-sources));
`read-only` and the SSH host appear next to the name.

![SQL Server tree](../../screenshots/database-sqlserver-tree.png)

## Data sources

**New Data Source…** (the **+** button or Find Action) opens **Data Sources and Drivers**:

![Data source dialog](../../screenshots/database-data-source-dialog.png)

| Field | Notes |
| --- | --- |
| Name, Comment | |
| Driver | MySQL, MariaDB, PostgreSQL, SQLite, SQL Server, ClickHouse, Oracle, Redis, MongoDB |
| Host, Port, Database / SQLite file | SQLite files are chosen with a file picker. Oracle: **Service** (e.g. `FREEPDB1`); Redis: **Database index** (`0`); MongoDB: also a full `mongodb://` or `mongodb+srv://` URL as the host |
| Authentication | User & Password, or none |
| Password | stored in the OS keychain (service `forge-db`), never in project files |
| SSH/SSL tab | SSH tunnel and SSL/TLS, see below |
| Options | read-only, colour, transaction control (Auto / Manual), single session mode, connect timeout, startup script |
| Scope | **Project Data Sources** (`<project>/.forge/dataSources.toml`, shareable) or **Global Data Sources** (`dataSources.toml` in Forge's config folder) |

**Test Connection** shows success with the server version, or the error.

### SSH tunnel and SSL/TLS

The **SSH/SSL** tab reaches databases that are only visible from a bastion host and secures
the connection itself.

![SSH/SSL tab](../../screenshots/database-ssh-ssl.png)

**Use SSH tunnel**: Forge opens the SSH connection itself (the same SSH client as
[remote development](remote-host-and-ssh.md)) and forwards a local port to the database
**host and port as the SSH host sees them** — `localhost:5432` is the database on the bastion.

- **SSH configuration**: one of your saved configurations (Settings | Tools | SSH
  Configurations), which brings its own host, user, authentication and stored secret, or
  **Ad-hoc** with the fields below.
- Ad-hoc: **Host** (also a `~/.ssh/config` alias, with its `HostName`, `Port`, `User`,
  `IdentityFile` and `ProxyJump`), **Port**, **User name** and **Authentication type**:
  **Password**, **Key pair** (private key file and passphrase) or **OpenSSH config and
  authentication agent**.
- The SSH password or key passphrase is stored in the OS keychain (account `<id>#ssh`).
- The first connection to an unknown host shows its key fingerprint: **Trust Host Key and Test
  Again** adds it to `~/.ssh/known_hosts`. A changed key is reported and refused until you trust
  it again.

**SSL** **Mode**:

| Mode | Meaning |
| --- | --- |
| Disable | no TLS |
| Prefer | TLS when the server offers it, certificate not checked (ClickHouse and Redis: plain) |
| Require | TLS, certificate not checked |
| Verify CA | TLS, certificate signed by the CA file (or the system roots) |
| Verify Full | like Verify CA, and the certificate must name the host |

**CA file**, **Client certificate file** and **Client key file** are PEM files; relative paths
are resolved against the project. An encrypted client key (PKCS#8 `ENCRYPTED PRIVATE KEY`) is
decrypted in memory with the **Client key password**, which is stored in the keychain (account
`<id>#ssl-key`); legacy OpenSSL-encrypted keys must be converted with
`openssl pkcs8 -topk8`. What each driver does with them:

| Driver | SSL/TLS |
| --- | --- |
| MySQL, MariaDB | `ssl-mode` Disabled / Preferred / Required / Verify CA / Verify Identity, CA and client certificate |
| PostgreSQL | `sslmode` with `sslrootcert`, `sslcert`, `sslkey` |
| SQL Server | Prefer encrypts the login, Require encrypts everything without checking the certificate, Verify uses the CA; client certificate and **unencrypted** key files |
| ClickHouse | HTTPS from Require on (use the HTTPS port, usually 8443); Verify CA also checks the host name |
| Redis | `rediss://` from Require on, CA and client certificate |
| MongoDB | TLS from Require on; the client certificate **and its key in one PEM file** (set as the client certificate) |
| Oracle | configured in the Oracle client (`sqlnet.ora`, wallet) |

Through an SSH tunnel, Verify Full checks the certificate against the configured database host
for SQL Server, Redis and MongoDB; for MySQL and PostgreSQL use Verify CA.

### Oracle

Forge does not bundle Oracle's client and there is no permissive pure-Rust driver for Oracle's
network protocol, so Oracle data sources run through **SQL\*Plus** (from the Oracle Instant
Client "SQL\*Plus" package) or **SQLcl**, which you install yourself; Forge finds `sqlplus` (or
SQLcl's `sql`) on `PATH`. Every statement starts `sqlplus -S -L /nolog`, logs in through the
process's input (the password never appears on a command line), prints CSV and exits, so:

- each statement is committed on its own; there are no manual transactions,
- **Cancel** stops the process,
- PL/SQL blocks (`BEGIN … END;`, `CREATE PROCEDURE …`) end with a line containing only `/`.

### From `.env`

When a project's `.env` has Laravel's `DB_*` variables or Symfony's `DATABASE_URL`, Forge shows
a balloon "Database found in .env" with **Add data source**; the tree also offers **Data
Source from .env**. Turn this off with Settings | Tools | Database | **Suggest data sources
from .env files** (`database.suggestFromEnv`).

## Query console

**New Query Console** on a data source (or **Open Query Console**) opens a console tab:

![Query console](../../screenshots/database-console.png)

- SQL highlighting for the data source's dialect and completion of keywords, tables and
  columns.
- **Execute** (`Ctrl+Enter`, macOS `Cmd+Enter`) runs the statement at the caret, or the
  selection. Several statements separated by `;` run one after another.
- SQL in PHP and JavaScript strings runs the same way: `Ctrl+Enter` in an injected SQL string
  opens a console with it (see [Language injections](language-injections.md#run-injected-sql)).
- Results appear below in tabs (**Output** plus one tab per result set), paged by
  **Result set page size** (`database.pageSize`, 500).
- **Cancel Running Statements** stops a long query (MySQL `KILL QUERY`, PostgreSQL
  `pg_cancel_backend`, ClickHouse `KILL QUERY`, SQL Server by closing and reopening the
  console's connection).
- **Tx: Auto / Manual** switches the transaction mode; in manual mode use **Commit** and
  **Rollback**. ClickHouse, Oracle, Redis and MongoDB consoles run every statement on its own.
- **New session / Shared session** chooses the console's connection, see [Sessions](#sessions).
- The schema selector at the top right sets the current schema.

SQL [scratch files](scratches-and-http.md) attached to a data source open as consoles too.

### Sessions

Every console runs in a **session** (an open connection). By default each console has its own
session, so transactions in one console are invisible to the others. **Shared session** in the
console toolbar (or **Single session mode** in the data source's Options) makes all consoles of
the data source share one connection and its transaction.

**Sessions** (the pulse icon in the Database tool window toolbar) lists the open sessions with
their data source, transaction mode, open transaction (statements and age), the running or
last statement, and **Commit**, **Rollback**, **Cancel** and **Close Session**.

![Sessions](../../screenshots/database-sessions.png)

### Redis

A Redis data source shows its databases (`db0`, … from `INFO keyspace`). Expanding one lists the
keys matching the **key pattern** at the top (`*`, `user:*`, …; `Enter` reloads) with their type
and TTL; at most 1000 keys are listed. Double-click a key to open its **value viewer**: strings
as text (JSON pretty-printed), lists and sets as numbered rows, hashes as field/value rows,
sorted sets with scores and streams as JSON.

![Redis keys and value viewer](../../screenshots/database-redis.png)

The console runs one command per line (`GET user:1`, `HGETALL user:1`, `SCAN 0 MATCH s*`,
quotes for spaces: `SET motd "hello world"`).

### MongoDB

A MongoDB data source shows databases → collections → fields (sampled from the first documents,
with their BSON types). Double-click a collection to open a console that finds its documents;
**JSON** in the result toolbar shows them as documents instead of a grid.

![MongoDB documents](../../screenshots/database-mongodb.png)

The console understands the shell calls you would type in `mongosh`:

```text
db.users.find({ age: { $gt: 30 } }, { name: 1, email: 1 }).sort({ age: -1 }).limit(5)
db.orders.aggregate([{ $group: { _id: "$status", n: { $sum: 1 } } }])
db.orders.countDocuments({ status: "paid" })
db.users.distinct("address.country")
db.users.insertOne({ name: "Ada" })   db.users.updateMany({...}, { $set: {...} })   db.users.deleteMany({...})
db.users.getIndexes()   db.getCollectionNames()   show dbs   show collections   use shop
```

Arguments are relaxed JSON: unquoted keys, single quotes, `ObjectId("…")`, `ISODate("…")`,
`NumberLong(…)`, `NumberDecimal("…")` and `/regex/i`.

## Read-only and production data sources

**Read-only** (Options tab) protects a data source: the console refuses statements that change
data or schema (DML and DDL, Redis write commands, MongoDB writes) with a clear message, and the
driver refuses them too — MySQL and PostgreSQL sessions are `READ ONLY`, SQLite opens the file
read-only, ClickHouse gets `readonly=2`, Oracle `SET TRANSACTION READ ONLY`, SQL Server declares
`ApplicationIntent=ReadOnly`, Redis and MongoDB connections only accept reads.

**Colour** marks a data source as development (**green**), staging (**yellow**) or production
(**red**); the tree, its console and table tabs and the console editor are tinted, and the
console shows a badge. Before running `UPDATE` or `DELETE` without `WHERE`, `TRUNCATE`,
`FLUSHDB`/`FLUSHALL`, `deleteMany({})`/`updateMany({}, …)` or `drop()` on a **production**
data source, Forge asks for confirmation:

![Production warning](../../screenshots/database-production-warning.png)

## Data grid and table editor

**Edit Data** (or double-click a table) opens the table in an editor tab with paging (**First,
Previous, Next, Last Page**), a **WHERE** filter and an **ORDER BY** field.

![Editing data](../../screenshots/database-grid-edit.png)

| Action | Windows/Linux | macOS |
| --- | --- | --- |
| Edit a cell | double-click or `Enter` | same |
| Add Row | `Alt+Insert` | `Cmd+N` |
| Delete Rows | `Ctrl+Y` | `Cmd+Backspace` |
| Revert Selected | `Ctrl+Alt+Z` | `Cmd+Alt+Z` |
| Submit | `Ctrl+Enter` | `Cmd+Enter` |
| Reload Page | toolbar | toolbar |

Cell context menu: **Copy**, **Copy with Header**, **Set NULL**. Changes are applied in one
transaction on **Submit**. Tables without a primary key cannot identify changed rows: only new
rows can be submitted there.

NULL is displayed as `<null>` (**Show NULL as**, `database.showNullAs`). Binary values are shown
as hex.

## Export

**Export Data** (the download icon in a result or table) writes the rows as **CSV**, **JSON**
or SQL **INSERT** statements for the dialect.

## Settings | Tools | Database

| Option | Setting | Default |
| --- | --- | --- |
| Result set page size | `database.pageSize` | 500 |
| Show NULL as | `database.showNullAs` | `<null>` |
| Suggest data sources from .env files | `database.suggestFromEnv` | on |

## Remote projects

In a [remote project](remote-development.md), project data sources (and the `.env`
suggestions) are read from the host, and Forge connects to them through an SSH tunnel to their
host and port **as the remote host sees them** — `localhost:3306` means the database on the
server. The data source shows which host it goes through. Global data sources connect directly
from your machine.

![Remote data source](../../screenshots/remote-parity-data-source-dialog.png)

![Remote database](../../screenshots/remote-parity-database.png)

A data source with its own SSH tunnel uses that tunnel instead.

> **Note:** SQLite files on the remote host cannot be opened.

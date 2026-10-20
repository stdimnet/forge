---
title: Scratches, consoles and HTTP client
description: Scratch files and buffers in any language, running PHP/JS/TS/Python/shell/Lua scratches, SQL scratches as database consoles, and .http request files with variables and environments.
order: 180
tags: [scratches, consoles, http client, rest, sql]
---

# Scratches, consoles and HTTP client

**Scratch files** are throw-away files for experiments, like PHPStorm's "Scratches and
Consoles". They are **global**: they do not belong to a project and are kept in Forge's config
folder (`<config dir>/forge/scratches/`), so they are available in every project.

## Creating scratches

| Action | Windows/Linux | macOS |
| --- | --- | --- |
| New Scratch File… | `Ctrl+Alt+Shift+Insert` | `Cmd+Shift+N` |
| New Scratch Buffer | Find Action | Find Action |

**New Scratch File** asks for the language: type to filter (fuzzy), recently used languages
come first. Every file type with an extension is available.

![New Scratch File](../../screenshots/scratches-new-popup.png)

New scratches are numbered `scratch.php`, `scratch_1.php`, …; HTTP scratches start as
`rest-api.http`. **Scratch buffers** are plain text files `buffer1.txt` … `buffer5.txt`; after
five, the oldest is reused.

## Scratches and Consoles in the Project view

The bottom of the Project tool window has a **Scratches and Consoles** node with **Scratches**
and **Database Consoles**. A scratch's context menu has **Rename…**, **Delete** and, for SQL
scratches, **Attach Data Source…**.

![Scratches in the Project view](../../screenshots/scratches-project-view.png)

## Running scratches

Scratches with a runner get a ▶ icon in the gutter; `Ctrl+Shift+F10` runs the current one.
The output appears in the Run tool window.

| Language | Runs with |
| --- | --- |
| PHP | `php.interpreter`, with `-d auto_prepend_file=<project>/vendor/autoload.php` when the open project has an autoloader, so your classes are available |
| JavaScript | `node` |
| TypeScript | `node --experimental-strip-types` (Node 22.6+) |
| Python | `python3` (`python` on Windows) |
| Shell (`.sh`, `.bash`) | `sh` |
| Lua | `lua` |

![PHP scratch run](../../screenshots/scratches-php-run.png)

Other languages show "Scratches of this language have no runner."

## SQL scratches

A `.sql` scratch can be attached to a data source (**Attach Data Source…**); it then opens as a
query console of that data source with execution, results and transactions. See
[Database tools](database.md).

![SQL scratch](../../screenshots/scratches-sql-console.png)

## HTTP client (`.http` files)

An HTTP scratch holds one or more requests in PHPStorm's `.http` format. Each request gets a ▶
icon; click it to send the request. The response (status, headers, body) appears in the Run
tool window.

![HTTP request](../../screenshots/scratches-http-request.png)

### Syntax

```http
@host = localhost:8000

### List users
GET http://{{host}}/api/users?page=1
Accept: application/json

###
# @name createUser
POST http://{{host}}/api/users
Content-Type: application/json
Authorization: Bearer {{token}}

{ "name": "Ada" }

### Long query strings can continue on indented lines
GET https://{{host}}/search
    ?q=forge
    &limit=10
```

- `###` separates requests; text after it names the request. `# @name x` also names one.
- A request is a request line `METHOD URL [HTTP/version]` (the method defaults to `GET`;
  `http://` is added when the URL has no scheme), optional indented `?`/`&` lines continuing the
  query string, headers, a blank line and the body.
- `@name = value` defines a file variable; `{{name}}` uses it in the URL, headers and body.
  Variables may refer to other variables. Unknown variables are left as they are.
- Comments start with `#` or `//`.

### Environments

Put environment files in the project root, as in PHPStorm:

```json
// http-client.env.json (commit it)
{
  "dev":  { "host": "localhost:8000" },
  "prod": { "host": "shop.example.com" }
}
```

```json
// http-client.private.env.json (keep it out of Git)
{
  "dev": { "token": "s3cret" }
}
```

Values from the private file win over the public one, and file variables (`@name = …`) win over
both. The environment `dev` is used, or the first one in the file when there is no `dev`.

> **Note:** choosing another environment, response handler scripts (`> {% … %}`,
> `> handler.js`), response references (`<> file`) and dynamic variables such as
> `{{$uuid}}`, `{{$timestamp}}` or `{{$randomInt}}` are not supported yet: handlers and
> references are skipped, and dynamic variables are sent literally. `.http` files are run
> from scratches; `.http` files inside the project are highlighted but have no ▶ icons yet.

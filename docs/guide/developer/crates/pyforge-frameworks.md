---
title: pyforge-frameworks
description: "PyForge: Django, Flask, FastAPI and Celery read statically from the sources - settings, URL patterns, routes and routers, models, Celery tasks, template folders and references, manage.py commands and the web run configurations' command lines."
order: 296
tags: [crate, rust, pack, python, django, flask, fastapi, celery, templates]
---

# pyforge-frameworks

`packs/pyforge/frameworks` — PyForge's web framework support (plugins `forge.python-web`,
`forge.django`, `forge.flask`, `forge.fastapi`, `forge.celery`, see
[Django, Flask, FastAPI and Celery](../../user/python-web-frameworks.md)). Pure: files in,
data out; the desktop app (`commands/python_web.rs`) and the WebAssembly build
(`crates/forge-wasm/src/python_web.rs`) share it. Nothing of the project runs.

## Purpose

- `source`: a forgiving token scanner — logical lines (brackets and `\` continuations
  joined, comments dropped), names, operators and string literals with the byte range of
  their content, call arguments, decorators, definitions and assignments. The scanner of
  `pyforge-testing` keeps only line text; navigation needs the offsets of string literals.
- `modules`: source roots (`""`, `src`, the folders of `manage.py`), module ↔ file, the
  names a file imports (relative imports included), so `views.index` resolves to its `def`.
- `detect`: frameworks from `requirements*.txt`, `pyproject.toml`, `Pipfile` and imports
  (Django also needs `manage.py`); `FLASK_APP` (also `module:create_app()`), the uvicorn app
  (`module:attr`, `--app-dir src`) and the Celery `-A` app.
- `django`: the settings module (setting, `manage.py`, a `settings.py` next to `urls.py`),
  `INSTALLED_APPS`, `ROOT_URLCONF`, `TEMPLATES` (`DIRS` from `BASE_DIR`, `APP_DIRS`, Jinja2)
  through `from .base import *`; URL patterns from `path` / `re_path` / `url` through
  `include()` with namespaces; models and fields; `manage.py` commands (built in, the
  project's `management/commands`, parsing `help --commands`).
- `flask`, `fastapi`: routes, blueprints and their registration prefixes; path operations
  of apps and `APIRouter`s with nested `include_router` prefixes.
- `celery`: `@shared_task` / `@app.task` functions and their registered names.
- `templates`: template folders and names, references in Python and templates, Go to
  Declaration, completion, gutter marks and the unresolved template / URL name inspections.
- `run`: `django-server`, `django-manage`, `flask-server`, `fastapi`, `celery-worker` and
  `celery-beat` as Python runs (target, arguments, environment, working directory, port).

## Tests

Unit tests per module with fixture projects (`tests/fixtures/django`), no network:
`cargo test -p pyforge-frameworks`.

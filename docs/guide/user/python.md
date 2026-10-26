---
title: PyForge (Python)
description: The PyForge pack - Python interpreters (venv, uv, Poetry, conda, Pipenv, Hatch, Docker, SSH, WSL), the Python Packages tool window, requirements inspections, basedpyright / ty / ruff, Black and the Python run configuration.
order: 115
tags: [python, pyforge, interpreter, venv, uv, poetry, conda, pip, ruff, basedpyright]
---

# PyForge (Python)

**PyForge** brings PyCharm's Python support to Forge. It has two plugins:

- **Python** (`forge.python`) — highlighting, scope-aware completion and syntax checks for
  `.py` files, pip requirements files, the Python language servers and Reformat Code. It is
  active in every project, so a lone script is still a Python file.
- **Python Interpreters and Packages** (`forge.python-env`) — interpreters, the Python
  Packages tool window, requirements inspections and the Python run configuration. It is
  active in [Python projects](../getting-started/project-detection.md): `pyproject.toml`,
  `setup.py`, `setup.cfg`, `requirements*.txt`, `Pipfile`, `environment.yml`, Python files in
  the root, or two or more in a folder.

![A Python file with basedpyright's errors and the requirements inspection](../../screenshots/python-editor.png)

## Python interpreter

The status bar shows the project's interpreter, PyCharm style: **Python 3.12 (shop-api)**.
Click it (or run **Switch Python Interpreter…** from Find Action) to switch between the
configured interpreters, the project's environments and the Pythons found on this machine,
to add a new one, or to open the settings.

![The interpreter popup in the status bar](../../screenshots/python-interpreter-popup.png)

Without a selection Forge uses the project's own environment (`.venv`, `venv` or the Poetry,
Pipenv or Hatch environment of the project) automatically.

### Settings | Project | Python Interpreter

The page selects the interpreter (`python.interpreter`: the path of a local `python`, or the
name of a configured interpreter) and lists its packages with the latest version on PyPI.
**Add Interpreter…** opens the **Add Python Interpreter** dialog.

![Settings | Project | Python Interpreter](../../screenshots/python-interpreter-settings.png)

Forge finds, without running anything inside the project:

| Kind | Where |
| --- | --- |
| Virtualenv / venv | `.venv`, `venv`, `env` with a `pyvenv.cfg` (typed by `pyvenv.cfg`: uv and virtualenv write their own key) |
| uv | an environment created by `uv venv` / `uv sync`, or any project environment next to `uv.lock` |
| Poetry | `<cache>/pypoetry/virtualenvs/<name>-…`, else `poetry env info -p` |
| Pipenv | `~/.local/share/virtualenvs/<folder>-…` (`WORKON_HOME`), else `pipenv --venv` |
| Hatch | `<data>/hatch/env/virtual/<name>/…` |
| conda / mamba | `~/miniconda3`, `~/anaconda3`, `~/miniforge3`, `~/mambaforge`, `~/micromamba`, `/opt/conda` and their `envs/`, `~/.conda/environments.txt`; the version comes from `conda-meta` |
| pyenv | `~/.pyenv/versions/*` (`PYENV_ROOT`) |
| System | `python`, `python3`, `python3.X` on `PATH`, `/usr/bin`, `/usr/local/bin`, Homebrew; python.org installs on Windows (`%LOCALAPPDATA%\Programs\Python`) |

Only system Pythons are asked for their version (`python --version`); environments report
theirs from their metadata. The tools (`poetry`, `pipenv`) are asked only outside
[Safe Mode](project-trust.md).

### Add Python Interpreter

![Add Python Interpreter](../../screenshots/python-add-interpreter.png)

- **Local Interpreter | Generate new** creates an environment:
  - **Virtualenv**: `<base python> -m venv <location>`, then `pip install -r requirements.txt`.
  - **uv**: `uv venv <location> --python <base or version>`, then `uv sync` (with `uv.lock`)
    or `uv pip install -r requirements.txt`.
  - **Poetry**: `poetry env use <base python>`, then `poetry install`.
  - **Conda**: `conda create --yes --prefix <location> python=<version>`, then pip.

  The commands run one after another in the Run tool window; the new environment becomes the
  project's interpreter. "Install the project's dependencies" can be turned off.
- **Local Interpreter | Select existing** picks a detected interpreter or a path.
- **Docker**, **Docker Compose**, **SSH** and **WSL** work like PHP's
  [CLI interpreters](php.md): an image (the project is mounted at `/opt/project`), a compose
  service (`docker compose run` or `exec`), an SSH configuration of Settings | Tools | SSH
  Configurations, or a WSL distribution, plus the path of `python` there. Paths in arguments
  and the Run console are mapped both ways. Such interpreters are stored in
  `python.projectInterpreters` (or `python.interpreters` for every project).

## Python Packages tool window

**View | Tool Windows | Python Packages** lists the packages of the project's interpreter
with their versions; a newer release on PyPI is shown next to the version. Type in the search
field to filter them and to search PyPI. The right side shows the selected package: summary,
latest version, required Python, licence and home page, with **Install** (choose a version),
**Upgrade to …** and **Delete Package**.

![Python Packages](../../screenshots/python-packages.png)

Packages are installed with the tool that manages the environment:

| Environment | Command |
| --- | --- |
| uv (or a venv next to `uv.lock`) | `uv pip install --python <python> …` / `uv pip uninstall` |
| Poetry | `poetry add` / `poetry remove` / `poetry update` (also changes `pyproject.toml`) |
| Pipenv | `pipenv install` / `uninstall` / `update` |
| anything else | `python -m pip install` / `uninstall --yes` / `install --upgrade` |

The commands run in the Run tool window and are refused in [Safe Mode](project-trust.md).
Installed packages are read with a small `importlib.metadata` script, so environments without
pip (uv's) work too.

PyPI answers are cached for 12 hours in `<cache dir>/forge/pypi` and the cache is used when
PyPI cannot be reached. Settings | Languages & Frameworks | Python | **Look up packages on
PyPI** (`python.packages.pypi`) turns the lookups off; **Package index**
(`python.packages.index`) points them at a mirror with PyPI's JSON API.

![Searching PyPI](../../screenshots/python-packages-search.png)

## Requirements inspections

- **Package requirement 'X' is not satisfied**: a dependency in `requirements*.txt` or
  `pyproject.toml` (PEP 621 `dependencies` and `optional-dependencies`, PEP 735
  `dependency-groups`, Poetry's `tool.poetry.*dependencies`) is not installed, or the
  installed version does not match its specifiers. `Alt+Enter`: **Install requirement**,
  **Install all missing requirements**, **Ignore requirement**.
- **Package containing module 'x' is not listed in the project requirements**: an import of a
  third-party package that no dependency file declares. `Alt+Enter`: **Add requirement 'x' to
  pyproject.toml** (or `requirements.txt` when the project has no PEP 621 / Poetry
  dependencies), **Install package x** when it is not installed, **Ignore requirement**.
  The distribution name is taken from the installed packages (`import yaml` → `PyYAML`).

![Install requirement in requirements-dev.txt](../../screenshots/python-requirements.png)

Versions are compared with PEP 440 rules (`~=`, `==1.4.*`, pre-releases, local versions,
Poetry's `^` and `~`). Dependencies with environment markers are not checked. **Install
Requirements** (Find Action) installs everything that is not satisfied. Settings | Languages &
Frameworks | Python | Inspections turns the two inspections off and lists ignored packages
(`python.inspections.ignoredPackages`).

Requirements files are highlighted (names, extras, specifiers, markers, options, URLs) and
complete package names (installed ones and PyPI) and versions; so do the dependency strings
of `pyproject.toml`.

## Language servers

Settings | Languages & Frameworks | Python lists the Python servers of the
[language server catalog](language-servers.md):

| Server | Role |
| --- | --- |
| **basedpyright** (default) | types, completion, navigation, hover; Forge installs it from npm with one click |
| **Pyright** | starts when basedpyright is missing or disabled |
| **ty** (preview) | Astral's type checker; off by default, enable it and disable basedpyright and Pyright |
| **Ruff** | `ruff server`: lint diagnostics and fixes next to the type checker |

Forge downloads ruff and ty itself, with your consent, as the official wheel of your platform
from PyPI, checked against the SHA-256 digest PyPI publishes, into
`<data dir>/forge/langservers/bin`. Nothing is bundled with Forge.

**Use language servers from the interpreter's environment**
(`python.lsp.useEnvironmentTools`, on by default): a basedpyright, ruff or ty installed in the
selected environment (or the project's `.venv`) is used before Forge's download and `PATH`.
basedpyright and Pyright get the interpreter as `python.pythonPath`, so imports resolve
against its site-packages.

## Reformat Code

`Ctrl+Alt+L` (and "Reformat code" in [Actions on Save](actions-on-save.md)) formats Python files
with the formatter of **Reformat Code with** (`python.formatter`): **ruff format** (default),
**Black**, **the language server** or **do not format**. Their settings in `pyproject.toml`,
`ruff.toml` or `.ruff.toml` apply. The executables come from `python.ruff.path` /
`python.black.path`, else the interpreter's environment, Forge's download (ruff) or `PATH`.

## Running Python code

The **Python** run configuration runs a **Script path** or a **Module name** (`python -m`) with
script parameters, interpreter options, environment variables and a working directory,
through the configuration's interpreter or the project's (local, Docker, Docker Compose, SSH or
WSL). `PYTHONUNBUFFERED=1` keeps the output live. A ▶ in the gutter of
`if __name__ == "__main__":` runs the file, as does `Ctrl+Shift+F10` in a Python file.

Running scripts, modules, pip and environment tools is running project code: it is refused in
[Safe Mode](project-trust.md). The debugger, test runners and the Python console are not part
of PyForge yet.

## Settings

| Setting | Key | Default |
| --- | --- | --- |
| Python Interpreter | `python.interpreter` | the project's environment |
| Reformat Code with | `python.formatter` | `ruff` |
| ruff / Black executable | `python.ruff.path`, `python.black.path` | found automatically |
| Use language servers from the interpreter's environment | `python.lsp.useEnvironmentTools` | on |
| Look up packages on PyPI | `python.packages.pypi` | on |
| Package index | `python.packages.index` | `https://pypi.org` |
| Report unsatisfied requirements | `python.inspections.requirements` | on |
| Report imports of packages missing from the requirements | `python.inspections.imports` | on |
| Ignored packages | `python.inspections.ignoredPackages` | none |

## Browser mode

`?project=python` opens the demo service `shop-api` (FastAPI, SQLAlchemy, a uv environment).

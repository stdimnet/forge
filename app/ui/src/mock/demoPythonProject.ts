/**
 * A small FastAPI service for browser mode, opened with `?project=python` (the default demo
 * is the Laravel shop in demoProject.ts). Its interpreter is a uv environment in `.venv`
 * with the packages below; pythonMock.ts answers the `python_*` commands, running the Rust
 * inspections through WebAssembly.
 */
import type { ProjectInfo } from "@/ipc/project";
import type { InstalledPackage } from "@/ipc/python";

export const pythonDemoName = "shop-api";

/** True when the page asks for the Python demo project. */
export const isPythonDemo = (): boolean =>
  typeof location !== "undefined" && new URLSearchParams(location.search).get("project") === "python";

export const pythonDemoInfo: ProjectInfo = {
  languages: ["python"],
  frameworks: [],
  packs: ["pyforge"],
  tools: ["git", "uv", "docker"],
  primary: { id: "python", name: "Python", language: "python" },
};

/** The project's `.venv` (uv) and its packages. */
export const pythonDemoInstalled: InstalledPackage[] = [
  { name: "annotated-types", version: "0.7.0", modules: ["annotated_types"] },
  { name: "anyio", version: "4.6.0", modules: ["anyio"] },
  { name: "certifi", version: "2024.8.30", modules: ["certifi"] },
  { name: "click", version: "8.1.7", modules: ["click"] },
  { name: "fastapi", version: "0.115.0", modules: ["fastapi"] },
  { name: "h11", version: "0.14.0", modules: ["h11"] },
  { name: "httpcore", version: "1.0.5", modules: ["httpcore"] },
  { name: "httpx", version: "0.27.2", modules: ["httpx"] },
  { name: "idna", version: "3.10", modules: ["idna"] },
  { name: "iniconfig", version: "2.0.0", modules: ["iniconfig"] },
  { name: "packaging", version: "24.1", modules: ["packaging"] },
  { name: "pluggy", version: "1.5.0", modules: ["pluggy"] },
  { name: "pydantic", version: "2.9.2", modules: ["pydantic"] },
  { name: "pydantic-settings", version: "2.5.2", modules: ["pydantic_settings"] },
  { name: "pydantic_core", version: "2.23.4", modules: ["pydantic_core"] },
  { name: "pytest", version: "8.3.3", modules: ["_pytest", "py", "pytest"] },
  { name: "python-dotenv", version: "1.0.1", modules: ["dotenv"] },
  { name: "PyYAML", version: "6.0.2", modules: ["_yaml", "yaml"] },
  { name: "sniffio", version: "1.3.1", modules: ["sniffio"] },
  { name: "SQLAlchemy", version: "2.0.35", modules: ["sqlalchemy"] },
  { name: "starlette", version: "0.38.6", modules: ["starlette"] },
  { name: "typing_extensions", version: "4.12.2", modules: ["typing_extensions"] },
  { name: "uvicorn", version: "0.30.6", modules: ["uvicorn"] },
];

/** PyPI as the browser demo knows it: name, latest version, summary, licence. */
export const pythonDemoPypi: { name: string; version: string; summary: string; license: string; requiresPython: string; homePage: string }[] = [
  { name: "fastapi", version: "0.115.4", summary: "FastAPI framework, high performance, easy to learn, fast to code, ready for production", license: "MIT", requiresPython: ">=3.8", homePage: "https://github.com/fastapi/fastapi" },
  { name: "SQLAlchemy", version: "2.0.36", summary: "Database Abstraction Library", license: "MIT", requiresPython: ">=3.7", homePage: "https://www.sqlalchemy.org" },
  { name: "pydantic", version: "2.9.2", summary: "Data validation using Python type hints", license: "MIT", requiresPython: ">=3.8", homePage: "https://github.com/pydantic/pydantic" },
  { name: "pydantic-settings", version: "2.6.1", summary: "Settings management using Pydantic", license: "MIT", requiresPython: ">=3.8", homePage: "https://github.com/pydantic/pydantic-settings" },
  { name: "httpx", version: "0.27.2", summary: "The next generation HTTP client.", license: "BSD-3-Clause", requiresPython: ">=3.8", homePage: "https://github.com/encode/httpx" },
  { name: "uvicorn", version: "0.32.0", summary: "The lightning-fast ASGI server.", license: "BSD-3-Clause", requiresPython: ">=3.8", homePage: "https://www.uvicorn.org" },
  { name: "pytest", version: "8.3.3", summary: "pytest: simple powerful testing with Python", license: "MIT", requiresPython: ">=3.8", homePage: "https://docs.pytest.org" },
  { name: "PyYAML", version: "6.0.2", summary: "YAML parser and emitter for Python", license: "MIT", requiresPython: ">=3.8", homePage: "https://pyyaml.org" },
  { name: "rich", version: "13.9.3", summary: "Render rich text, tables, progress bars, syntax highlighting, markdown and more to the terminal", license: "MIT", requiresPython: ">=3.8", homePage: "https://github.com/Textualize/rich" },
  { name: "ruff", version: "0.7.1", summary: "An extremely fast Python linter and code formatter, written in Rust.", license: "MIT", requiresPython: ">=3.7", homePage: "https://docs.astral.sh/ruff" },
  { name: "mypy", version: "1.13.0", summary: "Optional static typing for Python", license: "MIT", requiresPython: ">=3.8", homePage: "https://www.mypy-lang.org" },
  { name: "requests", version: "2.32.3", summary: "Python HTTP for Humans.", license: "Apache-2.0", requiresPython: ">=3.8", homePage: "https://requests.readthedocs.io" },
  { name: "requests-oauthlib", version: "2.0.0", summary: "OAuthlib authentication support for Requests.", license: "ISC", requiresPython: ">=3.4", homePage: "https://github.com/requests/requests-oauthlib" },
  { name: "starlette", version: "0.41.2", summary: "The little ASGI library that shines.", license: "BSD-3-Clause", requiresPython: ">=3.8", homePage: "https://www.starlette.io" },
  { name: "anyio", version: "4.6.2", summary: "High level compatibility layer for multiple asynchronous event loop implementations", license: "MIT", requiresPython: ">=3.9", homePage: "https://anyio.readthedocs.io" },
  { name: "certifi", version: "2024.8.30", summary: "Python package for providing Mozilla's CA Bundle.", license: "MPL-2.0", requiresPython: ">=3.6", homePage: "https://github.com/certifi/python-certifi" },
  { name: "click", version: "8.1.7", summary: "Composable command line interface toolkit", license: "BSD-3-Clause", requiresPython: ">=3.7", homePage: "https://palletsprojects.com/p/click/" },
  { name: "packaging", version: "24.2", summary: "Core utilities for Python packages", license: "Apache-2.0", requiresPython: ">=3.8", homePage: "https://github.com/pypa/packaging" },
  { name: "idna", version: "3.10", summary: "Internationalized Domain Names in Applications (IDNA)", license: "BSD-3-Clause", requiresPython: ">=3.6", homePage: "https://github.com/kjd/idna" },
];

export const pythonDemoFiles: Record<string, string> = {
  "pyproject.toml": `[project]
name = "shop-api"
version = "0.4.0"
description = "Orders and checkout API of the shop"
readme = "README.md"
requires-python = ">=3.11"
dependencies = [
    "fastapi>=0.115",
    "SQLAlchemy~=2.0",
    "pydantic-settings>=2.5",
    "httpx>=0.27",
    "uvicorn>=0.30",
]

[project.optional-dependencies]
test = ["pytest>=8"]

[tool.ruff]
line-length = 100
target-version = "py312"

[tool.ruff.lint]
select = ["E", "F", "I", "UP", "B"]

[tool.pytest.ini_options]
testpaths = ["tests"]
`,
  "requirements-dev.txt": `# Tools for local development (pip install -r requirements-dev.txt)
-r requirements.txt
pytest>=8.0
ruff==0.7.1
mypy>=1.11
httpx>=0.28  # the test client needs the new transport API
`,
  "requirements.txt": `# Generated with: uv pip compile pyproject.toml -o requirements.txt
fastapi==0.115.0
sqlalchemy==2.0.35
pydantic-settings==2.5.2
httpx==0.27.2
uvicorn==0.30.6
`,
  "uv.lock": `version = 1
requires-python = ">=3.11"

[[package]]
name = "fastapi"
version = "0.115.0"
source = { registry = "https://pypi.org/simple" }
`,
  ".gitignore": `.venv/
__pycache__/
*.pyc
.pytest_cache/
.env
`,
  ".python-version": "3.12\n",
  "README.md": `# shop-api

Orders and checkout API of the shop, built with FastAPI and SQLAlchemy.

\`\`\`sh
uv sync
uv run uvicorn shop_api.main:app --reload
\`\`\`
`,
  "src/shop_api/__init__.py": `"""Orders and checkout API of the shop."""

__version__ = "0.4.0"
`,
  "src/shop_api/main.py": `"""The FastAPI application: routes for orders and the health check."""
from contextlib import asynccontextmanager

import yaml
from fastapi import Depends, FastAPI, HTTPException
from rich.console import Console
from sqlalchemy.orm import Session

from shop_api.db import engine, get_session
from shop_api.models import Base, Order, OrderIn
from shop_api.settings import settings

console = Console()


@asynccontextmanager
async def lifespan(app: FastAPI):
    Base.metadata.create_all(engine)
    console.log(f"[green]shop-api[/green] listening on {settings.host}:{settings.port}")
    yield


app = FastAPI(title="shop-api", lifespan=lifespan)


@app.get("/health")
def health() -> dict[str, str]:
    return {"status": "ok"}


@app.get("/orders/{order_id}")
def read_order(order_id: int, session: Session = Depends(get_session)) -> Order:
    order = session.get(Order, order_id)
    if order is None:
        raise HTTPException(status_code=404, detail="Order not found")
    return order


@app.post("/orders", status_code=201)
def create_order(data: OrderIn, session: Session = Depends(get_session)) -> Order:
    order = Order(customer=data.customer, total=data.total)
    session.add(order)
    session.commit()
    return order.totl


def load_fixtures(path: str) -> list[dict]:
    with open(path, encoding="utf-8") as f:
        return yaml.safe_load(f)


if __name__ == "__main__":
    import uvicorn

    uvicorn.run("shop_api.main:app", host=settings.host, port=settings.port, reload=True)
`,
  "src/shop_api/models.py": `"""Database models and the request schemas."""
from decimal import Decimal

from pydantic import BaseModel, Field
from sqlalchemy import Numeric, String
from sqlalchemy.orm import DeclarativeBase, Mapped, mapped_column


class Base(DeclarativeBase):
    pass


class Order(Base):
    __tablename__ = "orders"

    id: Mapped[int] = mapped_column(primary_key=True)
    customer: Mapped[str] = mapped_column(String(120))
    total: Mapped[Decimal] = mapped_column(Numeric(10, 2))


class OrderIn(BaseModel):
    customer: str = Field(min_length=1, max_length=120)
    total: Decimal = Field(gt=0)
`,
  "src/shop_api/db.py": `"""The database engine and the session dependency."""
from collections.abc import Iterator

from sqlalchemy import create_engine
from sqlalchemy.orm import Session

from shop_api.settings import settings

engine = create_engine(settings.database_url, echo=settings.debug)


def get_session() -> Iterator[Session]:
    with Session(engine) as session:
        yield session
`,
  "src/shop_api/settings.py": `"""Settings from the environment and .env (pydantic-settings)."""
from pydantic_settings import BaseSettings, SettingsConfigDict


class Settings(BaseSettings):
    model_config = SettingsConfigDict(env_file=".env", env_prefix="SHOP_")

    database_url: str = "sqlite:///./shop.db"
    host: str = "127.0.0.1"
    port: int = 8000
    debug: bool = False


settings = Settings()
`,
  "tests/__init__.py": "",
  "tests/test_orders.py": `from fastapi.testclient import TestClient

from shop_api.main import app

client = TestClient(app)


def test_health():
    assert client.get("/health").json() == {"status": "ok"}


def test_unknown_order_is_404():
    assert client.get("/orders/999").status_code == 404
`,
  "scripts/seed.py": `"""Load the demo orders into the database."""
from shop_api.db import engine
from shop_api.models import Base

if __name__ == "__main__":
    Base.metadata.create_all(engine)
    print("seeded")
`,
  "compose.yaml": `services:
  api:
    image: python:3.12-slim
    working_dir: /app
    volumes:
      - .:/app
    command: uv run uvicorn shop_api.main:app --host 0.0.0.0
    ports:
      - "8000:8000"
`,
};

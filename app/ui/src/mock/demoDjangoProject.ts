/**
 * A Django bookshop for browser mode, opened with `?project=django`: two apps (catalog,
 * orders) with models, URL patterns with namespaces, templates in `templates/` and the
 * apps' folders, Celery tasks and a custom manage.py command. The Python mocks treat it as
 * a PyForge project (`demoPythonProject.ts` switches to these files); the Django, Flask,
 * FastAPI and Celery features read it through the Rust analysis in WebAssembly.
 */
import type { ProjectInfo } from "@/ipc/project";
import type { InstalledPackage } from "@/ipc/python";

export const djangoDemoName = "bookshop";

export const djangoDemoInfo: ProjectInfo = {
  languages: ["python", "html"],
  frameworks: [
    { id: "django", name: "Django", version: "5.1.2", language: "python" },
    { id: "celery", name: "Celery", version: "5.4.0", language: "python" },
  ],
  packs: ["pyforge"],
  tools: ["git"],
  primary: { id: "django", name: "Django", version: "5.1.2", language: "python" },
};

export const djangoDemoInstalled: InstalledPackage[] = [
  { name: "amqp", version: "5.2.0", modules: ["amqp"] },
  { name: "asgiref", version: "3.8.1", modules: ["asgiref"] },
  { name: "billiard", version: "4.2.1", modules: ["billiard"] },
  { name: "celery", version: "5.4.0", modules: ["celery"] },
  { name: "Django", version: "5.1.2", modules: ["django"] },
  { name: "iniconfig", version: "2.0.0", modules: ["iniconfig"] },
  { name: "kombu", version: "5.4.2", modules: ["kombu"] },
  { name: "packaging", version: "24.1", modules: ["packaging"] },
  { name: "pluggy", version: "1.5.0", modules: ["pluggy"] },
  { name: "pytest", version: "8.3.3", modules: ["_pytest", "py", "pytest"] },
  { name: "pytest-django", version: "4.9.0", modules: ["pytest_django"] },
  { name: "redis", version: "5.1.1", modules: ["redis"] },
  { name: "sqlparse", version: "0.5.1", modules: ["sqlparse"] },
  { name: "vine", version: "5.1.0", modules: ["vine"] },
];

export const djangoDemoFiles: Record<string, string> = {
  "requirements.txt": `Django==5.1.2
celery[redis]==5.4.0
redis==5.1.1
pytest-django==4.9.0
`,
  ".gitignore": `.venv/
__pycache__/
*.pyc
db.sqlite3
media/
`,
  ".python-version": "3.12\n",
  "README.md": `# bookshop

A small Django shop: the book catalog, the cart and orders, and Celery tasks for review
digests.

\`\`\`sh
python manage.py migrate
python manage.py runserver
celery -A bookshop worker -l INFO
\`\`\`
`,
  "pytest.ini": `[pytest]
DJANGO_SETTINGS_MODULE = bookshop.settings
python_files = tests.py test_*.py
`,
  "manage.py": `#!/usr/bin/env python
"""Django's command-line utility for administrative tasks."""
import os
import sys


def main():
    """Run administrative tasks."""
    os.environ.setdefault("DJANGO_SETTINGS_MODULE", "bookshop.settings")
    try:
        from django.core.management import execute_from_command_line
    except ImportError as exc:
        raise ImportError(
            "Couldn't import Django. Is it installed and available on your PYTHONPATH?"
        ) from exc
    execute_from_command_line(sys.argv)


if __name__ == "__main__":
    main()
`,
  "bookshop/__init__.py": `from .celery import app as celery_app

__all__ = ("celery_app",)
`,
  "bookshop/celery.py": `import os

from celery import Celery

os.environ.setdefault("DJANGO_SETTINGS_MODULE", "bookshop.settings")

app = Celery("bookshop")
app.config_from_object("django.conf:settings", namespace="CELERY")
app.autodiscover_tasks()


@app.task(bind=True, ignore_result=True)
def debug_task(self):
    print(f"Request: {self.request!r}")
`,
  "bookshop/settings.py": `"""Settings of the bookshop project."""
from pathlib import Path

BASE_DIR = Path(__file__).resolve().parent.parent

SECRET_KEY = "django-insecure-bookshop-demo"
DEBUG = True
ALLOWED_HOSTS: list[str] = []

INSTALLED_APPS = [
    "django.contrib.admin",
    "django.contrib.auth",
    "django.contrib.contenttypes",
    "django.contrib.sessions",
    "django.contrib.messages",
    "django.contrib.staticfiles",
    "catalog.apps.CatalogConfig",
    "orders",
]

MIDDLEWARE = [
    "django.middleware.security.SecurityMiddleware",
    "django.contrib.sessions.middleware.SessionMiddleware",
    "django.middleware.common.CommonMiddleware",
    "django.middleware.csrf.CsrfViewMiddleware",
    "django.contrib.auth.middleware.AuthenticationMiddleware",
    "django.contrib.messages.middleware.MessageMiddleware",
]

ROOT_URLCONF = "bookshop.urls"

TEMPLATES = [
    {
        "BACKEND": "django.template.backends.django.DjangoTemplates",
        "DIRS": [BASE_DIR / "templates"],
        "APP_DIRS": True,
        "OPTIONS": {
            "context_processors": [
                "django.template.context_processors.request",
                "django.contrib.auth.context_processors.auth",
                "django.contrib.messages.context_processors.messages",
            ],
        },
    },
]

WSGI_APPLICATION = "bookshop.wsgi.application"

DATABASES = {
    "default": {
        "ENGINE": "django.db.backends.sqlite3",
        "NAME": BASE_DIR / "db.sqlite3",
    }
}

STATIC_URL = "static/"
DEFAULT_AUTO_FIELD = "django.db.models.BigAutoField"

CELERY_BROKER_URL = "redis://localhost:6379/0"
`,
  "bookshop/urls.py": `from django.contrib import admin
from django.urls import include, path

from catalog import views as catalog_views

urlpatterns = [
    path("", catalog_views.home, name="home"),
    path("admin/", admin.site.urls),
    path("books/", include("catalog.urls")),
    path("orders/", include("orders.urls")),
    path("accounts/", include("django.contrib.auth.urls")),
]
`,
  "bookshop/wsgi.py": `import os

from django.core.wsgi import get_wsgi_application

os.environ.setdefault("DJANGO_SETTINGS_MODULE", "bookshop.settings")

application = get_wsgi_application()
`,
  "catalog/__init__.py": "",
  "catalog/apps.py": `from django.apps import AppConfig


class CatalogConfig(AppConfig):
    default_auto_field = "django.db.models.BigAutoField"
    name = "catalog"
`,
  "catalog/admin.py": `from django.contrib import admin

from .models import Author, Book, Review

admin.site.register([Author, Book, Review])
`,
  "catalog/models.py": `from django.conf import settings
from django.db import models


class Author(models.Model):
    name = models.CharField(max_length=120)
    born = models.DateField(null=True, blank=True)

    def __str__(self):
        return self.name


class Book(models.Model):
    title = models.CharField(max_length=200)
    slug = models.SlugField(unique=True)
    author = models.ForeignKey(Author, on_delete=models.PROTECT, related_name="books")
    price = models.DecimalField(max_digits=8, decimal_places=2)
    published = models.DateField()
    in_stock = models.BooleanField(default=True)

    class Meta:
        ordering = ["title"]

    def __str__(self):
        return self.title


class Review(models.Model):
    book = models.ForeignKey(Book, on_delete=models.CASCADE, related_name="reviews")
    user = models.ForeignKey(settings.AUTH_USER_MODEL, on_delete=models.CASCADE)
    rating = models.PositiveSmallIntegerField()
    text = models.TextField(blank=True)
    created = models.DateTimeField(auto_now_add=True)
`,
  "catalog/urls.py": `from django.urls import path

from . import views

app_name = "catalog"

urlpatterns = [
    path("", views.BookListView.as_view(), name="book-list"),
    path("<slug:slug>/", views.book_detail, name="book-detail"),
    path("<slug:slug>/reviews/", views.add_review, name="add-review"),
    path("authors/<int:pk>/", views.author_detail, name="author-detail"),
]
`,
  "catalog/views.py": `from django.contrib.auth.decorators import login_required
from django.shortcuts import get_object_or_404, redirect, render
from django.views.generic import ListView

from .models import Author, Book
from .tasks import send_review_digest


def home(request):
    latest = Book.objects.order_by("-published")[:6]
    return render(request, "home.html", {"latest": latest})


class BookListView(ListView):
    model = Book
    paginate_by = 20
    template_name = "catalog/book_list.html"


def book_detail(request, slug):
    book = get_object_or_404(Book, slug=slug)
    return render(request, "catalog/book_detail.html", {"book": book})


@login_required
def add_review(request, slug):
    book = get_object_or_404(Book, slug=slug)
    book.reviews.create(user=request.user, rating=int(request.POST["rating"]), text=request.POST.get("text", ""))
    send_review_digest.delay(book.pk)
    return redirect("catalog:book-detail", slug=slug)


def author_detail(request, pk):
    author = get_object_or_404(Author, pk=pk)
    return render(request, "catalog/author_detail.html", {"author": author})
`,
  "catalog/tasks.py": `from celery import shared_task
from django.core.mail import mail_admins

from .models import Book


@shared_task
def send_review_digest(book_id: int) -> int:
    book = Book.objects.get(pk=book_id)
    count = book.reviews.count()
    mail_admins(f"New review for {book}", f"{book} has {count} reviews.")
    return count


@shared_task(name="catalog.reindex", autoretry_for=(ConnectionError,), max_retries=3)
def reindex_books() -> None:
    for book in Book.objects.filter(in_stock=True):
        book.save(update_fields=["title"])
`,
  "catalog/tests.py": `from datetime import date
from decimal import Decimal

from django.test import TestCase
from django.urls import reverse

from .models import Author, Book


class BookViewsTest(TestCase):
    @classmethod
    def setUpTestData(cls):
        author = Author.objects.create(name="Ursula K. Le Guin")
        cls.book = Book.objects.create(
            title="The Dispossessed", slug="the-dispossessed", author=author,
            price=Decimal("12.90"), published=date(1974, 5, 1),
        )

    def test_list(self):
        response = self.client.get(reverse("catalog:book-list"))
        self.assertContains(response, "The Dispossessed")

    def test_detail(self):
        response = self.client.get(reverse("catalog:book-detail", args=["the-dispossessed"]))
        self.assertEqual(response.status_code, 200)
`,
  "catalog/management/__init__.py": "",
  "catalog/management/commands/__init__.py": "",
  "catalog/management/commands/import_books.py": `import csv

from django.core.management.base import BaseCommand

from catalog.models import Author, Book


class Command(BaseCommand):
    help = "Import books from a CSV export of the warehouse."

    def add_arguments(self, parser):
        parser.add_argument("path")

    def handle(self, *args, path, **options):
        with open(path, encoding="utf-8") as f:
            for row in csv.DictReader(f):
                author, _ = Author.objects.get_or_create(name=row["author"])
                Book.objects.update_or_create(slug=row["slug"], defaults={"title": row["title"], "author": author})
        self.stdout.write(self.style.SUCCESS("Imported"))
`,
  "catalog/templates/catalog/book_list.html": `{% extends "base.html" %}

{% block title %}Books{% endblock %}

{% block content %}
  <h1>Books</h1>
  <ul class="books">
    {% for book in page_obj %}
      <li>
        <a href="{% url 'catalog:book-detail' book.slug %}">{{ book.title }}</a>
        by <a href="{% url 'catalog:author-detail' book.author.pk %}">{{ book.author }}</a>
        <span class="price">{{ book.price|floatformat:2 }} €</span>
      </li>
    {% empty %}
      <li>No books yet.</li>
    {% endfor %}
  </ul>
  {% include "catalog/pagination.html" %}
{% endblock %}
`,
  "catalog/templates/catalog/book_detail.html": `{% extends "base.html" %}
{% load static %}

{% block title %}{{ book.title }}{% endblock %}

{% block content %}
  <article class="book">
    <h1>{{ book.title }}</h1>
    <p>by {{ book.author.name }}, {{ book.published|date:"Y" }}</p>
    <form method="post" action="{% url 'orders:add' book.slug %}">
      {% csrf_token %}
      <button type="submit">Add to cart</button>
    </form>
  </article>
  {% include "catalog/reviews.html" with reviews=book.reviews.all %}
  <a href="{% url 'catalog:book-list' %}">All books</a>
{% endblock %}
`,
  "catalog/templates/catalog/reviews.html": `<section class="reviews">
  {% for review in reviews %}
    <blockquote>{{ review.text|linebreaksbr }} <cite>{{ review.user }} · {{ review.rating }}/5</cite></blockquote>
  {% empty %}
    <p>No reviews yet.</p>
  {% endfor %}
</section>
`,
  "catalog/templates/catalog/pagination.html": `{% if page_obj.has_other_pages %}
<nav class="pagination">
  {% if page_obj.has_previous %}<a href="?page={{ page_obj.previous_page_number }}">Previous</a>{% endif %}
  <span>Page {{ page_obj.number }} of {{ page_obj.paginator.num_pages }}</span>
  {% if page_obj.has_next %}<a href="?page={{ page_obj.next_page_number }}">Next</a>{% endif %}
</nav>
{% endif %}
`,
  "orders/__init__.py": "",
  "orders/apps.py": `from django.apps import AppConfig


class OrdersConfig(AppConfig):
    default_auto_field = "django.db.models.BigAutoField"
    name = "orders"
`,
  "orders/models.py": `from django.conf import settings
from django.db import models

from catalog.models import Book


class Order(models.Model):
    class Status(models.TextChoices):
        CART = "cart"
        PAID = "paid"
        SHIPPED = "shipped"

    user = models.ForeignKey(settings.AUTH_USER_MODEL, on_delete=models.CASCADE, related_name="orders")
    status = models.CharField(max_length=10, choices=Status.choices, default=Status.CART)
    created = models.DateTimeField(auto_now_add=True)


class OrderLine(models.Model):
    order = models.ForeignKey(Order, on_delete=models.CASCADE, related_name="lines")
    book = models.ForeignKey(Book, on_delete=models.PROTECT)
    quantity = models.PositiveIntegerField(default=1)
`,
  "orders/urls.py": `from django.urls import path

from . import views

app_name = "orders"

urlpatterns = [
    path("cart/", views.cart, name="cart"),
    path("cart/add/<slug:slug>/", views.add_to_cart, name="add"),
    path("<int:pk>/", views.OrderDetailView.as_view(), name="detail"),
]
`,
  "orders/views.py": `from django.contrib.auth.decorators import login_required
from django.shortcuts import get_object_or_404, redirect, render
from django.views.generic import DetailView

from catalog.models import Book

from .models import Order


@login_required
def cart(request):
    order, _ = Order.objects.get_or_create(user=request.user, status=Order.Status.CART)
    return render(request, "orders/cart.html", {"order": order})


@login_required
def add_to_cart(request, slug):
    book = get_object_or_404(Book, slug=slug)
    order, _ = Order.objects.get_or_create(user=request.user, status=Order.Status.CART)
    order.lines.create(book=book)
    return redirect("orders:cart")


class OrderDetailView(DetailView):
    model = Order
    template_name = "orders/order_detail.html"
`,
  "orders/templates/orders/cart.html": `{% extends "base.html" %}

{% block content %}
  <h1>Your cart</h1>
  <table>
    {% for line in order.lines.all %}
      <tr><td>{{ line.book.title }}</td><td>{{ line.quantity }}</td></tr>
    {% endfor %}
  </table>
  <a href="{% url 'catalog:book-list' %}">Continue shopping</a>
{% endblock %}
`,
  "orders/templates/orders/order_detail.html": `{% extends "base.html" %}

{% block content %}
  <h1>Order #{{ order.pk }}</h1>
  <p>Status: {{ order.get_status_display }}</p>
{% endblock %}
`,
  "templates/base.html": `{% load static %}
<!doctype html>
<html lang="en">
<head>
  <meta charset="utf-8">
  <title>{% block title %}Bookshop{% endblock %}</title>
  <link rel="stylesheet" href="{% static 'css/site.css' %}">
</head>
<body>
  <header>
    <a href="{% url 'home' %}">Bookshop</a>
    <nav>
      <a href="{% url 'catalog:book-list' %}">Books</a>
      <a href="{% url 'orders:cart' %}">Cart</a>
    </nav>
  </header>
  <main>{% block content %}{% endblock %}</main>
</body>
</html>
`,
  "templates/home.html": `{% extends "base.html" %}

{% block content %}
  <h1>New books</h1>
  <div class="grid">
    {% for book in latest %}
      {% include "catalog/card.html" %}
    {% endfor %}
  </div>
{% endblock %}
`,
  "templates/registration/login.html": `{% extends "base.html" %}

{% block content %}
  <form method="post">{% csrf_token %}{{ form.as_p }}<button>Log in</button></form>
{% endblock %}
`,
};

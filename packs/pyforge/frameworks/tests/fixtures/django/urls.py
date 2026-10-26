from django.contrib import admin
from django.urls import include, path, re_path

from shop import views

urlpatterns = [
    path("admin/", admin.site.urls),
    path("", views.home, name="home"),
    path("shop/", include("shop.urls")),
    path("api/v1/", include(([path("health/", views.health, name="health")], "api"))),
    re_path(r"^legacy/(?P<year>[0-9]{4})/$", views.archive, name="legacy-archive"),
]

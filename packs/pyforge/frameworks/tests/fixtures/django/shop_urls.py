from django.urls import path

from . import views

app_name = "shop"

urlpatterns = [
    path("", views.ProductListView.as_view(), name="product-list"),
    path("products/<int:pk>/", views.product_detail, name="product-detail"),
    path("cart/", views.cart, name="cart"),
]

from django.http import JsonResponse
from django.shortcuts import get_object_or_404, redirect, render
from django.views.generic import ListView

from .models import Product


def home(request):
    return render(request, "home.html", {"featured": Product.objects.all()[:4]})


class ProductListView(ListView):
    model = Product
    template_name = "shop/product_list.html"


def product_detail(request, pk):
    product = get_object_or_404(Product, pk=pk)
    return render(request, "shop/product_detail.html", {"product": product})


def cart(request):
    if not request.session.get("cart"):
        return redirect("shop:product-list")
    return render(request, "shop/cart.html")


def health(request):
    return JsonResponse({"status": "ok"})


def archive(request, year):
    return render(request, "shop/archive.html", {"year": year})

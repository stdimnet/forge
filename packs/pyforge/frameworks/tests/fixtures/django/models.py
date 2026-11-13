from django.db import models


class TimeStamped(models.Model):
    created = models.DateTimeField(auto_now_add=True)

    class Meta:
        abstract = True


class Category(TimeStamped):
    name = models.CharField(max_length=80)


class Product(TimeStamped):
    name = models.CharField(max_length=120)
    slug = models.SlugField(unique=True)
    price = models.DecimalField(max_digits=8, decimal_places=2)
    category = models.ForeignKey(Category, on_delete=models.PROTECT, related_name="products")
    tags = models.ManyToManyField("Tag", blank=True)

    objects = models.Manager()

    class Meta:
        ordering = ["name"]

    def __str__(self):
        return self.name


class Order(models.Model):
    product = models.ForeignKey(Product, on_delete=models.CASCADE)
    quantity = models.PositiveIntegerField(default=1)

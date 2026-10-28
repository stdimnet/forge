using Bookstore.Core.Catalog;
using FluentAssertions;

namespace Bookstore.Tests;

/// <summary>Cart totals. "[Fact]" in a comment is not a test.</summary>
public class CartTests
{
    private static readonly Book Dune = new("978-0441013593", "Dune", "Frank Herbert", 9.99m);

    [Fact]
    public void Total_IsZero_ForAnEmptyCart()
    {
        new Cart().Total.Should().Be(0m);
    }

    [Fact]
    public void Total_AddsLines()
    {
        var cart = new Cart();
        cart.Add(Dune, 2);
        cart.Total.Should().Be(19.98m);
    }

    [Theory]
    [InlineData(0)]
    [InlineData(-1)]
    public void Add_RejectsNonPositiveQuantities(int quantity)
    {
        var act = () => new Cart().Add(Dune, quantity);
        act.Should().Throw<ArgumentOutOfRangeException>();
    }

    [Fact(Skip = "Discounts are not implemented yet")]
    public async Task Total_AppliesVouchers()
    {
        await Task.CompletedTask;
    }

    public class WhenEmpty
    {
        [Fact]
        public void HasNoLines() => Assert.Equal(0m, new Cart().Total);
    }

    private void Helper() { }
}

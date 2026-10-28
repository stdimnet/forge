namespace Bookstore.Core.Catalog
{
    public class Cart
    {
        private readonly List<(Book Book, int Quantity)> _lines = new();

        public void Add(Book book, int quantity = 1)
        {
            if (quantity <= 0) throw new ArgumentOutOfRangeException(nameof(quantity));
            _lines.Add((book, quantity));
        }

        public decimal Total => _lines.Sum(l => l.Book.Price * l.Quantity);
    }
}

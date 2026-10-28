namespace Bookstore.Core.Catalog;

public sealed record Book(string Isbn, string Title, string Author, decimal Price);

public sealed class Inventory
{
    private readonly Dictionary<string, Book> _books = new();

    public IEnumerable<Book> All() => _books.Values;

    public Book? Find(string isbn) => _books.TryGetValue(isbn, out var b) ? b : null;

    public void Add(Book book) => _books[book.Isbn] = book;
}

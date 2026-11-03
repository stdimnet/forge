using Bookstore.Core.Catalog;
using Microsoft.EntityFrameworkCore;

namespace Bookstore.Data;

/// The bookstore's database.
public class BookstoreContext(DbContextOptions<BookstoreContext> options) : DbContext(options)
{
    public DbSet<Book> Books => Set<Book>();

    public DbSet<Author> Authors { get; set; } = null!;

    protected override void OnModelCreating(ModelBuilder model)
    {
        var books = model.Entity<Book>();
        books.HasKey(b => b.Id);
    }
}

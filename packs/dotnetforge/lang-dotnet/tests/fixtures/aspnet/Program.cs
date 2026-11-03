using Bookstore.Api;
using Bookstore.Core.Catalog;

var builder = WebApplication.CreateBuilder(args);
builder.Services.AddSingleton<Catalog>();
builder.Services.AddHealthChecks();

var app = builder.Build();

// The landing page.
app.MapGet("/", () => "Bookstore API");

var api = app.MapGroup("/api");
var books = api.MapGroup("/books").WithTags("Books");
books.MapGet("/", (Catalog catalog) => catalog.All());
books.MapGet("/{id:int}", (int id, Catalog catalog) =>
        catalog.Find(id) is { } book ? Results.Ok(book) : Results.NotFound())
    .WithName("GetBook");
books.MapPost("/", BookHandlers.Create);
books.MapDelete("/{id:int}", async (int id, Catalog catalog, CancellationToken ct) =>
{
    await catalog.RemoveAsync(id, ct);
    return Results.NoContent();
}).RequireAuthorization();

api.MapGroup("/authors").MapGet("/{authorId}/books", (string authorId) => Results.Ok());

app.MapMethods("/search", new[] { "GET", "HEAD" }, ([FromQuery] string q, int page) => Results.Ok());
app.MapHealthChecks("/health");
/* app.MapGet("/commented", () => 1); */
app.Run();

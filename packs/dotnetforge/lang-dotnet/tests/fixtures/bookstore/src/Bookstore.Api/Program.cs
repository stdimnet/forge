using Bookstore.Core.Catalog;

var builder = WebApplication.CreateBuilder(args);
builder.Services.AddEndpointsApiExplorer();
builder.Services.AddSwaggerGen();
builder.Services.AddSingleton<Inventory>();

var app = builder.Build();
app.UseSwagger();
app.UseSwaggerUI();

app.MapGet("/books", (Inventory inventory) => inventory.All());
app.MapGet("/books/{isbn}", (string isbn, Inventory inventory) =>
    inventory.Find(isbn) is { } book ? Results.Ok(book) : Results.NotFound());

app.Run();

var builder = DistributedApplication.CreateBuilder(args);

var cache = builder.AddRedis("cache");
var pg = builder.AddPostgres("pg")
    .WithDataVolume();
var db = pg.AddDatabase("bookstore");

builder.AddContainer("mail", "axllent/mailpit", "v1.20").WithHttpEndpoint(port: 8025, targetPort: 8025);

var api = builder.AddProject<Projects.Bookstore_Api>("api")
    .WithReference(db)
    .WithReference(cache)
    .WaitFor(pg)
    .WithExternalHttpEndpoints();

builder.AddNpmApp("web", "../web")
    .WithReference(api);

builder.Build().Run();

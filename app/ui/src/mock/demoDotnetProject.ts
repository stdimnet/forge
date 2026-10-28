/**
 * A small .NET solution for browser mode, opened with `?project=dotnet`: `Bookstore`, an
 * ASP.NET Core API (`Bookstore.Api`, launch profiles), a class library (`Bookstore.Core`,
 * two target frameworks), xUnit tests (`Bookstore.Tests`) and an F# tool (`Reports`), with
 * central package management, `global.json` and `.editorconfig`. The same files are
 * DotNetForge's test fixture (`packs/dotnetforge/lang-dotnet/tests/fixtures/bookstore`);
 * dotnetMock.ts answers the `dotnet_*` and `nuget_*` commands, running the Rust crates
 * through WebAssembly. `?build=broken` makes the build fail with compiler errors.
 */
import type { ProjectInfo } from "@/ipc/project";

export const dotnetDemoName = "Bookstore";

/** True when the page asks for the .NET demo project. */
export const isDotnetDemo = (): boolean =>
  typeof location !== "undefined" && new URLSearchParams(location.search).get("project") === "dotnet";

/** `?build=broken`: the build fails with compiler errors. */
export const dotnetBrokenBuild = (): boolean =>
  typeof location !== "undefined" && new URLSearchParams(location.search).get("build") === "broken";

export const dotnetDemoInfo: ProjectInfo = {
  languages: ["csharp", "fsharp"],
  frameworks: [{ id: "aspnetcore", name: "ASP.NET Core", language: "csharp", dir: "src/Bookstore.Api" }],
  packs: ["dotnetforge"],
  tools: ["git", "dotnet"],
  primary: { id: "aspnetcore", name: "ASP.NET Core", language: "csharp" },
};

export const dotnetDemoFiles: Record<string, string> = {
  ".editorconfig": `root = true

[*.cs]
indent_style = space
indent_size = 4
csharp_new_line_before_open_brace = all
dotnet_sort_system_directives_first = true
`,
  ".gitignore": `bin/
obj/
.vs/
*.user
TestResults/
`,
  "Bookstore.sln": `
Microsoft Visual Studio Solution File, Format Version 12.00
# Visual Studio Version 17
VisualStudioVersion = 17.9.34607.119
MinimumVisualStudioVersion = 10.0.40219.1
Project("{2150E333-8FDC-42A3-9474-1A3956D46DE8}") = "src", "src", "{8E1A2C70-0E4A-4C55-9B0E-3B1D1C7A0001}"
EndProject
Project("{9A19103F-16F7-4668-BE54-9A1E7A4F7556}") = "Bookstore.Api", "src\\Bookstore.Api\\Bookstore.Api.csproj", "{3F2B6C10-7A1D-4E0B-9C51-4B6E0D2A0101}"
EndProject
Project("{9A19103F-16F7-4668-BE54-9A1E7A4F7556}") = "Bookstore.Core", "src\\Bookstore.Core\\Bookstore.Core.csproj", "{3F2B6C10-7A1D-4E0B-9C51-4B6E0D2A0102}"
EndProject
Project("{2150E333-8FDC-42A3-9474-1A3956D46DE8}") = "tests", "tests", "{8E1A2C70-0E4A-4C55-9B0E-3B1D1C7A0002}"
EndProject
Project("{9A19103F-16F7-4668-BE54-9A1E7A4F7556}") = "Bookstore.Tests", "tests\\Bookstore.Tests\\Bookstore.Tests.csproj", "{3F2B6C10-7A1D-4E0B-9C51-4B6E0D2A0103}"
EndProject
Project("{F2A71F9B-5D33-465A-A702-920D77279786}") = "Reports", "tools\\Reports\\Reports.fsproj", "{3F2B6C10-7A1D-4E0B-9C51-4B6E0D2A0104}"
EndProject
Project("{2150E333-8FDC-42A3-9474-1A3956D46DE8}") = "Services", "Services", "{8E1A2C70-0E4A-4C55-9B0E-3B1D1C7A0003}"
EndProject
Global
	GlobalSection(SolutionConfigurationPlatforms) = preSolution
		Debug|Any CPU = Debug|Any CPU
		Release|Any CPU = Release|Any CPU
	EndGlobalSection
	GlobalSection(ProjectConfigurationPlatforms) = postSolution
		{3F2B6C10-7A1D-4E0B-9C51-4B6E0D2A0101}.Debug|Any CPU.ActiveCfg = Debug|Any CPU
		{3F2B6C10-7A1D-4E0B-9C51-4B6E0D2A0101}.Debug|Any CPU.Build.0 = Debug|Any CPU
	EndGlobalSection
	GlobalSection(NestedProjects) = preSolution
		{3F2B6C10-7A1D-4E0B-9C51-4B6E0D2A0101} = {8E1A2C70-0E4A-4C55-9B0E-3B1D1C7A0001}
		{3F2B6C10-7A1D-4E0B-9C51-4B6E0D2A0102} = {8E1A2C70-0E4A-4C55-9B0E-3B1D1C7A0001}
		{3F2B6C10-7A1D-4E0B-9C51-4B6E0D2A0103} = {8E1A2C70-0E4A-4C55-9B0E-3B1D1C7A0002}
		{8E1A2C70-0E4A-4C55-9B0E-3B1D1C7A0003} = {8E1A2C70-0E4A-4C55-9B0E-3B1D1C7A0001}
	EndGlobalSection
EndGlobal
`,
  "Bookstore.slnx": `<Solution>
  <Configurations>
    <BuildType Name="Debug" />
    <BuildType Name="Release" />
    <Platform Name="Any CPU" />
    <Platform Name="x64" />
  </Configurations>
  <Folder Name="/src/">
    <Project Path="src/Bookstore.Api/Bookstore.Api.csproj" />
    <Project Path="src/Bookstore.Core/Bookstore.Core.csproj" />
  </Folder>
  <Folder Name="/tests/">
    <Project Path="tests/Bookstore.Tests/Bookstore.Tests.csproj" />
  </Folder>
  <Project Path="tools/Reports/Reports.fsproj" />
</Solution>
`,
  "Directory.Build.props": `<Project>
  <PropertyGroup>
    <LangVersion>12.0</LangVersion>
    <Nullable>enable</Nullable>
    <ImplicitUsings>enable</ImplicitUsings>
    <Company>Acme Books</Company>
    <TreatWarningsAsErrors Condition="'$(Configuration)' == 'Release'">true</TreatWarningsAsErrors>
  </PropertyGroup>
</Project>
`,
  "Directory.Packages.props": `<Project>
  <PropertyGroup>
    <ManagePackageVersionsCentrally>true</ManagePackageVersionsCentrally>
  </PropertyGroup>
  <ItemGroup>
    <PackageVersion Include="Swashbuckle.AspNetCore" Version="6.5.0" />
    <PackageVersion Include="Serilog.AspNetCore" Version="8.0.0" />
    <PackageVersion Include="Microsoft.NET.Test.Sdk" Version="17.9.0" />
    <PackageVersion Include="xunit" Version="2.7.0" />
    <PackageVersion Include="xunit.runner.visualstudio" Version="2.5.7" />
    <PackageVersion Include="FluentAssertions" Version="6.12.0" />
  </ItemGroup>
</Project>
`,
  "README.md": `# Bookstore

An ASP.NET Core API for the Acme bookstore with a catalog library, xUnit tests and an F# report tool.

\`\`\`sh
dotnet build
dotnet test
dotnet run --project src/Bookstore.Api
\`\`\`
`,
  "global.json": `{
  // The SDK the solution was written with.
  "sdk": {
    "version": "8.0.400",
    "rollForward": "latestFeature",
    "allowPrerelease": false
  },
  "msbuild-sdks": {
    "Microsoft.Build.Traversal": "4.1.0"
  }
}
`,
  "src/Bookstore.Api/Bookstore.Api.csproj": `<Project Sdk="Microsoft.NET.Sdk.Web">

  <PropertyGroup>
    <TargetFramework>net8.0</TargetFramework>
    <RootNamespace>Bookstore.Api</RootNamespace>
    <UserSecretsId>b7c1e7a2-6f0e-4f53-9d55-3f0b1e5d9a11</UserSecretsId>
    <InvariantGlobalization>true</InvariantGlobalization>
  </PropertyGroup>

  <ItemGroup>
    <PackageReference Include="Swashbuckle.AspNetCore" />
    <PackageReference Include="Serilog.AspNetCore" VersionOverride="8.0.1" />
  </ItemGroup>

  <ItemGroup>
    <ProjectReference Include="..\\Bookstore.Core\\Bookstore.Core.csproj" />
  </ItemGroup>

</Project>
`,
  "src/Bookstore.Api/Program.cs": `using Bookstore.Core.Catalog;

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
`,
  "src/Bookstore.Api/Properties/launchSettings.json": `{
  "$schema": "https://json.schemastore.org/launchsettings.json",
  "iisSettings": {
    "windowsAuthentication": false,
    "anonymousAuthentication": true,
    "iisExpress": { "applicationUrl": "http://localhost:31412", "sslPort": 44312 }
  },
  "profiles": {
    "http": {
      "commandName": "Project",
      "dotnetRunMessages": true,
      "launchBrowser": true,
      "launchUrl": "swagger",
      "applicationUrl": "http://localhost:5180",
      "environmentVariables": {
        "ASPNETCORE_ENVIRONMENT": "Development"
      }
    },
    "https": {
      "commandName": "Project",
      "launchBrowser": false,
      "commandLineArgs": "--seed-data",
      "applicationUrl": "https://localhost:7180;http://localhost:5180",
      "environmentVariables": {
        "ASPNETCORE_ENVIRONMENT": "Development",
        "BOOKSTORE_CACHE": "memory"
      }
    },
    "IIS Express": {
      "commandName": "IISExpress",
      "launchBrowser": true,
      "environmentVariables": { "ASPNETCORE_ENVIRONMENT": "Development" }
    },
  }
}
`,
  "src/Bookstore.Api/appsettings.json": `{
  "Logging": {
    "LogLevel": {
      "Default": "Information",
      "Microsoft.AspNetCore": "Warning"
    }
  },
  "AllowedHosts": "*"
}
`,
  "src/Bookstore.Core/Bookstore.Core.csproj": `<Project Sdk="Microsoft.NET.Sdk">

  <PropertyGroup>
    <TargetFrameworks>net8.0;netstandard2.0</TargetFrameworks>
    <AssemblyName>Acme.Bookstore.Core</AssemblyName>
    <Version>1.4.0</Version>
    <Description>Catalog and cart of $(Company)</Description>
  </PropertyGroup>

  <ItemGroup Condition="'$(TargetFramework)' == 'netstandard2.0'">
    <PackageReference Include="System.Text.Json" Version="8.0.4" />
  </ItemGroup>

</Project>
`,
  "src/Bookstore.Core/Catalog/Book.cs": `namespace Bookstore.Core.Catalog;

public sealed record Book(string Isbn, string Title, string Author, decimal Price);

public sealed class Inventory
{
    private readonly Dictionary<string, Book> _books = new();

    public IEnumerable<Book> All() => _books.Values;

    public Book? Find(string isbn) => _books.TryGetValue(isbn, out var b) ? b : null;

    public void Add(Book book) => _books[book.Isbn] = book;
}
`,
  "src/Bookstore.Core/Catalog/Cart.cs": `namespace Bookstore.Core.Catalog
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
`,
  "tests/Bookstore.Tests/Bookstore.Tests.csproj": `<Project Sdk="Microsoft.NET.Sdk">

  <PropertyGroup>
    <TargetFramework>net8.0</TargetFramework>
    <IsPackable>false</IsPackable>
    <IsTestProject>true</IsTestProject>
  </PropertyGroup>

  <ItemGroup>
    <PackageReference Include="Microsoft.NET.Test.Sdk" />
    <PackageReference Include="xunit" />
    <PackageReference Include="xunit.runner.visualstudio">
      <IncludeAssets>runtime; build; native; contentfiles; analyzers; buildtransitive</IncludeAssets>
      <PrivateAssets>all</PrivateAssets>
    </PackageReference>
    <PackageReference Include="FluentAssertions" />
  </ItemGroup>

  <ItemGroup>
    <ProjectReference Include="..\\..\\src\\Bookstore.Core\\Bookstore.Core.csproj" />
  </ItemGroup>

  <ItemGroup>
    <Using Include="Xunit" />
  </ItemGroup>

</Project>
`,
  "tests/Bookstore.Tests/CartTests.cs": `using Bookstore.Core.Catalog;
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
`,
  "tests/Bookstore.Tests/InventoryTests.cs": `using Bookstore.Core.Catalog;

namespace Bookstore.Tests;

public class InventoryTests
{
    [Fact]
    public void Find_ReturnsAddedBooks()
    {
        var inventory = new Inventory();
        inventory.Add(new Book("978-0553293357", "Foundation", "Isaac Asimov", 8.99m));
        Assert.NotNull(inventory.Find("978-0553293357"));
    }

    [Fact]
    public void Find_ReturnsNull_ForUnknownIsbn() => Assert.Null(new Inventory().Find("nope"));
}
`,
  "tools/Reports/Program.fs": `open Bookstore.Reports.Sales

[<EntryPoint>]
let main argv =
    let sales = [ { Isbn = "978-0441013593"; Quantity = 3; Price = 9.99m } ]
    printfn "Revenue: %M" (revenue sales)
    0
`,
  "tools/Reports/Reports.fsproj": `<Project Sdk="Microsoft.NET.Sdk">

  <PropertyGroup>
    <OutputType>Exe</OutputType>
    <TargetFramework>net8.0</TargetFramework>
  </PropertyGroup>

  <ItemGroup>
    <Compile Include="Sales.fs" />
    <Compile Include="Program.fs" />
  </ItemGroup>

  <ItemGroup>
    <PackageReference Include="FSharp.Data" Version="6.4.0" />
    <PackageReference Update="FSharp.Core" Version="8.0.300" />
  </ItemGroup>

</Project>
`,
  "tools/Reports/Sales.fs": `module Bookstore.Reports.Sales

type Sale = { Isbn: string; Quantity: int; Price: decimal }

let revenue (sales: Sale list) =
    sales |> List.sumBy (fun s -> decimal s.Quantity * s.Price)
`,
};

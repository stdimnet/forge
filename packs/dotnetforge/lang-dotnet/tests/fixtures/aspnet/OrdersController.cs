using Microsoft.AspNetCore.Authorization;
using Microsoft.AspNetCore.Mvc;

namespace Bookstore.Api.Controllers;

[ApiController]
[Route("api/[controller]")]
public class OrdersController(IOrderStore store) : ControllerBase
{
    [HttpGet]
    public IEnumerable<Order> List([FromQuery] int page = 1) => store.Page(page);

    [HttpGet("{id:guid}", Name = "GetOrder")]
    public ActionResult<Order> Get(Guid id) => store.Find(id) is { } o ? o : NotFound();

    [HttpPost]
    [Authorize(Roles = "Clerk")]
    public IActionResult Create(NewOrder order, CancellationToken ct)
    {
        var created = store.Add(order);
        return CreatedAtAction(nameof(Get), new { id = created.Id }, created);
    }

    [HttpPut("{id:guid}/status")]
    public IActionResult SetStatus(Guid id, [FromBody] OrderStatus status) => NoContent();

    [HttpGet("[action]")]
    public FileResult Export() => File(Array.Empty<byte>(), "text/csv");

    [HttpGet("~/admin/stats/[action]")]
    public object Summary() => new { };

    private void Helper() { }

    [NonAction]
    public void NotAnEndpoint() { }
}

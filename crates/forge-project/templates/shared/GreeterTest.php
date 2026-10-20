<?php

declare(strict_types=1);

namespace {{namespace}}\Tests;

use {{namespace}}\Greeter;
use PHPUnit\Framework\TestCase;

final class GreeterTest extends TestCase
{
    public function testGreetsByName(): void
    {
        self::assertSame('Hello, Forge!', (new Greeter())->greet('Forge'));
    }
}

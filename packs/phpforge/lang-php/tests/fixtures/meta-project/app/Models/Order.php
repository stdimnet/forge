<?php

namespace App\Models;

use JetBrains\PhpStorm\ArrayShape;
use JetBrains\PhpStorm\Deprecated;
use JetBrains\PhpStorm\ExpectedValues;
use JetBrains\PhpStorm\Immutable;
use JetBrains\PhpStorm\Pure;
use App\Services\Mailer;

class Order
{
    public const ASC = 'asc';
    public const DESC = 'desc';

    #[Immutable]
    public string $number = '';

    public string $note = '';

    public function __construct(string $number)
    {
        $this->number = $number;
    }

    #[Deprecated(reason: 'Use pay() instead', replacement: '%class%->pay(%parametersList%)')]
    public function settle(int $amount): void
    {
    }

    public function pay(int $amount): void
    {
    }

    #[Pure]
    public function total(): int
    {
        return 0;
    }

    public function setStatus(string $status): void
    {
    }

    public function status(): string
    {
        return 'draft';
    }

    public function sortBy(string $column, string $direction): void
    {
    }

    public function priority(#[ExpectedValues(['low', 'normal', 'high'])] string $level): void
    {
    }

    #[ArrayShape(['id' => 'int', 'mailer' => Mailer::class, 'tags' => 'string[]'])]
    public function toArray(): array
    {
        return [];
    }
}

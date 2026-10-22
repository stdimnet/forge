<?php

use JetBrains\PhpStorm\NoReturn;

function app(string $id = null)
{
    return null;
}

function tap($value)
{
    return $value;
}

function first_of(array $items)
{
    return $items[0] ?? null;
}

#[NoReturn]
function abort(int $code): void
{
    exit($code);
}

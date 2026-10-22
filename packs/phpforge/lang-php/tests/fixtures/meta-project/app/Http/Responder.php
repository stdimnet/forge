<?php

namespace App\Http;

class Responder
{
    public static function fail(string $message): void
    {
        throw new \RuntimeException($message);
    }
}

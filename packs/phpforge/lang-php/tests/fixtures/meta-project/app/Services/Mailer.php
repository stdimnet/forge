<?php

namespace App\Services;

class Mailer
{
    public const HTML = 1;
    public const QUEUED = 2;

    public function send(string $to, int $options = 0): bool
    {
        return true;
    }
}

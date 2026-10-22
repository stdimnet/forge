<?php

namespace App;

use Psr\Container\ContainerInterface;

class Container implements ContainerInterface
{
    public function get(string $id)
    {
        return null;
    }

    public function has(string $id): bool
    {
        return false;
    }
}

<?php

namespace Acme\Toolkit;

class Registry
{
    public function make(string $class)
    {
        return new $class();
    }
}

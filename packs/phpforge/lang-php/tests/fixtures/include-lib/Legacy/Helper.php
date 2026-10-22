<?php

namespace Legacy;

class Helper
{
    public function shout(string $text): string
    {
        return strtoupper($text);
    }
}

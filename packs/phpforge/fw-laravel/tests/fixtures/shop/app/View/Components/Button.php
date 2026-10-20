<?php

namespace App\View\Components;

use Illuminate\View\Component;

class Button extends Component
{
    public function __construct(public string $variant = 'primary', public bool $isDisabled = false)
    {
    }

    public function render()
    {
        return view('components.button');
    }
}

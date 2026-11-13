<?php

namespace App\Http\Controllers;

class ShowProfile extends Controller
{
    public function __invoke()
    {
        return view('profile');
    }
}

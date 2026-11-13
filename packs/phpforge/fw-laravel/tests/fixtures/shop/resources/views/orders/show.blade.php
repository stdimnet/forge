@extends('layouts.app')

@section('content')
    <x-alert type="info" :message="__('messages.welcome')" />
    <x-forms.input name="email" />
    <h1>{{ $order->status }}</h1>
    @include('partials.footer')
@endsection

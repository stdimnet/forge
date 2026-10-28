# Third-party notices

Forge is MIT-licensed (see the repository's licence). Its binaries also contain the following
third-party works, each under its own licence. Rust crates from crates.io are listed with their
licences in `Cargo.lock` / `cargo metadata`; this file names the components whose notices must
travel with Forge.

## php-lsp

The built-in PHP language server (`packs/phpforge/builtin-lsp`), linked into `forge` and
`forge-remote-agent`: https://github.com/stdimnet/php-lsp, revision
`0fd60bc7f242112582544e4dfa0bd51232268287`, licensed under the MIT License **or** the Apache
License 2.0, at your option. Its own notices:

### JetBrains phpstorm-stubs

The PHP stubs embedded by php-lsp's `php-stubs` crate are taken from
[JetBrains/phpstorm-stubs](https://github.com/JetBrains/phpstorm-stubs) (commit
`912ae721089fc48534d3a5795997ee8a2802b8cd`), licensed under the Apache License 2.0
(https://www.apache.org/licenses/LICENSE-2.0).

### js-beautify

php-lsp's `php-fmt` crate contains a Rust port of the HTML, CSS and JavaScript beautifiers of
[js-beautify](https://github.com/beautifier/js-beautify) 1.15.1.

> The MIT License (MIT)
>
> Copyright (c) 2007-2018 Einar Lielmanis, Liam Newman, and contributors.
>
> Permission is hereby granted, free of charge, to any person obtaining a copy
> of this software and associated documentation files (the "Software"), to deal
> in the Software without restriction, including without limitation the rights
> to use, copy, modify, merge, publish, distribute, sublicense, and/or sell
> copies of the Software, and to permit persons to whom the Software is
> furnished to do so, subject to the following conditions:
>
> The above copyright notice and this permission notice shall be included in
> all copies or substantial portions of the Software.
>
> THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF ANY KIND, EXPRESS OR
> IMPLIED, INCLUDING BUT NOT LIMITED TO THE WARRANTIES OF MERCHANTABILITY,
> FITNESS FOR A PARTICULAR PURPOSE AND NONINFRINGEMENT. IN NO EVENT SHALL THE
> AUTHORS OR COPYRIGHT HOLDERS BE LIABLE FOR ANY CLAIM, DAMAGES OR OTHER
> LIABILITY, WHETHER IN AN ACTION OF CONTRACT, TORT OR OTHERWISE, ARISING FROM,
> OUT OF OR IN CONNECTION WITH THE SOFTWARE OR THE USE OR OTHER DEALINGS IN THE
> SOFTWARE.

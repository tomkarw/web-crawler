# Web Crawler
Simple binary that allows recursively crawling a webpage, while searching for a keyword.

Multiple pages are crawled efficiently and concurrently

## Usage 

```bash
$ cargo run -- --help
web-crawler 0.2.0


USAGE:
    web-crawler [OPTIONS] <URL> <QUERY>

ARGS:
    <URL>      Url to crawl
    <QUERY>    Query to match on visited pages

OPTIONS:
    -d, --depth <DEPTH>    Crawling recursion depth [default: 2]
    -h, --help             Print help information
        --range <RANGE>    Size of context surrounding matched phrase [default: 3]
    -V, --version          Print version information
```

## Example

```bash
$ cargo run https://www.rust-lang.org/ "type system" --range 7
https://www.rust-lang.org/ => 'Rust’s rich type system and ow'
https://www.rust-lang.org/what/embedded => 'icated type system help u'
https://www.rust-lang.org/what/networking => 'p. Its type system allows'
Failed to parse page: deadline has elapsed
...
Crawled 17 pages. Found 3 pages with the term `type system`.
```
use clap::{arg, Arg, Command};

use reqwest::Url;
use scraper::{Html, Selector};
use select::document::Document;
use select::predicate::Name;
use std::collections::{HashMap, HashSet};
use std::error::Error;
use std::io;
use std::io::Write;

fn main() -> Result<(), Box<dyn Error>> {
    let arg_matches = Command::new(env!("CARGO_PKG_NAME"))
        .version(env!("CARGO_PKG_VERSION"))
        .author(env!("CARGO_PKG_AUTHORS"))
        .arg(Arg::new("URL").required(true).help("url to crawl").index(1))
        .arg(
            Arg::new("QUERY")
                .required(true)
                .help("query to match on visited pages")
                .index(2),
        )
        .arg(arg!(-d --depth <DEPTH>).required(false))
        .arg(arg!(--range <RANGE>).required(false))
        .get_matches();

    let url = arg_matches.value_of("URL").unwrap();
    let query = arg_matches.value_of("QUERY").unwrap();
    let depth = arg_matches
        .value_of("depth")
        .unwrap_or("")
        .parse::<usize>()
        .unwrap_or(2);
    let range = arg_matches
        .value_of("range")
        .unwrap_or("")
        .parse::<usize>()
        .unwrap_or(3);

    let (link_count, matches) = crawl_page(Url::parse(url)?, query, depth, range)?;

    let stdout = io::stdout();
    let mut stdout = stdout.lock();
    stdout.write_all(
        (&format!(
            "Crawled {} pages. Found {} pages with the term `{}`\n",
            link_count,
            matches.len(),
            query
        ))
            .as_ref(),
    )?;
    for (url, snippet) in matches {
        stdout.write_all((&format!("{} => {}\n", url, snippet)).as_ref())?;
    }

    Ok(())
}

fn crawl_page(
    url: Url,
    query: &str,
    depth: usize,
    range: usize,
) -> Result<(usize, HashMap<Url, String>), Box<dyn Error>> {
    if depth == 0 {
        return Ok((0, HashMap::new()));
    }

    // TODO: this is very naive and slow approach which could definitely use async
    //  had I more time, I sketched out an approach with
    //  async threadpool, link_queue and results map
    let response = reqwest::blocking::get(url.clone())?;
    // TODO: handle redirects
    if !response.status().is_success() {
        return Ok((0, HashMap::new()));
    }
    let body = response.text()?;

    let mut visited_count = 1;
    let mut matches = HashMap::new();

    if let Some(matched) = find_query(&body, query, range) {
        matches.insert(url.clone(), matched);
    }

    let links = Document::from(body.as_str())
        .find(Name("a"))
        .filter_map(|anchor| anchor.attr("href"))
        .filter_map(|raw_url| {
            match Url::parse(raw_url) {
                Ok(new_url) => {
                    // ignore pages on different domain
                    if new_url.host() == url.host() {
                        Some(new_url)
                    } else {
                        None
                    }
                }
                Err(_) => {
                    // TODO: could there be relative links?
                    if raw_url.starts_with('/') {
                        url.join(raw_url).ok()
                    } else {
                        None
                    }
                }
            }
        })
        .collect::<HashSet<_>>();

    for link in links {
        // don't visit already visited links
        if matches.contains_key(&link) {
            continue;
        }

        let (child_visited_count, child_matches) = crawl_page(link, query, depth - 1, range)?;
        visited_count += child_visited_count;

        // only add new links
        for (child_url, child_match) in child_matches {
            matches.entry(child_url).or_insert(child_match);
        }
    }

    Ok((visited_count, matches))
}

// TODO: only search visible text
fn find_query(html: &str, query: &str, range: usize) -> Option<String> {
    let fragment = Html::parse_fragment(html);
    let selector = Selector::parse("html").unwrap();
    let body = match fragment.select(&selector).next() {
        None => return None,
        Some(body) => body,
    };
    let text = body.text().collect::<Vec<_>>().join("");
    text.find(query)
        .map(|cursor| text[cursor - range..cursor + query.len() + range].to_string())
}

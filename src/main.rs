use reqwest;
use select::document::Document;
use select::predicate::Name;
use std::collections::HashSet;
use std::error::Error;
use std::io;
use std::io::Write;
use reqwest::Url;

const RANGE: usize = 3;
const URL: &str = "https://www.layer0.co/";
const QUERY: &str = "website";

fn main() -> Result<(), Box<dyn Error>> {
    let (link_count, matches) = crawl_page(Url::parse(URL)?, QUERY, 2)?;

    let stdout = io::stdout();
    let mut stdout = stdout.lock();
    stdout.write_all((&format!("Crawled {} pages. Found {} pages with the term `{}`\n", link_count, matches.len(), QUERY)).as_ref())?;
    for (url, snippet) in matches {
        stdout.write_all((&format!("{} => {}\n", url, snippet)).as_ref())?;
    }

    Ok(())
}

fn crawl_page(url: Url, query: &str, depth: usize) -> Result<(usize, Vec<(Url, String)>), Box<dyn Error>> {
    if depth == 0 {
        return Ok((0, Vec::new()));
    }

    let response = reqwest::blocking::get(url.clone())?;
    // TODO: handle redirects
    if !response.status().is_success() {
        return Ok((0, Vec::new()));
    }
    let body = response.text()?;

    let mut link_count = 1;
    let mut matches = Vec::new();
    if let Some(cursor) = body.find(query) {
        matches.push((url.clone(), body[cursor-RANGE..cursor+query.len()+RANGE].to_string()));
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
                },
                Err(_) => {
                    // TODO: could there be relative links?
                    if raw_url.starts_with("/") {
                        url.join(raw_url).ok()
                    } else {
                        None
                    }
                }
            }
        })
        .collect::<HashSet<_>>();

    for link in links {
        let (child_link_count, mut child_matches) = crawl_page(link, query, depth - 1)?;
        link_count += child_link_count;
        matches.append(&mut child_matches);
    }

    Ok((link_count, matches))

}
use clap::{arg, Arg, Command};
use concurrent_queue::ConcurrentQueue;
use reqwest::Url;
use scraper::{Html, Selector};
use select::document::Document;
use select::predicate::Name;
use std::collections::{HashMap, HashSet};
use std::error::Error;
use std::io;
use std::io::Write;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::{RwLock};
use tokio::task::JoinHandle;

type QueryMatches = Arc<RwLock<HashMap<Url, String>>>;

const TIMEOUT: Duration = Duration::from_secs(5);

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>> {
    let args = Command::new(env!("CARGO_PKG_NAME"))
        .version(env!("CARGO_PKG_VERSION"))
        .author(env!("CARGO_PKG_AUTHORS"))
        .arg(Arg::new("URL").required(true).help("Url to crawl").index(1))
        .arg(
            Arg::new("QUERY")
                .required(true)
                .help("Query to match on visited pages")
                .index(2),
        )
        .arg(
            arg!(-d --depth <DEPTH> "Crawling recursion depth")
                .required(false)
                .default_value("2"),
        )
        .arg(
            arg!(--range <RANGE> "Size of context surrounding matched phrase")
                .required(false)
                .default_value("3"),
        )
        .get_matches();

    let url = args.value_of("URL").unwrap();
    let query = args.value_of("QUERY").unwrap();
    let depth = args
        .value_of("depth")
        .unwrap()
        .parse::<usize>()
        .unwrap_or(2);
    let range = args
        .value_of("range")
        .unwrap()
        .parse::<usize>()
        .unwrap_or(3);

    let link_count = Arc::new(AtomicUsize::new(0));
    let query_matches = QueryMatches::new(Default::default());

    let handle_queue = Arc::new(ConcurrentQueue::unbounded());
    handle_queue.push(crawl_page(
        Url::parse(url)?,
        query.to_string(),
        depth,
        range,
        link_count.clone(),
        query_matches.clone(),
        handle_queue.clone(),
    )).unwrap();

    while let Ok(handle) = handle_queue.pop() {
        tokio::join!(handle).0.unwrap();
    }

    let stdout = io::stdout();
    let mut stdout = stdout.lock();
    stdout.write_all(
        (&format!(
            "Crawled {} pages. Found {} pages with the term `{}`.\n",
            link_count.load(Ordering::SeqCst),
            query_matches.read().await.len(),
            query
        ))
            .as_ref(),
    )?;
    for (url, snippet) in query_matches.read().await.iter() {
        stdout.write_all((&format!("{} => {}\n", url, snippet)).as_ref())?;
    }

    Ok(())
}

fn crawl_page(
    url: Url,
    query: String,
    depth: usize,
    range: usize,
    link_count: Arc<AtomicUsize>,
    query_matches: QueryMatches,
    handle_queue: Arc<ConcurrentQueue<JoinHandle<()>>>,
) -> JoinHandle<()> {
    tokio::spawn(async move {
        if depth == 0 {
            return; // Ok(());
        }

        // TODO: make sure what ordering is needed
        link_count.fetch_add(1, Ordering::AcqRel);

        let response = reqwest::get(url.clone()).await.unwrap();
        // TODO: handle redirects
        if !response.status().is_success() {
            return; // Ok(());
        }
        let body = response.text().await.unwrap();

        if let Some(matched) = find_query(&body, &query, range) {
            query_matches.write().await.insert(url.clone(), matched);
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
            if query_matches.read().await.contains_key(&link) {
                continue;
            }

            // link_queue_sender.send((link, depth - 1))?;
            handle_queue.push(crawl_page(
                link,
                query.clone(),
                depth - 1,
                range,
                link_count.clone(),
                query_matches.clone(),
                handle_queue.clone(),
            )).unwrap();
        }

        // Ok(())
    })
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
    text.find(query).map(|cursor| {
        let start = if cursor as isize - range as isize > 0 {
            cursor - range
        } else {
            0
        };
        let end = if cursor + query.len() + range > text.len() {
            text.len()
        } else {
            cursor + query.len() + range
        };
        text[start..end].to_string()
    })
}

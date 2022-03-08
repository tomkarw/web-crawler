use clap::{arg, Arg, Command};

use futures_util::StreamExt;
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
use tokio::sync::mpsc::UnboundedSender;
use tokio::sync::{mpsc, RwLock};
use tokio_stream::wrappers::UnboundedReceiverStream;

type QueryMatches = Arc<RwLock<HashMap<Url, String>>>;

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>> {
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

    let link_count = Arc::new(AtomicUsize::new(0));
    let query_matches = QueryMatches::new(Default::default());
    let (link_queue_sender, link_queue_receiver) = mpsc::unbounded_channel::<(Url, usize)>();
    let mut link_queue_receiver = UnboundedReceiverStream::new(link_queue_receiver);

    let query_clone = query.to_string();
    let link_count_clone = link_count.clone();
    let query_matches_clone = query_matches.clone();
    let link_queue_sender_clone = link_queue_sender.clone();
    tokio::task::spawn(async move {
        let query = query_clone.clone();
        let link_count = link_count_clone.clone();
        let query_matches = query_matches_clone.clone();
        let link_queue_sender = link_queue_sender_clone.clone();
        while let Some((url, depth)) = link_queue_receiver.next().await {
            let query = query.clone();
            let link_count = link_count.clone();
            let query_matches = query_matches.clone();
            let link_queue_sender = link_queue_sender.clone();
            tokio::task::spawn(async move {
                crawl_page(
                    url,
                    &query,
                    depth,
                    range,
                    link_count,
                    query_matches,
                    link_queue_sender,
                )
                .await
                .unwrap();
            });
        }
    });

    link_queue_sender.send((Url::parse(url)?, depth))?;

    // TODO: block until link_queue_receiver queue is empty and no more tasks are running.
    //  Sleeping here is terrible, but at least we get some results.
    tokio::time::sleep(Duration::from_secs(5)).await;

    let stdout = io::stdout();
    let mut stdout = stdout.lock();
    stdout.write_all(
        (&format!(
            "Crawled {} pages. Found {} pages with the term `{}`\n",
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

async fn crawl_page(
    url: Url,
    query: &str,
    depth: usize,
    range: usize,
    link_count: Arc<AtomicUsize>,
    query_matches: QueryMatches,
    link_queue_sender: UnboundedSender<(Url, usize)>,
) -> Result<(), Box<dyn Error>> {
    if depth == 0 {
        return Ok(());
    }

    // TODO: make sure what ordering is needed
    link_count.fetch_add(1, Ordering::AcqRel);

    let response = reqwest::get(url.clone()).await?;
    // TODO: handle redirects
    if !response.status().is_success() {
        return Ok(());
    }
    let body = response.text().await?;

    if let Some(matched) = find_query(&body, query, range) {
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

        link_queue_sender.send((link, depth - 1))?;
    }

    Ok(())
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

use anyhow::{Context, Result};
use clap::Parser;
use indicatif::{ProgressBar, ProgressStyle};
use reqwest::Client;
use serde::Deserialize;

#[derive(Parser)]
#[command(about = "Delete Firefly III transactions by tag")]
struct Args {
    /// Base URL of the Firefly III instance (e.g. https://firefly.example.com)
    #[arg(long)]
    url: String,

    /// Personal access token for authentication
    #[arg(long)]
    token: String,

    /// Tag to filter transactions by
    #[arg(long)]
    tag: String,

    /// List transactions that would be deleted without actually deleting them
    #[arg(long)]
    dry_run: bool,
}

#[derive(Deserialize)]
struct TagArray {
    data: Vec<TagRead>,
    meta: Meta,
}

#[derive(Deserialize)]
struct TagRead {
    id: String,
    attributes: TagModel,
}

#[derive(Deserialize)]
struct TagModel {
    tag: String,
}

#[derive(Deserialize)]
struct TransactionArray {
    data: Vec<TransactionRead>,
    meta: Meta,
}

#[derive(Deserialize)]
struct TransactionRead {
    id: String,
    attributes: Transaction,
}

#[derive(Deserialize)]
struct Transaction {
    transactions: Vec<TransactionSplit>,
}

#[derive(Deserialize)]
struct TransactionSplit {
    description: String,
    date: String,
    amount: String,
}

#[derive(Deserialize)]
struct Meta {
    pagination: Pagination,
}

#[derive(Deserialize)]
struct Pagination {
    total: u64,
    total_pages: u32,
}

/// Fetch all tags from the server and return the numeric ID of the tag matching `name`.
async fn resolve_tag_id(client: &Client, base_url: &str, token: &str, name: &str) -> Result<String> {
    let url = format!("{}/api/v1/tags", base_url);
    let mut page = 1u32;
    loop {
        let resp = client
            .get(&url)
            .bearer_auth(token)
            .query(&[("page", page)])
            .send()
            .await
            .context("Failed to fetch tags")?;

        if !resp.status().is_success() {
            let status = resp.status();
            let body = resp.text().await.unwrap_or_default();
            anyhow::bail!("HTTP {} fetching tags: {}", status, body);
        }

        let tag_array = resp.json::<TagArray>().await.context("Failed to parse tags response")?;

        if let Some(found) = tag_array.data.into_iter().find(|t| t.attributes.tag == name) {
            return Ok(found.id);
        }

        if page >= tag_array.meta.pagination.total_pages {
            break;
        }
        page += 1;
    }

    anyhow::bail!("Tag '{}' not found", name);
}

async fn fetch_transactions_page(
    client: &Client,
    base_url: &str,
    token: &str,
    tag_id: &str,
    page: u32,
) -> Result<TransactionArray> {
    let url = format!("{}/api/v1/tags/{}/transactions", base_url, tag_id);
    let resp: reqwest::Response = client
        .get(&url)
        .bearer_auth(token)
        .query(&[("page", page)])
        .send()
        .await
        .context("Failed to send request")?;

    if !resp.status().is_success() {
        let status = resp.status();
        let body = resp.text().await.unwrap_or_default();
        anyhow::bail!("HTTP {}: {}", status, body);
    }

    resp.json::<TransactionArray>()
        .await
        .context("Failed to parse response")
}

async fn delete_transaction(client: &Client, base_url: &str, token: &str, id: &str) -> Result<()> {
    let url = format!("{}/api/v1/transactions/{}", base_url, id);
    let resp: reqwest::Response = client
        .delete(&url)
        .bearer_auth(token)
        .send()
        .await
        .context("Failed to send delete request")?;

    if !resp.status().is_success() {
        let status = resp.status();
        let body = resp.text().await.unwrap_or_default();
        anyhow::bail!("HTTP {}: {}", status, body);
    }

    Ok(())
}

#[tokio::main]
async fn main() -> Result<()> {
    let args = Args::parse();
    let base_url = args.url.trim_end_matches('/');
    let client = Client::new();

    let tag_id = resolve_tag_id(&client, base_url, &args.token, &args.tag).await?;

    let first_page = fetch_transactions_page(&client, base_url, &args.token, &tag_id, 1).await?;
    let total = first_page.meta.pagination.total;
    let total_pages = first_page.meta.pagination.total_pages;

    if total == 0 {
        println!("No transactions found with tag '{}'.", args.tag);
        return Ok(());
    }

    let verb = if args.dry_run {
        "Would delete"
    } else {
        "Deleting"
    };
    let pb = ProgressBar::new(total);
    pb.set_style(ProgressStyle::with_template("{msg}\n{bar:40} {pos}/{len} ({eta})").unwrap());
    pb.set_message(format!(
        "{} {} transaction(s) with tag '{}'",
        verb, total, args.tag
    ));

    let process_page =|txns: Vec<TransactionRead>| async {
        for txn in txns {
            if !args.dry_run {
                delete_transaction(&client, base_url, &args.token, &txn.id).await?;
            }
            pb.inc(1);
            if let Some(split) = txn.attributes.transactions.first() {
                pb.set_message(format!(
                    "{} {} transaction(s) with tag '{}'\n  {} | {} | {}",
                    verb, total, args.tag, split.date, split.description, split.amount
                ));
            }
        }
        Ok::<(), anyhow::Error>(())
    };

    if args.dry_run {
        // Data isn't changing, so normal sequential pagination is stable.
        process_page(first_page.data).await?;
        for page in 2..=total_pages {
            let page_data =
                fetch_transactions_page(&client, base_url, &args.token, &tag_id, page).await?;
            process_page(page_data.data).await?;
        }
    } else {
        // After deleting page 1, what was page 2 shifts down to page 1.
        // Always re-fetch page 1 until the server returns nothing.
        process_page(first_page.data).await?;
        loop {
            let page_data =
                fetch_transactions_page(&client, base_url, &args.token, &tag_id, 1).await?;
            if page_data.data.is_empty() {
                break;
            }
            process_page(page_data.data).await?;
        }
    }

    pb.finish_with_message(format!(
        "{} {} transaction(s) with tag '{}'.",
        if args.dry_run {
            "Would have deleted"
        } else {
            "Deleted"
        },
        total,
        args.tag
    ));

    Ok(())
}

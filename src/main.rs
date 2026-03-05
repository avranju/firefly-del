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

async fn fetch_transactions_page(
    client: &Client,
    base_url: &str,
    token: &str,
    tag: &str,
    page: u32,
) -> Result<TransactionArray> {
    let url = format!("{}/api/v1/tags/{}/transactions", base_url, tag);
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

    let first_page = fetch_transactions_page(&client, base_url, &args.token, &args.tag, 1).await?;
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

    for txn in first_page.data {
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

    for page in 2..=total_pages {
        let page_data =
            fetch_transactions_page(&client, base_url, &args.token, &args.tag, page).await?;
        for txn in page_data.data {
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

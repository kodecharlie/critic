mod config;
mod renderer;

use clap::{Parser, Subcommand, Args};
use config::Config;
use renderer::{RenderBus, RenderEvent, run_renderer_consumer};

#[derive(Parser)]
#[command(name = "critic", version = "0.1.0", about = "AI-powered Apple App Review Insights CLI")]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Initialize vector db and ingest reviews for an app
    Init { app_name: String },
    /// Update existing database with latest reviews
    Update { app_name: String },
    /// Print aggregate review summary
    Summary {
        app_name: String,
        #[arg(long)]
        begin: Option<String>,
        #[arg(long)]
        end: Option<String>,
    },
    /// Surface and rank software bugs from reviews
    Bugs {
        app_name: String,
        #[arg(long)]
        begin: Option<String>,
        #[arg(long)]
        end: Option<String>,
        #[arg(long)]
        json: bool,
        #[arg(long)]
        bugtracker: Option<String>,
    },
    /// Tally review counts over time buckets
    Counts {
        app_name: String,
        #[arg(long)]
        begin: Option<String>,
        #[arg(long)]
        end: Option<String>,
        #[arg(long)]
        ascending: bool,
        #[arg(long)]
        descending: bool,
        #[arg(long, default_value = "90")]
        binsize: i64,
    },
    /// Gauge aggregate user sentiment over time
    Sentiment {
        app_name: String,
        #[arg(long)]
        begin: Option<String>,
        #[arg(long)]
        end: Option<String>,
        #[arg(long)]
        ascending: bool,
        #[arg(long)]
        descending: bool,
        #[arg(long, default_value = "90")]
        binsize: i64,
    },
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let _config = Config::load()?;
    let cli = Cli::parse();

    let (bus, receiver) = RenderBus::new();
    let consumer_handle = tokio::spawn(run_renderer_consumer(receiver));

    match cli.command {
        Commands::Init { app_name } => {
            println!("Initializing data pipeline and embedding store for app: {}...", app_name);
            // 1. Query Apple API
            // 2. Scrub PII/Spurious chars
            // 3. Generate embeddings via Ollama (nomic-embed-text) and persist to pgvector
        }
        Commands::Update { app_name } => {
            println!("Fetching incremental reviews and updating embeddings for {}...", app_name);
        }
        Commands::Summary { app_name, begin, end } => {
            // Fetch reviews from vector DB bounded by begin/end, compute stats via LLM/aggregates
            let sample_data = format!("Weighted Rating: 4.6 / 5.0 (Total Ratings: 1,420). Range: {:?} to {:?}", begin, end);
            bus.publish(RenderEvent::SummaryOutput { app: app_name, data: sample_data }).await;
        }
        Commands::Bugs { app_name, begin: _, end: _, json, bugtracker: _ } => {
            // Query LLM for bug identification and severity ranking formula: frequency * proximity
            let sample_bugs = vec![
                renderer::BugItem { description: "Crash on launch when offline".into(), severity: 0.89, status: Some("OPEN".into()) },
                renderer::BugItem { description: "Dark mode color contrast bug in settings".into(), severity: 0.42, status: Some("CLOSED".into()) },
            ];
            bus.publish(RenderEvent::BugsOutput { app: app_name, bugs: sample_bugs, json_flag: json }).await;
        }
        Commands::Counts { app_name, begin: _, end: _, ascending: _, descending: _, binsize: _ } => {
            let sample_counts = vec![("Q1 2024".into(), 120), ("Q2 2024".into(), 240), ("Q3 2024".into(), 180)];
            bus.publish(RenderEvent::CountsOutput { app: app_name, series: sample_counts }).await;
        }
        Commands::Sentiment { app_name, begin: _, end: _, ascending: _, descending: _, binsize: _ } => {
            let sample_sentiment = vec![("Q1 2024".into(), 0.75), ("Q2 2024".into(), 0.82), ("Q3 2024".into(), 0.68)];
            bus.publish(RenderEvent::SentimentOutput { app: app_name, series: sample_sentiment }).await;
        }
    }

    // Give consumer time to process final render events before exit
    tokio::time::sleep(tokio::time::Duration::from_millis(50)).await;
    drop(bus);
    let _ = consumer_handle.await;

    Ok(())
}
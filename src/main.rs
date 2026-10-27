mod config;
mod renderer;
mod pipeline;
mod summary;
mod bugs;
mod counts;
mod sentiment;

use clap::{Parser, Subcommand};
use config::Config;
use renderer::{RenderBus, run_renderer_consumer};

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
            pipeline::run_init(&_config, &app_name).await.unwrap();
        }
        Commands::Update { app_name } => {
            println!("Fetching incremental reviews and updating embeddings for {}...", app_name);
            pipeline::run_update(&_config, &app_name).await.unwrap();
        }
        Commands::Summary { app_name, begin, end } => {
            println!("Fetching reviews from vector DB bounded by begin/end, compute stats via LLM/aggregates");
            summary::run_summary(&_config, &app_name, begin, end, &bus).await.unwrap();
        }
        Commands::Bugs { app_name, begin, end, json, bugtracker } => {
            println!("Query LLM for bug identification and severity ranking formula: frequency * proximity");
            bugs::run_bugs(&_config, &app_name, begin, end, json, bugtracker, &bus).await.unwrap();
        }
        Commands::Counts { app_name, begin, end, ascending, descending, binsize } => {
            println!("Tallying review counts over time buckets");
            counts::run_counts(&_config, &app_name, begin, end, ascending, descending, binsize, &bus).await.unwrap();
        }
        Commands::Sentiment { app_name, begin, end, ascending, descending, binsize } => {
            println!("Gauge aggregate user sentiment over time");
            sentiment::run_sentiment(&_config, &app_name, begin, end, ascending, descending, binsize, &bus).await.unwrap();
        }
    }

    // Give consumer time to process final render events before exit
    tokio::time::sleep(tokio::time::Duration::from_millis(50)).await;
    drop(bus);
    let _ = consumer_handle.await;

    Ok(())
}
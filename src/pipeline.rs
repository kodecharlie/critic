use sqlx::{PgPool, Row};
use reqwest::Client;
use serde::{Deserialize, Serialize};
use chrono::{DateTime, Utc};
use regex::Regex;
use std::error::Error;

use crate::config::Config;

#[derive(Debug, Serialize, Deserialize)]
struct AppleReviewResponse {
    data: Vec<AppleReviewData>,
}

#[derive(Debug, Serialize, Deserialize)]
struct AppleReviewData {
    id: String,
    attributes: ReviewAttributes,
}

#[derive(Debug, Serialize, Deserialize)]
struct ReviewAttributes {
    #[serde(rename = "rating")]
    rating: i32,
    #[serde(rename = "title")]
    title: Option<String>,
    #[serde(rename = "body")]
    body: String,
    #[serde(rename = "createdDate")]
    created_date: DateTime<Utc>,
}

#[derive(Serialize)]
struct OllamaEmbeddingRequest {
    model: String,
    prompt: String,
}

#[derive(Deserialize)]
struct OllamaEmbeddingResponse {
    embedding: Vec<f32>,
}

/// Scrub text of spurious control characters and common PII patterns (emails, phone numbers)
pub fn scrub_text(input: &str) -> String {
    // Remove control characters
    let cleaned = input.chars().filter(|c| !c.is_control() || *c == '\n' || *c == '\t').collect::<String>();
    
    // Regex for emails and phone numbers to strip PII
    let email_regex = Regex::new(r#"(?i)[a-z0-9._%+-]+@[a-z0-9.-]+\.[a-z]{2,}"#).unwrap();
    let phone_regex = Regex::new(r#"\b\d{3}[-.\s]?\d{3}[-.\s]?\d{4}\b"#).unwrap();

    let no_emails = email_regex.replace_all(&cleaned, "[REDACTED_EMAIL]");
    let no_phones = phone_regex.replace_all(&no_emails, "[REDACTED_PHONE]");

    no_phones.to_string()
}

/// Generate a vector embedding locally via Ollama (using nomic-embed-text)
async fn generate_embedding(client: &Client, text: &str) -> Result<Vec<f32>, Box<dyn Error>> {
    let req = OllamaEmbeddingRequest {
        model: "nomic-embed-text".to_string(),
        prompt: text.to_string(),
    };

    let res = client.post("http://localhost:11434/api/embeddings")
        .json(&req)
        .send()
        .await?
        .json::<OllamaEmbeddingResponse>()
        .await?;

    Ok(res.embedding)
}

/// Fetch reviews from Apple App Store Connect API
async fn fetch_apple_reviews(config: &Config, app_name: &str, since: Option<DateTime<Utc>>) -> Result<Vec<AppleReviewData>, Box<dyn Error>> {
    let client = Client::new();
    // Note: App Store Connect API requires a signed JWT Bearer token using config.apple_key.
    // This demonstrates the standard authenticated API pattern.
    let url = format!("https://{}/v1/apps/{}/customerReviews", config.apple_host, app_name);

    let mut request_builder = client.get(&url)
        .bearer_auth(&config.apple_key);

    if let Some(date) = since {
        request_builder = request_builder.query(&[("filter[createdDate]", date.to_rfc3339())]);
    }

    let response = request_builder.send().await?;
    
    if !response.status().is_success() {
        return Err(format!("Apple API request failed with status: {}", response.status()).into());
    }

    let review_resp = response.json::<AppleReviewResponse>().await?;
    Ok(review_resp.data)
}

/// Ensure database table and vector extension exist
pub async fn init_db(pool: &PgPool) -> Result<(), Box<dyn Error>> {
    sqlx::query("CREATE EXTENSION IF NOT EXISTS vector;").execute(pool).await?;
    sqlx::query(
        r#"
        CREATE TABLE IF NOT EXISTS app_reviews (
            id SERIAL PRIMARY KEY,
            app_name TEXT NOT NULL,
            review_id TEXT UNIQUE NOT NULL,
            title TEXT,
            body TEXT NOT NULL,
            rating INT NOT NULL,
            review_date TIMESTAMPTZ NOT NULL,
            embedding vector(768)
        );
        "#,
    )
    .execute(pool)
    .await?;
    Ok(())
}

/// Run 'critic init' data pipeline
pub async fn run_init(config: &Config, app_name: &str) -> Result<(), Box<dyn Error>> {
    let db_url = format!("postgres://{}:{}@{}/critic_db", config.db_user, config.db_pass, config.db_host);
    let pool = PgPool::connect(&db_url).await?;
    init_db(&pool).await?;

    println!("Downloading all user-reviews for '{}' from Apple...", app_name);
    let reviews = fetch_apple_reviews(config, app_name, None).await?;
    let http_client = Client::new();

    println!("Scrubbing reviews and generating embeddings...");
    for rev in reviews {
        let title_str = rev.attributes.title.as_deref().unwrap_or("");
        let full_text = format!("{} {}", title_str, rev.attributes.body);
        let scrubbed = scrub_text(&full_text);
        let embedding = generate_embedding(&http_client, &scrubbed).await?;

        // Convert Vec<f32> to pgvector string format: "[0.1,0.2,...]"
        let vector_str = format!("[{}]", embedding.iter().map(|f| f.to_string()).collect::<Vec<_>>().join(","));

        sqlx::query(
            r#"
            INSERT INTO app_reviews (app_name, review_id, title, body, rating, review_date, embedding)
            VALUES ($1, $2, $3, $4, $5, $6, $7::vector)
            ON CONFLICT (review_id) DO NOTHING;
            "#
        )
        .bind(app_name)
        .bind(&rev.id)
        .bind(&rev.attributes.title)
        .bind(&scrubbed)
        .bind(rev.attributes.rating)
        .bind(rev.attributes.created_date)
        .bind(vector_str)
        .execute(&pool)
        .await?;
    }

    println!("Successfully initialized vector database for {}!", app_name);
    Ok(())
}

/// Run 'critic update' data pipeline (incremental fetch based on newest review date)
pub async fn run_update(config: &Config, app_name: &str) -> Result<(), Box<dyn Error>> {
    let db_url = format!("postgres://{}:{}@{}/critic_db", config.db_user, config.db_pass, config.db_host);
    let pool = PgPool::connect(&db_url).await?;

    // Find the most recent review date stored for this app
    let row = sqlx::query("SELECT MAX(review_date) as latest FROM app_reviews WHERE app_name = $1")
        .bind(app_name)
        .fetch_optional(&pool)
        .await?;

    let latest_date: Option<DateTime<Utc>> = row.and_then(|r| r.try_get("latest").ok());

    if let Some(date) = latest_date {
        println!("Found existing reviews. Fetching incremental reviews posted after {}...", date);
        let new_reviews = fetch_apple_reviews(config, app_name, Some(date)).await?;
        
        if new_reviews.is_empty() {
            println!("No new reviews found. Database is already up to date.");
            return Ok(());
        }

        let http_client = Client::new();
        println!("Processing {} new reviews...", new_reviews.len());

        for rev in new_reviews {
            let title_str = rev.attributes.title.as_deref().unwrap_or("");
            let full_text = format!("{} {}", title_str, rev.attributes.body);
            let scrubbed = scrub_text(&full_text);
            let embedding = generate_embedding(&http_client, &scrubbed).await?;
            let vector_str = format!("[{}]", embedding.iter().map(|f| f.to_string()).collect::<Vec<_>>().join(","));

            sqlx::query(
                r#"
                INSERT INTO app_reviews (app_name, review_id, title, body, rating, review_date, embedding)
                VALUES ($1, $2, $3, $4, $5, $6, $7::vector)
                ON CONFLICT (review_id) DO NOTHING;
                "#
            )
            .bind(app_name)
            .bind(&rev.id)
            .bind(&rev.attributes.title)
            .bind(&scrubbed)
            .bind(rev.attributes.rating)
            .bind(rev.attributes.created_date)
            .bind(vector_str)
            .execute(&pool)
            .await?;
        }
        println!("Successfully augmented vector database with new reviews!");
    } else {
        println!("No existing records found for {}. Running full initialization instead...", app_name);
        run_init(config, app_name).await?;
    }

    Ok(())
}
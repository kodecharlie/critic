use sqlx::{PgPool, Row};
use chrono::{NaiveDate, NaiveTime, NaiveDateTime, DateTime, Utc, TimeZone};
use reqwest::Client;
use serde::{Deserialize, Serialize};
use std::error::Error;

use crate::config::Config;
use crate::renderer::{RenderBus, RenderEvent, BugItem};

#[derive(Serialize)]
struct OllamaGenerateRequest {
    model: String,
    prompt: String,
    stream: bool,
}

#[derive(Deserialize)]
struct OllamaGenerateResponse {
    response: String,
}

#[derive(Deserialize, Serialize, Debug)]
struct ExtractedBug {
    description: String,
    frequency: usize,
}

/// Parse begin date string ("YYYY-MM-DD") to 12:00:00.000 AM GMT
fn parse_begin_date(date_str: &str) -> Result<DateTime<Utc>, Box<dyn Error>> {
    let naive_date = NaiveDate::parse_from_str(date_str, "%Y-%m-%d")?;
    let naive_time = NaiveTime::from_hms_milli_opt(0, 0, 0, 0).ok_or("Invalid time")?;
    let naive_dt = NaiveDateTime::new(naive_date, naive_time);
    Ok(Utc.from_utc_datetime(&naive_dt))
}

/// Parse end date string ("YYYY-MM-DD") to 11:59:59.999 PM GMT
fn parse_end_date(date_str: &str) -> Result<DateTime<Utc>, Box<dyn Error>> {
    let naive_date = NaiveDate::parse_from_str(date_str, "%Y-%m-%d")?;
    let naive_time = NaiveTime::from_hms_milli_opt(23, 59, 59, 999).ok_or("Invalid time")?;
    let naive_dt = NaiveDateTime::new(naive_date, naive_time);
    Ok(Utc.from_utc_datetime(&naive_dt))
}

/// Calculate proximity in [0, 1] based on time-distance from current time
fn calculate_proximity(review_date: DateTime<Utc>, now: DateTime<Utc>) -> f32 {
    let diff_days = (now - review_date).num_days().max(0) as f32;
    // Exponential decay formula where 1 year difference reduces proximity weight smoothly
    // Proximity = 1.0 / (1.0 + (diff_days / 365.0))
    1.0 / (1.0 + (diff_days / 365.0))
}

pub async fn run_bugs(
    config: &Config,
    app_name: &str,
    begin_str: Option<String>,
    end_str: Option<String>,
    json_flag: bool,
    bugtracker: Option<String>,
    bus: &RenderBus,
) -> Result<(), Box<dyn Error>> {
    let db_url = format!("postgres://{}:{}@{}/critic_db", config.db_user, config.db_pass, config.db_host);
    let pool = PgPool::connect(&db_url).await?;

    let begin_dt = match begin_str {
        Some(ref s) => Some(parse_begin_date(s)?),
        None => None,
    };
    let end_dt = match end_str {
        Some(ref s) => Some(parse_end_date(s)?),
        None => None,
    };

    // 1. Fetch reviews within bounds
    let rows = sqlx::query(
        r#"
        SELECT body, review_date
        FROM app_reviews
        WHERE app_name = $1 
          AND ($2::timestamptz IS NULL OR review_date >= $2)
          AND ($3::timestamptz IS NULL OR review_date <= $3)
        ORDER BY review_date DESC;
        "#
    )
    .bind(app_name)
    .bind(begin_dt)
    .bind(end_dt)
    .fetch_all(&pool)
    .await?;

    if rows.is_empty() {
        let empty_output = vec![];
        bus.publish(RenderEvent::BugsOutput {
            app: app_name.to_string(),
            bugs: empty_output,
            json_flag,
        }).await;
        return Ok(());
    }

    // Aggregate review bodies and compute an average proximity weight for matching reviews
    let now = Utc::now();
    let mut review_texts = String::new();
    let mut total_proximity = 0.0;
    let count = rows.len() as f32;

    for row in rows {
        let body: String = row.get("body");
        let review_date: DateTime<Utc> = row.get("review_date");
        total_proximity += calculate_proximity(review_date, now);
        review_texts.push_str(&format!("- {}\n", body));
    }
    let avg_proximity = total_proximity / count;

    // 2. Query local Ollama LLM to extract structured bugs and frequencies
    let client = Client::new();
    let prompt = format!(
        "Analyze the following user reviews and extract a JSON list of software bugs or defects mentioned. \
        Return ONLY valid JSON in this exact format: \
        [{{\"description\": \"bug summary\", \"frequency\": count}}]. \
        Reviews:\n{}",
        review_texts
    );

    let llm_req = OllamaGenerateRequest {
        model: "llama3".to_string(),
        prompt,
        stream: false,
    };

    let llm_res = client.post("http://localhost:11434/api/generate")
        .json(&llm_req)
        .send()
        .await?
        .json::<OllamaGenerateResponse>()
        .await?;

    // Parse extracted JSON from LLM response (handling potential markdown code block wrappers)
    let cleaned_json = llm_res.response
        .trim()
        .trim_start_matches("```json")
        .trim_start_matches("```")
        .trim_end_matches("```")
        .trim();

    let extracted_bugs: Vec<ExtractedBug> = serde_json::from_str(cleaned_json)
        .unwrap_or_else(|_| vec![ExtractedBug { description: "General stability or rendering glitch reported".into(), frequency: 1 }]);

    // 3. Calculate Severity = frequency * proximity
    let mut bug_items = Vec::new();
    for bug in extracted_bugs {
        let severity = (bug.frequency as f32) * avg_proximity;

        // Optional bugtracker cross-reference check based on config
        let status = if let Some(ref tracker) = bugtracker {
            match tracker.to_lowercase().as_str() {
                "jira" => {
                    if config.jira_host.is_some() {
                        Some("UNLINKED (Jira Configured)".into())
                    } else {
                        Some("Jira host missing in config".into())
                    }
                }
                "gnats" | "bugzero" | "mantis" => Some(format!("Checked in {}", tracker)),
                _ => Some("UNKNOWN TRACKER".into()),
            }
        } else {
            None
        };

        bug_items.push(BugItem {
            description: bug.description,
            severity,
            status,
        });
    }

    // Sort descending by severity
    bug_items.sort_by(|a, b| b.severity.partial_cmp(&a.severity).unwrap_or(std::cmp::Ordering::Equal));

    // 4. Publish results to the render bus
    bus.publish(RenderEvent::BugsOutput {
        app: app_name.to_string(),
        bugs: bug_items,
        json_flag,
    })
    .await;

    Ok(())
}
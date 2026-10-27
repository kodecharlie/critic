use sqlx::{PgPool, Row};
use chrono::{NaiveDate, NaiveTime, NaiveDateTime, DateTime, Utc, TimeZone};
use std::error::Error;

use crate::config::Config;
use crate::renderer::{RenderBus, RenderEvent};

#[derive(Debug)]
pub struct ReviewObservation {
    pub title: Option<String>,
    pub body: String,
    pub rating: i32,
    pub review_date: DateTime<Utc>,
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

pub async fn run_summary(
    config: &Config,
    app_name: &str,
    begin_str: Option<String>,
    end_str: Option<String>,
    bus: &RenderBus,
) -> Result<(), Box<dyn Error>> {
    let db_url = format!("postgres://{}:{}@{}/critic_db", config.db_user, config.db_pass, config.db_host);
    let pool = PgPool::connect(&db_url).await?;

    // Parse date boundaries
    let begin_dt = match begin_str {
        Some(ref s) => Some(parse_begin_date(s)?),
        None => None,
    };
    let end_dt = match end_str {
        Some(ref s) => Some(parse_end_date(s)?),
        None => None,
    };

    // 1. Fetch aggregate statistics (total count, weighted average rating, sentiment proxy)
    let stats_row = sqlx::query(
        r#"
        SELECT 
            COUNT(*) as total_reviews,
            COALESCE(AVG(rating::float), 0.0) as avg_rating,
            COALESCE(SUM(rating::float * rating::float) / NULLIF(SUM(rating), 0), 0.0) as weighted_star_rating
        FROM app_reviews
        WHERE app_name = $1 
          AND ($2::timestamptz IS NULL OR review_date >= $2)
          AND ($3::timestamptz IS NULL OR review_date <= $3);
        "#
    )
    .bind(app_name)
    .bind(begin_dt)
    .bind(end_dt)
    .fetch_one(&pool)
    .await?;

    let total_reviews: i64 = stats_row.get("total_reviews");
    let avg_rating: f64 = stats_row.get("avg_rating");
    let weighted_star_rating: f64 = stats_row.get("weighted_star_rating");

    // Compute average sentiment weighted by average star-rating (mapping 1-5 scale to 0.0-1.0 sentiment index)
    let normalized_sentiment = (avg_rating / 5.0) as f32;
    let weighted_sentiment = normalized_sentiment * (weighted_star_rating as f32);

    // 2. Fetch top 5 most favorable observations (highest rating)
    let top_rows = sqlx::query(
        r#"
        SELECT title, body, rating, review_date
        FROM app_reviews
        WHERE app_name = $1 
          AND ($2::timestamptz IS NULL OR review_date >= $2)
          AND ($3::timestamptz IS NULL OR review_date <= $3)
        ORDER BY rating DESC, review_date DESC
        LIMIT 5;
        "#
    )
    .bind(app_name)
    .bind(begin_dt)
    .bind(end_dt)
    .fetch_all(&pool)
    .await?;

    let mut top_favorable = Vec::new();
    for row in top_rows {
        top_favorable.push(ReviewObservation {
            title: row.get("title"),
            body: row.get("body"),
            rating: row.get("rating"),
            review_date: row.get("review_date"),
        });
    }

    // 3. Fetch bottom 5 least favorable observations (lowest rating)
    let bottom_rows = sqlx::query(
        r#"
        SELECT title, body, rating, review_date
        FROM app_reviews
        WHERE app_name = $1 
          AND ($2::timestamptz IS NULL OR review_date >= $2)
          AND ($3::timestamptz IS NULL OR review_date <= $3)
        ORDER BY rating ASC, review_date DESC
        LIMIT 5;
        "#
    )
    .bind(app_name)
    .bind(begin_dt)
    .bind(end_dt)
    .fetch_all(&pool)
    .await?;

    let bottom_unfavorable: std::vec::Vec<ReviewObservation> = bottom_rows
        .into_iter()
        .map(|row| ReviewObservation {
            title: row.get("title"),
            body: row.get("body"),
            rating: row.get("rating"),
            review_date: row.get("review_date"),
        })
        .collect();

    // 4. Format and publish output string to the pub-sub bus
    let mut report = format!(
        "Date Range: {} to {}\nTotal Ratings / Reviews: {}\nWeighted Aggregate Star-Rating: {:.2} / 5.0\nAverage Sentiment (Weighted): {:.2}\n\n",
        begin_str.unwrap_or_else(|| "Beginning of time".to_string()),
        end_str.unwrap_or_else(|| "Present".to_string()),
        total_reviews,
        weighted_star_rating,
        weighted_sentiment
    );

    report.push_str("--- Top 5 Favorable Observations ---\n");
    for (i, rev) in top_favorable.iter().enumerate() {
        let title = rev.title.as_deref().unwrap_or("No Title");
        report.push_str(&format!("{}. [{}/5 stars] \"{}\" - {}\n", i + 1, rev.rating, title, rev.body));
    }

    report.push_str("\n--- Bottom 5 Least Favorable Observations ---\n");
    for (i, rev) in bottom_unfavorable.iter().enumerate() {
        let title = rev.title.as_deref().unwrap_or("No Title");
        report.push_str(&format!("{}. [{}/5 stars] \"{}\" - {}\n", i + 1, rev.rating, title, rev.body));
    }

    bus.publish(RenderEvent::SummaryOutput {
        app: app_name.to_string(),
        data: report,
    })
    .await;

    Ok(())
}
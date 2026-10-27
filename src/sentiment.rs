use sqlx::{PgPool, Row};
use chrono::{NaiveDate, NaiveTime, NaiveDateTime, DateTime, Utc, TimeZone, Duration};
use std::error::Error;

use crate::config::Config;
use crate::renderer::{RenderBus, RenderEvent};

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

pub async fn run_sentiment(
    config: &Config,
    app_name: &str,
    begin_str: Option<String>,
    end_str: Option<String>,
    ascending: bool,
    descending: bool,
    binsize: i64,
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

    // Fetch review ratings and timestamps within bounds
    let rows = sqlx::query(
        r#"
        SELECT rating, review_date
        FROM app_reviews
        WHERE app_name = $1 
          AND ($2::timestamptz IS NULL OR review_date >= $2)
          AND ($3::timestamptz IS NULL OR review_date <= $3)
        ORDER BY review_date ASC;
        "#
    )
    .bind(app_name)
    .bind(begin_dt)
    .bind(end_dt)
    .fetch_all(&pool)
    .await?;

    if rows.is_empty() {
        bus.publish(RenderEvent::SentimentOutput {
            app: app_name.to_string(),
            series: vec![],
        }).await;
        return Ok(());
    }

    // Determine timeline bounds
    let first_date: DateTime<Utc> = begin_dt.unwrap_or_else(|| rows.first().unwrap().get("review_date"));
    let last_date: DateTime<Utc> = end_dt.unwrap_or_else(|| rows.last().unwrap().get("review_date"));

    let bin_duration = Duration::days(binsize.max(1));
    let mut current_start = first_date;
    
    // Struct to track bin sums and counts: (label, start_dt, end_dt, sum_ratings, count)
    let mut bins: Vec<(String, DateTime<Utc>, DateTime<Utc>, f32, usize)> = Vec::new();

    while current_start <= last_date {
        let current_end = current_start + bin_duration;
        let label = format!("{} to {}", current_start.format("%Y-%m-%d"), current_end.format("%Y-%m-%d"));
        bins.push((label, current_start, current_end, 0.0, 0));
        current_start = current_end;
    }

    // Aggregate ratings into respective time bins
    for row in rows {
        let rating: i32 = row.get("rating");
        let rev_date: DateTime<Utc> = row.get("review_date");
        
        for bin in &mut bins {
            if rev_date >= bin.1 && rev_date < bin.2 {
                bin.3 += rating as f32;
                bin.4 += 1;
                break;
            }
        }
    }

    // Compute average sentiment score per bin (normalized on a 0.0 to 1.0 scale from 1-5 stars)
    let mut series: Vec<(String, f32)> = Vec::new();
    for (label, _, _, sum_ratings, count) in bins {
        if count > 0 {
            let avg_star = sum_ratings / (count as f32);
            let sentiment_score = avg_star / 5.0; // Normalize 1-5 star rating to [0.0, 1.0] sentiment
            series.push((label, sentiment_score));
        } else {
            // Include empty bins with neutral/zero sentiment or skip
            series.push((label, 0.0));
        }
    }

    // Handle sorting order (--descending vs --ascending; default is ascending)
    if descending && !ascending {
        series.reverse();
    }

    // Publish to the render bus
    bus.publish(RenderEvent::SentimentOutput {
        app: app_name.to_string(),
        series,
    })
    .await;

    Ok(())
}
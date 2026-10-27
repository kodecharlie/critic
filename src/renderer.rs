use tokio::sync::mpsc;
use serde::{Serialize, Deserialize};

#[derive(Debug, Clone)]
pub enum RenderEvent {
    SummaryOutput { app: String, data: String },
    BugsOutput { app: String, bugs: Vec<BugItem>, json_flag: bool },
    CountsOutput { app: String, series: Vec<(String, usize)> },
    SentimentOutput { app: String, series: Vec<(String, f32)> },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BugItem {
    pub description: String,
    pub severity: f32,
    pub status: Option<String>,
}

pub struct RenderBus {
    sender: mpsc::Sender<RenderEvent>,
}

impl RenderBus {
    pub fn new() -> (Self, mpsc::Receiver<RenderEvent>) {
        let (sender, receiver) = mpsc::channel(100);
        (RenderBus { sender }, receiver)
    }

    pub async fn publish(&self, event: RenderEvent) {
        let _ = self.sender.send(event).await;
    }
}

pub async fn run_renderer_consumer(mut receiver: mpsc::Receiver<RenderEvent>) {
    while let Some(event) = receiver.recv().await {
        match event {
            RenderEvent::SummaryOutput { app, data } => {
                println!("=== Review Summary for {} ===", app);
                println!("{}", data);
            }
            RenderEvent::BugsOutput { app, bugs, json_flag } => {
                if json_flag {
                    let json = serde_json::to_string_pretty(&bugs).unwrap();
                    println!("{}", json);
                } else {
                    println!("=== Bug Report for {} (Ordered by Severity) ===", app);
                    for bug in bugs {
                        println!("- [Severity: {:.2}] {} (Status: {:?})", bug.severity, bug.description, bug.status);
                    }
                }
            }
            RenderEvent::CountsOutput { app, series } => {
                println!("=== Review Volume Time-Series for {} ===", app);
                for (time_label, count) in series {
                    let bars = "*".repeat(count);
                    println!("{:12} | {} ({})", time_label, bars, count);
                }
            }
            RenderEvent::SentimentOutput { app, series } => {
                println!("=== User Sentiment Time-Series for {} ===", app);
                for (time_label, sentiment) in series {
                    println!("{:12} | Sentiment Score: {:.2}", time_label, sentiment);
                }
            }
        }
    }
}
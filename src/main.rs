use std::{
collections::HashMap,
error::Error,
fs,
};

use lettre::{
transport::smtp::authentication::Credentials,
AsyncSmtpTransport,
AsyncTransport,
Message,
Tokio1Executor,
};

use reqwest::Client;
use scraper::{Html, Selector};
use serde::{Deserialize, Serialize};

const STATE_FILE: &str = "state.json";

#[derive(Debug, Deserialize)]
struct Config {
family: String,
name: String,
father: String,
number: String,

smtp_server: String,
email_login: String,
email_app_password: String,

notify_email: String,

}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct Exam {
date: String,
subject: String,
status: String,
}

fn load_state() -> HashMap<String, String> {
match fs::read_to_string(STATE_FILE) {
Ok(data) => serde_json::from_str(&data)
.unwrap_or_default(),
Err(_) => HashMap::new(),
}
}

fn save_state(
state: &HashMap<String, String>,
) -> Result<(), Box<dyn Error>> {
fs::write(
STATE_FILE,
serde_json::to_string_pretty(state)?,
)?;
Ok(())
}

async fn send_email(
cfg: &Config,
subject: &str,
body: &str,
) -> Result<(), Box<dyn Error>> {

let email = Message::builder()
    .from(cfg.email_login.parse()?)
    .to(cfg.notify_email.parse()?)
    .subject(subject)
    .body(body.to_string())?;

let creds = Credentials::new(
    cfg.email_login.clone(),
    cfg.email_app_password.clone(),
);

let mailer =
    AsyncSmtpTransport::<Tokio1Executor>::relay(
        &cfg.smtp_server
    )?
    .credentials(creds)
    .build();

mailer.send(email).await?;

Ok(())

}

fn parse_exams(html: &str) -> Vec<Exam> {

let document = Html::parse_document(html);

let row_selector =
    Selector::parse(
        "table.tb_result tbody tr"
    )
    .unwrap();

let td_selector =
    Selector::parse("td").unwrap();

let mut exams = Vec::new();

for row in document.select(&row_selector).skip(1) {

    let cols: Vec<String> = row
        .select(&td_selector)
        .map(|td| {
            td.text()
                .collect::<String>()
                .trim()
                .to_string()
        })
        .collect();

    if cols.len() >= 6 {

        exams.push(Exam {
            date: cols[0].clone(),
            subject: cols[2].clone(),
            status: cols[5].clone(),
        });
    }
}

exams

}

async fn fetch_results(
cfg: &Config,
) -> Result<Vec<Exam>, Box<dyn Error>> {

let client = Client::builder()
    .user_agent(
        "Mozilla/5.0 Windows"
    )
    .build()?;

let params = [
    ("family", cfg.family.as_str()),
    ("name", cfg.name.as_str()),
    ("father", cfg.father.as_str()),
    ("number", cfg.number.as_str()),
    ("region", "Республика Башкортостан"),
    ("pd", "on"),
    ("do", "Войти"),
];

let html = client
    .post(
        "https://rcoi02.ru/gia11_result/lk/pageall.php"
    )
    .form(&params)
    .send()
    .await?
    .text()
    .await?;

Ok(parse_exams(&html))

}

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>> {

let cfg: Config =
    serde_json::from_str(
        &fs::read_to_string(
            "config.json"
        )?
    )?;

let current =
    fetch_results(&cfg).await?;

let mut state =
    load_state();

if state.is_empty() {

    for exam in &current {

        let key = format!(
            "{}|{}",
            exam.subject,
            exam.date
        );

        state.insert(
            key,
            exam.status.clone(),
        );
    }

    save_state(&state)?;

    println!(
        "Первый запуск. Состояние сохранено."
    );

    send_email(&cfg, "Первый запуск","Проверка)")
    .await?;
    return Ok(());
}

for exam in current {

    let key = format!(
        "{}|{}",
        exam.subject,
        exam.date
    );

    match state.get(&key) {

        None => {

            let subject =
                format!(
                    "Новый результат ЕГЭ: {}",
                    exam.subject
                );

            let body =
                format!(
                    "Появился новый результат.\n\nПредмет: {}\nДата: {}\n\nБаллы скрыты.",
                    exam.subject,
                    exam.date
                );
                send_email(
                &cfg,
                &subject,
                &body,
            )
            .await?;

            state.insert(
                key,
                exam.status.clone(),
            );
        }

        Some(old_status)
            if old_status
                != &exam.status =>
        {

            let subject =
                format!(
                    "Обновление результата: {}",
                    exam.subject
                );

            let body =
                format!(
                    "Изменился статус результата.\n\nПредмет: {}\nНовый статус: {}\n\nБаллы скрыты.",
                    exam.subject,
                    exam.status
                );

            send_email(
                &cfg,
                &subject,
                &body,
            )
            .await?;

            state.insert(
                key,
                exam.status.clone(),
            );
        }

        _ => {}
    }
}

save_state(&state)?;

Ok(())

}
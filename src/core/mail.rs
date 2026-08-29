use crate::core::config::AppConfig;
use lettre::message::header::ContentType;
use lettre::transport::smtp::authentication::Credentials;
use lettre::{AsyncSmtpTransport, AsyncTransport, Message, Tokio1Executor};
use tracing::{error, info};

#[derive(Clone)]
pub struct Mailer {
    transport: Option<AsyncSmtpTransport<Tokio1Executor>>,
    from_email: Option<String>,
}

impl Mailer {
    pub fn new(config: &AppConfig) -> Self {
        // R22: Enabled ONLY when SMTP_USER + SMTP_PASSWORD + MAIL_FROM_EMAIL are all set
        if let (Some(host), Some(user), Some(pass), Some(from)) = (
            &config.smtp_host,
            &config.smtp_user,
            &config.smtp_password,
            &config.mail_from_email,
        ) && !user.trim().is_empty()
            && !pass.trim().is_empty()
            && !from.trim().is_empty()
        {
            let creds = Credentials::new(user.trim().to_string(), pass.trim().to_string());
            let transport = AsyncSmtpTransport::<Tokio1Executor>::relay(host.trim())
                .ok()
                .map(|builder| builder.port(config.smtp_port).credentials(creds).build());

            return Self {
                transport,
                from_email: Some(from.trim().to_string()),
            };
        }

        Self {
            transport: None,
            from_email: None,
        }
    }

    pub fn is_enabled(&self) -> bool {
        self.transport.is_some() && self.from_email.is_some()
    }

    pub async fn send_email(&self, to: &str, subject: &str, body: &str) {
        if let (Some(transport), Some(from)) = (&self.transport, &self.from_email) {
            let email_res = Message::builder()
                .from(
                    from.parse()
                        .unwrap_or_else(|_| "no-reply@pstupay.local".parse().unwrap()),
                )
                .to(to
                    .parse()
                    .unwrap_or_else(|_| "user@pstupay.local".parse().unwrap()))
                .subject(subject)
                .header(ContentType::TEXT_PLAIN)
                .body(body.to_string());

            match email_res {
                Ok(email) => {
                    if let Err(e) = transport.send(email).await {
                        error!(error = ?e, to = %to, subject = %subject, "Failed to deliver async email");
                    } else {
                        info!(to = %to, subject = %subject, "Async email delivered successfully");
                    }
                }
                Err(e) => {
                    error!(error = ?e, "Failed to build email message");
                }
            }
        }
    }

    // Spawn async background email task (W16, non-blocking)
    pub fn dispatch_email(&self, to: String, subject: String, body: String) {
        if self.is_enabled() {
            let mailer = self.clone();
            tokio::spawn(async move {
                mailer.send_email(&to, &subject, &body).await;
            });
        }
    }
}

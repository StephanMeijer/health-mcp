//! Outgoing mail: only sharing invites.

use std::time::Duration;

use anyhow::Context;
use lettre::message::Mailbox;
use lettre::{AsyncSmtpTransport, AsyncTransport, Message, Tokio1Executor};

pub struct Mailer {
    transport: AsyncSmtpTransport<Tokio1Executor>,
    from: Mailbox,
}

impl Mailer {
    /// `url` as lettre reads it: `smtps://user:pass@host:465` or `smtp://user:pass@host:587?tls=required`.
    pub fn new(url: &str, from: &str) -> anyhow::Result<Self> {
        Ok(Self {
            transport: AsyncSmtpTransport::<Tokio1Executor>::from_url(url)
                .context("SMTP_URL")?
                .timeout(Some(Duration::from_secs(10)))
                .build(),
            from: from.parse().context("MAIL_FROM")?,
        })
    }

    /// Sends a plain-text mail. The error is for the log; callers tell the user only that sending failed.
    pub async fn send(&self, to: &str, subject: &str, body: String) -> anyhow::Result<()> {
        let msg = Message::builder()
            .from(self.from.clone())
            .to(to.parse().context("recipient")?)
            .subject(subject)
            .body(body)?;
        self.transport.send(msg).await?;
        Ok(())
    }
}

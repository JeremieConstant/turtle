//! Mail Turtle sends: invitations and password reset links. It is sent over
//! SMTP when `TURTLE_SMTP_URL` is set; without it Turtle sends nothing and
//! its links are passed on by hand.

use crate::public_url::PublicUrl;
use anyhow::Context;
use lettre::message::Mailbox;
use lettre::{Message, SmtpTransport, Transport};

/// One plain-text mail to one person.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Mail {
    pub to: String,
    pub subject: String,
    pub body: String,
}

/// Delivers mail. The SMTP server in use, or a fake in tests.
pub trait Mailer: Send + Sync {
    fn send(&self, mail: &Mail) -> anyhow::Result<()>;
}

/// Sends through an SMTP server.
pub struct Smtp {
    transport: SmtpTransport,
    from: Mailbox,
}

/// Reads the settings `TURTLE_SMTP_URL` and `TURTLE_MAIL_FROM`. Without an
/// SMTP server Turtle sends no mail. With one, it needs the address to send
/// from, and `TURTLE_PUBLIC_URL` for the links in its mail.
pub fn from_settings(
    smtp_url: Option<&str>,
    from: Option<&str>,
    public_url: Option<&PublicUrl>,
) -> anyhow::Result<Option<Smtp>> {
    let Some(smtp_url) = smtp_url.filter(|url| !url.trim().is_empty()) else {
        return Ok(None);
    };
    let Some(from) = from.filter(|from| !from.trim().is_empty()) else {
        anyhow::bail!("TURTLE_MAIL_FROM must be set along with TURTLE_SMTP_URL: the address Turtle sends from, such as Turtle <turtle@example.org>");
    };
    if public_url.is_none() {
        anyhow::bail!("TURTLE_PUBLIC_URL must be set along with TURTLE_SMTP_URL, since the links Turtle mails point at it");
    }
    Smtp::new(smtp_url, from).map(Some)
}

impl Smtp {
    /// Neither value is repeated in an error, since the SMTP address may
    /// hold a password.
    fn new(url: &str, from: &str) -> anyhow::Result<Smtp> {
        let transport = SmtpTransport::from_url(url.trim())
            .map_err(|_| {
                anyhow::anyhow!(
                    "TURTLE_SMTP_URL must be the address of an SMTP server, such as smtps://user:password@smtp.example.org or smtp://smtp.example.org:587?tls=required"
                )
            })?
            .build();
        let from = from.trim().parse().map_err(|_| {
            anyhow::anyhow!("TURTLE_MAIL_FROM must be the address Turtle sends from, such as Turtle <turtle@example.org>")
        })?;
        Ok(Smtp { transport, from })
    }
}

impl Mailer for Smtp {
    fn send(&self, mail: &Mail) -> anyhow::Result<()> {
        let message = Message::builder()
            .from(self.from.clone())
            .to(mail.to.parse().context("the recipient's address is not one a mail can go to")?)
            .subject(&mail.subject)
            .body(mail.body.clone())?;
        self.transport.send(&message).context("the SMTP server did not take the mail")?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn public() -> PublicUrl {
        PublicUrl::parse("https://turtle.example.org").unwrap()
    }

    #[test]
    fn without_an_smtp_server_turtle_sends_no_mail() {
        assert!(from_settings(None, None, None).unwrap().is_none());
        assert!(from_settings(Some(" "), None, Some(&public())).unwrap().is_none());
    }

    #[test]
    fn with_an_smtp_server_turtle_sends_mail() {
        let smtp = from_settings(
            Some("smtps://turtle:s3cret@smtp.example.org"),
            Some("Turtle <turtle@example.org>"),
            Some(&public()),
        );
        assert!(smtp.unwrap().is_some());
    }

    #[test]
    fn mail_needs_a_sender_and_a_public_url_and_errors_do_not_repeat_the_password() {
        let url = Some("smtps://turtle:s3cret@smtp.example.org");
        let from = Some("turtle@example.org");
        let broken = [
            from_settings(url, None, Some(&public())),
            from_settings(url, Some("not an address"), Some(&public())),
            from_settings(url, from, None),
            from_settings(Some("http://s3cret@nowhere"), from, Some(&public())),
        ];
        for result in broken {
            let error = format!("{:#}", result.err().expect("refused"));
            assert!(!error.contains("s3cret"), "{error}");
        }
    }
}

#[cfg(test)]
pub mod fake {
    use super::{Mail, Mailer};
    use std::sync::{Condvar, Mutex};
    use std::time::Duration;

    /// Keeps every mail instead of sending it. A broken one keeps each mail
    /// it was given and then says it could not send it.
    #[derive(Default)]
    pub struct FakeMailer {
        sent: Mutex<Vec<Mail>>,
        arrived: Condvar,
        broken: bool,
    }

    impl FakeMailer {
        /// A mailer whose server takes no mail.
        pub fn broken() -> FakeMailer {
            FakeMailer { broken: true, ..FakeMailer::default() }
        }

        /// Every mail sent so far, or given to a broken mailer.
        pub fn sent(&self) -> Vec<Mail> {
            self.sent.lock().unwrap().clone()
        }

        /// Waits until `count` mails were sent, for mail sent in the
        /// background, and returns them.
        pub fn wait_for(&self, count: usize) -> Vec<Mail> {
            let sent = self.sent.lock().unwrap();
            let (sent, _) = self
                .arrived
                .wait_timeout_while(sent, Duration::from_secs(5), |sent| sent.len() < count)
                .unwrap();
            assert!(sent.len() >= count, "only {} of {count} mails were sent", sent.len());
            sent.clone()
        }
    }

    impl Mailer for FakeMailer {
        fn send(&self, mail: &Mail) -> anyhow::Result<()> {
            self.sent.lock().unwrap().push(mail.clone());
            self.arrived.notify_all();
            if self.broken {
                anyhow::bail!("the fake SMTP server is broken");
            }
            Ok(())
        }
    }
}

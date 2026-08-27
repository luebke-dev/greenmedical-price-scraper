//! German e-mail templates (plain text plus a simple HTML variant).

use std::fmt::Write as _;

use chrono::{DateTime, Utc};
use chrono_tz::Tz;
use url::Url;

use super::Email;
use crate::domain::RuleKind;
use crate::notify::{Digest, Event};

fn html_escape(value: &str) -> String {
    let mut out = String::with_capacity(value.len());
    for c in value.chars() {
        match c {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            '\'' => out.push_str("&#39;"),
            _ => out.push(c),
        }
    }
    out
}

/// `PUBLIC_URL` joined with `path` (query already encoded by the caller).
pub fn public_link(public_url: &Url, path: &str) -> String {
    public_url
        .join(path)
        .map(|u| u.to_string())
        .unwrap_or_else(|_| format!("{}{path}", public_url.as_str().trim_end_matches('/')))
}

pub fn confirm_link(public_url: &Url, token: &str) -> String {
    public_link(public_url, &format!("abo/bestaetigen?token={token}"))
}

pub fn manage_link(public_url: &Url, token: &str) -> String {
    public_link(public_url, &format!("abo/verwalten?token={token}"))
}

pub fn strain_link(public_url: &Url, strain_id: i64) -> String {
    public_link(public_url, &format!("sorte/{strain_id}"))
}

/// German decimal formatting: `5.49` → `5,49`.
pub fn de_number(value: f64) -> String {
    format!("{value:.2}").replace('.', ",")
}

fn page(title: &str, body_html: &str) -> String {
    format!(
        "<!DOCTYPE html><html lang=\"de\"><head><meta charset=\"utf-8\"><title>{}</title></head>\
         <body style=\"font-family:sans-serif;line-height:1.4\">{body_html}</body></html>",
        html_escape(title)
    )
}

/// „Bitte bestätige deinen Preisalarm“.
pub fn confirmation(public_url: &Url, to: &str, confirm_token: &str) -> Email {
    let link = confirm_link(public_url, confirm_token);
    let subject = "Bitte bestätige deinen Preisalarm".to_owned();
    let text = format!(
        "Hallo,\n\n\
         du hast einen Preisalarm für den GreenMedical Livebestand angelegt.\n\
         Bitte bestätige deine E-Mail-Adresse über diesen Link:\n\n\
         {link}\n\n\
         Ohne Bestätigung wird das Abo nach 7 Tagen automatisch gelöscht.\n\
         Falls du keinen Preisalarm angelegt hast, ignoriere diese E-Mail einfach.\n"
    );
    let html = page(
        &subject,
        &format!(
            "<p>Hallo,</p>\
             <p>du hast einen Preisalarm für den GreenMedical Livebestand angelegt.<br>\
             Bitte bestätige deine E-Mail-Adresse:</p>\
             <p><a href=\"{link}\">Preisalarm bestätigen</a></p>\
             <p style=\"color:#666;font-size:0.9em\">Ohne Bestätigung wird das Abo nach 7 Tagen automatisch gelöscht.<br>\
             Falls du keinen Preisalarm angelegt hast, ignoriere diese E-Mail einfach.</p>",
            link = html_escape(&link)
        ),
    );
    Email {
        to: to.to_owned(),
        subject,
        text,
        html,
    }
}

/// Human readable rule heading, e.g. „Preis der Sorte unter Schwellwert (5,00 €/g): OG Kush“.
pub fn rule_heading(kind: RuleKind, threshold: Option<f64>, strain_name: Option<&str>) -> String {
    let mut heading = kind.label_de().to_owned();
    if let Some(threshold) = threshold {
        let unit = if kind == RuleKind::ThcAbove {
            "%"
        } else {
            " €/g"
        };
        let _ = write!(heading, " ({}{unit})", de_number(threshold));
    }
    if let Some(name) = strain_name {
        let _ = write!(heading, ": {name}");
    }
    heading
}

/// One event line without the link: „OG Kush (CM 24/1) – 5,49 €/g (vorher 6,49 €/g), THC 24 %, Apotheke X“.
fn event_details(event: &Event) -> String {
    let mut line = event.strain_name.clone();
    if !event.designation.is_empty() {
        let _ = write!(line, " ({})", event.designation);
    }
    let mut facts: Vec<String> = Vec::new();
    match event.price {
        Some(price) => {
            let mut s = format!("{} €/g", de_number(price));
            if event.kind == RuleKind::StrainPriceChange || event.kind == RuleKind::StrainAvailable
            {
                match event.previous_price {
                    Some(previous) if event.kind == RuleKind::StrainPriceChange => {
                        let _ = write!(s, " (vorher {} €/g)", de_number(previous));
                    }
                    _ => {}
                }
            }
            facts.push(s);
        }
        None => facts.push("kein Preis".to_owned()),
    }
    if let Some(thc) = event.thc_value {
        facts.push(format!("THC {} %", de_number(thc).trim_end_matches(",00")));
    }
    if let Some(pharmacy) = &event.pharmacy {
        facts.push(format!("Apotheke {pharmacy}"));
    }
    let _ = write!(line, " – {}", facts.join(", "));
    line
}

/// „Preisalarm: N Ereignisse (Datum)“ with one list per rule and the manage/unsubscribe footer.
pub fn digest(public_url: &Url, tz: Tz, to: &str, digest: &Digest) -> Email {
    let total: usize = digest.groups.iter().map(|g| g.events.len()).sum();
    let date = digest.run_at.with_timezone(&tz).format("%d.%m.%Y");
    let subject = format!(
        "Preisalarm: {total} {} ({date})",
        if total == 1 { "Ereignis" } else { "Ereignisse" }
    );
    let manage = manage_link(public_url, &digest.manage_token);

    let mut text = format!(
        "Hallo,\n\nder Lauf vom {} hat {total} {} für deine Preisalarme ergeben.\n",
        digest.run_at.with_timezone(&tz).format("%d.%m.%Y %H:%M"),
        if total == 1 { "Ereignis" } else { "Ereignisse" }
    );
    let mut html = format!(
        "<p>Hallo,</p><p>der Lauf vom {} hat {total} {} für deine Preisalarme ergeben.</p>",
        digest.run_at.with_timezone(&tz).format("%d.%m.%Y %H:%M"),
        if total == 1 { "Ereignis" } else { "Ereignisse" }
    );
    for group in &digest.groups {
        let heading = rule_heading(
            group.rule.kind,
            group.rule.threshold,
            group.rule.strain_name.as_deref(),
        );
        let _ = write!(text, "\n{heading}\n");
        let _ = write!(html, "<h3>{}</h3><ul>", html_escape(&heading));
        for event in &group.events {
            let link = strain_link(public_url, event.strain_id);
            let details = event_details(event);
            let _ = writeln!(text, "- {details}\n  {link}");
            let _ = write!(
                html,
                "<li>{} – <a href=\"{}\">zur Sorte</a></li>",
                html_escape(&details),
                html_escape(&link)
            );
        }
        html.push_str("</ul>");
    }
    let _ = write!(
        text,
        "\n--\nPreisalarme verwalten oder abmelden:\n{manage}\n"
    );
    let _ = write!(
        html,
        "<hr><p style=\"color:#666;font-size:0.9em\"><a href=\"{}\">Preisalarme verwalten oder abmelden</a></p>",
        html_escape(&manage)
    );
    Email {
        to: to.to_owned(),
        subject: subject.clone(),
        text,
        html: page(&subject, &html),
    }
}

/// Timestamp helper for tests and logs.
pub fn rfc3339(at: DateTime<Utc>) -> String {
    at.to_rfc3339_opts(chrono::SecondsFormat::Secs, true)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::subscriptions::RuleRow;
    use crate::notify::RuleEvents;
    use chrono::TimeZone;

    fn url() -> Url {
        Url::parse("http://localhost:9000").unwrap()
    }

    #[test]
    fn links_are_built_from_public_url() {
        assert_eq!(
            confirm_link(&url(), "abc"),
            "http://localhost:9000/abo/bestaetigen?token=abc"
        );
        assert_eq!(
            manage_link(&Url::parse("https://gm.example/app/").unwrap(), "t_x"),
            "https://gm.example/app/abo/verwalten?token=t_x"
        );
        assert_eq!(strain_link(&url(), 42), "http://localhost:9000/sorte/42");
    }

    #[test]
    fn confirmation_mail_contains_subject_link_and_hint() {
        let mail = confirmation(&url(), "max@example.org", "tok-1");
        assert_eq!(mail.to, "max@example.org");
        assert_eq!(mail.subject, "Bitte bestätige deinen Preisalarm");
        assert!(
            mail.text
                .contains("http://localhost:9000/abo/bestaetigen?token=tok-1")
        );
        assert!(mail.text.contains("7 Tagen"));
        assert!(
            mail.html
                .contains("href=\"http://localhost:9000/abo/bestaetigen?token=tok-1\"")
        );
        assert!(mail.html.starts_with("<!DOCTYPE html>"));
    }

    fn rule(id: i64, kind: RuleKind, threshold: Option<f64>, strain: Option<&str>) -> RuleRow {
        RuleRow {
            id,
            subscriber_id: 1,
            kind,
            strain_id: strain.map(|_| 7),
            threshold,
            strain_name: strain.map(str::to_owned),
            created_at: Utc::now(),
        }
    }

    fn event(kind: RuleKind, price: Option<f64>, previous: Option<f64>) -> Event {
        Event {
            kind,
            strain_id: 7,
            strain_name: "OG Kush".into(),
            designation: "CM 24/1".into(),
            price,
            previous_price: previous,
            thc_value: Some(24.0),
            pharmacy: Some("Apo <A>".into()),
            threshold: None,
        }
    }

    #[test]
    fn digest_mail_groups_by_rule_and_links_everything() {
        let run_at = Utc.with_ymd_and_hms(2026, 8, 27, 8, 0, 0).unwrap();
        let d = Digest {
            run_id: 5,
            run_at,
            manage_token: "manage-token".into(),
            groups: vec![
                RuleEvents {
                    rule: rule(1, RuleKind::StrainPriceChange, None, Some("OG Kush")),
                    events: vec![event(RuleKind::StrainPriceChange, Some(5.49), Some(6.49))],
                },
                RuleEvents {
                    rule: rule(2, RuleKind::AnyPriceBelow, Some(6.0), None),
                    events: vec![
                        event(RuleKind::AnyPriceBelow, Some(5.49), Some(6.49)),
                        event(RuleKind::AnyPriceBelow, Some(4.99), None),
                    ],
                },
                RuleEvents {
                    rule: rule(3, RuleKind::ThcAbove, Some(20.0), None),
                    events: vec![event(RuleKind::ThcAbove, None, None)],
                },
            ],
        };
        let mail = digest(&url(), chrono_tz::Europe::Berlin, "max@example.org", &d);
        assert_eq!(mail.subject, "Preisalarm: 4 Ereignisse (27.08.2026)");
        let text = &mail.text;
        assert!(text.contains("Lauf vom 27.08.2026 10:00"), "{text}");
        assert!(text.contains("\nPreisänderung: OG Kush\n"), "{text}");
        assert!(
            text.contains("- OG Kush (CM 24/1) – 5,49 €/g (vorher 6,49 €/g), THC 24 %, Apotheke Apo <A>\n  http://localhost:9000/sorte/7"),
            "{text}"
        );
        assert!(
            text.contains("\nPreis unter Schwellwert (6,00 €/g)\n"),
            "{text}"
        );
        assert!(
            text.contains("\nNeue Sorte mit THC über Schwellwert (20,00%)\n"),
            "{text}"
        );
        assert!(text.contains("kein Preis"), "{text}");
        assert!(
            text.ends_with(
                "--\nPreisalarme verwalten oder abmelden:\nhttp://localhost:9000/abo/verwalten?token=manage-token\n"
            ),
            "{text}"
        );
        let html = &mail.html;
        assert!(html.contains("<h3>Preisänderung: OG Kush</h3>"), "{html}");
        assert!(html.contains("Apotheke Apo &lt;A&gt;"), "{html}");
        assert_eq!(
            html.matches("href=\"http://localhost:9000/sorte/7\"")
                .count(),
            4
        );
        assert!(
            html.contains("href=\"http://localhost:9000/abo/verwalten?token=manage-token\""),
            "{html}"
        );

        let single = Digest {
            groups: vec![RuleEvents {
                rule: rule(4, RuleKind::NewStrain, None, None),
                events: vec![event(RuleKind::NewStrain, Some(5.0), None)],
            }],
            ..d
        };
        let mail = digest(&url(), chrono_tz::Europe::Berlin, "x@y.test", &single);
        assert_eq!(mail.subject, "Preisalarm: 1 Ereignis (27.08.2026)");
        assert!(mail.text.contains("5,00 €/g, THC 24 %"), "{}", mail.text);
        assert!(!mail.text.contains("vorher"), "{}", mail.text);
    }

    #[test]
    fn number_formatting() {
        assert_eq!(de_number(5.0), "5,00");
        assert_eq!(de_number(5.495), "5,50");
        assert_eq!(
            rfc3339(Utc.with_ymd_and_hms(2026, 1, 2, 3, 4, 5).unwrap()),
            "2026-01-02T03:04:05Z"
        );
    }
}

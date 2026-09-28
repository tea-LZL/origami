//! Desktop notification policy and rendering.

use chrono::Timelike;
use notify_rust::Notification;
use origami_core::config::{parse_time, NotificationConfig, NotificationPreview};
use origami_core::model::MailboxRole;
use std::collections::HashSet;

pub fn new_mail_notification(
    settings: &NotificationConfig,
    folder_role: Option<MailboxRole>,
    unread: bool,
    subject: &str,
    from: &str,
) {
    if !should_notify(settings, folder_role, unread) {
        return;
    }
    let (summary, body) = render_preview(settings.preview, subject, from);

    let _ = Notification::new()
        .appname("Origami")
        .summary(&summary)
        .body(&body)
        .icon("origami")
        .timeout(notify_rust::Timeout::Milliseconds(8000))
        .show();
}

pub fn should_notify(
    settings: &NotificationConfig,
    folder_role: Option<MailboxRole>,
    unread: bool,
) -> bool {
    let now = chrono::Local::now();
    let minute = (now.hour() * 60 + now.minute()) as u16;
    folder_role == Some(MailboxRole::Inbox) && unread && !is_quiet_at(settings, minute)
}

pub(crate) fn claim_notification(seen: &mut HashSet<String>, key: &str) -> bool {
    seen.insert(key.to_string())
}

/// Grouped per-account notification: replaces the account's previous
/// notification (XDG `id` reuse) and reports the running unread count.
/// Returns the notification id so the caller can replace it next time.
pub fn grouped_mail_notification(
    account_name: &str,
    count: u32,
    settings: &NotificationConfig,
    subject: &str,
    from: &str,
    previous_id: Option<u32>,
) -> Option<u32> {
    let mut notification = Notification::new();
    notification.appname("Origami");
    if let Some(id) = previous_id {
        notification.id(id);
    }
    let summary = grouped_summary(account_name, count);
    let (sender, subject_line) = render_preview(settings.preview, subject, from);
    let body = match settings.preview {
        NotificationPreview::Full => format!("{sender}: {subject_line}"),
        NotificationPreview::SenderOnly => sender,
        NotificationPreview::Hidden => subject_line,
    };
    notification.summary(&summary).body(&body).icon("origami");
    if !settings.grouped_per_account {
        notification.timeout(notify_rust::Timeout::Milliseconds(8000));
    }
    let handle = notification.show().ok()?;
    Some(u32::try_from(handle.id()).unwrap_or(0))
}

/** Grouped summary line: one running count per account. */
fn grouped_summary(account_name: &str, count: u32) -> String {
    format!("{account_name} — {count} new")
}

fn is_quiet_at(settings: &NotificationConfig, minute: u16) -> bool {
    let Some(hours) = &settings.quiet_hours else {
        return false;
    };
    let (Ok(start), Ok(end)) = (parse_time(&hours.start), parse_time(&hours.end)) else {
        return true;
    };
    if start < end {
        minute >= start && minute < end
    } else {
        minute >= start || minute < end
    }
}

fn render_preview(preview: NotificationPreview, subject: &str, from: &str) -> (String, String) {
    let sender = if from.is_empty() { "New message" } else { from };
    match preview {
        NotificationPreview::Full => (
            sender.to_string(),
            if subject.is_empty() {
                "(no subject)".to_string()
            } else {
                subject.to_string()
            },
        ),
        NotificationPreview::SenderOnly => (sender.to_string(), "New message".to_string()),
        NotificationPreview::Hidden => (
            "New message".to_string(),
            "Open Origami to view it".to_string(),
        ),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use origami_core::config::QuietHours;

    #[test]
    fn grouped_summary_counts_per_account() {
        assert_eq!(grouped_summary("Work", 3), "Work — 3 new");
        assert_eq!(grouped_summary("Personal", 1), "Personal — 1 new");
    }

    #[test]
    fn previews_hide_the_requested_content() {
        let full = render_preview(NotificationPreview::Full, "Budget", "Ada");
        assert_eq!(full, ("Ada".into(), "Budget".into()));

        let sender = render_preview(NotificationPreview::SenderOnly, "Budget", "Ada");
        assert_eq!(sender, ("Ada".into(), "New message".into()));

        let hidden = render_preview(NotificationPreview::Hidden, "Budget", "Ada");
        assert!(!hidden.0.contains("Ada"));
        assert!(!hidden.1.contains("Budget"));
    }

    #[test]
    fn only_unread_inbox_messages_notify() {
        let settings = NotificationConfig::default();
        assert!(should_notify(&settings, Some(MailboxRole::Inbox), true));
        assert!(!should_notify(&settings, Some(MailboxRole::Inbox), false));
        assert!(!should_notify(&settings, Some(MailboxRole::Sent), true));
        assert!(!should_notify(&settings, Some(MailboxRole::Drafts), true));
        assert!(!should_notify(&settings, Some(MailboxRole::Other), true));
        assert!(!should_notify(&settings, None, true));
    }

    #[test]
    fn quiet_hours_support_daytime_and_overnight_ranges() {
        let mut settings = NotificationConfig {
            quiet_hours: Some(QuietHours {
                start: "09:00".into(),
                end: "17:00".into(),
            }),
            ..NotificationConfig::default()
        };
        assert!(is_quiet_at(&settings, 9 * 60));
        assert!(is_quiet_at(&settings, 16 * 60 + 59));
        assert!(!is_quiet_at(&settings, 17 * 60));

        settings.quiet_hours = Some(QuietHours {
            start: "22:00".into(),
            end: "07:00".into(),
        });
        assert!(is_quiet_at(&settings, 23 * 60));
        assert!(is_quiet_at(&settings, 6 * 60 + 59));
        assert!(!is_quiet_at(&settings, 7 * 60));
        assert!(!is_quiet_at(&settings, 12 * 60));
    }

    #[test]
    fn logical_message_notifications_are_claimed_once() {
        let mut seen = HashSet::new();
        assert!(claim_notification(&mut seen, "account:message"));
        assert!(!claim_notification(&mut seen, "account:message"));
        assert!(claim_notification(&mut seen, "account:other-message"));
    }
}

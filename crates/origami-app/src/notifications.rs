//! Desktop notification policy and rendering.

use chrono::Timelike;
use notify_rust::Notification;
use origami_core::config::{parse_time, NotificationConfig, NotificationPreview};
use origami_core::model::MailboxRole;
use std::collections::{HashMap, HashSet};

/// Rendered notification, before it reaches the OS. Keeping this separate
/// from `notify_rust` lets tests count exactly what a new mail produces.
pub struct NotificationDraft {
    pub summary: String,
    pub body: String,
    /// XDG notification id for grouped replacement, when grouping is on.
    pub id: Option<u32>,
    pub timeout_ms: Option<u32>,
}

/// Per-process state for the notification policy: which logical messages
/// were already announced, and the running grouped counts/ids per account.
#[derive(Default)]
pub struct NotificationBookkeeping {
    seen: HashSet<String>,
    grouped_counts: HashMap<String, u32>,
    grouped_ids: HashMap<String, u32>,
    next_id: u32,
}

impl NotificationBookkeeping {
    pub fn new(first_id: u32) -> Self {
        Self {
            next_id: first_id,
            ..Self::default()
        }
    }

    /// Forget a removed account's grouped count and notification id.
    pub fn forget_account(&mut self, account_id: &str) {
        self.grouped_counts.remove(account_id);
        self.grouped_ids.remove(account_id);
    }
}

/// Decide and render exactly one notification for a new envelope. Returns
/// whether a notification was produced; physical copies of the same logical
/// message are announced once.
#[allow(clippy::too_many_arguments)]
pub fn notify_new_mail(
    bookkeeping: &mut NotificationBookkeeping,
    settings: &NotificationConfig,
    account_name: &str,
    account_id: &str,
    logical_id: &str,
    folder_role: Option<MailboxRole>,
    unread: bool,
    subject: &str,
    from: &str,
    show: &mut dyn FnMut(NotificationDraft),
) -> bool {
    if !should_notify(settings, folder_role, unread) {
        return false;
    }
    if !claim_notification(&mut bookkeeping.seen, logical_id) {
        return false;
    }
    if settings.grouped_per_account {
        let count = bookkeeping
            .grouped_counts
            .entry(account_id.to_string())
            .or_insert(0);
        *count += 1;
        let id = match bookkeeping.grouped_ids.get(account_id) {
            Some(id) => *id,
            None => {
                let id = bookkeeping.next_id;
                bookkeeping.next_id += 1;
                bookkeeping.grouped_ids.insert(account_id.to_string(), id);
                id
            }
        };
        let (sender, subject_line) = render_preview(settings.preview, subject, from);
        let body = match settings.preview {
            NotificationPreview::Full => format!("{sender}: {subject_line}"),
            NotificationPreview::SenderOnly => sender,
            NotificationPreview::Hidden => subject_line,
        };
        show(NotificationDraft {
            summary: grouped_summary(account_name, *count),
            body,
            id: Some(id),
            timeout_ms: None,
        });
    } else {
        let (summary, body) = render_preview(settings.preview, subject, from);
        show(NotificationDraft {
            summary,
            body,
            id: None,
            timeout_ms: Some(8000),
        });
    }
    true
}

/// Default sink: hand the draft to the desktop notification service.
pub fn show_notification(draft: &NotificationDraft) {
    let mut notification = Notification::new();
    notification
        .appname("Origami")
        .summary(&draft.summary)
        .body(&draft.body)
        .icon("origami");
    if let Some(id) = draft.id {
        notification.id(id);
    }
    if let Some(timeout_ms) = draft.timeout_ms {
        notification.timeout(notify_rust::Timeout::Milliseconds(timeout_ms));
    }
    let _ = notification.show();
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
    fn one_notification_per_new_mail_in_both_modes() {
        let mut book = NotificationBookkeeping::new(1);
        let mut settings = NotificationConfig::default();
        settings.grouped_per_account = true;
        let mut sends: Vec<NotificationDraft> = Vec::new();

        {
            let mut show = |draft: NotificationDraft| sends.push(draft);
            assert!(notify_new_mail(
                &mut book,
                &settings,
                "Work",
                "acc",
                "logical-1",
                Some(MailboxRole::Inbox),
                true,
                "Subject",
                "Ada",
                &mut show,
            ));
            assert!(!notify_new_mail(
                &mut book,
                &settings,
                "Work",
                "acc",
                "logical-1",
                Some(MailboxRole::Inbox),
                true,
                "Subject",
                "Ada",
                &mut show,
            ));
        }
        assert_eq!(sends.len(), 1);
        assert!(sends[0].id.is_some());
        assert_eq!(sends[0].summary, "Work — 1 new");

        settings.grouped_per_account = false;
        {
            let mut show = |draft: NotificationDraft| sends.push(draft);
            assert!(notify_new_mail(
                &mut book,
                &settings,
                "Work",
                "acc",
                "logical-2",
                Some(MailboxRole::Inbox),
                true,
                "Other",
                "Bob",
                &mut show,
            ));
        }
        assert_eq!(sends.len(), 2);
        assert!(sends[1].id.is_none());
    }

    #[test]
    fn logical_message_notifications_are_claimed_once() {
        let mut seen = HashSet::new();
        assert!(claim_notification(&mut seen, "account:message"));
        assert!(!claim_notification(&mut seen, "account:message"));
        assert!(claim_notification(&mut seen, "account:other-message"));
    }
}

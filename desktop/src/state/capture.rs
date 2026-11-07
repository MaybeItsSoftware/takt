//! A typed task's trailing words, read by the core (`capture::parse`):
//! `Write the notes 45m #work @fri !1` files a task with its estimate, tag,
//! due day and priority in the one undo step. The add row and quick capture
//! both go through it. `TaskCaptureSyntax.swift` and
//! `QuickCapturePanelController.swift`.

use super::App;
use super::store::now_ms;
use takt_core::capture::{self, CaptureParts};
use takt_core::tasks::NewTask;

impl App {
    /// What the core reads off `text`, in this workspace's zone.
    pub fn capture_parts(&self, text: &str) -> CaptureParts {
        capture::parse(text, now_ms(), takt_core::periodic::zone(&self.store.zone))
    }

    /// The chips a typed title would file under: "45m", "Fri", "#work".
    pub fn capture_labels(&self, text: &str) -> Vec<String> {
        capture::detail_labels(
            &self.capture_parts(text),
            now_ms(),
            takt_core::periodic::zone(&self.store.zone),
        )
    }

    /// A task to create from typed text, with what it says about itself.
    /// Err with the reason when it can't be filed as read.
    pub fn new_task_from(&self, text: &str, list_id: &str) -> Result<NewTask, String> {
        let parts = self.capture_parts(text);
        if let Some(problem) = parts.problem {
            return Err(problem);
        }
        if parts.title.trim().is_empty() {
            return Err("A task needs a title.".into());
        }
        Ok(NewTask {
            list_id: list_id.into(),
            title: parts.title,
            kind: "task".into(),
            estimate_seconds: parts.estimate_seconds,
            due_at_ms: parts.due_at_ms,
            start_at_ms: parts.start_at_ms,
            tags: parts.tags,
            priority: parts.priority,
            waiting_on: parts.waiting_on,
            external_links: parts.links,
            ..Default::default()
        })
    }

    /// Where quick capture files a task: the chosen list while it exists,
    /// else the inbox, else the first list.
    pub fn quick_capture_destination(&self) -> Option<(String, String)> {
        self.prefs
            .quick_capture_list
            .as_deref()
            .and_then(|id| self.list(id))
            .or_else(|| self.inbox())
            .or_else(|| self.lists.first())
            .map(|list| (list.id.clone(), list.name.clone()))
    }

    /// Files `text` from the quick capture window, at the top of its list.
    /// Returns what to say: Ok with the title filed, Err with why not.
    pub fn quick_capture(&mut self, text: &str) -> Result<String, String> {
        let Some((list_id, list_name)) = self.quick_capture_destination() else {
            return Err("Make a list first.".into());
        };
        let mut new = self.new_task_from(text, &list_id)?;
        new.at_top = true;
        let title = new.title.clone();
        let created = self.write(|store| store.core.create_task(new, now_ms()));
        match created {
            Some(_) => {
                self.reload_rows();
                self.reload_today();
                self.message = Some(format!("Added “{title}” to {list_name}."));
                Ok(title)
            }
            None => Err(self
                .message
                .clone()
                .unwrap_or_else(|| "Couldn't add it.".into())),
        }
    }
}

#[cfg(test)]
mod tests {
    use takt_core::capture::parse;

    #[test]
    fn trailing_words_are_read_off_the_title() {
        let parts = parse(
            "Write the notes 45m #work !1",
            0,
            takt_core::periodic::zone("UTC"),
        );
        assert_eq!(parts.title, "Write the notes");
        assert_eq!(parts.estimate_seconds, Some(45 * 60));
        assert_eq!(parts.tags, vec!["work".to_string()]);
        assert_eq!(parts.priority, Some(1));
    }
}

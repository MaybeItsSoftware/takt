//! The one error every call into the core can return.

/// What went wrong in the core, as a client sees it.
///
/// The field is `detail` rather than `message`: Kotlin's generated exception
/// would otherwise clash with `Throwable.message`.
#[derive(Debug, thiserror::Error, uniffi::Error)]
pub enum CoreError {
    #[error("The workspace database failed: {detail}")]
    Database { detail: String },
    #[error("Migration {identifier} left {count} broken foreign key(s); nothing was saved.")]
    ForeignKeys { identifier: String, count: u32 },
    #[error("The workspace has no undo journal to record into.")]
    NoJournal,
    #[error("No task with id {id}.")]
    MissingTask { id: String },
    #[error("No daily with id {id}.")]
    MissingDaily { id: String },
    #[error("No list with id {id}.")]
    MissingList { id: String },
    #[error("No folder with id {id}.")]
    MissingFolder { id: String },
    #[error("The Inbox cannot be archived or deleted. You can rename it instead.")]
    SystemListIsPermanent,
    #[error("An archived or completed list can't be prioritised. Restore it first.")]
    ListCannotBePrioritised,
    #[error("A workspace item needs a name.")]
    EmptyName,
    #[error("A folder cannot go inside itself or one of its own folders.")]
    InvalidFolderMove,
    #[error("A required condition is missing, archived or belongs to another workspace.")]
    InvalidCondition,
    #[error("{status} is not a task status; use open, completed or cancelled.")]
    InvalidStatus { status: String },
    #[error("Start must be before the deadline.")]
    InvalidSchedule,
    #[error("Enter a minimum useful block of at least one minute.")]
    InvalidMinimum,
    #[error("One-sitting tasks need a positive estimate at least as long as their minimum block.")]
    EstimateRequired,
    #[error("Choose a valid calendar date.")]
    InvalidDate,
    #[error("This task changed while it was open. Review the latest version before saving.")]
    EditorConflict,
    #[error("That task cannot be the list's visible root.")]
    InvalidVisibleRoot,
    #[error("The tasks to import contain the same source id twice.")]
    DuplicateSourceId,
    #[error("There is no task in focus to finish.")]
    NoActiveFocusTask,
    #[error("That task is not available to focus on now.")]
    Unavailable,
    #[error("A task cannot be moved into itself or one of its subtasks.")]
    InvalidTaskMove,
    #[error("A task cannot wait on itself, or on a task that already waits on it.")]
    DependencyCycle,
    #[error("No filter list with id {id}.")]
    MissingFilterList { id: String },
    #[error("{detail}")]
    InvalidFilter { detail: String },
    #[error("{detail}")]
    File { detail: String },
    /// A board column edit or a drop into a column that cannot be made.
    #[error("{detail}")]
    ColumnRefused { detail: String },
    #[error(
        "“{rule}” is not a schedule. Try daily, weekdays, every friday, every 2 weeks or mon, wed, fri."
    )]
    InvalidRecurrence { rule: String },
    /// A Checkvist request the core refused to make, or an answer it could
    /// not read (`checkvist.rs`).
    #[error("{detail}")]
    Checkvist { detail: String },
    /// An estimate or minimum block typed in minutes that does not read as
    /// one (`editor_draft.rs`).
    #[error("Enter a non-negative number of minutes within the supported range.")]
    InvalidEstimate,
    /// A follow-up repeat outside 1 to 365 days (`waiting.rs`).
    #[error("A follow-up repeats every 1 to 365 days, not {days}.")]
    InvalidFollowUpRepeat { days: u32 },
    /// A Google Calendar answer the core could not read
    /// (`google_calendar.rs`).
    #[error("{detail}")]
    GoogleCalendar { detail: String },
}

impl From<rusqlite::Error> for CoreError {
    fn from(error: rusqlite::Error) -> Self {
        CoreError::Database {
            detail: error.to_string(),
        }
    }
}

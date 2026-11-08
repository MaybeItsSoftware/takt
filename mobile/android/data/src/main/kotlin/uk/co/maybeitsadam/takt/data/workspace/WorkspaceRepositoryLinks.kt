package uk.co.maybeitsadam.takt.data.workspace

import java.time.Instant

// A task's links (docs/links.md, WorkspaceStore+Links.swift): the addresses
// and the names a picker knew are the Rust core's (`core/src/links.rs`).

/**
 * Links [url] to [taskId] as one "Add Link" undo step, naming it [title] with
 * the file type [mimeType] when the picker knew them. Returns whether the
 * address is new; one already there takes the name.
 */
suspend fun WorkspaceRepository.addLink(
    taskId: String,
    url: String,
    title: String? = null,
    mimeType: String? = null,
    now: Instant = now(),
): Boolean = coreWrite { it.addTaskLink(taskId, url, title, mimeType, now.toEpochMilli()) }

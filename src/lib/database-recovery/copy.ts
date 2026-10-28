import type { DatabaseProblemKind, RecoveryStep } from "@/bindings";

/**
 * Every word on the recovery screen. Which steps appear for which problem is
 * decided by the backend (src-tauri/src/recovery.rs); only the wording lives
 * here.
 */
export const PROBLEM_COPY: Record<
	DatabaseProblemKind,
	{ title: string; lede: string }
> = {
	damaged: {
		title: "Citadel’s database is damaged",
		lede: "Your books are safe: Calibre libraries are kept separately. Citadel’s own records, like your library list and sharing password, can’t be read.",
	},
	newerVersion: {
		title: "This database is from a newer Citadel",
		lede: "A newer version of Citadel has updated it. Install the latest version to keep using it.",
	},
	migrationFailed: {
		title: "Citadel couldn’t update its database",
		lede: "Nothing was changed, and the previous version of Citadel can still open it. Check for a newer version, or start fresh.",
	},
	migrationInterrupted: {
		title: "Updating Citadel’s database keeps stopping",
		lede: "It stopped partway through twice. Nothing was changed.",
	},
	unreadable: {
		title: "Citadel can’t open its database",
		lede: "Check that the folder below exists and that Citadel can write to it, then open Citadel again.",
	},
};

export const STEP_COPY: Record<
	Exclude<RecoveryStep, "quit">,
	{ title: string; description: (backupDate: string | null) => string }
> = {
	restoreBackup: {
		title: "Restore the backup",
		description: (backupDate) =>
			backupDate
				? `Anything changed after ${backupDate} is lost.`
				: "Anything changed since it was taken is lost.",
	},
	startFresh: {
		title: "Start fresh",
		description: () =>
			"Add your libraries again and set a new sharing password. The current file is kept, not deleted.",
	},
	tryAgain: {
		title: "Try again",
		description: () => "Run the update once more.",
	},
};

export const QUIT_LABEL = "Quit Citadel";
export const DETAILS_LABEL = "Details";
export const REVEAL_LABEL = "Show in Folder";
export const WORKING_LABEL = "Working…";
export const FAILED_PREFIX = "That didn’t work:";

export const formatBackupDate = (createdAt: string | null): string | null =>
	createdAt
		? new Date(createdAt).toLocaleString("en-CA", {
				dateStyle: "long",
				timeStyle: "short",
			})
		: null;

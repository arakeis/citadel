import type {
	DetectedLibrary,
	LegacyLibraryImport,
	LibraryEntry,
	LibraryStats,
	SyncStatus,
} from "@/bindings";
import type { Library, Options } from "../library";

export interface LibraryCatalog {
	open(options: Options): Promise<Library>;
	create(path: string): Promise<void>;
	isValid(path: string): Promise<boolean>;
	detect(): Promise<DetectedLibrary | null>;
	defaultNewPath(): Promise<string>;
	stats(path: string): Promise<LibraryStats>;
	syncStatus(path: string): Promise<SyncStatus>;
	/** Registered libraries, keyed by Calibre library UUID (ADR 0009). */
	list(): Promise<LibraryEntry[]>;
	/** Adds the library at `path`, or returns its existing entry. */
	register(path: string): Promise<LibraryEntry>;
	rename(id: string, displayName: string): Promise<LibraryEntry>;
	/** Moves a pre-registry settings.json library list into the registry. */
	importLegacy(entries: LibraryEntry[]): Promise<LegacyLibraryImport>;
	/** Called with the full list whenever any window changes it. */
	onChange(listener: (libraries: LibraryEntry[]) => void): Promise<() => void>;
}

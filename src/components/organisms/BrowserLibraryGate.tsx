import type { ReactNode } from "react";
import { Button, LoadingOverlay } from "@/components/ui";
import { useBrowserLibrary } from "@/lib/hooks/use-browser-library";
import { LibraryState } from "@/stores/library/store";

export const BrowserLibraryGate = ({ children }: { children: ReactNode }) => {
	const { state, error, retry } = useBrowserLibrary();
	if (state === LibraryState.ready) return <>{children}</>;
	if (state === LibraryState.error)
		return (
			<main style={{ padding: "3rem", maxWidth: "40rem", margin: "auto" }}>
				<h1>Couldn’t open your library</h1>
				<p role="alert">
					{error?.message ?? "The Citadel server could not be reached."}
				</p>
				<Button onClick={() => void retry()}>Try again</Button>
			</main>
		);
	return (
		<div style={{ height: "100vh", position: "relative" }}>
			<LoadingOverlay visible />
		</div>
	);
};

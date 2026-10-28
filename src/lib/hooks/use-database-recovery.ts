import { useCallback, useEffect, useState } from "react";
import type { DatabaseProblem, RecoveryStep } from "@/bindings";
import { usePlatform } from "@/lib/platform/context";

export interface DatabaseRecovery {
	/** The step running now; the app restarts when it succeeds. */
	running: RecoveryStep | null;
	error: string | null;
	recover: (step: RecoveryStep) => void;
	reveal: () => void;
}

export const useDatabaseRecovery = (
	problem: DatabaseProblem,
): DatabaseRecovery => {
	const platform = usePlatform();
	const [running, setRunning] = useState<RecoveryStep | null>(null);
	const [error, setError] = useState<string | null>(null);

	// Settings never load in this state, so nothing else shows the window.
	useEffect(() => {
		void platform.window.show();
	}, [platform]);

	const recover = useCallback(
		(step: RecoveryStep) => {
			setRunning(step);
			setError(null);
			platform.recovery.recover(step).catch((cause: unknown) => {
				setRunning(null);
				setError(cause instanceof Error ? cause.message : String(cause));
			});
		},
		[platform],
	);

	const reveal = useCallback(() => {
		void platform.fileOpener.revealInFileManager(problem.databasePath);
	}, [platform, problem.databasePath]);

	return { running, error, recover, reveal };
};

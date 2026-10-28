import type { RecoveryAdapter } from "../types";
import { commands } from "@/bindings";

export const tauriRecovery: RecoveryAdapter = {
	problem: () => commands.clbQueryDatabaseProblem(),
	recover: async (step) => {
		const result = await commands.clbCmdRecoverDatabase(step);
		if (result.status === "error") throw new Error(result.error);
	},
};

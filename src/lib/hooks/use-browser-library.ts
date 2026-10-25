import { useEffect } from "react";
import { useLibraryStore } from "@/stores/library/store";

/** Browser connections belong to the serving origin, never desktop path settings. */
export const useBrowserLibrary = () => {
	const actions = useLibraryStore((state) => state.actions);
	const state = useLibraryStore((state) => state.libraryState);
	const error = useLibraryStore((state) => state.libraryError);
	useEffect(() => {
		void actions.initialize({
			libraryType: "calibre",
			connectionType: "remote",
			url: window.location.origin,
		});
	}, [actions]);
	return {
		state,
		error,
		retry: () =>
			actions.initialize({
				libraryType: "calibre",
				connectionType: "remote",
				url: window.location.origin,
			}),
	};
};

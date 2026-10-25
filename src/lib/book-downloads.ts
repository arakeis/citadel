import type { BookFile } from "@/bindings";

export interface BookDownload {
	url: string;
	label: string;
}

export const remoteBookDownloads = (files: BookFile[]): BookDownload[] =>
	files.flatMap((file) => {
		if (!("Remote" in file)) return [];
		try {
			const url = new URL(file.Remote.url);
			if (url.protocol !== "http:" && url.protocol !== "https:") return [];
			const filename = decodeURIComponent(url.pathname.split("/").at(-1) ?? "");
			const extension = filename.split(".").at(-1);
			return [
				{
					url: url.href,
					label:
						extension && extension !== filename
							? extension.toUpperCase()
							: "File",
				},
			];
		} catch {
			return [];
		}
	});

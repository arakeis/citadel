import { describe, expect, it } from "vitest";
import { remoteBookDownloads } from "./book-downloads";

describe("remote book downloads", () => {
	it("offers HTTP files with a format label and preserves encoded filenames", () => {
		expect(
			remoteBookDownloads([
				{
					Remote: {
						url: "https://books.example/api/books/1/files/EPUB/A%20Book.epub",
					},
				},
				{ Remote: { url: "https://books.example/file" } },
			]),
		).toEqual([
			{
				url: "https://books.example/api/books/1/files/EPUB/A%20Book.epub",
				label: "EPUB",
			},
			{ url: "https://books.example/file", label: "File" },
		]);
	});

	it("omits malformed, script, data, and local file URLs", () => {
		expect(
			remoteBookDownloads([
				{ Remote: { url: "javascript:alert(1)" } },
				{ Remote: { url: "data:text/html,hi" } },
				{ Remote: { url: "file:///books/title.epub" } },
				{ Remote: { url: "not a URL" } },
				{ Remote: { url: "https://books.example/%invalid" } },
			]),
		).toEqual([]);
	});
});

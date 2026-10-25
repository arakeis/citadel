import { describe, expect, it, vi } from "vitest";
import type { LibraryBook, LibraryBookQuery } from "@/bindings";
import { createHttpLibrary } from "./http";

const query: LibraryBookQuery = {
	text: "search",
	author_id: "1",
	series_id: null,
	tag_id: 2,
	publisher_id: null,
	hide_read: false,
	sort: "AuthorAsc",
	limit: 100,
	offset: 200,
};
const book: LibraryBook = {
	id: "7",
	uuid: null,
	title: "A book",
	author_list: [],
	tag_list: [],
	sortable_title: null,
	file_list: [{ Remote: { url: "/api/books/7/files/epub/A%20book.epub" } }],
	cover_image: {
		kind: "Remote",
		local_path: null,
		url: "/api/books/7/cover",
	},
	identifier_list: [],
	description: null,
	is_read: false,
	series: null,
	series_index: null,
	language_list: [],
};
const json = (data: unknown, status = 200) =>
	new Response(JSON.stringify(data), {
		status,
		headers: { "content-type": "application/json" },
	});

describe("HTTP Library", () => {
	it("preserves paged search/facets and resolves cover/download URLs without local paths", async () => {
		const requests: Request[] = [];
		const library = createHttpLibrary({
			baseUrl: "https://citadel.test",
			fetch: async (request) => {
				requests.push(request);
				return request.method === "POST"
					? json({ items: [book], total: 300 })
					: json({ ...book, cover_image: null });
			},
		});
		const page = await library.queryBooks(query);
		expect(requests[0]?.url).toBe("https://citadel.test/api/books/query");
		expect(await requests[0]?.json()).toEqual(query);
		expect(page.total).toBe(300);
		expect(page.items[0]?.cover_image?.url).toBe(
			"https://citadel.test/api/books/7/cover",
		);
		expect(page.items[0]?.file_list[0]).toEqual({
			Remote: {
				url: "https://citadel.test/api/books/7/files/epub/A%20book.epub",
			},
		});
		expect(library.getCoverUrlForBook("7")).toBe(
			"https://citadel.test/api/books/7/cover",
		);
		expect(library.getCoverPathForBook("7")).toBeUndefined();
		expect(library.getDefaultFilePathForBook("7")).toBeUndefined();
		await library.getBook("7");
		expect(requests[1]?.url).toBe("https://citadel.test/api/books/7");
		expect(library.getCoverUrlForBook("7")).toBeUndefined();
	});

	it("routes facet and custom-field reads through the generated HTTP contract", async () => {
		const paths: string[] = [];
		const library = createHttpLibrary({
			baseUrl: "https://citadel.test",
			fetch: async (request) => {
				paths.push(new URL(request.url).pathname);
				return json([]);
			},
		});
		await library.listAuthors();
		await library.listSeries();
		await library.listTags();
		await library.listPublishers();
		await library.listCustomColumns();
		await library.getCustomValuesForBook("7");
		expect(paths).toEqual([
			"/api/authors",
			"/api/series",
			"/api/tags",
			"/api/publishers",
			"/api/custom-columns",
			"/api/books/7/custom-values",
		]);
	});

	it("batches thumbnail requests and leaves whole-library warming to the server", async () => {
		const batches: string[][] = [];
		const library = createHttpLibrary({
			baseUrl: "https://citadel.test",
			fetch: async (request) => {
				if (request.method === "GET") return json([]);
				const { book_ids } = (await request.json()) as { book_ids: string[] };
				batches.push(book_ids);
				return json(
					book_ids.map((id) => ({
						book_id: id,
						url: `/api/books/${id}/thumbnail`,
						width: 200,
						height: 300,
					})),
				);
			},
		});
		const thumbs = await library.ensureCoverThumbnails(
			Array.from({ length: 201 }, (_, index) => String(index + 1)),
		);
		expect(batches.map((batch) => batch.length)).toEqual([200, 1]);
		expect(thumbs).toHaveLength(201);
		expect(thumbs[0]?.url).toBe("https://citadel.test/api/books/1/thumbnail");
		expect(await library.listCoverThumbnails()).toEqual([]);
		expect(await library.warmCoverThumbnails()).toEqual([]);
	});

	it("propagates structured authentication and library errors", async () => {
		const library = createHttpLibrary({
			baseUrl: "https://citadel.test",
			fetch: async () =>
				json({ code: "unauthorized", message: "Authentication required" }, 401),
		});
		await expect(library.queryBooks(query)).rejects.toThrow(
			"Authentication required",
		);
		await expect(library.listAuthors()).rejects.toThrow(
			"Authentication required",
		);
	});

	it("rejects writes/imports before issuing any request", async () => {
		const fetch = vi.fn();
		const library = createHttpLibrary({
			baseUrl: "https://citadel.test",
			fetch,
		});
		await expect(
			library.updateBook("7", {
				title: "Changed",
				author_id_list: null,
				tag_list: null,
				timestamp: null,
				publication_date: null,
				is_read: null,
				description: null,
				series: null,
				series_index: null,
				language_list: null,
			}),
		).rejects.toThrow("browsing and downloads only");
		await expect(library.deleteBooks(["7"])).rejects.toThrow(
			"browsing and downloads only",
		);
		await expect(
			library.checkFileImportable("/server/book.epub"),
		).rejects.toThrow("browsing and downloads only");
		expect(fetch).not.toHaveBeenCalled();
	});
});

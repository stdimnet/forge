import { describe, expect, it } from "vitest";
import "@/mock/workspaceMock";
import "@/mock/gitMock";
import "@/mock/gitChangesMock";
import { changesApi } from "./changesApi";
import { commitSpecs, inclusionOf, isHunkIncluded, loadHunks, setHunkIncluded, setIncluded, setInclusion } from "./changelists";

const USER = "app/Models/User.php";

describe("commit inclusion", () => {
  it("includes files and single hunks", async () => {
    const hunks = await loadHunks(USER);
    const ids = hunks.map((h) => h.id);
    expect(ids.length).toBeGreaterThan(1);

    setIncluded([USER]);
    expect(inclusionOf(USER, null)).toBe("all");
    expect(await commitSpecs([USER])).toEqual([{ path: USER, hunks: null }]);

    // Excluding one hunk makes the file partly included.
    setHunkIncluded(USER, ids[0], false, ids);
    expect(isHunkIncluded(USER, ids[0])).toBe(false);
    expect(inclusionOf(USER, null)).toBe("some");
    expect(await commitSpecs([USER])).toEqual([{ path: USER, hunks: ids.slice(1) }]);

    // Excluding every hunk unchecks the file.
    for (const id of ids.slice(1)) setHunkIncluded(USER, id, false, ids);
    expect(inclusionOf(USER, null)).toBe("none");
    expect(await commitSpecs([USER])).toEqual([]);

    // Checking one hunk of an unchecked file includes only that hunk.
    setInclusion(USER, [ids[1]], true, ids);
    expect(await commitSpecs([USER])).toEqual([{ path: USER, hunks: [ids[1]] }]);
    expect(inclusionOf(USER, [ids[1]])).toBe("all");
    expect(inclusionOf(USER, [ids[0]])).toBe("none");

    setInclusion(USER, null, false);
    expect(inclusionOf(USER, null)).toBe("none");
  });

  it("commits what is included", async () => {
    const ids = (await loadHunks(USER)).map((h) => h.id);
    setIncluded([USER]);
    setHunkIncluded(USER, ids[0], false, ids);
    const specs = await commitSpecs([USER]);
    await changesApi.commitFiles("Part", specs, { amend: false, signOff: false, noVerify: false, author: null });
    const left = await changesApi.fileHunks(USER);
    expect(left.map((h) => h.id)).toContain(ids[0]);
    expect(left).toHaveLength(1);
  });
});
